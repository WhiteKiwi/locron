//! Canonical five-file package sources bound to an actual retained WinGet index.
//! Alias rows and structural PE checks do not grant lifecycle or execution authority.

#[path = "indexed_sources/gates.rs"]
mod gates;

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::{
    DirectoryGuard, FileIdentity, GuardedFile, file_identity, read_owned_executable,
};
use sha2::{Digest, Sha256};

use super::super::sha256_hex;
use super::super::windows_ownership::native_target;
use super::super::windows_package::{LIMIT, PAIRED_FILES};
use super::super::windows_package_ownership::{RegisteredIndex, read_registered_index_until};
use super::super::windows_paired_package::{Archive, verify_archive};
use super::super::windows_protocol::maintenance_path;
use super::super::windows_receipt::same_path;
use super::super::windows_release_source::Release;

fn clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original indexed-source deadline expired"
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
    path.to_str().context("indexed package path is not Unicode")
}

fn canonical(
    release: &Release,
    target: &str,
    checksums: &[u8],
    archive: &[u8],
) -> Result<(Archive, String)> {
    ensure!(
        checksums.len() <= 128 * 1024
            && release.digests.get("SHA256SUMS.txt") == Some(&sha256_hex(checksums)),
        "package checksums differ from the final canonical API digest"
    );
    let sums = release.checksums(checksums)?;
    let name = format!("locron-v{}-{target}.zip", release.version);
    let hash = sums
        .get(&name)
        .context("canonical paired package archive is missing")?;
    ensure!(
        archive.len() <= LIMIT,
        "canonical paired archive exceeds its byte bound"
    );
    Ok((
        verify_archive(archive, &release.version, target, hash)?,
        hash.clone(),
    ))
}

struct Leaf {
    file: GuardedFile,
    identity: FileIdentity,
    sha256: String,
    length: u64,
}

fn read_leaf(path: &Path, hash: &str, length: u64, deadline: Instant) -> Result<Leaf> {
    clock(deadline)?;
    maintenance_path(text(path)?)?;
    ensure!(
        length <= LIMIT as u64,
        "canonical source length exceeds its bound"
    );
    let mut file = observe(deadline, || Ok(read_owned_executable(path)?))?;
    ensure!(
        same_path(text(file.normalized_path())?, text(path)?)?,
        "package source differs from its exact fixed path"
    );
    maintenance_path(text(file.normalized_path())?)?;
    let identity = observe(deadline, || Ok(file_identity(&file)?))?;
    ensure!(
        observe(deadline, || Ok(file.metadata()?.len()))? == length,
        "package source length differs from the canonical payload"
    );
    observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    let mut digest = Sha256::new();
    let mut consumed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let capacity = usize::try_from((length + 1 - consumed).min(buffer.len() as u64))?;
        let count = observe(deadline, || Ok(file.read(&mut buffer[..capacity])?))?;
        if count == 0 {
            break;
        }
        consumed += count as u64;
        ensure!(
            consumed <= length,
            "package source grew beyond its canonical length"
        );
        digest.update(&buffer[..count]);
    }
    let sha256 = format!("{:x}", digest.finalize());
    ensure!(
        consumed == length
            && sha256 == hash
            && observe(deadline, || Ok(file_identity(&file)?))? == identity
            && observe(deadline, || Ok(file.metadata()?.len()))? == length,
        "package source differs from its canonical bytes or retained identity"
    );
    observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    Ok(Leaf {
        file,
        identity,
        sha256,
        length,
    })
}

struct Sources {
    root: DirectoryGuard,
    files: BTreeMap<String, Leaf>,
}

impl Sources {
    fn read(directory: &Path, archive: &Archive, deadline: Instant) -> Result<Self> {
        clock(deadline)?;
        let root = observe(deadline, || Ok(DirectoryGuard::ancestors(directory)?))?;
        maintenance_path(text(root.normalized_path())?)?;
        ensure!(
            archive.files.len() == PAIRED_FILES.len()
                && PAIRED_FILES
                    .iter()
                    .all(|name| archive.files.contains_key(*name)),
            "canonical source inventory is not the exact five-file pair"
        );
        let mut files = BTreeMap::<String, Leaf>::new();
        for name in PAIRED_FILES {
            let bytes = &archive.files[name];
            let leaf = read_leaf(
                &root.normalized_path().join(name),
                &sha256_hex(bytes),
                bytes.len() as u64,
                deadline,
            )?;
            ensure!(
                files.values().all(|prior| prior.identity != leaf.identity),
                "indexed package payload names alias one native file"
            );
            files.insert(name.to_owned(), leaf);
        }
        clock(deadline)?;
        Ok(Self { root, files })
    }

