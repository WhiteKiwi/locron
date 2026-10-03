//! Canonical installed-pair qualification and no-write exclusive handoff.
//! This preserves live guards; it does not authorize replacement or task effects.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::{
    DirectoryGuard, FileIdentity, GuardedFile, file_identity, open_private_exclusive,
};

use super::super::sha256_hex;
use super::super::windows_ownership::{VerifiedFile, immutable_private_until};
use super::super::windows_paired_ownership::{self, PairedInventory};
use super::super::windows_paired_receipt::{EXECUTABLES, PAYLOADS, RECEIPT, Receipt};
use super::super::windows_protocol::maintenance_path;
use super::super::windows_receipt::same_path;
use super::paired::Payloads;

const PAYLOAD_LIMIT: usize = 64 * 1024 * 1024;
const RECEIPT_LIMIT: usize = 128 * 1024;

fn clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original paired handoff deadline expired"
    );
    Ok(())
}

fn observe<T>(deadline: Instant, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    clock(deadline)?;
    let result = operation();
    clock(deadline)?;
    result
}

fn text(path: &Path) -> Result<&str> {
    path.to_str().context("paired handoff path is not Unicode")
}

/// No serialized or arbitrary-facts constructor exists. Canonical bytes and all
/// eight original objects stay bound until the explicit exclusive transition.
pub(in crate::self_update) struct CanonicalPair<'a> {
    inventory: PairedInventory,
    canonical: &'a Payloads,
    deadline: Instant,
}

impl<'a> CanonicalPair<'a> {
    pub(in crate::self_update) fn verify_until(
        directory: &Path,
        canonical: &'a Payloads,
        deadline: Instant,
    ) -> Result<Self> {
        clock(deadline)?;
        let inventory = windows_paired_ownership::verify_until(directory, deadline)?;
        let sid = locron_core::windows::current_user_sid_until(deadline)?;
        observe(deadline, || {
            canonical.verify_receipt(inventory.receipt(), &sid, text(inventory.directory())?)
        })?;
        for name in PAYLOADS {
            let proof = &inventory.payloads()[name];
            let bytes = &canonical.files()[name];
            observe(deadline, || {
                ensure!(
                    proof.file.metadata()?.len() == bytes.len() as u64
                        && proof.sha256 == sha256_hex(bytes),
                    "installed paired {name} differs from canonical bytes"
                );
                Ok(())
            })?;
        }
        clock(deadline)?;
        Ok(Self {
            inventory,
            canonical,
            deadline,
        })
    }

    pub(in crate::self_update) fn inventory(&self) -> &PairedInventory {
        &self.inventory
    }

    /// Checks mapped/open holders; this does not stop tasks or authorize effects.
    pub(in crate::self_update) fn into_exclusive(self) -> Result<ExclusivePair<'a>> {
        self.exclusive_after(|| Ok(()))
    }

    // The callback is private and only tests inject a substitution into the real
    // read-to-exclusive gap. Production callers cannot supply alternate admission.
    fn exclusive_after(
        self,
        after_release: impl FnOnce() -> Result<()>,
    ) -> Result<ExclusivePair<'a>> {
        let Self {
            inventory,
            canonical,
            deadline,
        } = self;
        clock(deadline)?;
        let directory = observe(deadline, || {
            Ok(DirectoryGuard::existing_private(inventory.directory())?)
        })?;
        let expected = [
            ImageFact::capture(&inventory.payloads()[EXECUTABLES[0]], deadline)?,
            ImageFact::capture(&inventory.payloads()[EXECUTABLES[1]], deadline)?,
        ];
        let mut companions = BTreeMap::new();
        for name in PAYLOADS.into_iter().chain([RECEIPT]) {
            if EXECUTABLES.contains(&name) {
                continue;
            }
            let original = if name == RECEIPT {
                inventory.receipt_file()
            } else {
                &inventory.payloads()[name]
            };
            let limit = if name == RECEIPT {
                RECEIPT_LIMIT
            } else {
                PAYLOAD_LIMIT
            };
            let path = directory.normalized_path().join(name);
            let (retained, _) = immutable_private_until(&path, limit, deadline)?;
            observe(deadline, || {
                ensure!(
                    retained.identity == original.identity
                        && retained.sha256 == original.sha256
                        && same_path(text(retained.file.normalized_path())?, text(&path)?)?,
                    "paired companion changed during retained-guard handoff"
                );
                Ok(())
            })?;
            companions.insert(name.to_owned(), retained);
        }
        let receipt = inventory.receipt().clone();
        // All non-executable guards overlap. Only the two image read handles are
        // now released; the gap is defended by exact-ID and byte revalidation.
        drop(inventory);
        after_release()?;
        clock(deadline)?;
        let console = expected[0].open(deadline)?;
        let launcher = expected[1].open(deadline)?;
        clock(deadline)?;
        Ok(ExclusivePair {
            directory,
            receipt,
            images: [console, launcher],
            companions,
            identities: [expected[0].identity, expected[1].identity],
            _canonical: canonical,
        })
    }
}

