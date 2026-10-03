//! Canonical seven-file bytes and v2 receipt assembly for the unpublished pair.
//! This result supplies neither live file ownership nor native version/ABI evidence.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};

use super::super::sha256_hex;
use super::super::windows_ownership::native_target;
use super::super::windows_package::LIMIT;
use super::super::windows_paired_package;
use super::super::windows_paired_receipt::{
    EXECUTABLES, Executable, LAUNCHER_ABI, PAYLOADS, RECEIPT, Receipt,
};
use super::super::windows_protocol::{self, maintenance_path};
use super::super::windows_receipt::UserPath;
use super::super::windows_release_source::{Release, Remote};

/// Immutable canonical bytes only; a receipt never grants authority over a live path.
pub(in crate::self_update) struct Payloads {
    version: String,
    target: String,
    archive_url: String,
    archive_sha256: String,
    files: BTreeMap<String, Vec<u8>>,
    hashes: BTreeMap<String, String>,
}

impl Payloads {
    pub(in crate::self_update) fn verify(
        release: &Release,
        target: &str,
        checksums: &[u8],
        archive: &[u8],
        installer: &[u8],
        uninstaller: &[u8],
    ) -> Result<Self> {
        windows_protocol::native_target(target)?;
        for (name, bytes, limit) in [
            ("SHA256SUMS.txt", checksums, 128 * 1024),
            ("install.ps1", installer, LIMIT),
            ("uninstall.ps1", uninstaller, LIMIT),
        ] {
            ensure!(
                bytes.len() <= limit && release.digests.get(name) == Some(&sha256_hex(bytes)),
                "paired {name} differs from the bounded canonical release bytes"
            );
        }
        let sums = release.checksums(checksums)?;
        let asset = format!("locron-v{}-{target}.zip", release.version);
        let archive_sha256 = sums
            .get(&asset)
            .ok_or_else(|| anyhow::anyhow!("canonical paired native archive is missing"))?;
        let verified = windows_paired_package::verify_archive(
            archive,
            &release.version,
            target,
            archive_sha256,
        )?;
        let mut files = verified.files;
        files.insert(".locron-installer.ps1".into(), installer.to_vec());
        files.insert("uninstall.ps1".into(), uninstaller.to_vec());
        ensure!(
            files.len() == PAYLOADS.len() && PAYLOADS.iter().all(|name| files.contains_key(*name)),
            "canonical paired payload inventory disagrees"
        );
        let hashes: BTreeMap<_, _> = files
            .iter()
            .map(|(name, bytes)| (name.clone(), sha256_hex(bytes)))
            .collect();
        for (name, image) in EXECUTABLES.iter().zip(&verified.executables) {
            ensure!(
                hashes.get(*name) == Some(&image.sha256),
                "canonical paired image digest disagrees"
            );
        }
        Ok(Self {
            version: release.version.clone(),
            target: target.to_owned(),
            archive_url: release.asset_url(&asset)?,
            archive_sha256: archive_sha256.clone(),
            files,
            hashes,
        })
    }

    /// Reuse the bounded canonical transport; complete downloads before lifecycle work.
    pub(in crate::self_update) async fn download(
        remote: &Remote,
        selected: Option<&str>,
    ) -> Result<Self> {
        let target = native_target()?;
        let release = remote.release(selected).await?;
        let asset = format!("locron-v{}-{target}.zip", release.version);
        let checksums = remote.asset(&release, "SHA256SUMS.txt").await?;
        let archive = remote.asset(&release, &asset).await?;
        let installer = remote.asset(&release, "install.ps1").await?;
        let uninstaller = remote.asset(&release, "uninstall.ps1").await?;
        Self::verify(
            &release,
            target,
            &checksums,
            &archive,
            &installer,
            &uninstaller,
        )
    }

