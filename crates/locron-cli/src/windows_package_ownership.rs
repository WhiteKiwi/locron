//! Read-only qualification of a current-user portable WinGet executable.
//! Registry references alone cannot authorize lifecycle effects. Keep the source
//! guards and re-read the exact registration before the later operation engine
//! can use its independently verified canonical archive and typed task snapshot.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;

use anyhow::{Result, ensure};
use locron_core::filesystem::{
    DirectoryGuard, FileIdentity, GuardedFile, file_identity, read_owned_executable,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::sha256_hex;
use super::windows_ownership::{VerifiedFile, native_target};
use super::windows_package::{LIMIT, verify_archive, verify_pe};
use super::windows_package_index::{self, IndexFacts, SNAPSHOT_LIMIT};
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

fn index_clock(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original registered-index deadline expired"
    );
    Ok(())
}

fn index_observe<T>(deadline: Instant, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    index_clock(deadline)?;
    let result = operation();
    index_clock(deadline)?;
    result
}

fn read_snapshot_until(sid: &str, deadline: Instant) -> Result<Snapshot> {
    let value = index_observe(deadline, || {
        Ok(locron_core::windows::run_script_json_until(
            READ_REGISTRY,
            &json!({"schema": QUERY_SCHEMA, "sid": sid}),
            deadline,
        )?)
    })?;
    index_observe(deadline, || snapshot(&value, sid))
}

fn refuse_index_sidecars(path: &Path, deadline: Instant) -> Result<()> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        index_observe(deadline, || {
            match fs::symlink_metadata(Path::new(&sidecar)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
                Ok(_) => anyhow::bail!("portable index has an unsupported {suffix} sidecar"),
            }
        })?;
    }
    Ok(())
}

fn index_bytes_until(file: &mut GuardedFile, deadline: Instant) -> Result<Vec<u8>> {
    let identity = index_observe(deadline, || Ok(file_identity(file)?))?;
    let size = index_observe(deadline, || Ok(file.metadata()?.len()))?;
    ensure!(
        (100..=SNAPSHOT_LIMIT as u64).contains(&size),
        "portable index is not a bounded complete SQLite file"
    );
    index_observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    let mut bytes = Vec::with_capacity(usize::try_from(size)?);
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let capacity = chunk.len().min(SNAPSHOT_LIMIT + 1 - bytes.len());
        let count = index_observe(deadline, || Ok(file.read(&mut chunk[..capacity])?))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        ensure!(
            bytes.len() <= SNAPSHOT_LIMIT,
            "portable index exceeds its byte bound"
        );
    }
    ensure!(
        bytes.len() as u64 == size
            && index_observe(deadline, || Ok(file.metadata()?.len()))? == size
            && index_observe(deadline, || Ok(file_identity(file)?))? == identity,
        "portable index changed under its retained read guard"
    );
    index_observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
    Ok(bytes)
}

struct IndexSource {
    location: DirectoryGuard,
    root: DirectoryGuard,
    file: GuardedFile,
    identity: FileIdentity,
    facts: IndexFacts,
}

fn read_index_source_until(
    registration: &Registration,
    console: &str,
    expected_alias: &str,
    target: &str,
    deadline: Instant,
) -> Result<IndexSource> {
    index_clock(deadline)?;
    let location = index_observe(deadline, || {
        Ok(DirectoryGuard::ancestors(Path::new(
            &registration.install_location,
        ))?)
    })?;
    let location_path = text(location.normalized_path())?;
    maintenance_path(location_path)?;
    let root_path = location
        .normalized_path()
        .join(format!("locron-v{}-{target}", registration.version));
    maintenance_path(text(&root_path)?)?;
    ensure!(
        same_path(console, text(&root_path.join("locron.exe"))?)?,
        "registered index requires the exact nested console path"
    );
    let root = index_observe(deadline, || Ok(DirectoryGuard::ancestors(&root_path)?))?;
    let index_path = location
        .normalized_path()
        .join(format!("{}.db", registration.key));
    maintenance_path(text(&index_path)?)?;
    let metadata = windows_package_index::Registration {
        product_code: &registration.key,
        package_id: &registration.package_id,
        source_id: &registration.source_id,
        version: &registration.version,
        target,
        install_location: location_path,
        index_path: text(&index_path)?,
        console_alias: expected_alias,
    };
    // The existing package-source primitive validates a regular current-SID-owned
    // leaf with trusted mutation rights. It does not inspect or execute PE bytes.
    let mut file = index_observe(deadline, || Ok(read_owned_executable(&index_path)?))?;
    ensure!(
        same_path(text(file.normalized_path())?, text(&index_path)?)?,
        "guarded index differs from its exact selected path"
    );
    let identity = index_observe(deadline, || Ok(file_identity(&file)?))?;
    refuse_index_sidecars(file.normalized_path(), deadline)?;
    let bytes = index_bytes_until(&mut file, deadline)?;
    let facts = windows_package_index::verify_index_snapshot(
        &bytes,
        &metadata,
        text(root.normalized_path())?,
        deadline,
    )?;
    refuse_index_sidecars(file.normalized_path(), deadline)?;
    ensure!(
        index_observe(deadline, || Ok(file_identity(&file)?))? == identity,
        "guarded index identity changed during parsing"
    );
    index_clock(deadline)?;
    Ok(IndexSource {
        location,
        root,
        file,
        identity,
        facts,
    })
}

