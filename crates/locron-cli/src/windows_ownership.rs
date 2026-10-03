//! Read-only proof of the exact standalone payload inventory.
//!
//! Retain these immutable read guards until durable backups and confirmed
//! quiescence permit reacquiring the recorded objects through exclusive gates.
//! This proof never repairs ownership, creates a missing directory, or deletes
//! an unlisted companion file.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Result, ensure};
use locron_core::filesystem::{
    DirectoryGuard, FileIdentity, GuardedFile, file_identity, is_private, read_owned_executable,
};

use super::sha256_hex;
use super::windows_receipt::{PAYLOADS, RECEIPT, Receipt};

const RECEIPT_LIMIT: usize = 128 * 1024;
const PAYLOAD_LIMIT: usize = 64 * 1024 * 1024;

pub(super) struct VerifiedFile {
    pub file: GuardedFile,
    pub identity: FileIdentity,
    pub sha256: String,
}

pub(super) struct Standalone {
    pub directory: DirectoryGuard,
    pub receipt: Receipt,
    pub receipt_file: VerifiedFile,
    pub files: BTreeMap<String, VerifiedFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Retention {
    Changed,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedPayload {
    pub name: String,
    pub reason: Retention,
}

/// Removal never acquires authority over a changed or unverifiable companion.
pub(super) struct Removal {
    pub directory: DirectoryGuard,
    pub receipt: Receipt,
    pub receipt_file: VerifiedFile,
    pub files: BTreeMap<String, VerifiedFile>,
    pub retained: Vec<RetainedPayload>,
}

/// Select the actual binary architecture, never a request-supplied architecture.
pub(super) fn native_target() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64-pc-windows-msvc"),
        "aarch64" => Ok("aarch64-pc-windows-msvc"),
        _ => anyhow::bail!("Windows distribution requires a native x64 or ARM64 binary"),
    }
}

fn clock(deadline: Option<Instant>) -> Result<()> {
    ensure!(
        deadline.is_none_or(|deadline| Instant::now() < deadline),
        "original inventory deadline expired"
    );
    Ok(())
}

fn observe<T>(deadline: Option<Instant>, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    clock(deadline)?;
    let result = operation();
    clock(deadline)?;
    result
}

pub(super) fn immutable_private(path: &Path, limit: usize) -> Result<(VerifiedFile, Vec<u8>)> {
    immutable_private_at(path, limit, None)
}

/// Uses only the caller's existing clock; native work belongs to its retained owner.
pub(super) fn immutable_private_until(
    path: &Path,
    limit: usize,
    deadline: Instant,
) -> Result<(VerifiedFile, Vec<u8>)> {
    locron_core::windows::current_user_sid_until(deadline)?;
    immutable_private_at(path, limit, Some(deadline))
}