    pub(in crate::self_update) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    /// Serialize the existing v2 wire contract, not a new claim of installed ownership.
    pub(in crate::self_update) fn receipt(
        &self,
        sid: &str,
        directory: &str,
        user_path: Option<UserPath>,
    ) -> Result<(Receipt, Vec<u8>)> {
        let directory = destination(directory)?;
        let executables = EXECUTABLES.map(|name| Executable {
            path: format!("{directory}\\{name}"),
            sha256: self.hashes[name].clone(),
        });
        let receipt = Receipt {
            schema: "locron.install/windows-v2".into(),
            sid: sid.to_owned(),
            channel: "standalone".into(),
            directory,
            executables,
            target: self.target.clone(),
            version: self.version.clone(),
            launcher_abi: LAUNCHER_ABI.into(),
            archive_url: self.archive_url.clone(),
            archive_sha256: self.archive_sha256.clone(),
            files: self.hashes.clone(),
            user_path,
        };
        self.verify_receipt(&receipt, sid, &receipt.directory)?;
        let bytes = serde_json::to_vec(&receipt)?;
        ensure!(
            bytes.len() <= 128 * 1024,
            "paired receipt exceeds its bound"
        );
        Ok((receipt, bytes))
    }

    /// Compare all canonical release facts after rechecking account/path/schema bindings.
    /// Callers still need retained local guards and native probes before any effect.
    pub(in crate::self_update) fn verify_receipt(
        &self,
        receipt: &Receipt,
        sid: &str,
        directory: &str,
    ) -> Result<()> {
        let directory = destination(directory)?;
        receipt.validate(sid, &directory, &self.target)?;
        ensure!(
            receipt.version == self.version
                && receipt.archive_url == self.archive_url
                && receipt.archive_sha256 == self.archive_sha256
                && receipt.files == self.hashes,
            "paired receipt differs from the canonical release payloads"
        );
        Ok(())
    }
}

