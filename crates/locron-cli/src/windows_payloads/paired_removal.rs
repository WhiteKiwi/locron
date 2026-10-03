//! Offline v2 removal observations; no deletion or lifecycle authority is granted.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::DirectoryGuard;

use super::super::windows_ownership::{
    RetainedPayload, Retention, VerifiedFile, immutable_private_until, native_target,
};
use super::super::windows_package::verify_pe_subsystem;
use super::super::windows_paired_receipt::{EXECUTABLES, PAYLOADS, RECEIPT, Receipt};
use super::super::windows_protocol::maintenance_path;
use super::super::windows_receipt::same_path;

const PAYLOAD_LIMIT: usize = 64 * 1024 * 1024;
const RECEIPT_LIMIT: usize = 128 * 1024;
const BOOTSTRAP: &str = ".locron-installer.ps1";

fn clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original paired removal deadline expired"
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
    path.to_str().context("paired removal path is not Unicode")
}

fn read_leaf(
    root: &Path,
    name: &str,
    limit: usize,
    deadline: Instant,
) -> Result<(VerifiedFile, Vec<u8>)> {
    let path = root.join(name);
    observe(deadline, || {
        maintenance_path(text(&path)?)?;
        Ok(())
    })?;
    let (proof, bytes) = immutable_private_until(&path, limit, deadline)?;
    observe(deadline, || {
        maintenance_path(text(proof.file.normalized_path())?)?;
        ensure!(
            same_path(text(proof.file.normalized_path())?, text(&path)?)?,
            "paired removal leaf differs from its fixed path"
        );
        Ok(())
    })?;
    Ok((proof, bytes))
}

fn mandatory(name: &str) -> bool {
    EXECUTABLES.contains(&name) || name == BOOTSTRAP
}

/// Receipt and every candidate stay bound to actual immutable private handles.
/// An omitted candidate has no delete/repair authority; reasons are observations,
/// not a promise that a missing or inaccessible path can safely be removed later.
pub(in crate::self_update) struct RemovalPair {
    directory: DirectoryGuard,
    receipt: Receipt,
    raw_receipt: Vec<u8>,
    receipt_file: VerifiedFile,
    files: BTreeMap<String, VerifiedFile>,
    retained: Vec<RetainedPayload>,
}

impl RemovalPair {
    pub(in crate::self_update) fn verify_until(directory: &Path, deadline: Instant) -> Result<Self> {
        clock(deadline)?;
        let sid = locron_core::windows::current_user_sid_until(deadline)?;
        let target = observe(deadline, native_target)?;
        let directory = observe(deadline, || {
            Ok(DirectoryGuard::existing_private(directory)?)
        })?;
        let root = directory.normalized_path();
        // Qualify all fixed future leaf paths before any optional-error handling.
        observe(deadline, || {
            maintenance_path(text(root)?)?;
            for name in PAYLOADS.into_iter().chain([RECEIPT]) {
                maintenance_path(text(&root.join(name))?)?;
            }
            Ok(())
        })?;
        let (receipt_file, raw_receipt) = read_leaf(root, RECEIPT, RECEIPT_LIMIT, deadline)?;
        let receipt = observe(deadline, || {
            Receipt::parse(&raw_receipt, &sid, text(root)?, target)
        })?;
        let mut files = BTreeMap::new();
        let mut identities = vec![receipt_file.identity];
        let mut retained = Vec::new();
        // Qualify essential source/helper provenance before optional companions.
        let names = EXECUTABLES
            .into_iter()
            .chain([BOOTSTRAP])
            .chain(PAYLOADS.into_iter().filter(|name| !mandatory(name)));
        for name in names {
            let result = read_leaf(root, name, PAYLOAD_LIMIT, deadline);
            // Expiration must never become an Unverifiable removal observation.
            clock(deadline)?;
            let (proof, bytes) = if mandatory(name) {
                result.with_context(|| format!("paired removal requires exact {name}"))?
            } else {
                match result {
                    Ok((proof, bytes)) if receipt.files.get(name) == Some(&proof.sha256) => {
                        (proof, bytes)
                    }
                    Ok(_) => {
                        retained.push(RetainedPayload {
                            name: name.to_owned(),
                            reason: Retention::Changed,
                        });
                        continue;
                    }
                    Err(_) => {
                        retained.push(RetainedPayload {
                            name: name.to_owned(),
                            reason: Retention::Unverifiable,
                        });
                        continue;
                    }
                }
            };
            observe(deadline, || {
                ensure!(
                    receipt.files.get(name) == Some(&proof.sha256),
                    "mandatory paired removal payload differs from its receipt"
                );
                ensure!(
                    !identities.contains(&proof.identity),
                    "paired removal names alias the same native file identity"
                );
                if let Some(index) = EXECUTABLES.iter().position(|candidate| *candidate == name) {
                    ensure!(
                        receipt.executables[index].sha256 == proof.sha256
                            && same_path(
                                &receipt.executables[index].path,
                                text(proof.file.normalized_path())?,
                            )?,
                        "paired removal image differs from its ordered binding"
                    );
                    verify_pe_subsystem(&bytes, target, if index == 0 { 3 } else { 2 })?;
                }
                Ok(())
            })?;
            drop(bytes);
            identities.push(proof.identity);
            files.insert(name.to_owned(), proof);
        }
        clock(deadline)?;
        Ok(Self {
            directory,
            receipt,
            raw_receipt,
            receipt_file,
            files,
            retained,
        })
    }

