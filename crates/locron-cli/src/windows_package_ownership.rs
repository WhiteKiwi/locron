//! Read-only qualification of a current-user portable WinGet executable.
//! Registry references alone cannot authorize lifecycle effects. Keep the source
//! guards and re-read the exact registration before the later operation engine
//! can use its independently verified canonical archive and typed task snapshot.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Result, ensure};
use locron_core::filesystem::{DirectoryGuard, file_identity, read_owned_executable};
use serde::Deserialize;
use serde_json::{Value, json};

use super::sha256_hex;
use super::windows_ownership::{VerifiedFile, native_target};
use super::windows_package::{LIMIT, verify_archive, verify_pe};
use super::windows_protocol::{PackageBinding, maintenance_path};
use super::windows_receipt::{required_nullable, same_path};
use super::windows_release_source::Remote;

const REGISTRY_LIMIT: usize = 128 * 1024;
const QUERY_SCHEMA: &str = "locron.windows-package-query/v1";
const SNAPSHOT_SCHEMA: &str = "locron.windows-package-registry/v1";

// Exact names/view follow Microsoft's PortableARPEntry.cpp. No key is created,
// no command is interpolated, and raw strings never expand registry variables.
const READ_REGISTRY: &str = r"
if ($request.schema -cne 'locron.windows-package-query/v1') { throw 'invalid package query' }
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if ($request.sid -cne $sid) { throw 'package query belongs to another account' }
function Read-String($key, [string]$name, [bool]$required = $true) {
    if ($key.GetValueNames() -notcontains $name) { return $null }
    $kind = $key.GetValueKind($name)
    if ($kind -ne [Microsoft.Win32.RegistryValueKind]::String -and
        $kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) {
        if ($required) { throw 'package metadata must be a string' }
        return $null
    }
    return $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
}
$entries = @()
$hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,
    [Microsoft.Win32.RegistryView]::Registry64)
try {
    $uninstall = $hive.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall', $false)
    if ($null -ne $uninstall) {
        try {
            foreach ($name in $uninstall.GetSubKeyNames()) {
                $key = $uninstall.OpenSubKey($name, $false)
                if ($null -eq $key) { continue }
                try {
                    $identifier = Read-String $key 'WinGetPackageIdentifier' $false
                    if ($identifier -cne 'WhiteKiwi.locron') { continue }
                    $type = Read-String $key 'WinGetInstallerType'
                    if ($type -cne 'portable') { continue }
                    $entries += [ordered]@{ key = $name; package_id = $identifier; installer_type = $type
                        source_id = Read-String $key 'WinGetSourceIdentifier'
                        version = Read-String $key 'DisplayVersion'
                        install_location = Read-String $key 'InstallLocation'
                        legacy_path = Read-String $key 'TargetFullPath' }
                } finally { $key.Dispose() }
            }
        } finally { $uninstall.Dispose() }
    }
} finally { $hive.Dispose() }
[ordered]@{ schema = 'locron.windows-package-registry/v1'; sid = $sid; entries = @($entries) } |
    ConvertTo-Json -Compress -Depth 6
";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    key: String,
    package_id: String,
    installer_type: String,
    source_id: String,
    version: String,
    install_location: String,
    #[serde(deserialize_with = "required_nullable")]
    legacy_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: String,
    sid: String,
    entries: Vec<Registration>,
}

fn snapshot(value: &Value, sid: &str) -> Result<Snapshot> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= REGISTRY_LIMIT,
        "package registry snapshot exceeds its adapter bound"
    );
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    ensure!(
        snapshot.schema == SNAPSHOT_SCHEMA && snapshot.sid == sid,
        "package registry snapshot does not belong to this account/protocol"
    );
    Ok(snapshot)
}

fn read_snapshot(sid: &str) -> Result<Snapshot> {
    let value = locron_core::windows::run_script_json(
        READ_REGISTRY,
        &json!({"schema":QUERY_SCHEMA,"sid":sid}),
    )?;
    snapshot(&value, sid)
}

fn select(snapshot: &Snapshot, executable: &str, target: &str) -> Result<Registration> {
    maintenance_path(executable)?;
    super::windows_protocol::native_target(target)?;
    let mut selected = None;
    for entry in &snapshot.entries {
        if entry.package_id != "WhiteKiwi.locron" || entry.installer_type != "portable" {
            continue;
        }
        let location = maintenance_path(&entry.install_location)?;
        let nested = format!("{location}\\locron-v{}-{target}\\locron.exe", entry.version);
        let expected = match entry.legacy_path.as_deref().filter(|path| !path.is_empty()) {
            Some(path) => {
                maintenance_path(path)?;
                ensure!(
                    same_path(path, &nested)?
                        || same_path(path, &format!("{location}\\locron.exe"))?,
                    "legacy package executable escapes its registered location"
                );
                path
            }
            None => &nested,
        };
        if !same_path(executable, expected)? {
            continue;
        }
        // Hashes here are placeholders for syntax qualification only. The live
        // source and canonical archive must supply their real final hashes.
        binding(entry, executable, target, &"0".repeat(64), &"0".repeat(64)).validate()?;
        ensure!(selected.is_none(), "ambiguous package registrations");
        selected = Some(entry.clone());
    }
    selected
        .ok_or_else(|| anyhow::anyhow!("executable has no unique current-user portable binding"))
}