/// Current native registration bound to one retained read-only index observation.
///
/// This opaque, move-only prerequisite grants no executable, alias or lifecycle authority.
/// The caller must own the finite native worker and retain it through uncertain I/O; these
/// synchronous deadline gates cannot cancel a stalled filesystem call.
/// Do not retain the generic/COM script permit across native registry queries.
pub(super) struct RegisteredIndex {
    sid: String,
    registration: Registration,
    console: String,
    expected_alias: String,
    target: &'static str,
    source: IndexSource,
    original_deadline: Instant,
}

impl RegisteredIndex {
    /// Re-observes native registration and the retained object within the original budget.
    /// A later caller deadline cannot revive an expired observation or confer effect authority.
    pub(super) fn revalidate_until(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.original_deadline);
        index_clock(deadline)?;
        let sid = locron_core::windows::current_user_sid_until(deadline)?;
        ensure!(
            sid == self.sid,
            "registered index belongs to a different current SID"
        );
        self.revalidate_source_until(deadline)?;
        // Make native registration the final binding observation. It is still a
        // snapshot, not a lock against a later registry writer.
        let observed = read_snapshot_until(&sid, deadline)?;
        self.revalidate_observed_until(&observed, deadline)?;
        refuse_index_sidecars(self.source.file.normalized_path(), deadline)?;
        index_clock(deadline)
    }

    fn revalidate_observed_until(&self, observed: &Snapshot, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.original_deadline);
        index_clock(deadline)?;
        ensure!(
            observed.schema == SNAPSHOT_SCHEMA && observed.sid == self.sid,
            "registered index no longer belongs to its native account/protocol"
        );
        ensure!(
            select(observed, &self.console, self.target)? == self.registration,
            "selected portable registration changed"
        );
        index_clock(deadline)
    }

    fn revalidate_source_until(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.original_deadline);
        index_clock(deadline)?;
        let current = read_index_source_until(
            &self.registration,
            &self.console,
            &self.expected_alias,
            self.target,
            deadline,
        )?;
        ensure!(
            current.identity == self.source.identity
                && index_observe(deadline, || Ok(file_identity(&self.source.file)?))?
                    == self.source.identity
                && current.facts == self.source.facts
                && same_path(
                    text(current.location.normalized_path())?,
                    text(self.source.location.normalized_path())?
                )?
                && same_path(
                    text(current.root.normalized_path())?,
                    text(self.source.root.normalized_path())?
                )?,
            "retained portable index or ancestry no longer matches the original observation"
        );
        index_clock(deadline)
    }
}