struct ImageFact {
    path: PathBuf,
    identity: FileIdentity,
    bytes: u64,
    sha256: String,
}

impl ImageFact {
    fn capture(proof: &VerifiedFile, deadline: Instant) -> Result<Self> {
        observe(deadline, || {
            maintenance_path(text(proof.file.normalized_path())?)?;
            ensure!(
                file_identity(&proof.file)? == proof.identity,
                "paired image identity changed"
            );
            Ok(Self {
                path: proof.file.normalized_path().to_owned(),
                identity: proof.identity,
                bytes: proof.file.metadata()?.len(),
                sha256: proof.sha256.clone(),
            })
        })
    }

    fn open(&self, deadline: Instant) -> Result<GuardedFile> {
        let mut file = observe(deadline, || Ok(open_private_exclusive(&self.path)?))?;
        observe(deadline, || {
            ensure!(
                file_identity(&file)? == self.identity
                    && same_path(text(file.normalized_path())?, text(&self.path)?)?
                    && file.metadata()?.len() == self.bytes
                    && self.bytes <= PAYLOAD_LIMIT as u64,
                "exclusive paired image is not the original exact object"
            );
            file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut *file)
                .take(PAYLOAD_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 == self.bytes
                    && sha256_hex(&bytes) == self.sha256
                    && file_identity(&file)? == self.identity
                    && file.metadata()?.len() == self.bytes,
                "exclusive paired image bytes differ from the verified source"
            );
            file.seek(SeekFrom::Start(0))?;
            Ok(())
        })?;
        Ok(file)
    }
}

/// Both existing images are exclusively held, with receipt and companion reads
/// continuously protected. No mutable handle or deletion API is exposed here.
pub(in crate::self_update) struct ExclusivePair<'a> {
    directory: DirectoryGuard,
    receipt: Receipt,
    images: [GuardedFile; 2],
    companions: BTreeMap<String, VerifiedFile>,
    identities: [FileIdentity; 2],
    _canonical: &'a Payloads,
}

