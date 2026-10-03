//! Select a complete paired replacement while preserving actual source ownership.
//! Selection and exclusive observation are not helper/task/transaction authority.

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};

use super::super::windows_paired_receipt::Receipt;
use super::super::windows_receipt::stable_version;
use super::paired::Payloads;
use super::paired_handoff::{CanonicalPair, ExclusivePair};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::self_update) enum Decision {
    Unchanged,
    Replace,
}

fn clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original paired update admission deadline expired"
    );
    Ok(())
}

// Pure metadata comparison is deliberately private. Only the guarded factory
// below can produce a plan; callers cannot turn an arbitrary receipt into one.
fn compare(current: &Receipt, desired: &Receipt) -> Result<Decision> {
    current.validate(&current.sid, &current.directory, &current.target)?;
    desired.validate(&current.sid, &current.directory, &current.target)?;
    ensure!(
        current.user_path == desired.user_path,
        "paired update must preserve the exact raw PATH restoration record"
    );
    let current_version = stable_version(&current.version)?;
    let desired_version = stable_version(&desired.version)?;
    ensure!(
        desired_version >= current_version,
        "paired update refuses a release downgrade"
    );
    if desired_version == current_version {
        ensure!(
            current.archive_url == desired.archive_url
                && current.archive_sha256 == desired.archive_sha256
                && current.files == desired.files,
            "same-version paired release bytes or archive identity changed"
        );
        return Ok(Decision::Unchanged);
    }
    Ok(Decision::Replace)
}

/// Owns all source read guards and borrows both already-verified release sets.
/// No filesystem, registry, PATH, journal or service effect occurs in this factory.
pub(in crate::self_update) struct Prepared<'a> {
    source: CanonicalPair<'a>,
    target: &'a Payloads,
    desired: Receipt,
    receipt_bytes: Vec<u8>,
    decision: Decision,
    deadline: Instant,
}

impl<'a> Prepared<'a> {
    pub(in crate::self_update) fn verify_until(
        directory: &Path,
        current: &'a Payloads,
        target: &'a Payloads,
        deadline: Instant,
    ) -> Result<Self> {
        clock(deadline)?;
        let source = CanonicalPair::verify_until(directory, current, deadline)?;
        clock(deadline)?;
        let inventory = source.inventory();
        let old = inventory.receipt();
        let directory = inventory
            .directory()
            .to_str()
            .context("guarded paired update directory is not Unicode")?;
        let (desired, receipt_bytes) =
            target.receipt(&old.sid, directory, old.user_path.clone())?;
        clock(deadline)?;
        let decision = compare(old, &desired)?;
        clock(deadline)?;
        Ok(Self {
            source,
            target,
            desired,
            receipt_bytes,
            decision,
            deadline,
        })
    }

    pub(in crate::self_update) fn decision(&self) -> Decision {
        self.decision
    }

    pub(in crate::self_update) fn source(&self) -> &CanonicalPair<'a> {
        &self.source
    }

    // Same-version equality does not rewrite a receipt or require exclusive opens.
    pub(in crate::self_update) fn new_receipt_bytes(&self) -> Option<&[u8]> {
        (self.decision == Decision::Replace).then_some(self.receipt_bytes.as_slice())
    }

    /// The downstream engine still needs native probes, quiescence and a durable
    /// protected transaction. This method never deletes, creates or writes a leaf.
    pub(in crate::self_update) fn into_exclusive(self) -> Result<ExclusiveReplacement<'a>> {
        clock(self.deadline)?;
        ensure!(
            self.decision == Decision::Replace,
            "unchanged paired installation has no replacement to admit"
        );
        let source = self.source.into_exclusive()?;
        clock(self.deadline)?;
        Ok(ExclusiveReplacement {
            source,
            target: self.target,
            desired: self.desired,
            receipt_bytes: self.receipt_bytes,
            deadline: self.deadline,
        })
    }
}

/// The old dual-image gate cannot be separated from its selected new bytes.
/// There is deliberately no mutable handle, serialization or success-status API.
pub(in crate::self_update) struct ExclusiveReplacement<'a> {
    source: ExclusivePair<'a>,
    target: &'a Payloads,
    desired: Receipt,
    receipt_bytes: Vec<u8>,
    deadline: Instant,
}

