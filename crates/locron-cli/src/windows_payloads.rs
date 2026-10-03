//! Complete canonical six-file payloads and a pure strict receipt builder.
//! No archive is extracted and no installation, staging or state object is created.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};

use super::sha256_hex;
use super::windows_ownership::native_target;
use super::windows_package::{LIMIT, verify_archive};
use super::windows_protocol;
use super::windows_receipt::{PAYLOADS, Receipt, UserPath, local_path};
use super::windows_release_source::{Release, Remote};

#[path = "windows_payloads/paired.rs"]
pub(super) mod paired;

#[path = "windows_payloads/paired_handoff.rs"]
pub(super) mod paired_handoff;

#[path = "windows_payloads/paired_removal.rs"]
pub(super) mod paired_removal;

pub(super) struct Payloads {
    pub version: String,
    pub target: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    files: BTreeMap<String, Vec<u8>>,
}

impl Payloads {
    /// Revalidate protected bootstrap bytes against the full canonical release.
    pub(super) fn verify(
        release: &Release,
        target: &str,
        checksums: &[u8],
        archive: &[u8],
        installer: &[u8],
        uninstaller: &[u8],
    ) -> Result<Self> {
        windows_protocol::native_target(target)?;
        for (name, bytes) in [
            ("SHA256SUMS.txt", checksums),
            ("install.ps1", installer),
            ("uninstall.ps1", uninstaller),
        ] {
            ensure!(
                bytes.len() <= LIMIT && release.digests.get(name) == Some(&sha256_hex(bytes)),
                "protected {name} differs from the final canonical release bytes"
            );
        }
        let sums = release.checksums(checksums)?;
        let asset = format!("locron-v{}-{target}.zip", release.version);
        let archive_sha256 = sums
            .get(&asset)
            .ok_or_else(|| anyhow::anyhow!("canonical native archive is missing"))?;
        let verified = verify_archive(archive, &release.version, target, archive_sha256)?;
        let mut files = verified.files;
        files.insert(".locron-installer.ps1".into(), installer.to_vec());
        files.insert("uninstall.ps1".into(), uninstaller.to_vec());
        ensure!(
            files.len() == PAYLOADS.len() && PAYLOADS.iter().all(|name| files.contains_key(*name)),
            "complete installation payload inventory disagrees"
        );
        Ok(Self {
            version: release.version.clone(),
            target: target.to_owned(),
            archive_url: release.asset_url(&asset)?,
            archive_sha256: archive_sha256.clone(),
            binary_sha256: verified.binary_sha256,
            files,
        })
    }

