//! Read-only v2 inventory backed by actual private files and retained full identities.
//!
//! This test-only foundation proves local receipt bytes, digests and structural PE facts.
//! Receipt version/ABI and release metadata are not executed or independently authenticated;
//! the inventory grants no installation, removal, helper or service activation authority.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::DirectoryGuard;

use super::windows_ownership::{VerifiedFile, immutable_private_until, native_target};
use super::windows_package::verify_pe_subsystem;
use super::windows_paired_receipt::{EXECUTABLES, PAYLOADS, RECEIPT, Receipt};
use super::windows_protocol::maintenance_path;
use super::windows_receipt::same_path;

const RECEIPT_LIMIT: usize = 128 * 1024;
const PAYLOAD_LIMIT: usize = 64 * 1024 * 1024;

/// Observed local inventory, with no constructor from serialized receipt metadata.
/// Private fields keep all retained guards and their immutable facts together.
pub(super) struct PairedInventory {
    directory: DirectoryGuard,
    receipt: Receipt,
    raw_receipt: Vec<u8>,
    receipt_file: VerifiedFile,
    payloads: BTreeMap<String, VerifiedFile>,
}

impl PairedInventory {
    pub(super) fn directory(&self) -> &Path {
        self.directory.normalized_path()
    }

    /// Version, launcher ABI and archive source remain validated receipt assertions.
    pub(super) fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    pub(super) fn raw_receipt(&self) -> &[u8] {
        &self.raw_receipt
    }

    pub(super) fn receipt_file(&self) -> &VerifiedFile {
        &self.receipt_file
    }

    pub(super) fn payloads(&self) -> &BTreeMap<String, VerifiedFile> {
        &self.payloads
    }
}

fn before_deadline(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original paired inventory deadline expired"
    );
    Ok(())
}

fn observe<T>(deadline: Instant, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    before_deadline(deadline)?;
    let result = operation();
    before_deadline(deadline)?;
    result
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .context("guarded paired inventory path is not Unicode")
}

fn read_leaf(
    root: &Path,
    name: &str,
    limit: usize,
    deadline: Instant,
) -> Result<(VerifiedFile, Vec<u8>)> {
    let path = root.join(name);
    observe(deadline, || {
        maintenance_path(path_text(&path)?)?;
        Ok(())
    })?;
    let (file, bytes) = immutable_private_until(&path, limit, deadline)?;
    observe(deadline, || {
        let actual = path_text(file.file.normalized_path())?;
        maintenance_path(actual)?;
        ensure!(
            same_path(actual, path_text(&path)?)?,
            "guarded paired payload differs from its fixed inventory path"
        );
        Ok(())
    })?;
    Ok((file, bytes))
}

