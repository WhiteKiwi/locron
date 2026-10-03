//! Composes live WinGet index guards with canonical five-file package bytes.
//! Expected aliases and static PE properties are not runtime/lifecycle authority.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::{DirectoryGuard, GuardedFile, file_identity, read_owned_executable};

use super::super::sha256_hex;
use super::super::windows_ownership::{VerifiedFile, native_target};
use super::super::windows_package::{LIMIT, PAIRED_FILES, verify_pe_subsystem};
use super::super::windows_package_ownership::{RegisteredIndex, read_registered_index_until};
use super::super::windows_paired_package;
use super::super::windows_paired_receipt::EXECUTABLES;
use super::super::windows_protocol::maintenance_path;
use super::super::windows_receipt::same_path;
use super::super::windows_release_source::Release;

fn clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original indexed-pair deadline expired"
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
    path.to_str().context("indexed-pair path is not Unicode")
}

fn bytes_until(file: &mut GuardedFile, size: u64, deadline: Instant) -> Result<Vec<u8>> {
    ensure!(size <= LIMIT as u64, "indexed payload exceeds its bound");
    let identity = observe(deadline, || Ok(file_identity(file)?))?;
    observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    let mut bytes = Vec::new();
    observe(deadline, || {
        Read::by_ref(&mut **file)
            .take(size + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == size
                && file.metadata()?.len() == size
                && file_identity(file)? == identity,
            "indexed payload changed during its guarded read"
        );
        file.seek(SeekFrom::Start(0))?;
        Ok(())
    })?;
    Ok(bytes)
}

struct PackageFiles {
    root: DirectoryGuard,
    files: BTreeMap<String, VerifiedFile>,
    sizes: BTreeMap<String, u64>,
    version: String,
    target: &'static str,
    archive_sha256: String,
    deadline: Instant,
}

impl PackageFiles {
    fn verify_until(
        directory: &Path,
        release: &Release,
        checksums: &[u8],
        archive: &[u8],
        deadline: Instant,
    ) -> Result<Self> {
        clock(deadline)?;
        let target = native_target()?;
        let (verified, archive_sha256) = observe(deadline, || {
            ensure!(
                checksums.len() <= 128 * 1024
                    && release.digests.get("SHA256SUMS.txt") == Some(&sha256_hex(checksums)),
                "indexed-pair checksums differ from the final release digest"
            );
            let sums = release.checksums(checksums)?;
            let name = format!("locron-v{}-{target}.zip", release.version);
            let digest = sums
                .get(&name)
                .context("canonical paired archive is missing")?;
            ensure!(
                archive.len() <= LIMIT,
                "paired archive exceeds its byte bound"
            );
            let verified = windows_paired_package::verify_archive(
                archive,
                &release.version,
                target,
                digest,
            )?;
            Ok((verified, digest.clone()))
        })?;
        locron_core::windows::current_user_sid_until(deadline)?;
        let root = observe(deadline, || Ok(DirectoryGuard::ancestors(directory)?))?;
        observe(deadline, || {
            maintenance_path(text(root.normalized_path())?)?;
            let parent = root
                .normalized_path()
                .parent()
                .context("package root has no parent")?;
            let expected = parent.join(format!("locron-v{}-{target}", release.version));
            ensure!(
                same_path(text(root.normalized_path())?, text(&expected)?)?,
                "package root differs from the canonical version and native target"
            );
            Ok(())
        })?;
        let mut files = BTreeMap::new();
        let mut sizes = BTreeMap::new();
        let mut identities = Vec::new();
        for name in PAIRED_FILES {
            let expected = &verified.files[name];
            let path = root.normalized_path().join(name);
            observe(deadline, || {
                maintenance_path(text(&path)?)?;
                Ok(())
            })?;
            let mut file = observe(deadline, || Ok(read_owned_executable(&path)?))?;
            let identity = observe(deadline, || {
                maintenance_path(text(file.normalized_path())?)?;
                ensure!(
                    same_path(text(file.normalized_path())?, text(&path)?)?
                        && file.metadata()?.len() == expected.len() as u64,
                    "guarded package payload differs from its canonical path or length"
                );
                Ok(file_identity(&file)?)
            })?;
            let bytes = bytes_until(&mut file, expected.len() as u64, deadline)?;
            let sha256 = observe(deadline, || {
                let hash = sha256_hex(&bytes);
                ensure!(
                    hash == sha256_hex(expected) && file_identity(&file)? == identity,
                    "guarded package payload differs from its canonical bytes or identity"
                );
                ensure!(
                    !identities.contains(&identity),
                    "indexed payload names alias the same native file identity"
                );
                if let Some(index) = EXECUTABLES.iter().position(|item| *item == name) {
                    verify_pe_subsystem(&bytes, target, if index == 0 { 3 } else { 2 })?;
                }
                Ok(hash)
            })?;
            identities.push(identity);
            sizes.insert(name.to_owned(), bytes.len() as u64);
            files.insert(
                name.to_owned(),
                VerifiedFile {
                    file,
                    identity,
                    sha256,
                },
            );
        }
        clock(deadline)?;
        Ok(Self {
            root,
            files,
            sizes,
            version: release.version.clone(),
            target,
            archive_sha256,
            deadline,
        })
    }