    pub(super) async fn download(remote: &Remote, selected: Option<&str>) -> Result<Self> {
        let target = native_target()?;
        let release = remote.release(selected).await?;
        let asset = format!("locron-v{}-{target}.zip", release.version);
        let checksums = remote.asset(&release, "SHA256SUMS.txt").await?;
        // All network work precedes task quiescence and installed-target effects.
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

    pub(super) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    /// The later helper must bind this metadata to actual guarded destination objects.
    pub(super) fn receipt(
        &self,
        sid: &str,
        directory: &str,
        user_path: Option<UserPath>,
    ) -> Result<(Receipt, Vec<u8>)> {
        let directory = local_path(directory)?;
        let receipt = Receipt {
            schema: "locron.install/windows-v1".into(),
            sid: sid.to_owned(),
            channel: "standalone".into(),
            executable: format!("{directory}\\locron.exe"),
            directory,
            target: self.target.clone(),
            version: self.version.clone(),
            archive_url: self.archive_url.clone(),
            archive_sha256: self.archive_sha256.clone(),
            binary_sha256: self.binary_sha256.clone(),
            files: self
                .files
                .iter()
                .map(|(name, bytes)| (name.clone(), sha256_hex(bytes)))
                .collect(),
            user_path,
        };
        receipt.validate(sid, &receipt.directory, &self.target)?;
        let bytes = serde_json::to_vec(&receipt)?;
        ensure!(
            bytes.len() <= 128 * 1024,
            "complete new receipt exceeds its bound"
        );
        Ok((receipt, bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use serde_json::json;
    use zip::write::SimpleFileOptions;

    use super::super::windows_receipt::PathKind;
    use super::super::windows_release_source::payload_inventory;
    use super::*;

    const TARGET: &str = "x86_64-pc-windows-msvc";
    const DIRECTORY: &str = r"C:\test-owned\locron";
    const SID: &str = "S-1-5-21-1-2-3-1001";

    fn fixture() -> (Release, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut binary = vec![0; 512];
        binary[..2].copy_from_slice(b"MZ");
        binary[60..64].copy_from_slice(&128_u32.to_le_bytes());
        binary[128..132].copy_from_slice(b"PE\0\0");
        binary[132..134].copy_from_slice(&0x8664_u16.to_le_bytes());
        binary[134..136].copy_from_slice(&1_u16.to_le_bytes());
        binary[148..150].copy_from_slice(&240_u16.to_le_bytes());
        binary[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
        binary[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        binary[260..264].copy_from_slice(&16_u32.to_le_bytes());
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in super::super::windows_package::FILES {
            writer
                .start_file(
                    format!("locron-v0.10.0-{TARGET}/{name}"),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(if name == "locron.exe" {
                    &binary
                } else {
                    b"fixture"
                })
                .unwrap();
        }
        let archive = writer.finish().unwrap().into_inner();
        let installer = b"fixture installer bytes are never executed".to_vec();
        let uninstaller = b"fixture offline removal bytes are never executed".to_vec();
        let asset = format!("locron-v0.10.0-{TARGET}.zip");
        let mut digests: BTreeMap<_, _> = payload_inventory("0.10.0")
            .unwrap()
            .into_iter()
            .map(|name| (name, "ab".repeat(32)))
            .collect();
        digests.insert(asset, sha256_hex(&archive));
        let sums: Vec<u8> = digests
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>()
            .into_bytes();
        digests.insert("SHA256SUMS.txt".into(), sha256_hex(&sums));
        digests.insert("install.ps1".into(), sha256_hex(&installer));
        digests.insert("uninstall.ps1".into(), sha256_hex(&uninstaller));
        digests.insert("install.sh".into(), "ab".repeat(32));
        let metadata = json!({"tag_name":"v0.10.0","draft":false,"prerelease":false,
            "assets":digests.into_iter().map(|(name, hash)|json!({"browser_download_url":format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/{name}"),"name":name,"digest":format!("sha256:{hash}")})).collect::<Vec<_>>()});
        (
            Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some("0.10.0")).unwrap(),
            sums,
            archive,
            installer,
            uninstaller,
        )
    }

    #[test]
    fn complete_canonical_inventory_builds_a_strict_receipt_without_extracting_or_executing() {
        let (release, sums, archive, installer, uninstaller) = fixture();
        let payloads =
            Payloads::verify(&release, TARGET, &sums, &archive, &installer, &uninstaller).unwrap();
        assert_eq!(payloads.files().len(), 6);
        assert_eq!(payloads.files()[".locron-installer.ps1"], installer);
        assert_eq!(payloads.files()["uninstall.ps1"], uninstaller);
        let (receipt, encoded) = payloads.receipt(SID, DIRECTORY, None).unwrap();
        let decoded = Receipt::parse(&encoded, SID, DIRECTORY, TARGET).unwrap();
        assert_eq!(decoded.version, "0.10.0");
        assert_eq!(decoded.files["locron.exe"], decoded.binary_sha256);
        assert_eq!(receipt.archive_sha256, sha256_hex(&archive));
        assert_eq!(
            decoded.files[".locron-installer.ps1"],
            sha256_hex(&installer)
        );
    }

    #[test]
    fn staged_digest_architecture_and_path_receipt_tampering_refuse_before_effects() {
        let (release, sums, archive, installer, uninstaller) = fixture();
        for (bad_sums, bad_archive, bad_installer, bad_uninstaller) in [
            (
                [sums.as_slice(), b"\n"].concat(),
                archive.clone(),
                installer.clone(),
                uninstaller.clone(),
            ),
            (
                sums.clone(),
                b"different archive".to_vec(),
                installer.clone(),
                uninstaller.clone(),
            ),
            (
                sums.clone(),
                archive.clone(),
                b"different installer".to_vec(),
                uninstaller.clone(),
            ),
            (
                sums.clone(),
                archive.clone(),
                installer.clone(),
                b"different uninstaller".to_vec(),
            ),
        ] {
            assert!(
                Payloads::verify(
                    &release,
                    TARGET,
                    &bad_sums,
                    &bad_archive,
                    &bad_installer,
                    &bad_uninstaller
                )
                .is_err()
            );
        }
        assert!(
            Payloads::verify(
                &release,
                "aarch64-pc-windows-msvc",
                &sums,
                &archive,
                &installer,
                &uninstaller
            )
            .is_err()
        );
        let payloads =
            Payloads::verify(&release, TARGET, &sums, &archive, &installer, &uninstaller).unwrap();
        assert!(payloads.receipt(SID, r"C:\foreign:stream", None).is_err());
        let invalid = UserPath {
            before: None,
            before_kind: None,
            after: DIRECTORY.into(),
            after_kind: PathKind::ExpandString,
        };
        assert!(payloads.receipt(SID, DIRECTORY, Some(invalid)).is_err());
    }
}