fn destination(directory: &str) -> Result<String> {
    let directory = maintenance_path(directory)?;
    for name in PAYLOADS.into_iter().chain([RECEIPT]) {
        maintenance_path(&format!("{directory}\\{name}"))?;
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use serde_json::json;
    use zip::write::SimpleFileOptions;

    use super::super::super::windows_package::{FILES, PAIRED_FILES};
    use super::super::super::windows_receipt::{PathKind, appended_path};
    use super::super::super::windows_release_source::payload_inventory;
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const DIRECTORY: &str = r"C:\test-owned\paired 한글";
    const X64: &str = "x86_64-pc-windows-msvc";
    const ARM64: &str = "aarch64-pc-windows-msvc";

    fn pe(machine: u16, subsystem: u16) -> Vec<u8> {
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

    struct Fixture {
        release: Release,
        sums: Vec<u8>,
        archive: Vec<u8>,
        installer: Vec<u8>,
        uninstaller: Vec<u8>,
        target: &'static str,
    }

    impl Fixture {
        fn new(target: &'static str, paired: bool) -> Self {
            let machine = if target == X64 { 0x8664 } else { 0xaa64 };
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let names: &[&str] = if paired { &PAIRED_FILES } else { &FILES };
            for name in names {
                writer
                    .start_file(
                        format!("locron-v0.10.0-{target}/{name}"),
                        SimpleFileOptions::default(),
                    )
                    .unwrap();
                let bytes = match *name {
                    "locron.exe" => pe(machine, 3),
                    "locron-service-launcher.exe" => pe(machine, 2),
                    _ => b"canonical paired fixture documentation".to_vec(),
                };
                writer.write_all(&bytes).unwrap();
            }
            let archive = writer.finish().unwrap().into_inner();
            let installer = b"canonical installer fixture, never executed".to_vec();
            let uninstaller = b"canonical uninstaller fixture, never executed".to_vec();
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
            digests.insert("install.ps1".into(), sha256_hex(&installer));
            digests.insert("uninstall.ps1".into(), sha256_hex(&uninstaller));
            digests.insert("install.sh".into(), "ab".repeat(32));
            let assets = digests
                .into_iter()
                .map(|(name, hash)| {
                    json!({
                        "browser_download_url": format!(
                            "https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/{name}"
                        ),
                        "name": name,
                        "digest": format!("sha256:{hash}")
                    })
                })
                .collect::<Vec<_>>();
            let metadata = json!({
                "tag_name": "v0.10.0", "draft": false, "prerelease": false, "assets": assets
            });
            let release =
                Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some("0.10.0")).unwrap();
            Self {
                release,
                sums,
                archive,
                installer,
                uninstaller,
                target,
            }
        }

        fn verify(&self) -> Result<Payloads> {
            Payloads::verify(
                &self.release,
                self.target,
                &self.sums,
                &self.archive,
                &self.installer,
                &self.uninstaller,
            )
        }
    }

    #[test]
    fn both_architectures_build_exact_seven_payloads_and_ordered_v2_receipts() {
        // Type-check the existing-source downloader without issuing a request.
        let _download = Payloads::download;
        for target in [X64, ARM64] {
            let fixture = Fixture::new(target, true);
            let payloads = fixture.verify().unwrap();
            assert_eq!(payloads.files().len(), 7);
            assert_eq!(payloads.files()[".locron-installer.ps1"], fixture.installer);
            assert_eq!(payloads.files()["uninstall.ps1"], fixture.uninstaller);
            let (receipt, bytes) = payloads.receipt(SID, DIRECTORY, None).unwrap();
            let decoded = Receipt::parse(&bytes, SID, DIRECTORY, target).unwrap();
            payloads.verify_receipt(&decoded, SID, DIRECTORY).unwrap();
            assert_eq!(decoded.schema, "locron.install/windows-v2");
            assert_eq!(decoded.archive_sha256, sha256_hex(&fixture.archive));
            assert_eq!(decoded.launcher_abi, LAUNCHER_ABI);
            assert!(decoded.user_path.is_none());
            for (index, name) in EXECUTABLES.iter().enumerate() {
                assert_eq!(
                    decoded.executables[index].path,
                    format!("{DIRECTORY}\\{name}")
                );
                assert_eq!(decoded.executables[index].sha256, decoded.files[*name]);
            }
            for (name, content) in payloads.files() {
                assert_eq!(receipt.files[name], sha256_hex(content));
            }
        }
    }

    #[test]
    fn every_changed_download_refuses_before_any_extraction() {
        for selected in 0..4 {
            let mut fixture = Fixture::new(X64, true);
            let bytes = match selected {
                0 => &mut fixture.sums,
                1 => &mut fixture.archive,
                2 => &mut fixture.installer,
                _ => &mut fixture.uninstaller,
            };
            bytes.push(0);
            assert!(fixture.verify().is_err(), "download {selected}");
        }
    }

    #[test]
    fn checksum_document_hash_cannot_hide_inconsistent_or_missing_api_assets() {
        let mut fixture = Fixture::new(X64, true);
        fixture.sums = b"invalid authenticated checksum document".to_vec();
        fixture
            .release
            .digests
            .insert("SHA256SUMS.txt".into(), sha256_hex(&fixture.sums));
        assert!(fixture.verify().is_err());
        for name in [
            format!("locron-v0.10.0-{X64}.zip"),
            "install.ps1".into(),
            "uninstall.ps1".into(),
        ] {
            let mut fixture = Fixture::new(X64, true);
            fixture.release.digests.remove(&name);
            assert!(fixture.verify().is_err(), "missing {name}");
        }
        let mut fixture = Fixture::new(X64, true);
        fixture
            .release
            .digests
            .insert(format!("locron-v0.10.0-{X64}.zip"), "00".repeat(32));
        assert!(fixture.verify().is_err());
    }

    #[test]
    fn legacy_archive_and_wrong_target_never_become_a_pair() {
        assert!(Fixture::new(X64, false).verify().is_err());
        let mut fixture = Fixture::new(X64, true);
        fixture.target = ARM64;
        assert!(fixture.verify().is_err());
        fixture.target = "x86_64-pc-windows-gnu";
        assert!(fixture.verify().is_err());
    }

    #[test]
    fn intrinsically_valid_changed_receipt_hashes_still_fail_canonical_comparison() {
        let payloads = Fixture::new(X64, true).verify().unwrap();
        let (original, _) = payloads.receipt(SID, DIRECTORY, None).unwrap();
        for name in PAYLOADS {
            let mut changed = original.clone();
            changed.files.insert(name.into(), "00".repeat(32));
            if let Some(index) = EXECUTABLES.iter().position(|entry| *entry == name) {
                changed.executables[index].sha256 = "00".repeat(32);
            }
            changed.validate(SID, DIRECTORY, X64).unwrap();
            assert!(payloads.verify_receipt(&changed, SID, DIRECTORY).is_err());
        }
        let mut changed = original.clone();
        changed.archive_sha256 = "00".repeat(32);
        changed.validate(SID, DIRECTORY, X64).unwrap();
        assert!(payloads.verify_receipt(&changed, SID, DIRECTORY).is_err());
        let mut changed = original;
        changed.version = "0.10.1".into();
        changed.archive_url = format!(
            "https://github.com/WhiteKiwi/locron/releases/download/v0.10.1/locron-v0.10.1-{X64}.zip"
        );
        changed.validate(SID, DIRECTORY, X64).unwrap();
        assert!(payloads.verify_receipt(&changed, SID, DIRECTORY).is_err());
    }

    #[test]
    fn comparison_revalidates_user_channel_path_and_order_instead_of_only_hashes() {
        let payloads = Fixture::new(X64, true).verify().unwrap();
        let (original, _) = payloads.receipt(SID, DIRECTORY, None).unwrap();
        for selected in 0..5 {
            let mut changed = original.clone();
            match selected {
                0 => changed.sid = "S-1-5-18".into(),
                1 => changed.channel = "winget".into(),
                2 => changed.directory = r"C:\foreign".into(),
                3 => changed.executables.swap(0, 1),
                _ => changed.launcher_abi = "unknown".into(),
            }
            assert!(payloads.verify_receipt(&changed, SID, DIRECTORY).is_err());
        }
    }

    #[test]
    fn raw_path_kind_missingness_and_literals_round_trip_without_mutation() {
        let payloads = Fixture::new(X64, true).verify().unwrap();
        for (before, before_kind) in [
            (None, None),
            (Some(String::new()), Some(PathKind::String)),
            (
                Some(r"%SystemRoot%\System32".into()),
                Some(PathKind::ExpandString),
            ),
        ] {
            let path = UserPath {
                after: appended_path(before.as_deref(), DIRECTORY),
                after_kind: before_kind.unwrap_or(PathKind::String),
                before,
                before_kind,
            };
            let (_, bytes) = payloads
                .receipt(SID, DIRECTORY, Some(path.clone()))
                .unwrap();
            let decoded = Receipt::parse(&bytes, SID, DIRECTORY, X64).unwrap();
            assert_eq!(decoded.user_path, Some(path));
        }
    }

    #[test]
    fn every_destination_leaf_and_optional_path_record_are_qualified() {
        let payloads = Fixture::new(X64, true).verify().unwrap();
        for path in [r"C:relative", r"C:\foreign:stream", r"C:\test\..\foreign"] {
            assert!(payloads.receipt(SID, path, None).is_err());
        }
        let longest = PAYLOADS
            .into_iter()
            .chain([RECEIPT])
            .map(str::len)
            .max()
            .unwrap();
        let root = format!("C:\\{}", "a".repeat(4096 - 3 - longest));
        maintenance_path(&root).unwrap();
        assert!(payloads.receipt(SID, &root, None).is_err());
        let invalid = UserPath {
            before: None,
            before_kind: None,
            after: DIRECTORY.into(),
            after_kind: PathKind::ExpandString,
        };
        assert!(payloads.receipt(SID, DIRECTORY, Some(invalid)).is_err());
    }
}