/// Reads actual HKCU64 registration, guards its exact index and revalidates before returning.
/// expected_alias is only a row expectation; it never authenticates a real alias or image.
pub(super) fn read_registered_index_until(
    console: &Path,
    expected_alias: &Path,
    original_deadline: Instant,
) -> Result<RegisteredIndex> {
    index_clock(original_deadline)?;
    let console = maintenance_path(text(console)?)?;
    let expected_alias = maintenance_path(text(expected_alias)?)?;
    let sid = locron_core::windows::current_user_sid_until(original_deadline)?;
    let target = native_target()?;
    let registration = select(
        &read_snapshot_until(&sid, original_deadline)?,
        &console,
        target,
    )?;
    let source = read_index_source_until(
        &registration,
        &console,
        &expected_alias,
        target,
        original_deadline,
    )?;
    let observed = RegisteredIndex {
        sid,
        registration,
        console,
        expected_alias,
        target,
        source,
        original_deadline,
    };
    observed.revalidate_until(original_deadline)?;
    index_clock(original_deadline)?;
    Ok(observed)
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

#[cfg(test)]
mod registered_index_tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    use locron_core::filesystem::{create_private_new_exclusive, open_private_exclusive};
    use rusqlite::{Connection, MAIN_DB, params};

    use super::super::windows_fixture::PrivateFixture;
    use super::*;

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    fn sqlite_bytes(root: &str, alias: &str) -> Vec<u8> {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(
                "CREATE TABLE metadata(name TEXT PRIMARY KEY NOT NULL,value TEXT NOT NULL) WITHOUT ROWID;
             CREATE TABLE portable(filepath TEXT NOT NULL UNIQUE COLLATE NOCASE,filetype INT64 NOT NULL,sha256 BLOB,symlinktarget TEXT);
             INSERT INTO metadata VALUES('majorVersion','1'),('minorVersion','0');",
            )
            .unwrap();
        database
            .execute("INSERT INTO portable VALUES(?1,2,'','')", [root])
            .unwrap();
        database
            .execute(
                "INSERT INTO portable VALUES(?1,3,'',?2)",
                params![alias, format!("{root}\\locron.exe")],
            )
            .unwrap();
        database.serialize(MAIN_DB).unwrap().to_vec()
    }

    struct Fixture {
        registration: Registration,
        console: String,
        alias: String,
        root: PathBuf,
        index: PathBuf,
        _private: PrivateFixture,
    }

    impl Fixture {
        fn new() -> Self {
            let private = PrivateFixture::new("locron-registered-index-");
            let target = native_target().unwrap();
            let root = private.path().join(format!("locron-v0.10.0-{target}"));
            drop(DirectoryGuard::private(&root).unwrap());
            let console = text(&root.join("locron.exe")).unwrap().to_owned();
            // No actual alias or executable is created: this is index metadata,
            // and successful source parsing must not imply those objects exist.
            let alias = text(&private.path().join("links").join("locron.exe"))
                .unwrap()
                .to_owned();
            let registration = Registration {
                key: "WhiteKiwi.locron_fixture".into(),
                package_id: "WhiteKiwi.locron".into(),
                installer_type: "portable".into(),
                source_id: "fixture-source".into(),
                version: "0.10.0".into(),
                install_location: text(private.path()).unwrap().to_owned(),
                legacy_path: None,
            };
            let index = private.path().join(format!("{}.db", registration.key));
            let mut file = create_private_new_exclusive(&index).unwrap();
            file.write_all(&sqlite_bytes(text(&root).unwrap(), &alias))
                .unwrap();
            file.sync_all().unwrap();
            drop(file);
            Self {
                registration,
                console,
                alias,
                root,
                index,
                _private: private,
            }
        }

        fn read(&self) -> Result<IndexSource> {
            read_index_source_until(
                &self.registration,
                &self.console,
                &self.alias,
                native_target()?,
                deadline(),
            )
        }

        fn replace_index(&self, bytes: &[u8]) {
            let mut file = open_private_exclusive(&self.index).unwrap();
            file.set_len(0).unwrap();
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
        }

        fn snapshot(&self) -> Snapshot {
            Snapshot {
                schema: SNAPSHOT_SCHEMA.into(),
                sid: locron_core::windows::current_user_sid().unwrap(),
                entries: vec![self.registration.clone()],
            }
        }

        // Deterministic private unit-test setup only. No live HKCU registration is
        // created; production construction always uses read_registered_index_until.
        fn observation(&self) -> RegisteredIndex {
            let original_deadline = deadline();
            RegisteredIndex {
                sid: self.snapshot().sid,
                registration: self.registration.clone(),
                console: self.console.clone(),
                expected_alias: self.alias.clone(),
                target: native_target().unwrap(),
                source: read_index_source_until(
                    &self.registration,
                    &self.console,
                    &self.alias,
                    native_target().unwrap(),
                    original_deadline,
                )
                .unwrap(),
                original_deadline,
            }
        }
    }

    const FIXTURE_DESCRIPTOR: &str = r"
if ($null -ne $request.sddl) {
    $security = [IO.File]::GetAccessControl([string]$request.path)
    $security.SetSecurityDescriptorSddlForm([string]$request.sddl,
        [Security.AccessControl.AccessControlSections]::Access)
    [IO.File]::SetAccessControl([string]$request.path, $security)
}
$security = [IO.File]::GetAccessControl([string]$request.path)
@{sddl=$security.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)} |
    & $locronToJson -Compress