    pub(in crate::self_update) fn directory(&self) -> &Path {
        self.directory.normalized_path()
    }

    pub(in crate::self_update) fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    pub(in crate::self_update) fn raw_receipt(&self) -> &[u8] {
        &self.raw_receipt
    }

    pub(in crate::self_update) fn receipt_file(&self) -> &VerifiedFile {
        &self.receipt_file
    }

    pub(in crate::self_update) fn files(&self) -> &BTreeMap<String, VerifiedFile> {
        &self.files
    }

    pub(in crate::self_update) fn retained(&self) -> &[RetainedPayload] {
        &self.retained
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::time::Duration;

    use locron_core::filesystem::{create_private_new_exclusive, file_identity};
    use serde_json::{Value, json};

    use super::super::super::sha256_hex;
    use super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::windows_paired_receipt::LAUNCHER_ABI;
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

    fn fixture() -> (PrivateFixture, Value) {
        let root = PrivateFixture::new("locron-paired-removal-");
        let mut files = BTreeMap::new();
        for name in PAYLOADS {
            let bytes = match name {
                "locron.exe" => pe(3),
                "locron-service-launcher.exe" => pe(2),
                _ => format!("owned removal fixture {name}").into_bytes(),
            };
            write_new(&root.path().join(name), &bytes);
            files.insert(name, sha256_hex(&bytes));
        }
        let directory = root.path().to_str().unwrap();
        let target = native_target().unwrap();
        let receipt = json!({
            "schema": "locron.install/windows-v2", "channel": "standalone",
            "sid": locron_core::windows::current_user_sid().unwrap(),
            "directory": directory, "target": target, "version": "0.10.0",
            "launcher_abi": LAUNCHER_ABI,
            "executables": EXECUTABLES.map(|name| json!({
                "path": format!("{directory}\\{name}"), "sha256": files[name]
            })),
            "archive_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{target}.zip"),
            "archive_sha256": "ab".repeat(32), "files": files, "user_path": null
        });
        write_new(
            &root.path().join(RECEIPT),
            &serde_json::to_vec(&receipt).unwrap(),
        );
        fs::write(root.path().join("unlisted-marker"), b"keep unlisted").unwrap();
        (root, receipt)
    }

    fn read(root: &Path) -> Result<RemovalPair> {
        RemovalPair::verify_until(root, Instant::now() + Duration::from_secs(30))
    }

    fn untouched(root: &Path) {
        assert_eq!(
            fs::read(root.join("unlisted-marker")).unwrap(),
            b"keep unlisted"
        );
        for name in ["journal.bin", "request.json", "status.json", "backup"] {
            assert!(!root.join(name).exists(), "{name}");
        }
    }