    fn revalidate_until(&mut self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.deadline);
        clock(deadline)?;
        for (name, proof) in &mut self.files {
            observe(deadline, || {
                maintenance_path(text(proof.file.normalized_path())?)?;
                ensure!(
                    file_identity(&proof.file)? == proof.identity
                        && same_path(
                            text(proof.file.normalized_path())?,
                            text(&self.root.normalized_path().join(name))?,
                        )?,
                    "indexed payload no longer has its original identity and path"
                );
                Ok(())
            })?;
            let bytes = bytes_until(&mut proof.file, self.sizes[name], deadline)?;
            observe(deadline, || {
                ensure!(
                    sha256_hex(&bytes) == proof.sha256,
                    "retained indexed payload digest changed"
                );
                Ok(())
            })?;
        }
        clock(deadline)
    }
}

/// The registry/index and five payload objects stay together, with no serialized
/// constructor or mutation API. Native alias and version/ABI proof remains separate.
pub(in crate::self_update) struct IndexedPair {
    index: RegisteredIndex,
    package: PackageFiles,
}

impl IndexedPair {
    pub(in crate::self_update) fn verify_until(
        directory: &Path,
        expected_alias: &Path,
        release: &Release,
        checksums: &[u8],
        archive: &[u8],
        deadline: Instant,
    ) -> Result<Self> {
        let mut package =
            PackageFiles::verify_until(directory, release, checksums, archive, deadline)?;
        let console = package.root.normalized_path().join(EXECUTABLES[0]);
        let index = read_registered_index_until(&console, expected_alias, deadline)?;
        package.revalidate_until(deadline)?;
        index.revalidate_until(deadline)?;
        clock(deadline)?;
        Ok(Self { index, package })
    }