";

    fn descriptor(fixture: &Fixture, replacement: Option<&str>) -> String {
        let response = locron_core::windows::run_script_json_until(
            FIXTURE_DESCRIPTOR,
            &json!({"path": fixture.index, "sddl": replacement}),
            deadline(),
        )
        .unwrap();
        response["sddl"].as_str().unwrap().to_owned()
    }

    #[test]
    fn actual_index_snapshot_retains_identity_and_excludes_writes_without_attesting_images() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        let source = fixture.read().unwrap();
        assert_eq!(file_identity(&source.file).unwrap(), source.identity);
        assert_eq!(source.facts.snapshot_sha256, sha256_hex(&original));
        assert_eq!(source.facts.console, fixture.console);
        assert_eq!(source.facts.console_alias, fixture.alias);
        assert!(!Path::new(&fixture.console).exists());
        assert!(!Path::new(&fixture.alias).exists());
        assert!(OpenOptions::new().write(true).open(&fixture.index).is_err());
        assert!(fs::rename(&fixture.index, fixture.index.with_extension("moved")).is_err());
        assert!(fs::remove_file(&fixture.index).is_err());
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
        drop(source);
        assert!(open_private_exclusive(&fixture.index).is_ok());
    }

    #[test]
    fn package_source_acl_is_accepted_and_broad_mutation_refuses_without_repair() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        let sid = locron_core::windows::current_user_sid().unwrap();
        let accepted = format!("O:{sid}D:P(A;;FA;;;{sid})(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;WD)");
        let before = descriptor(&fixture, Some(&accepted));
        let source = fixture.read().unwrap();
        assert_eq!(descriptor(&fixture, None), before);
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
        drop(source);
        let unsafe_acl = format!("O:{sid}D:P(A;;FA;;;{sid})(A;;FA;;;SY)(A;;FW;;;WD)");
        let before = descriptor(&fixture, Some(&unsafe_acl));
        assert!(fixture.read().is_err());
        assert_eq!(descriptor(&fixture, None), before);
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
    }

    #[test]
    fn missing_index_and_version_root_remain_absent() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        fs::remove_dir(&fixture.root).unwrap();
        assert!(fixture.read().is_err());
        assert!(!fixture.root.exists());
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
        drop(DirectoryGuard::private(&fixture.root).unwrap());
        fs::remove_file(&fixture.index).unwrap();
        assert!(fixture.read().is_err());
        assert!(!fixture.index.exists());
    }

    #[test]
    fn oversized_live_index_and_existing_writer_refuse() {
        let fixture = Fixture::new();
        let writer = open_private_exclusive(&fixture.index).unwrap();
        assert!(fixture.read().is_err());
        writer.set_len(SNAPSHOT_LIMIT as u64 + 1).unwrap();
        writer.sync_all().unwrap();
        drop(writer);
        assert!(fixture.read().is_err());
        assert_eq!(
            fs::metadata(&fixture.index).unwrap().len(),
            SNAPSHOT_LIMIT as u64 + 1
        );
    }

    #[test]
    fn actual_sidecars_refuse_at_initial_read_and_retained_source_revalidation() {
        let fixture = Fixture::new();
        let observed = fixture.observation();
        let original = fs::read(&fixture.index).unwrap();
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut sidecar = fixture.index.as_os_str().to_os_string();
            sidecar.push(suffix);
            let path = Path::new(&sidecar);
            let mut file = create_private_new_exclusive(path).unwrap();
            file.write_all(b"test-owned sidecar").unwrap();
            drop(file);
            assert!(fixture.read().is_err(), "accepted {suffix}");
            assert!(
                observed.revalidate_source_until(deadline()).is_err(),
                "accepted {suffix}"
            );
            assert_eq!(fs::read(path).unwrap(), b"test-owned sidecar");
            assert_eq!(fs::read(&fixture.index).unwrap(), original);
            fs::remove_file(path).unwrap();
        }
        observed.revalidate_source_until(deadline()).unwrap();
    }

    #[test]
    fn live_file_bytes_are_parsed_with_the_existing_schema_root_and_wal_rules() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        let stale = sqlite_bytes(&format!("{}-stale", fixture.root.display()), &fixture.alias);
        fixture.replace_index(&stale);
        assert!(fixture.read().is_err());
        assert_eq!(fs::read(&fixture.index).unwrap(), stale);
        let mut wal = original;
        wal[18] = 2;
        fixture.replace_index(&wal);
        assert!(fixture.read().is_err());
        assert_eq!(fs::read(&fixture.index).unwrap(), wal);
    }

    #[test]
    fn registered_observation_rejects_changed_sid_reference_and_ambiguous_selection() {
        let fixture = Fixture::new();
        let observed = fixture.observation();
        observed
            .revalidate_observed_until(&fixture.snapshot(), deadline())
            .unwrap();
        let mut changed = fixture.snapshot();
        changed.sid = "S-1-5-18".into();
        if changed.sid == observed.sid {
            changed.sid = "S-1-5-19".into();
        }
        assert!(
            observed
                .revalidate_observed_until(&changed, deadline())
                .is_err()
        );
        let mut changed = fixture.snapshot();
        changed.entries[0].source_id = "a-different-source".into();
        assert!(
            observed
                .revalidate_observed_until(&changed, deadline())
                .is_err()
        );
        changed.entries.clear();
        assert!(
            observed
                .revalidate_observed_until(&changed, deadline())
                .is_err()
        );
        let mut changed = fixture.snapshot();
        changed.entries.push(changed.entries[0].clone());
        assert!(
            observed
                .revalidate_observed_until(&changed, deadline())
                .is_err()
        );
        assert!(OpenOptions::new().write(true).open(&fixture.index).is_err());
    }

    #[test]
    fn expiration_cannot_be_extended_and_late_observations_keep_the_original_guard() {
        let fixture = Fixture::new();
        assert!(
            read_registered_index_until(
                Path::new(&fixture.console),
                Path::new(&fixture.alias),
                Instant::now()
            )
            .is_err()
        );
        let mut observed = fixture.observation();
        observed.original_deadline = Instant::now();
        assert!(observed.revalidate_until(deadline()).is_err());
        let short = Instant::now() + Duration::from_millis(10);
        assert!(
            index_observe(short, || {
                file_identity(&observed.source.file)?;
                std::thread::sleep(
                    short.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                );
                Ok(())
            })
            .is_err()
        );
        assert!(OpenOptions::new().write(true).open(&fixture.index).is_err());
        drop(observed);
        assert!(open_private_exclusive(&fixture.index).is_ok());
    }

    #[test]
    fn actual_native_registration_reader_cannot_adopt_unregistered_fixture_metadata() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        assert!(fixture.read().is_ok());
        assert!(
            read_registered_index_until(
                Path::new(&fixture.console),
                Path::new(&fixture.alias),
                deadline()
            )
            .is_err()
        );
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
        assert!(!Path::new(&fixture.console).exists());
        assert!(!Path::new(&fixture.alias).exists());
    }

    #[test]
    fn version_root_junction_refuses_without_following_or_repairing_it() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.index).unwrap();
        fs::remove_dir(&fixture.root).unwrap();
        let target = fixture.index.parent().unwrap().join("separate-target");
        drop(DirectoryGuard::private(&target).unwrap());
        locron_core::windows::run_script_json_until(
            "New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null; @{created=$true} | & $locronToJson -Compress",
            &json!({"link": fixture.root, "target": target}),
            deadline(),
        )
        .unwrap();
        let refused = fixture.read().is_err();
        // Delete only the fixture-owned junction before any assertion/TempDir cleanup.
        locron_core::windows::run_script_json_until(
            "[IO.Directory]::Delete([string]$request.link); @{removed=$true} | & $locronToJson -Compress",
            &json!({"link": fixture.root}),
            deadline(),
        )
        .unwrap();
        assert!(refused);
        assert!(target.is_dir());
        assert_eq!(fs::read(&fixture.index).unwrap(), original);
    }
}