impl ExclusiveReplacement<'_> {
    pub(in crate::self_update) fn source(&self) -> &ExclusivePair<'_> {
        &self.source
    }

    pub(in crate::self_update) fn target(&self) -> &Payloads {
        self.target
    }

    pub(in crate::self_update) fn desired_receipt(&self) -> &Receipt {
        &self.desired
    }

    pub(in crate::self_update) fn new_receipt_bytes(&self) -> &[u8] {
        &self.receipt_bytes
    }

    pub(in crate::self_update) fn check_deadline(&self) -> Result<()> {
        clock(self.deadline)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write};
    use std::time::Duration;

    use locron_core::filesystem::create_private_new_exclusive;
    use serde_json::json;
    use zip::write::SimpleFileOptions;

    use super::super::super::sha256_hex;
    use super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::windows_ownership::native_target;
    use super::super::super::windows_package::PAIRED_FILES;
    use super::super::super::windows_paired_receipt::{EXECUTABLES, PAYLOADS, RECEIPT};
    use super::super::super::windows_receipt::{PathKind, UserPath, appended_path};
    use super::super::super::windows_release_source::{Release, payload_inventory};
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const DIRECTORY: &str = r"C:\paired update 한글";
    const X64: &str = "x86_64-pc-windows-msvc";

    fn payloads(version: &str, target: &str) -> Payloads {
        let machine: u16 = if target == X64 { 0x8664 } else { 0xaa64 };
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in PAIRED_FILES {
            let mut bytes = format!("fixture {name} {version}").into_bytes();
            if EXECUTABLES.contains(&name) {
                bytes = vec![0; 512];
                bytes[..2].copy_from_slice(b"MZ");
                bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
                bytes[128..132].copy_from_slice(b"PE\0\0");
                bytes[132..134].copy_from_slice(&machine.to_le_bytes());
                bytes[134..136].copy_from_slice(&1_u16.to_le_bytes());
                bytes[148..150].copy_from_slice(&240_u16.to_le_bytes());
                bytes[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
                bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
                let subsystem: u16 = if name == EXECUTABLES[0] { 3 } else { 2 };
                bytes[220..222].copy_from_slice(&subsystem.to_le_bytes());
                bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
            }
            writer
                .start_file(
                    format!("locron-v{version}-{target}/{name}"),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        let archive = writer.finish().unwrap().into_inner();
        let installer = format!("unexecuted installer {version}").into_bytes();
        let uninstaller = format!("unexecuted uninstaller {version}").into_bytes();
        let mut digests: BTreeMap<_, _> = payload_inventory(version)
            .unwrap()
            .into_iter()
            .map(|name| (name, "ab".repeat(32)))
            .collect();
        digests.insert(format!("locron-v{version}-{target}.zip"), sha256_hex(&archive));
        let sums = digests
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>()
            .into_bytes();
        digests.insert("SHA256SUMS.txt".into(), sha256_hex(&sums));
        digests.insert("install.sh".into(), "ab".repeat(32));
        digests.insert("install.ps1".into(), sha256_hex(&installer));
        digests.insert("uninstall.ps1".into(), sha256_hex(&uninstaller));
        let metadata = json!({
            "tag_name": format!("v{version}"), "draft": false, "prerelease": false,
            "assets": digests.into_iter().map(|(name, hash)| json!({
                "browser_download_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v{version}/{name}"),
                "name": name, "digest": format!("sha256:{hash}")
            })).collect::<Vec<_>>()
        });
        let release = Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some(version)).unwrap();
        Payloads::verify(&release, target, &sums, &archive, &installer, &uninstaller).unwrap()
    }

    fn receipt(version: &str) -> Receipt {
        payloads(version, X64).receipt(SID, DIRECTORY, None).unwrap().0
    }

    #[test]
    fn numeric_release_order_accepts_only_equal_or_newer_stable_versions() {
        for (old, new) in [
            ("0.10.9", "0.10.10"),
            ("0.10.99", "0.11.0"),
            ("0.99.9", "1.0.0"),
        ] {
            let old = receipt(old);
            let new = receipt(new);
            assert_eq!(compare(&old, &new).unwrap(), Decision::Replace);
            assert!(compare(&new, &old).is_err());
            assert_eq!(compare(&old, &old).unwrap(), Decision::Unchanged);
        }
    }

    #[test]
    fn same_version_refuses_each_changed_canonical_payload_and_archive() {
        let old = receipt("0.10.0");
        for name in PAYLOADS {
            let mut changed = old.clone();
            changed.files.insert(name.to_owned(), "cd".repeat(32));
            for (index, executable) in EXECUTABLES.iter().enumerate() {
                if name == *executable {
                    changed.executables[index].sha256 = "cd".repeat(32);
                }
            }
            changed.validate(SID, DIRECTORY, X64).unwrap();
            assert!(compare(&old, &changed).is_err(), "{name}");
        }
        let mut changed = old.clone();
        changed.archive_sha256 = "cd".repeat(32);
        assert!(compare(&old, &changed).is_err());
    }

    #[test]
    fn foreign_account_target_directory_channel_and_abi_never_become_a_plan() {
        let old = receipt("0.10.0");
        let next = receipt("0.10.1");
        for field in [
            "sid",
            "target",
            "directory",
            "channel",
            "launcher_abi",
            "archive_url",
        ] {
            let mut value = serde_json::to_value(&next).unwrap();
            value[field] = json!("foreign");
            let changed: Receipt = serde_json::from_value(value).unwrap();
            assert!(compare(&old, &changed).is_err(), "{field}");
        }
        let mut swapped = next;
        swapped.executables.swap(0, 1);
        assert!(compare(&old, &swapped).is_err());
    }

    #[test]
    fn existing_raw_path_value_kind_and_missingness_are_frozen() {
        for (before, before_kind) in [
            (None, None),
            (Some(String::new()), Some(PathKind::String)),
            (
                Some(r"%SystemRoot%\System32".into()),
                Some(PathKind::ExpandString),
            ),
        ] {
            let record = UserPath {
                after: appended_path(before.as_deref(), DIRECTORY),
                before,
                before_kind,
                after_kind: before_kind.unwrap_or(PathKind::String),
            };
            let mut old = receipt("0.10.0");
            let mut next = receipt("0.10.1");
            old.user_path = Some(record.clone());
            next.user_path = Some(record);
            assert_eq!(compare(&old, &next).unwrap(), Decision::Replace);
            next.user_path = None;
            assert!(compare(&old, &next).is_err());
        }
    }

    #[test]
    fn malformed_and_overflowing_versions_refuse_instead_of_ordering_as_text() {
        let old = receipt("0.10.0");
        for bad in [
            "v0.10.1",
            "0.010.1",
            "0.10.1-rc1",
            "0.10.18446744073709551616",
            "../0.10.1",
        ] {
            let mut next = old.clone();
            next.version = bad.into();
            assert!(compare(&old, &next).is_err(), "{bad}");
        }
    }

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn installed(old: &Payloads) -> PrivateFixture {
        let root = PrivateFixture::new("locron-paired-update-");
        for (name, bytes) in old.files() {
            write_new(&root.path().join(name), bytes);
        }
        let sid = locron_core::windows::current_user_sid().unwrap();
        let (_, bytes) = old
            .receipt(&sid, root.path().to_str().unwrap(), None)
            .unwrap();
        write_new(&root.path().join(RECEIPT), &bytes);
        write_new(
            &root.path().join("unowned-state-marker"),
            b"retain jobs/history",
        );
        root
    }

    fn assert_unchanged(root: &Path, old: &Payloads, receipt: &[u8]) {
        for (name, bytes) in old.files() {
            assert_eq!(&fs::read(root.join(name)).unwrap(), bytes);
        }
        assert_eq!(fs::read(root.join(RECEIPT)).unwrap(), receipt);
        assert_eq!(
            fs::read(root.join("unowned-state-marker")).unwrap(),
            b"retain jobs/history"
        );
        for name in ["journal.bin", "request.json", "status.json"] {
            assert!(!root.join(name).exists(), "{name}");
        }
    }

    #[test]
    fn same_version_keeps_read_guards_without_receipt_rewrite_or_exclusive_admission() {
        let old = payloads("0.10.0", native_target().unwrap());
        let root = installed(&old);
        let receipt = fs::read(root.path().join(RECEIPT)).unwrap();
        let reader = fs::File::open(root.path().join(EXECUTABLES[1])).unwrap();
        let plan = Prepared::verify_until(
            root.path(),
            &old,
            &old,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(plan.decision(), Decision::Unchanged);
        assert!(plan.new_receipt_bytes().is_none());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(EXECUTABLES[0]))
                .is_err()
        );
        assert!(plan.into_exclusive().is_err());
        drop(reader);
        assert_unchanged(root.path(), &old, &receipt);
    }

    #[test]
    fn newer_plan_carries_selected_receipt_and_both_gates_without_installing() {
        let old = payloads("0.10.0", native_target().unwrap());
        let next = payloads("0.10.1", native_target().unwrap());
        let root = installed(&old);
        let receipt = fs::read(root.path().join(RECEIPT)).unwrap();
        let plan = Prepared::verify_until(
            root.path(),
            &old,
            &next,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(plan.decision(), Decision::Replace);
        let desired = plan.new_receipt_bytes().unwrap().to_vec();
        assert_eq!(plan.source().inventory().receipt().version, "0.10.0");
        let exclusive = plan.into_exclusive().unwrap();
        assert_eq!(exclusive.source().receipt().version, "0.10.0");
        assert_eq!(exclusive.desired_receipt().version, "0.10.1");
        assert_eq!(exclusive.new_receipt_bytes(), desired);
        assert_eq!(exclusive.target().files(), next.files());
        exclusive.check_deadline().unwrap();
        for name in EXECUTABLES {
            assert!(fs::File::open(root.path().join(name)).is_err());
        }
        drop(exclusive);
        assert_unchanged(root.path(), &old, &receipt);
    }

    #[test]
    fn second_image_contention_and_expired_admission_preserve_all_old_bytes() {
        let old = payloads("0.10.0", native_target().unwrap());
        let next = payloads("0.10.1", native_target().unwrap());
        let root = installed(&old);
        let receipt = fs::read(root.path().join(RECEIPT)).unwrap();
        let reader = fs::File::open(root.path().join(EXECUTABLES[1])).unwrap();
        let plan = Prepared::verify_until(
            root.path(),
            &old,
            &next,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(plan.into_exclusive().is_err());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(root.path().join(EXECUTABLES[0]))
                .is_ok()
        );
        drop(reader);
        assert!(Prepared::verify_until(root.path(), &old, &next, Instant::now()).is_err());
        let absent = root.path().join("never-created");
        assert!(Prepared::verify_until(&absent, &old, &next, Instant::now()).is_err());
        assert!(!absent.exists());
        assert_unchanged(root.path(), &old, &receipt);
    }
}