fn binding(
    entry: &Registration,
    executable: &str,
    target: &str,
    binary_sha256: &str,
    archive_sha256: &str,
) -> PackageBinding {
    PackageBinding {
        key: entry.key.clone(),
        package_id: entry.package_id.clone(),
        source_id: entry.source_id.clone(),
        install_location: entry.install_location.clone(),
        executable: executable.to_owned(),
        version: entry.version.clone(),
        target: target.to_owned(),
        binary_sha256: binary_sha256.to_owned(),
        archive_sha256: archive_sha256.to_owned(),
    }
}

fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("package path is not Unicode"))
}

fn read_source(executable: &Path, target: &str) -> Result<VerifiedFile> {
    let mut file = read_owned_executable(executable)?;
    maintenance_path(text(file.normalized_path())?)?;
    let identity = file_identity(&file)?;
    let size = file.metadata()?.len();
    ensure!(
        size <= LIMIT as u64,
        "package executable exceeds its byte bound"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut *file)
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == size && file_identity(&file)? == identity,
        "package executable changed under its retained guard"
    );
    verify_pe(&bytes, target)?;
    let sha256 = sha256_hex(&bytes);
    file.seek(SeekFrom::Start(0))?;
    Ok(VerifiedFile {
        file,
        identity,
        sha256,
    })
}

/// Holds exact source/location guards; never writes package-owned files.
pub(super) struct Package {
    pub location: DirectoryGuard,
    pub source: VerifiedFile,
    pub binding: PackageBinding,
    pub archive: Vec<u8>,
    registration: Registration,
}

impl Package {
    /// Registry references are a checked snapshot, not an immutable registry lock.
    /// The later lifecycle engine must call this again before its first effect.
    pub(super) fn revalidate_registration(&self) -> Result<()> {
        let sid = locron_core::windows::current_user_sid()?;
        let registration = select(
            &read_snapshot(&sid)?,
            &self.binding.executable,
            native_target()?,
        )?;
        ensure!(
            registration == self.registration,
            "package registration changed"
        );
        ensure!(
            file_identity(&self.source.file)? == self.source.identity,
            "package source object changed"
        );
        maintenance_path(text(self.location.normalized_path())?)?;
        maintenance_path(text(self.source.file.normalized_path())?)?;
        Ok(())
    }
}