/// Reads one existing standalone pair using the caller's unchanged absolute deadline.
/// The synchronous caller retains ownership of native work; no worker or budget is created here.
pub(super) fn verify_until(directory: &Path, deadline: Instant) -> Result<PairedInventory> {
    let sid = observe(deadline, || {
        Ok(locron_core::windows::current_user_sid_until(deadline)?)
    })?;
    let target = observe(deadline, native_target)?;
    let directory = observe(deadline, || {
        Ok(DirectoryGuard::existing_private(directory)?)
    })?;
    let root = directory.normalized_path();
    observe(deadline, || {
        maintenance_path(path_text(root)?)?;
        Ok(())
    })?;
    let (receipt_file, raw_receipt) = read_leaf(root, RECEIPT, RECEIPT_LIMIT, deadline)?;
    let receipt = observe(deadline, || {
        Receipt::parse(&raw_receipt, &sid, path_text(root)?, target)
    })?;
    let mut identities = vec![receipt_file.identity];
    let mut payloads = BTreeMap::new();
    for name in PAYLOADS {
        // Only this iteration owns payload bytes; the completed inventory keeps no payload Vec.
        let (file, bytes) = read_leaf(root, name, PAYLOAD_LIMIT, deadline)?;
        observe(deadline, || {
            ensure!(
                receipt.files.get(name) == Some(&file.sha256),
                "paired {name} bytes differ from the exact receipt"
            );
            ensure!(
                !identities.contains(&file.identity),
                "paired inventory names alias the same native file identity"
            );
            if let Some(index) = EXECUTABLES.iter().position(|candidate| *candidate == name) {
                ensure!(
                    receipt.executables[index].sha256 == file.sha256
                        && same_path(
                            &receipt.executables[index].path,
                            path_text(file.file.normalized_path())?,
                        )?,
                    "paired executable differs from its ordered receipt binding"
                );
                verify_pe_subsystem(&bytes, target, if index == 0 { 3 } else { 2 })?;
            }
            Ok(())
        })?;
        drop(bytes);
        identities.push(file.identity);
        payloads.insert(name.to_owned(), file);
    }
    before_deadline(deadline)?;
    Ok(PairedInventory {
        directory,
        receipt,
        raw_receipt,
        receipt_file,
        payloads,
    })
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::time::Duration;

    use locron_core::filesystem::{create_private_new_exclusive, file_identity, is_private};
    use serde_json::{Value, json};

    use super::super::sha256_hex;
    use super::super::windows_fixture::PrivateFixture;
    use super::super::windows_paired_receipt::LAUNCHER_ABI;
    use super::*;

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    // These bytes exercise structure on actual private files; they are never executed.
    fn pe(subsystem: u16) -> Vec<u8> {
        let machine: u16 = match native_target().unwrap() {
            "x86_64-pc-windows-msvc" => 0x8664,
            "aarch64-pc-windows-msvc" => 0xaa64,
            _ => unreachable!(),
        };
        let mut bytes = vec![0; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&machine.to_le_bytes());
        bytes[134..136].copy_from_slice(&1_u16.to_le_bytes());
        bytes[148..150].copy_from_slice(&240_u16.to_le_bytes());
        bytes[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        bytes[220..222].copy_from_slice(&subsystem.to_le_bytes());
        bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
        bytes
    }

    fn fixture() -> (PrivateFixture, Value) {
        let root = PrivateFixture::new("locron-paired-inventory-");
        let directory = root.path().to_str().unwrap();
        let mut files = BTreeMap::new();
        for name in PAYLOADS {
            let bytes = match name {
                "locron.exe" => pe(3),
                "locron-service-launcher.exe" => pe(2),
                // Equal bytes are valid when their actual native identities differ.
                _ => b"paired inventory fixture documentation".to_vec(),
            };
            write_new(&root.path().join(name), &bytes);
            files.insert(name, sha256_hex(&bytes));
        }
        let target = native_target().unwrap();
        let receipt = json!({
            "schema": "locron.install/windows-v2", "channel": "standalone",
            "sid": locron_core::windows::current_user_sid().unwrap(), "directory": directory,
            "executables": [
                {"path": format!("{directory}\\locron.exe"), "sha256": files["locron.exe"]},
                {"path": format!("{directory}\\locron-service-launcher.exe"), "sha256": files["locron-service-launcher.exe"]}
            ],
            "target": target, "version": "0.10.0", "launcher_abi": LAUNCHER_ABI,
            "archive_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{target}.zip"),
            "archive_sha256": "ab".repeat(32), "files": files, "user_path": null
        });
        write_new(
            &root.path().join(RECEIPT),
            &serde_json::to_vec_pretty(&receipt).unwrap(),
        );
        (root, receipt)
    }

    fn read(root: &Path) -> Result<PairedInventory> {
        verify_until(root, Instant::now() + Duration::from_secs(30))
    }

    fn assert_no_operation_files(root: &Path) {
        for name in ["journal.bin", "status.json", "request.json"] {
            assert!(!root.join(name).exists(), "{name}");
        }
    }

    fn write_receipt(root: &Path, receipt: &Value) {
        fs::write(root.join(RECEIPT), serde_json::to_vec_pretty(receipt).unwrap()).unwrap();
    }

    #[test]
    fn live_inventory_retains_raw_receipt_seven_files_and_full_native_identities() {
        let (root, _) = fixture();
        let raw = fs::read(root.path().join(RECEIPT)).unwrap();
        let marker = root.path().join("unowned-marker.txt");
        fs::write(&marker, b"preserve unrelated bytes").unwrap();
        let inventory = read(root.path()).unwrap();
        assert_eq!(inventory.directory(), root.path());
        assert_eq!(inventory.raw_receipt(), raw);
        assert_eq!(inventory.receipt().version, "0.10.0");
        assert_eq!(inventory.payloads().len(), 7);
        assert!(!inventory.payloads().contains_key("unowned-marker.txt"));
        assert_eq!(inventory.receipt_file().sha256, sha256_hex(&raw));
        let mut identities = Vec::new();
        for (name, file) in inventory
            .payloads()
            .iter()
            .map(|(name, file)| (name.as_str(), file))
            .chain([(RECEIPT, inventory.receipt_file())])
        {
            assert_eq!(file_identity(&file.file).unwrap(), file.identity);
            assert!(!identities.contains(&file.identity));
            identities.push(file.identity);
            let path = root.path().join(name);
            assert_eq!(file.file.normalized_path(), path);
            assert!(is_private(&path, false).unwrap());
            assert_eq!(file.sha256, sha256_hex(&fs::read(&path).unwrap()));
            assert!(OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::rename(&path, root.path().join(format!("{name}.moved"))).is_err());
        }
        for (index, name) in EXECUTABLES.iter().enumerate() {
            assert_eq!(
                inventory.receipt().executables[index].sha256,
                inventory.payloads()[*name].sha256
            );
        }
        assert_eq!(identities.len(), 8);
        assert_eq!(fs::read(&marker).unwrap(), b"preserve unrelated bytes");
        assert_no_operation_files(root.path());
        drop(inventory);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(RECEIPT))
                .is_ok()
        );
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join("locron.exe"))
                .is_ok()
        );
    }

    #[test]
    fn missing_root_receipt_and_each_payload_refuse_without_creation_or_adoption() {
        let (root, _) = fixture();
        let absent = root.path().join("missing-parent").join("missing-install");
        assert!(read(&absent).is_err());
        assert!(!root.path().join("missing-parent").exists());
        let receipt_path = root.path().join(RECEIPT);
        let historical = root.path().join(".locron-install-receipt-v1");
        fs::rename(&receipt_path, &historical).unwrap();
        assert!(read(root.path()).is_err());
        assert!(!receipt_path.exists());
        assert!(historical.is_file());
        fs::rename(&historical, &receipt_path).unwrap();
        for name in PAYLOADS {
            let path = root.path().join(name);
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            assert!(read(root.path()).is_err(), "{name}");
            assert!(!path.exists(), "{name}");
            assert!(OpenOptions::new().write(true).open(&receipt_path).is_ok());
            write_new(&path, &bytes);
        }
        assert_no_operation_files(root.path());
    }

    #[test]
    fn strict_receipt_mismatches_and_changed_payload_bytes_are_preserved_on_refusal() {
        let (root, original) = fixture();
        let target = if native_target().unwrap() == "x86_64-pc-windows-msvc" {
            "aarch64-pc-windows-msvc"
        } else {
            "x86_64-pc-windows-msvc"
        };
        let foreign_sid = if original["sid"] == "S-1-5-18" {
            "S-1-5-19"
        } else {
            "S-1-5-18"
        };
        for (field, value) in [
            ("schema", json!("locron.install/windows-v1")),
            ("sid", json!(foreign_sid)),
            ("channel", json!("winget")),
            ("directory", json!(r"C:\foreign-directory")),
            ("target", json!(target)),
            ("launcher_abi", json!("unknown-abi")),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            write_receipt(root.path(), &changed);
            assert!(read(root.path()).is_err(), "{field}");
            assert_eq!(
                fs::read(root.path().join(RECEIPT)).unwrap(),
                serde_json::to_vec_pretty(&changed).unwrap()
            );
        }
        let malformed = b"{\"schema\": ";
        fs::write(root.path().join(RECEIPT), malformed).unwrap();
        assert!(read(root.path()).is_err());
        assert_eq!(fs::read(root.path().join(RECEIPT)).unwrap(), malformed);
        let mut swapped = original.clone();
        swapped["executables"].as_array_mut().unwrap().swap(0, 1);
        write_receipt(root.path(), &swapped);
        assert!(read(root.path()).is_err());
        write_receipt(root.path(), &original);
        for name in PAYLOADS {
            let path = root.path().join(name);
            let bytes = fs::read(&path).unwrap();
            fs::write(&path, b"changed without updating the receipt").unwrap();
            assert!(read(root.path()).is_err(), "{name}");
            assert_eq!(
                fs::read(&path).unwrap(),
                b"changed without updating the receipt"
            );
            fs::write(&path, bytes).unwrap();
        }
        assert_no_operation_files(root.path());
    }

    #[test]
    fn receipt_matching_wrong_pe_subsystem_and_architecture_still_refuse() {
        let (root, original) = fixture();
        for (index, name) in EXECUTABLES.iter().enumerate() {
            let path = root.path().join(name);
            let original_bytes = fs::read(&path).unwrap();
            let wrong_subsystem = pe(if index == 0 { 2 } else { 3 });
            let mut wrong_architecture = original_bytes.clone();
            wrong_architecture[132..134].copy_from_slice(&0x14c_u16.to_le_bytes());
            for bytes in [wrong_subsystem, wrong_architecture] {
                let mut receipt = original.clone();
                let hash = sha256_hex(&bytes);
                receipt["files"][*name] = json!(hash);
                receipt["executables"][index]["sha256"] = json!(hash);
                fs::write(&path, &bytes).unwrap();
                write_receipt(root.path(), &receipt);
                assert!(read(root.path()).is_err(), "{name}");
                assert_eq!(fs::read(&path).unwrap(), bytes);
            }
            fs::write(path, original_bytes).unwrap();
            write_receipt(root.path(), &original);
        }
        assert_no_operation_files(root.path());
    }

    #[test]
    fn different_names_for_one_native_file_refuse_even_with_matching_receipt_hashes() {
        let (root, _) = fixture();
        let source = root.path().join("README.md");
        let alias = root.path().join("LICENSE-MIT");
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&source, &alias).unwrap();
        assert!(read(root.path()).is_err());
        assert_eq!(fs::read(&source).unwrap(), fs::read(&alias).unwrap());
        assert!(OpenOptions::new().write(true).open(&source).is_ok());
        assert_no_operation_files(root.path());
    }

    #[test]
    fn byte_bounds_and_expired_original_deadline_refuse_without_retaining_guards() {
        let (root, _) = fixture();
        let receipt_path = root.path().join(RECEIPT);
        let receipt_bytes = fs::read(&receipt_path).unwrap();
        let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        assert!(verify_until(root.path(), expired).is_err());
        let absent = root.path().join("expired-missing-parent").join("install");
        assert!(verify_until(&absent, expired).is_err());
        assert!(!absent.parent().unwrap().exists());
        for (path, limit) in [
            (receipt_path.clone(), RECEIPT_LIMIT),
            (root.path().join("README.md"), PAYLOAD_LIMIT),
        ] {
            let original = fs::read(&path).unwrap();
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(limit as u64 + 1)
                .unwrap();
            assert!(read(root.path()).is_err());
            assert_eq!(fs::metadata(&path).unwrap().len(), limit as u64 + 1);
            fs::write(&path, original).unwrap();
        }
        assert_eq!(fs::read(&receipt_path).unwrap(), receipt_bytes);
        assert!(read(root.path()).is_ok());
        assert_no_operation_files(root.path());
    }

    #[test]
    fn actual_broad_read_acl_is_refused_without_repairing_the_descriptor() {
        let (root, _) = fixture();
        let path = root.path().join("locron-service-launcher.exe");
        let bytes = fs::read(&path).unwrap();
        let changed = locron_core::windows::run_script_json(
            r"
            $file = [IO.FileInfo]::new([string]$request.path)
            $acl = $file.GetAccessControl()
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'ReadAndExecute', 'Allow'))
            $file.SetAccessControl($acl)
            @{descriptor=$file.GetAccessControl().GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)} | & $locronToJson -Compress
            ",
            &json!({"path": path}),
        )
        .unwrap();
        assert!(!is_private(&path, false).unwrap());
        assert!(read(root.path()).is_err());
        let after = locron_core::windows::run_script_json(
            r"
            $file = [IO.FileInfo]::new([string]$request.path)
            @{descriptor=$file.GetAccessControl().GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)} | & $locronToJson -Compress
            ",
            &json!({"path": path}),
        )
        .unwrap();
        assert_eq!(changed, after);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_no_operation_files(root.path());
    }

    #[test]
    fn reparse_payload_is_refused_without_following_or_removing_its_target() {
        let (root, _) = fixture();
        let target = PrivateFixture::new("locron-paired-unrelated-");
        let marker = target.path().join("keep.txt");
        write_new(&marker, b"unrelated target");
        let link = root.path().join("locron-service-launcher.exe");
        fs::remove_file(&link).unwrap();
        locron_core::windows::run_script_json(
            r"
            New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null
            @{created=$true} | & $locronToJson -Compress
            ",
            &json!({"link": link, "target": target.path()}),
        )
        .unwrap();
        let refused = read(root.path()).is_err();
        // Remove only this test-owned junction before TempDir recursively cleans its container.
        fs::remove_dir(&link).unwrap();
        assert!(refused);
        assert_eq!(fs::read(&marker).unwrap(), b"unrelated target");
        assert_no_operation_files(root.path());
    }
}