    pub(in crate::self_update) fn revalidate_until(&mut self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.package.deadline);
        self.package.revalidate_until(deadline)?;
        // Native registry is the final observation, never a retained registry lock.
        self.index.revalidate_until(deadline)
    }

    pub(in crate::self_update) fn directory(&self) -> &Path {
        self.package.root.normalized_path()
    }

    pub(in crate::self_update) fn version(&self) -> &str {
        &self.package.version
    }

    pub(in crate::self_update) fn target(&self) -> &str {
        self.package.target
    }

    pub(in crate::self_update) fn archive_sha256(&self) -> &str {
        &self.package.archive_sha256
    }

    pub(in crate::self_update) fn files(&self) -> &BTreeMap<String, VerifiedFile> {
        &self.package.files
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write};
    use std::time::Duration;

    use locron_core::filesystem::create_private_new_exclusive;
    use serde_json::json;
    use zip::write::SimpleFileOptions;

    use super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::windows_release_source::payload_inventory;
    use super::*;

    struct Fixture {
        container: PrivateFixture,
        root: DirectoryGuard,
        release: Release,
        sums: Vec<u8>,
        archive: Vec<u8>,
        contents: BTreeMap<String, Vec<u8>>,
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

    impl Fixture {
        fn new() -> Self {
            let container = PrivateFixture::new("locron-indexed-pair-");
            let target = native_target().unwrap();
            let root = DirectoryGuard::private(
                &container.path().join(format!("locron-v0.10.0-{target}")),
            )
            .unwrap();
            let contents: BTreeMap<_, _> = PAIRED_FILES
                .into_iter()
                .map(|name| {
                    let bytes = match name {
                        "locron.exe" => pe(3),
                        "locron-service-launcher.exe" => pe(2),
                        _ => b"same canonical documentation bytes".to_vec(),
                    };
                    (name.to_owned(), bytes)
                })
                .collect();
            let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for (name, bytes) in &contents {
                let mut file =
                    create_private_new_exclusive(&root.normalized_path().join(name)).unwrap();
                file.write_all(bytes).unwrap();
                file.sync_all().unwrap();
                archive
                    .start_file(
                        format!("locron-v0.10.0-{target}/{name}"),
                        SimpleFileOptions::default(),
                    )
                    .unwrap();
                archive.write_all(bytes).unwrap();
            }
            let archive = archive.finish().unwrap().into_inner();
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
            for name in ["install.sh", "install.ps1", "uninstall.ps1"] {
                digests.insert(name.to_owned(), "ab".repeat(32));
            }
            let metadata = json!({"tag_name": "v0.10.0", "draft": false, "prerelease": false,
                "assets": digests.into_iter().map(|(name, hash)| json!({
                    "browser_download_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/{name}"),
                    "name": name, "digest": format!("sha256:{hash}")
                })).collect::<Vec<_>>()});
            let release =
                Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some("0.10.0")).unwrap();
            fs::write(root.normalized_path().join("unlisted-marker"), b"keep unlisted").unwrap();
            Self {
                container,
                root,
                release,
                sums,
                archive,
                contents,
            }
        }

        fn read(&self) -> Result<PackageFiles> {
            PackageFiles::verify_until(
                self.root.normalized_path(),
                &self.release,
                &self.sums,
                &self.archive,
                Instant::now() + Duration::from_secs(30),
            )
        }

        fn untouched(&self) {
            assert_eq!(
                fs::read(self.root.normalized_path().join("unlisted-marker")).unwrap(),
                b"keep unlisted"
            );
            for name in ["journal.bin", "request.json", "status.json", "backup"] {
                assert!(!self.root.normalized_path().join(name).exists());
            }
        }
    }

    #[test]
    fn actual_five_files_match_canonical_bytes_and_retain_distinct_native_read_guards() {
        let fixture = Fixture::new();
        let mut proof = fixture.read().unwrap();
        assert_eq!(proof.files.len(), 5);
        assert_eq!(proof.version, "0.10.0");
        assert_eq!(proof.target, native_target().unwrap());
        assert_eq!(proof.archive_sha256, sha256_hex(&fixture.archive));
        for (name, file) in &proof.files {
            assert_eq!(file.sha256, sha256_hex(&fixture.contents[name]));
            assert_eq!(file_identity(&file.file).unwrap(), file.identity);
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(fixture.root.normalized_path().join(name))
                    .is_err()
            );
        }
        proof
            .revalidate_until(Instant::now() + Duration::from_secs(60))
            .unwrap();
        fixture.untouched();
    }

    #[test]
    fn every_changed_or_missing_payload_refuses_and_preserves_other_bytes() {
        let fixture = Fixture::new();
        for name in PAIRED_FILES {
            let path = fixture.root.normalized_path().join(name);
            let mut modified = fixture.contents[name].clone();
            modified[0] ^= 1;
            fs::write(&path, &modified).unwrap();
            assert!(fixture.read().is_err(), "{name}");
            assert_eq!(fs::read(&path).unwrap(), modified);
            fs::remove_file(&path).unwrap();
            assert!(fixture.read().is_err(), "{name}");
            assert!(!path.exists());
            let mut file = create_private_new_exclusive(&path).unwrap();
            file.write_all(&fixture.contents[name]).unwrap();
            file.sync_all().unwrap();
        }
        fixture.untouched();
    }

    #[test]
    fn canonical_digest_refusal_precedes_filesystem_or_zip_admission() {
        let mut fixture = Fixture::new();
        fixture
            .release
            .digests
            .insert("SHA256SUMS.txt".into(), "cd".repeat(32));
        let error = PackageFiles::verify_until(
            &fixture.container.path().join("missing"),
            &fixture.release,
            &fixture.sums,
            b"not a ZIP",
            Instant::now() + Duration::from_secs(30),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("checksums differ"));
        assert!(!fixture.container.path().join("missing").exists());
        fixture.untouched();
    }

    #[test]
    fn wrong_version_root_and_duplicate_payload_identities_refuse_without_repair() {
        let fixture = Fixture::new();
        let error = PackageFiles::verify_until(
            fixture.container.path(),
            &fixture.release,
            &fixture.sums,
            &fixture.archive,
            Instant::now() + Duration::from_secs(30),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("package root differs"));
        let source = fixture.root.normalized_path().join("README.md");
        let alias = fixture.root.normalized_path().join("LICENSE-MIT");
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&source, &alias).unwrap();
        let error = fixture.read().err().unwrap();
        assert!(error.to_string().contains("same native file identity"));
        assert_eq!(fs::read(source).unwrap(), fs::read(alias).unwrap());
        fixture.untouched();
    }

    #[test]
    fn changed_retained_facts_and_expired_revalidation_do_not_gain_a_new_budget() {
        let fixture = Fixture::new();
        let mut proof = fixture.read().unwrap();
        proof.files.get_mut("README.md").unwrap().sha256 = "cd".repeat(32);
        assert!(
            proof
                .revalidate_until(Instant::now() + Duration::from_secs(60))
                .is_err()
        );
        proof.deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(
            proof
                .revalidate_until(Instant::now() + Duration::from_secs(60))
                .is_err()
        );
        fixture.untouched();
    }

    #[test]
    fn actual_unregistered_package_is_not_promoted_from_canonical_file_hashes() {
        let fixture = Fixture::new();
        let files = fixture.read().unwrap();
        assert_eq!(files.files.len(), 5);
        drop(files);
        let alias = fixture.container.path().join("locron.exe");
        let error = IndexedPair::verify_until(
            fixture.root.normalized_path(),
            &alias,
            &fixture.release,
            &fixture.sums,
            &fixture.archive,
            Instant::now() + Duration::from_secs(30),
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("no unique current-user portable binding")
        );
        assert!(!alias.exists());
        fixture.untouched();
    }
}