    #[test]
    fn exact_seven_payloads_and_raw_receipt_keep_immutable_native_guards() {
        let (root, _) = fixture();
        let raw = fs::read(root.path().join(RECEIPT)).unwrap();
        let proof = read(root.path()).unwrap();
        assert_eq!(proof.directory(), root.path());
        assert_eq!(proof.raw_receipt(), raw);
        assert_eq!(proof.receipt().version, "0.10.0");
        assert_eq!(proof.files().len(), 7);
        assert!(proof.retained().is_empty());
        assert_eq!(
            file_identity(&proof.receipt_file().file).unwrap(),
            proof.receipt_file().identity
        );
        for name in PAYLOADS.into_iter().chain([RECEIPT]) {
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(root.path().join(name))
                    .is_err()
            );
        }
        untouched(root.path());
        drop(proof);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(RECEIPT))
                .is_ok()
        );
    }

    #[test]
    fn changed_missing_and_locked_optional_files_are_preserved_with_distinct_reasons() {
        let (root, _) = fixture();
        fs::write(root.path().join("README.md"), b"local edits").unwrap();
        fs::remove_file(root.path().join("LICENSE-MIT")).unwrap();
        let locked = OpenOptions::new()
            .write(true)
            .open(root.path().join("LICENSE-APACHE"))
            .unwrap();
        let proof = read(root.path()).unwrap();
        assert_eq!(proof.files().len(), 4);
        for (name, reason) in [
            ("README.md", Retention::Changed),
            ("LICENSE-MIT", Retention::Unverifiable),
            ("LICENSE-APACHE", Retention::Unverifiable),
        ] {
            assert!(!proof.files().contains_key(name));
            assert!(proof.retained().contains(&RetainedPayload {
                name: name.to_owned(),
                reason,
            }));
        }
        assert_eq!(fs::read(root.path().join("README.md")).unwrap(), b"local edits");
        assert!(!root.path().join("LICENSE-MIT").exists());
        untouched(root.path());
        drop((proof, locked));
    }

    #[test]
    fn every_mandatory_image_or_bootstrap_tamper_and_absence_refuses() {
        let (root, _) = fixture();
        for name in EXECUTABLES.into_iter().chain([BOOTSTRAP]) {
            let path = root.path().join(name);
            let bytes = fs::read(&path).unwrap();
            fs::write(&path, b"modified essential payload").unwrap();
            assert!(read(root.path()).is_err(), "{name}");
            assert_eq!(fs::read(&path).unwrap(), b"modified essential payload");
            fs::remove_file(&path).unwrap();
            assert!(read(root.path()).is_err(), "{name}");
            assert!(!path.exists());
            write_new(&path, &bytes);
        }
        untouched(root.path());
    }

    #[test]
    fn receipt_matching_wrong_pe_subsystem_is_not_an_owned_service_launcher() {
        let (root, mut receipt) = fixture();
        let bytes = pe(3);
        let path = root.path().join(EXECUTABLES[1]);
        fs::write(&path, &bytes).unwrap();
        receipt["files"][EXECUTABLES[1]] = json!(sha256_hex(&bytes));
        receipt["executables"][1]["sha256"] = json!(sha256_hex(&bytes));
        fs::write(
            root.path().join(RECEIPT),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(read(root.path()).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
        untouched(root.path());
    }

    #[test]
    fn repeated_full_identity_does_not_authorize_two_independent_removal_leaves() {
        let (root, mut receipt) = fixture();
        let first = root.path().join("README.md");
        let second = root.path().join("LICENSE-MIT");
        fs::remove_file(&second).unwrap();
        fs::hard_link(&first, &second).unwrap();
        receipt["files"]["LICENSE-MIT"] = receipt["files"]["README.md"].clone();
        fs::write(
            root.path().join(RECEIPT),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(read(root.path()).is_err());
        assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
        untouched(root.path());
    }

    #[test]
    fn malformed_receipt_never_adopts_optional_or_unlisted_paths() {
        let (root, original) = fixture();
        for field in ["schema", "sid", "channel", "launcher_abi"] {
            let mut changed = original.clone();
            changed[field] = json!("foreign");
            let bytes = serde_json::to_vec(&changed).unwrap();
            fs::write(root.path().join(RECEIPT), &bytes).unwrap();
            assert!(read(root.path()).is_err(), "{field}");
            assert_eq!(fs::read(root.path().join(RECEIPT)).unwrap(), bytes);
        }
        untouched(root.path());
    }

    #[test]
    fn expired_and_missing_admission_never_creates_or_relabels_authority() {
        let (root, _) = fixture();
        let missing = root.path().join("missing-parent").join("missing-install");
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(RemovalPair::verify_until(&missing, expired).is_err());
        assert!(read(&missing).is_err());
        assert!(!root.path().join("missing-parent").exists());
        assert!(RemovalPair::verify_until(root.path(), expired).is_err());
        untouched(root.path());
    }
}