    fn revalidate(&self, deadline: Instant) -> Result<()> {
        clock(deadline)?;
        for (name, original) in &self.files {
            let current = read_leaf(
                &self.root.normalized_path().join(name),
                &original.sha256,
                original.length,
                deadline,
            )?;
            ensure!(
                current.identity == original.identity
                    && observe(deadline, || Ok(file_identity(&original.file)?))?
                        == original.identity,
                "indexed package source no longer names the original native object"
            );
        }
        clock(deadline)
    }
}

/// Actual index and all five canonical sources remain private, move-only and read-guarded.
/// This intentionally carries no actual-symlink, native-probe or task-effect authority.
pub(super) struct IndexedSources {
    index: RegisteredIndex,
    sources: Sources,
    version: String,
    target: &'static str,
    archive_sha256: String,
    original_deadline: Instant,
}

impl IndexedSources {
    pub(super) fn directory(&self) -> &Path {
        self.sources.root.normalized_path()
    }

    pub(super) fn version(&self) -> &str {
        &self.version
    }

    pub(super) fn target(&self) -> &str {
        self.target
    }

    pub(super) fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }

    pub(super) fn file_facts(&self) -> impl Iterator<Item = (&str, FileIdentity, &str, u64)> {
        self.sources.files.iter().map(|(name, leaf)| {
            (
                name.as_str(),
                leaf.identity,
                leaf.sha256.as_str(),
                leaf.length,
            )
        })
    }

    pub(super) fn revalidate_until(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.original_deadline);
        self.sources.revalidate(deadline)?;
        self.index.revalidate_until(deadline)?;
        clock(deadline)
    }
}