pub(super) async fn verify(executable: &Path, remote: &Remote) -> Result<Package> {
    let sid = locron_core::windows::current_user_sid()?;
    let target = native_target()?;
    let registration = select(&read_snapshot(&sid)?, text(executable)?, target)?;
    let location = DirectoryGuard::ancestors(Path::new(&registration.install_location))?;
    maintenance_path(text(location.normalized_path())?)?;
    let source = read_source(executable, target)?;
    let release = remote.release(Some(&registration.version)).await?;
    let name = format!("locron-v{}-{target}.zip", release.version);
    let sums = remote.asset(&release, "SHA256SUMS.txt").await?;
    let checksums = release.checksums(&sums)?;
    let hash = checksums
        .get(&name)
        .ok_or_else(|| anyhow::anyhow!("canonical release omits this native archive"))?;
    let archive = remote.asset(&release, &name).await?;
    let verified = verify_archive(&archive, &release.version, target, hash)?;
    ensure!(
        source.sha256 == verified.binary_sha256,
        "package executable differs from its canonical release bytes"
    );
    let mut binding = binding(
        &registration,
        text(source.file.normalized_path())?,
        target,
        &source.sha256,
        hash,
    );
    binding.install_location = text(location.normalized_path())?.to_owned();
    binding.validate()?;
    let proof = Package {
        location,
        source,
        binding,
        archive,
        registration,
    };
    proof.revalidate_registration()?;
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use locron_core::filesystem::create_private_new_exclusive;

    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const TARGET: &str = "x86_64-pc-windows-msvc";
    const LOCATION: &str = r"C:\test-owned\package Unicode 한글";

    fn entry() -> Value {
        json!({ "key":"test-owned-locron", "package_id":"WhiteKiwi.locron",
            "installer_type":"portable", "source_id":"test-source", "version":"0.10.0",
            "install_location":LOCATION, "legacy_path":null })
    }

    fn query(entries: Vec<Value>) -> Value {
        json!({"schema":SNAPSHOT_SCHEMA,"sid":SID,"entries":entries})
    }

    #[test]
    fn registry_snapshot_refuses_foreign_protocol_sid_extra_fields_types_and_oversize() {
        let valid = query(vec![entry()]);
        assert_eq!(snapshot(&valid, SID).unwrap().entries.len(), 1);
        for (field, value) in [
            ("schema", json!("unknown")),
            ("sid", json!("S-1-5-18")),
            ("entries", json!(false)),
            ("unexpected", json!(true)),
        ] {
            let mut changed = valid.clone();
            changed[field] = value;
            assert!(snapshot(&changed, SID).is_err(), "{field}");
        }
        let mut changed = valid.clone();
        changed["entries"][0]["legacy_path"] = json!(false);
        assert!(snapshot(&changed, SID).is_err());
        changed["entries"][0]
            .as_object_mut()
            .unwrap()
            .remove("legacy_path");
        assert!(snapshot(&changed, SID).is_err());
        let mut changed = valid;
        changed["entries"][0]["source_id"] = json!("X".repeat(REGISTRY_LIMIT));
        assert!(snapshot(&changed, SID).is_err());
    }

    #[test]
    fn selection_requires_one_exact_nested_or_registered_legacy_path_and_portable_channel() {
        let nested = format!("{LOCATION}\\locron-v0.10.0-{TARGET}\\locron.exe");
        let valid = snapshot(&query(vec![entry()]), SID).unwrap();
        assert_eq!(
            select(&valid, &nested, TARGET).unwrap().key,
            "test-owned-locron"
        );
        assert!(select(&valid, &format!("{LOCATION}\\locron.exe"), TARGET).is_err());
        assert!(select(&valid, r"C:\aliases\locron.exe", TARGET).is_err());
        assert!(select(&valid, &nested, "aarch64-pc-windows-msvc").is_err());
        assert!(
            select(
                &snapshot(&query(vec![entry(), entry()]), SID).unwrap(),
                &nested,
                TARGET
            )
            .is_err()
        );
        let mut legacy = entry();
        legacy["legacy_path"] = json!(format!("{LOCATION}\\locron.exe"));
        assert!(
            select(
                &snapshot(&query(vec![legacy.clone()]), SID).unwrap(),
                &format!("{LOCATION}\\locron.exe"),
                TARGET
            )
            .is_ok()
        );
        for (field, value) in [
            ("package_id", json!("foreign.package")),
            ("installer_type", json!("exe")),
            ("source_id", json!("")),
            ("version", json!("0.9.6")),
            ("key", json!("../foreign")),
            ("legacy_path", json!(r"C:\foreign\locron.exe")),
        ] {
            let mut changed = legacy.clone();
            changed[field] = value;
            assert!(
                select(
                    &snapshot(&query(vec![changed]), SID).unwrap(),
                    &format!("{LOCATION}\\locron.exe"),
                    TARGET
                )
                .is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn fixed_native_registry_reader_is_read_only_and_binds_the_actual_sid() {
        let sid = locron_core::windows::current_user_sid().unwrap();
        let observed = read_snapshot(&sid).unwrap();
        assert_eq!(observed.schema, SNAPSHOT_SCHEMA);
        assert_eq!(observed.sid, sid);
        let foreign = if sid == "S-1-5-18" {
            "S-1-5-19"
        } else {
            "S-1-5-18"
        };
        assert!(
            locron_core::windows::run_script_json(
                READ_REGISTRY,
                &json!({"schema":QUERY_SCHEMA,"sid":foreign})
            )
            .is_err()
        );
    }

    #[test]
    fn source_refusal_preserves_test_owned_bytes_and_does_not_create_missing_ancestry() {
        let temp =
            super::super::windows_fixture::PrivateFixture::new("locron-package-source-fixture-");
        let guard = DirectoryGuard::existing_private(temp.path()).unwrap();
        let missing = temp.path().join("absent").join("locron.exe");
        assert!(read_source(&missing, native_target().unwrap()).is_err());
        assert!(!missing.parent().unwrap().exists());
        let executable = guard.normalized_path().join("locron.exe");
        let bytes = b"test-owned invalid PE bytes, never executed";
        let mut file = create_private_new_exclusive(&executable).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert!(read_source(&executable, native_target().unwrap()).is_err());
        assert_eq!(fs::read(&executable).unwrap(), bytes);
        let file = locron_core::filesystem::open_private_exclusive(&executable).unwrap();
        file.set_len(LIMIT as u64 + 1).unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert!(read_source(&executable, native_target().unwrap()).is_err());
        assert_eq!(fs::metadata(&executable).unwrap().len(), LIMIT as u64 + 1);
        assert!(!temp.path().join("journal.bin").exists());
        assert!(!temp.path().join("status.json").exists());
    }
}