fn immutable_private_at(
    path: &Path,
    limit: usize,
    deadline: Option<Instant>,
) -> Result<(VerifiedFile, Vec<u8>)> {
    // This READ-only gate prevents competing writes and leaf replacement. The
    // additional strict check excludes the broader package-source descriptor.
    let mut file = observe(deadline, || Ok(read_owned_executable(path)?))?;
    ensure!(
        observe(deadline, || Ok(is_private(file.normalized_path(), false)?))?,
        "standalone payload does not have its strict private descriptor"
    );
    let identity = observe(deadline, || Ok(file_identity(&file)?))?;
    let size = observe(deadline, || Ok(file.metadata()?.len()))?;
    ensure!(size <= limit as u64, "owned payload exceeds its byte bound");
    observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    let mut bytes = Vec::new();
    observe(deadline, || {
        Ok(Read::by_ref(&mut *file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?)
    })?;
    ensure!(
        bytes.len() as u64 == size && observe(deadline, || Ok(file_identity(&file)?))? == identity,
        "owned payload differs from its guarded object/length"
    );
    let sha256 = observe(deadline, || Ok(sha256_hex(&bytes)))?;
    observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    Ok((
        VerifiedFile {
            file,
            identity,
            sha256,
        },
        bytes,
    ))
}

fn owned_receipt(
    directory: &Path,
    deadline: Option<Instant>,
) -> Result<(DirectoryGuard, Receipt, VerifiedFile)> {
    if let Some(deadline) = deadline {
        locron_core::windows::current_user_sid_until(deadline)?;
    }
    let directory = observe(deadline, || {
        Ok(DirectoryGuard::existing_private(directory)?)
    })?;
    let root = directory.normalized_path();
    let (receipt_file, bytes) = immutable_private_at(&root.join(RECEIPT), RECEIPT_LIMIT, deadline)?;
    let sid = match deadline {
        Some(deadline) => locron_core::windows::current_user_sid_until(deadline)?,
        None => locron_core::windows::current_user_sid()?,
    };
    let receipt = observe(deadline, || {
        Receipt::parse(
            &bytes,
            &sid,
            root.to_str()
                .ok_or_else(|| anyhow::anyhow!("standalone directory is not Unicode"))?,
            native_target()?,
        )
    })?;
    Ok((directory, receipt, receipt_file))
}

/// Verify existing standalone ownership with no filesystem/registry/task writes.
pub(super) fn verify(directory: &Path) -> Result<Standalone> {
    verify_at(directory, None)
}

pub(super) fn verify_until(directory: &Path, deadline: Instant) -> Result<Standalone> {
    verify_at(directory, Some(deadline))
}

fn verify_at(directory: &Path, deadline: Option<Instant>) -> Result<Standalone> {
    let (directory, receipt, receipt_file) = owned_receipt(directory, deadline)?;
    let root = directory.normalized_path();
    let mut files = BTreeMap::new();
    for name in PAYLOADS {
        let (file, _) = immutable_private_at(&root.join(name), PAYLOAD_LIMIT, deadline)?;
        ensure!(
            receipt.files.get(name) == Some(&file.sha256),
            "owned {name} bytes differ from the exact receipt"
        );
        files.insert(name.to_owned(), file);
    }
    Ok(Standalone {
        directory,
        receipt,
        receipt_file,
        files,
    })
}

/// Qualify only unchanged listed files for later removal; no file is changed here.
/// The protected receipt and original executable remain mandatory exact proofs.
pub(super) fn verify_removal(directory: &Path) -> Result<Removal> {
    verify_removal_at(directory, None)
}

pub(super) fn verify_removal_until(directory: &Path, deadline: Instant) -> Result<Removal> {
    verify_removal_at(directory, Some(deadline))
}

fn verify_removal_at(directory: &Path, deadline: Option<Instant>) -> Result<Removal> {
    let (directory, receipt, receipt_file) = owned_receipt(directory, deadline)?;
    let root = directory.normalized_path();
    let (binary, _) = immutable_private_at(&root.join("locron.exe"), PAYLOAD_LIMIT, deadline)?;
    ensure!(
        binary.sha256 == receipt.binary_sha256,
        "removal requires the exact receipt-owned executable"
    );
    let mut files = BTreeMap::from([("locron.exe".to_owned(), binary)]);
    let mut retained = Vec::new();
    for name in PAYLOADS.into_iter().filter(|name| *name != "locron.exe") {
        let result = immutable_private_at(&root.join(name), PAYLOAD_LIMIT, deadline);
        // Expiry is a phase refusal, never an Unverifiable removal permission.
        clock(deadline)?;
        let reason = match result {
            Ok((file, _)) if receipt.files.get(name) == Some(&file.sha256) => {
                files.insert(name.to_owned(), file);
                continue;
            }
            Ok(_) => Retention::Changed,
            // A missing, foreign, locked, nonregular or unreadable entry grants
            // no deletion/repair authority. Leave it entirely to the operator.
            Err(_) => Retention::Unverifiable,
        };
        retained.push(RetainedPayload {
            name: name.to_owned(),
            reason,
        });
    }
    Ok(Removal {
        directory,
        receipt,
        receipt_file,
        files,
        retained,
    })
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use locron_core::filesystem::create_private_new_exclusive;
    use serde_json::{Value, json};

    use super::*;

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn fixture() -> (super::super::windows_fixture::PrivateFixture, Value) {
        let root = super::super::windows_fixture::PrivateFixture::new("locron-ownership-fixture-");
        let guard = DirectoryGuard::existing_private(root.path()).unwrap();
        let directory = guard.normalized_path().to_str().unwrap().to_owned();
        let mut files = BTreeMap::new();
        for name in PAYLOADS {
            let bytes = format!("immutable test-owned {name}").into_bytes();
            write_new(&root.path().join(name), &bytes);
            files.insert(name.to_owned(), sha256_hex(&bytes));
        }
        let target = native_target().unwrap();
        let receipt = json!({
            "schema":"locron.install/windows-v1", "channel":"standalone",
            "sid":locron_core::windows::current_user_sid().unwrap(),
            "directory":directory, "executable":format!("{directory}\\locron.exe"),
            "target":target, "version":"0.10.0",
            "archive_url":format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{target}.zip"),
            "archive_sha256":"ab".repeat(32), "binary_sha256":files["locron.exe"],
            "files":files, "user_path":null
        });
        write_new(
            &root.path().join(RECEIPT),
            &serde_json::to_vec(&receipt).unwrap(),
        );
        (root, receipt)
    }

    #[test]
    fn original_inventory_deadline_refuses_without_repair_or_removal_permissions() {
        let (root, _) = fixture();
        let bytes = fs::read(root.path().join(RECEIPT)).unwrap();
        let deadline = Instant::now()
            .checked_sub(std::time::Duration::from_millis(1))
            .unwrap();
        assert!(verify_until(root.path(), deadline).is_err());
        assert!(verify_removal_until(root.path(), deadline).is_err());
        assert!(
            immutable_private_until(&root.path().join("locron.exe"), PAYLOAD_LIMIT, deadline)
                .is_err()
        );
        assert_eq!(fs::read(root.path().join(RECEIPT)).unwrap(), bytes);
        assert!(!root.path().join("journal.bin").exists());
        assert!(!root.path().join("status.json").exists());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join("README.md"))
                .is_ok()
        );
    }

    #[test]
    fn existing_proof_retains_immutable_guards_without_adopting_unlisted_files() {
        let (root, _) = fixture();
        let marker = root.path().join("unowned-marker.txt");
        fs::write(&marker, b"preserve me").unwrap();
        let proof = verify(root.path()).unwrap();
        assert_eq!(proof.files.len(), 6);
        assert_eq!(proof.receipt.version, "0.10.0");
        assert_eq!(
            proof.receipt_file.identity,
            file_identity(&proof.receipt_file.file).unwrap()
        );
        assert_eq!(
            proof.directory.normalized_path(),
            proof.receipt_file.file.normalized_path().parent().unwrap()
        );
        for name in PAYLOADS.into_iter().chain([RECEIPT]) {
            let path = root.path().join(name);
            assert!(OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::rename(&path, root.path().join(format!("{name}.moved"))).is_err());
        }
        assert_eq!(fs::read(&marker).unwrap(), b"preserve me");
        drop(proof);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join("README.md"))
                .is_ok()
        );
    }

    #[test]
    fn stale_or_foreign_receipt_and_changed_payload_refuse_without_repair() {
        let (root, receipt) = fixture();
        let path = root.path().join(RECEIPT);
        let original = fs::read(&path).unwrap();
        for (field, value) in [
            ("sid", json!("S-1-5-21-9-8-7-1001")),
            ("channel", json!("winget")),
            ("directory", json!("C:\\foreign-directory")),
        ] {
            let mut changed = receipt.clone();
            changed[field] = value;
            fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(verify(root.path()).is_err(), "{field}");
            assert!(verify_removal(root.path()).is_err(), "{field}");
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
                changed
            );
        }
        fs::write(&path, original).unwrap();
        fs::write(root.path().join("README.md"), b"changed payload").unwrap();
        assert!(verify(root.path()).is_err());
        assert_eq!(
            fs::read(root.path().join("README.md")).unwrap(),
            b"changed payload"
        );
        let missing = root.path().join("missing-directory");
        assert!(verify(&missing).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn removal_retains_changed_missing_and_unverifiable_companions_without_effects() {
        let (root, _) = fixture();
        let changed = root.path().join("README.md");
        fs::write(&changed, b"operator changes").unwrap();
        let missing = root.path().join("LICENSE-MIT");
        fs::remove_file(&missing).unwrap();
        let locked_path = root.path().join("LICENSE-APACHE");
        let locked = locron_core::filesystem::open_private_exclusive(&locked_path).unwrap();
        let marker = root.path().join("unlisted-marker.txt");
        fs::write(&marker, b"unowned").unwrap();

        let proof = verify_removal(root.path()).unwrap();
        assert_eq!(proof.files.len(), 3);
        assert_eq!(proof.retained.len(), 3);
        assert!(proof.retained.contains(&RetainedPayload {
            name: "README.md".into(),
            reason: Retention::Changed,
        }));
        for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
            assert!(proof.retained.contains(&RetainedPayload {
                name: name.into(),
                reason: Retention::Unverifiable,
            }));
        }
        assert_eq!(proof.receipt.version, "0.10.0");
        assert_eq!(
            proof.directory.normalized_path(),
            proof.receipt_file.file.normalized_path().parent().unwrap()
        );
        for file in proof.files.values() {
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(file.file.normalized_path())
                    .is_err()
            );
        }
        assert_eq!(fs::read(&changed).unwrap(), b"operator changes");
        assert!(!missing.exists());
        assert_eq!(fs::read(&marker).unwrap(), b"unowned");
        drop(locked);
        assert_eq!(
            fs::read(&locked_path).unwrap(),
            b"immutable test-owned LICENSE-APACHE"
        );
        drop(proof);
        assert!(!root.path().join("journal.bin").exists());
        assert!(!root.path().join("status.json").exists());
    }

    #[test]
    fn removal_refuses_a_changed_original_executable_and_missing_receipt() {
        let (root, _) = fixture();
        let binary = root.path().join("locron.exe");
        fs::write(&binary, b"foreign binary bytes").unwrap();
        assert!(verify_removal(root.path()).is_err());
        assert_eq!(fs::read(&binary).unwrap(), b"foreign binary bytes");
        fs::remove_file(root.path().join(RECEIPT)).unwrap();
        assert!(verify_removal(root.path()).is_err());
        assert!(!root.path().join(RECEIPT).exists());
    }
}