/// Network selection/downloads must finish before entering this caller-owned native phase.
pub(super) fn verify_until(
    console: &Path,
    expected_alias: &Path,
    release: &Release,
    checksums: &[u8],
    archive: &[u8],
    deadline: Instant,
) -> Result<IndexedSources> {
    clock(deadline)?;
    let target = native_target()?;
    // No registry or installed-file access precedes canonical byte qualification.
    let (archive, archive_sha256) =
        observe(deadline, || canonical(release, target, checksums, archive))?;
    let console = maintenance_path(text(console)?)?;
    let console = Path::new(&console);
    let directory = console.parent().context("package console has no parent")?;
    let expected_root = format!("locron-v{}-{target}", release.version);
    ensure!(
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(&expected_root)),
        "package directory version differs from its canonical release"
    );
    let index = read_registered_index_until(console, expected_alias, deadline)?;
    let sources = Sources::read(directory, &archive, deadline)?;
    let proof = IndexedSources {
        index,
        sources,
        version: release.version.clone(),
        target,
        archive_sha256,
        original_deadline: deadline,
    };
    proof.revalidate_until(deadline)?;
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write};
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    use locron_core::filesystem::create_private_new_exclusive;
    use zip::write::SimpleFileOptions;

    use super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::windows_release_source::payload_inventory;
    use super::*;

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

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
        for (offset, value) in [
            (132, machine),
            (134, 1),
            (148, 240),
            (150, 0x22),
            (152, 0x20b),
            (220, subsystem),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
        bytes
    }

    fn archive() -> (Release, Vec<u8>, Vec<u8>, Archive) {
        let target = native_target().unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in PAIRED_FILES {
            writer
                .start_file(
                    format!("locron-v0.10.0-{target}/{name}"),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            let bytes = match name {
                "locron.exe" => pe(3),
                "locron-service-launcher.exe" => pe(2),
                _ => b"canonical document".to_vec(),
            };
            writer.write_all(&bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let mut digests: BTreeMap<_, _> = payload_inventory("0.10.0")
            .unwrap()
            .into_iter()
            .map(|name| (name, "ab".repeat(32)))
            .collect();
        digests.insert(format!("locron-v0.10.0-{target}.zip"), sha256_hex(&bytes));
        let sums = digests
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>()
            .into_bytes();
        digests.insert("SHA256SUMS.txt".into(), sha256_hex(&sums));
        let release = Release {
            version: "0.10.0".into(),
            digests,
        };
        let (verified, _) = canonical(&release, target, &sums, &bytes).unwrap();
        (release, sums, bytes, verified)
    }

    fn fixture() -> (PrivateFixture, Archive) {
        let root = PrivateFixture::new("locron-indexed-sources-");
        let (_, _, _, archive) = archive();
        for (name, bytes) in &archive.files {
            write_new(&root.path().join(name), bytes);
        }
        (root, archive)
    }

    #[test]
    fn five_canonical_sources_remain_guarded_and_revalidation_keeps_their_ids() {
        let (root, archive) = fixture();
        let marker = root.path().join("unrelated");
        fs::write(&marker, b"keep").unwrap();
        let proof = Sources::read(root.path(), &archive, deadline()).unwrap();
        proof.revalidate(deadline()).unwrap();
        assert_eq!(proof.files.len(), 5);
        for (name, leaf) in &proof.files {
            assert_eq!(leaf.sha256, sha256_hex(&archive.files[name]));
            assert_eq!(file_identity(&leaf.file).unwrap(), leaf.identity);
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(root.path().join(name))
                    .is_err()
            );
            assert!(fs::rename(root.path().join(name), root.path().join("moved")).is_err());
        }
        assert_eq!(fs::read(marker).unwrap(), b"keep");
        drop(proof);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join("README.md"))
                .is_ok()
        );
    }

    #[test]
    fn every_missing_or_changed_payload_refuses_without_repair() {
        let (root, archive) = fixture();
        for name in PAIRED_FILES {
            let path = root.path().join(name);
            fs::write(&path, b"changed").unwrap();
            assert!(
                Sources::read(root.path(), &archive, deadline()).is_err(),
                "{name}"
            );
            assert_eq!(fs::read(&path).unwrap(), b"changed");
            fs::remove_file(&path).unwrap();
            assert!(
                Sources::read(root.path(), &archive, deadline()).is_err(),
                "{name}"
            );
            assert!(!path.exists());
            write_new(&path, &archive.files[name]);
        }
        assert!(!root.path().join("journal.bin").exists());
    }

    #[test]
    fn equal_length_modified_companion_is_not_canonical() {
        let (root, archive) = fixture();
        let path = root.path().join("README.md");
        let mut bytes = archive.files["README.md"].clone();
        bytes[0] ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert!(Sources::read(root.path(), &archive, deadline()).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn repeated_native_source_identity_refuses_even_when_all_hashes_match() {
        let (root, archive) = fixture();
        let path = root.path().join("LICENSE-MIT");
        fs::remove_file(&path).unwrap();
        fs::hard_link(root.path().join("README.md"), &path).unwrap();
        assert!(Sources::read(root.path(), &archive, deadline()).is_err());
        assert_eq!(fs::read(path).unwrap(), archive.files["LICENSE-MIT"]);
    }

    #[test]
    fn locked_second_image_refuses_and_releases_earlier_read_guards() {
        let (root, archive) = fixture();
        let path = root.path().join("locron-service-launcher.exe");
        let held = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(Sources::read(root.path(), &archive, deadline()).is_err());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join("locron.exe"))
                .is_ok()
        );
        drop(held);
        assert!(Sources::read(root.path(), &archive, deadline()).is_ok());
    }

    #[test]
    fn expired_and_missing_source_roots_never_create_or_adopt_paths() {
        let (root, archive) = fixture();
        let expired = Instant::now() - Duration::from_secs(1);
        assert!(Sources::read(root.path(), &archive, expired).is_err());
        let missing = root.path().join("absent");
        assert!(Sources::read(&missing, &archive, deadline()).is_err());
        assert!(!missing.exists());
        let proof = Sources::read(root.path(), &archive, deadline()).unwrap();
        assert!(proof.revalidate(expired).is_err());
    }

    #[test]
    fn canonical_checksum_and_archive_refusal_precedes_native_discovery() {
        let (mut release, sums, bytes, _) = archive();
        for (bad_sums, bad_archive) in [
            (b"bad".as_slice(), bytes.as_slice()),
            (sums.as_slice(), b"bad".as_slice()),
        ] {
            let error = verify_until(
                Path::new("not-a-path"),
                Path::new("not-an-alias"),
                &release,
                bad_sums,
                bad_archive,
                deadline(),
            )
            .err()
            .unwrap();
            assert!(!error.to_string().contains("local-drive"));
        }
        release.digests.remove("SHA256SUMS.txt");
        assert!(canonical(&release, native_target().unwrap(), &sums, &bytes).is_err());
    }

    #[test]
    fn canonical_version_directory_mismatch_refuses_before_registry_selection() {
        let (release, sums, bytes, _) = archive();
        let target = native_target().unwrap();
        let path = format!(r"C:\test-owned\locron-v0.10.1-{target}\locron.exe");
        let error = verify_until(
            Path::new(&path),
            Path::new(r"C:\test-owned\locron.exe"),
            &release,
            &sums,
            &bytes,
            deadline(),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("directory version"));
    }
}