impl ExclusivePair<'_> {
    pub(in crate::self_update) fn directory(&self) -> &Path {
        self.directory.normalized_path()
    }

    pub(in crate::self_update) fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    pub(in crate::self_update) fn identities(&self) -> &[FileIdentity; 2] {
        &self.identities
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write};
    use std::time::Duration;

    use locron_core::filesystem::create_private_new_exclusive;
    use serde_json::{Value, json};
    use zip::write::SimpleFileOptions;

    use super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::windows_ownership::native_target;
    use super::super::super::windows_package::PAIRED_FILES;
    use super::super::super::windows_release_source::{Release, payload_inventory};
    use super::*;

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn pe(subsystem: u16) -> Vec<u8> {
        let machine = if native_target().unwrap().starts_with("x86_64") {
            0x8664_u16
        } else {
            0xaa64_u16
        };
        let mut bytes = vec![0; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        for (at, value) in [
            (132, machine),
            (134, 1),
            (148, 240),
            (150, 0x22),
            (152, 0x20b),
            (220, subsystem),
        ] {
            bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
        bytes
    }

    fn payloads() -> Payloads {
        let target = native_target().unwrap();
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in PAIRED_FILES {
            archive
                .start_file(
                    format!("locron-v0.10.0-{target}/{name}"),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            let bytes = match name {
                "locron.exe" => pe(3),
                "locron-service-launcher.exe" => pe(2),
                _ => format!("canonical fixture {name}").into_bytes(),
            };
            archive.write_all(&bytes).unwrap();
        }
        let archive = archive.finish().unwrap().into_inner();
        let installer = b"canonical bootstrap fixture, never executed";
        let uninstaller = b"canonical removal fixture, never executed";
        let mut digests: BTreeMap<_, _> = payload_inventory("0.10.0")
            .unwrap()
            .into_iter()
            .map(|name| (name, "ab".repeat(32)))
            .collect();
        digests.insert(format!("locron-v0.10.0-{target}.zip"), sha256_hex(&archive));
        let sums = digests
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>()
            .into_bytes();
        digests.insert("SHA256SUMS.txt".into(), sha256_hex(&sums));
        digests.insert("install.sh".into(), "ab".repeat(32));
        digests.insert("install.ps1".into(), sha256_hex(installer));
        digests.insert("uninstall.ps1".into(), sha256_hex(uninstaller));
        let metadata = json!({"tag_name": "v0.10.0", "draft": false, "prerelease": false,
            "assets": digests.into_iter().map(|(name, hash)| json!({
                "browser_download_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/{name}"),
                "name": name, "digest": format!("sha256:{hash}")
            })).collect::<Vec<_>>()});
        let release =
            Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some("0.10.0")).unwrap();
        Payloads::verify(&release, target, &sums, &archive, installer, uninstaller).unwrap()
    }

    fn fixture() -> (PrivateFixture, Payloads) {
        let root = PrivateFixture::new("locron-paired-handoff-");
        let canonical = payloads();
        for (name, bytes) in canonical.files() {
            write_new(&root.path().join(name), bytes);
        }
        let sid = locron_core::windows::current_user_sid().unwrap();
        let (_, receipt) = canonical
            .receipt(&sid, root.path().to_str().unwrap(), None)
            .unwrap();
        write_new(&root.path().join(RECEIPT), &receipt);
        fs::write(root.path().join("unrelated-marker"), b"preserve me").unwrap();
        (root, canonical)
    }

    fn read<'a>(root: &Path, canonical: &'a Payloads) -> Result<CanonicalPair<'a>> {
        CanonicalPair::verify_until(root, canonical, Instant::now() + Duration::from_secs(30))
    }

    fn unchanged(root: &Path, canonical: &Payloads) {
        for (name, bytes) in canonical.files() {
            assert_eq!(&fs::read(root.join(name)).unwrap(), bytes, "{name}");
        }
        assert_eq!(
            fs::read(root.join("unrelated-marker")).unwrap(),
            b"preserve me"
        );
        for name in ["journal.bin", "request.json", "status.json", "backup"] {
            assert!(!root.join(name).exists(), "{name}");
        }
    }

    #[test]
    fn canonical_pair_retains_every_object_without_operation_effects() {
        let (root, canonical) = fixture();
        let pair = read(root.path(), &canonical).unwrap();
        assert_eq!(pair.inventory().receipt().version, "0.10.0");
        assert_eq!(pair.inventory().payloads().len(), 7);
        for name in PAYLOADS.into_iter().chain([RECEIPT]) {
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(root.path().join(name))
                    .is_err()
            );
        }
        unchanged(root.path(), &canonical);
        drop(pair);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(RECEIPT))
                .is_ok()
        );
    }

    #[test]
    fn self_consistent_forged_local_bytes_do_not_become_canonical_authority() {
        let (root, canonical) = fixture();
        let receipt_path = root.path().join(RECEIPT);
        let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        fs::write(root.path().join("README.md"), b"locally changed README").unwrap();
        receipt["files"]["README.md"] = json!(sha256_hex(b"locally changed README"));
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        // Establish that the local-only prerequisite accepts these matching bytes.
        let local = windows_paired_ownership::verify_until(
            root.path(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        drop(local);
        assert!(read(root.path(), &canonical).is_err());
        assert_eq!(
            fs::read(root.path().join("README.md")).unwrap(),
            b"locally changed README"
        );
    }

    #[test]
    fn locally_valid_archive_assertion_must_match_the_canonical_release() {
        let (root, canonical) = fixture();
        let receipt_path = root.path().join(RECEIPT);
        let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        receipt["archive_sha256"] = json!("cd".repeat(32));
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(read(root.path(), &canonical).is_err());
        unchanged(root.path(), &canonical);
    }

    #[test]
    fn both_exclusive_images_and_all_six_companions_remain_guarded() {
        let (root, canonical) = fixture();
        let pair = read(root.path(), &canonical).unwrap();
        let expected = EXECUTABLES.map(|name| pair.inventory().payloads()[name].identity);
        let held = pair.into_exclusive().unwrap();
        assert_eq!(held.directory(), root.path());
        assert_eq!(held.receipt().version, "0.10.0");
        assert_eq!(held.identities(), &expected);
        assert_eq!(held.companions.len(), 6);
        for (index, name) in EXECUTABLES.iter().enumerate() {
            assert_eq!(file_identity(&held.images[index]).unwrap(), expected[index]);
            assert!(fs::File::open(root.path().join(name)).is_err());
            assert!(
                fs::rename(
                    root.path().join(name),
                    root.path().join(format!("{name}.moved"))
                )
                .is_err()
            );
        }
        for name in held.companions.keys() {
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(root.path().join(name))
                    .is_err()
            );
        }
        drop(held);
        unchanged(root.path(), &canonical);
    }

    #[test]
    fn blocked_second_image_releases_first_gate_without_writing_any_leaf() {
        let (root, canonical) = fixture();
        let blocker = fs::File::open(root.path().join(EXECUTABLES[1])).unwrap();
        let pair = read(root.path(), &canonical).unwrap();
        assert!(pair.into_exclusive().is_err());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(EXECUTABLES[0]))
                .is_ok()
        );
        drop(blocker);
        unchanged(root.path(), &canonical);
    }

    #[test]
    fn identical_bytes_at_an_unrecorded_object_are_not_adopted() {
        for name in EXECUTABLES {
            let (root, canonical) = fixture();
            let pair = read(root.path(), &canonical).unwrap();
            let old_identity = pair.inventory().payloads()[name].identity;
            let path = root.path().join(name);
            let moved = root.path().join("retained-original-image");
            let result = pair.exclusive_after(|| {
                fs::rename(&path, &moved)?;
                write_new(&path, &canonical.files()[name]);
                Ok(())
            });
            assert!(result.is_err(), "{name}");
            let original = open_private_exclusive(&moved).unwrap();
            let replacement = open_private_exclusive(&path).unwrap();
            assert_eq!(file_identity(&original).unwrap(), old_identity);
            assert_ne!(file_identity(&replacement).unwrap(), old_identity);
            drop((original, replacement));
            unchanged(root.path(), &canonical);
            assert_eq!(fs::read(&moved).unwrap(), canonical.files()[name]);
        }
    }

    #[test]
    fn readonly_and_multiply_linked_images_keep_the_core_exclusive_refusal() {
        for linked in [false, true] {
            let (root, canonical) = fixture();
            let image = root.path().join(EXECUTABLES[1]);
            let original_permissions = fs::metadata(&image).unwrap().permissions();
            if linked {
                fs::hard_link(&image, root.path().join("external-image-alias")).unwrap();
            } else {
                let mut permissions = original_permissions.clone();
                permissions.set_readonly(true);
                fs::set_permissions(&image, permissions).unwrap();
            }
            let pair = read(root.path(), &canonical).unwrap();
            assert!(pair.into_exclusive().is_err());
            unchanged(root.path(), &canonical);
            if !linked {
                fs::set_permissions(&image, original_permissions).unwrap();
            }
        }
    }

    #[test]
    fn expired_admission_and_transition_create_no_new_budget_or_objects() {
        let (root, canonical) = fixture();
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        let missing = root.path().join("missing");
        assert!(CanonicalPair::verify_until(&missing, &canonical, expired).is_err());
        assert!(!missing.exists());
        let mut pair = read(root.path(), &canonical).unwrap();
        // Test-only clock injection, not an API accepting a replacement deadline.
        pair.deadline = expired;
        assert!(pair.into_exclusive().is_err());
        unchanged(root.path(), &canonical);
    }
}
