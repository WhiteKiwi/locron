//! Strict requests and truthful status for the retained Windows operation helper.

use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::windows_receipt::{
    local_path, required_nullable, same_path, stable_version, valid_hash,
};

const LIMIT: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Install,
    SelfUpdate,
    Uninstall,
    Recover,
    MaintenancePrepare,
    MaintenanceComplete,
    MaintenanceRemove,
}

impl Kind {
    pub(super) fn maintenance(self) -> bool {
        matches!(
            self,
            Self::MaintenancePrepare | Self::MaintenanceComplete | Self::MaintenanceRemove
        )
    }
}

fn operation_uuid<'de, D>(deserializer: D) -> std::result::Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    let spelling = String::deserialize(deserializer)?;
    let id = Uuid::parse_str(&spelling).map_err(serde::de::Error::custom)?;
    if id.is_nil() || spelling != id.hyphenated().to_string() {
        return Err(serde::de::Error::custom(
            "expected a canonical nonzero operation UUID",
        ));
    }
    Ok(id)
}

/// A registration reference, never an authority to execute arbitrary task source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PackageBinding {
    pub key: String,
    pub package_id: String,
    pub source_id: String,
    pub install_location: String,
    pub executable: String,
    pub version: String,
    pub target: String,
    pub binary_sha256: String,
    pub archive_sha256: String,
}

impl PackageBinding {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.package_id == "WhiteKiwi.locron",
            "foreign package identifier"
        );
        ensure!(
            !self.key.is_empty()
                && self.key.len() <= 255
                && !self.key.chars().any(char::is_control)
                && !self.key.contains(['\\', '/'])
                && !self.source_id.is_empty()
                && self.source_id.len() <= 255
                && !self.source_id.chars().any(char::is_control),
            "invalid package registration/source reference"
        );
        stable_version(&self.version)?;
        native_target(&self.target)?;
        let location = local_path(&self.install_location)?;
        let nested = format!(
            "{location}\\locron-v{}-{}\\locron.exe",
            self.version, self.target
        );
        ensure!(
            same_path(&self.executable, &nested)?
                || same_path(&self.executable, &format!("{location}\\locron.exe"))?,
            "package executable escapes its registered location"
        );
        ensure!(
            valid_hash(&self.binary_sha256) && valid_hash(&self.archive_sha256),
            "invalid canonical package digests"
        );
        Ok(())
    }
}

pub(super) fn native_target(target: &str) -> Result<()> {
    ensure!(
        matches!(target, "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"),
        "unsupported native Windows target"
    );
    Ok(())
}

/// Only this fixed protocol crosses the bootstrap/helper boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub schema: String,
    #[serde(deserialize_with = "operation_uuid")]
    pub operation_id: Uuid,
    pub kind: Kind,
    pub sid: String,
    pub executable: String,
    pub target: String,
    pub version: String,
    pub archive_sha256: String,
    pub helper_sha256: String,
    #[serde(deserialize_with = "required_nullable")]
    pub caller_pid: Option<u32>,
    pub no_service: bool,
    pub dashboard: bool,
    pub add_to_path: bool,
    #[serde(deserialize_with = "required_nullable")]
    pub state_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_file: Option<String>,
}

impl Request {
    pub(super) fn parse(bytes: &[u8], sid: &str, target: &str, operation: Uuid) -> Result<Self> {
        ensure!(
            bytes.len() <= LIMIT,
            "operation request exceeds its size limit"
        );
        let request: Self = serde_json::from_slice(bytes)?;
        request.validate(sid, target, operation)?;
        Ok(request)
    }

    pub(super) fn validate(&self, sid: &str, target: &str, operation: Uuid) -> Result<()> {
        ensure!(
            self.schema == "locron.windows-operation/v1"
                && !operation.is_nil()
                && self.operation_id == operation
                && self.sid == sid,
            "operation does not authorize this user/request directory"
        );
        native_target(target)?;
        ensure!(
            self.target == target,
            "operation is not for this native architecture"
        );
        stable_version(&self.version)?;
        let executable = local_path(&self.executable)?;
        ensure!(
            executable
                .rsplit('\\')
                .next()
                .is_some_and(|leaf| leaf.eq_ignore_ascii_case("locron.exe")),
            "operation does not select locron.exe"
        );
        ensure!(
            valid_hash(&self.archive_sha256) && valid_hash(&self.helper_sha256),
            "invalid operation digests"
        );
        ensure!(
            self.caller_pid != Some(0),
            "invalid caller process identifier"
        );
        if let Some(root) = &self.state_root {
            local_path(root)?;
        }
        ensure!(
            !(self.no_service && self.dashboard),
            "dashboard conflicts with disabled service roles"
        );
        if self.kind != Kind::Install {
            ensure!(
                !self.no_service
                    && !self.dashboard
                    && !self.add_to_path
                    && self.state_root.is_none(),
                "operation accepts no new-install options"
            );
        }
        if self.add_to_path {
            ensure!(
                !executable.contains(';'),
                "selected path cannot be represented in PATH"
            );
        }
        ensure!(
            self.package.is_some() == self.kind.maintenance(),
            "operation/package channel mismatch"
        );
        if let Some(package) = &self.package {
            package.validate()?;
            ensure!(
                same_path(&package.executable, &self.executable)?
                    && package.target == self.target
                    && package.version == self.version
                    && package.archive_sha256 == self.archive_sha256,
                "operation differs from its package registration binding"
            );
        }
        match &self.archive_file {
            None => {}
            Some(name) => {
                ensure!(
                    self.kind == Kind::MaintenanceComplete,
                    "only package completion selects a fresh staged archive"
                );
                let id = name
                    .strip_prefix("archive-")
                    .and_then(|name| name.strip_suffix(".zip"))
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .filter(|id| !id.is_nil())
                    .ok_or_else(|| anyhow::anyhow!("invalid staged archive name"))?;
                ensure!(
                    name == &format!("archive-{}.zip", id.hyphenated()),
                    "noncanonical staged archive name"
                );
            }
        }
        ensure!(
            self.kind != Kind::MaintenanceComplete || self.archive_file.is_some(),
            "package completion requires its new staged archive"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Accepted,
    Quiescing,
    Replacing,
    Restoring,
    Completed,
    Prepared,
    Removed,
    Failed,
    RolledBack,
}

impl Phase {
    pub(super) fn pending(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Quiescing | Self::Replacing | Self::Restoring
        )
    }
}

/// Read-only status cannot authorize effects or turn acceptance into completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Status {
    pub schema: String,
    #[serde(deserialize_with = "operation_uuid")]
    pub operation_id: Uuid,
    pub sid: String,
    pub executable: String,
    pub phase: Phase,
    pub current_version: String,
    pub new_version: String,
    pub updated: bool,
    pub prepared: bool,
    pub warnings: Vec<String>,
}

impl Status {
    pub(super) fn parse(bytes: &[u8], request: &Request) -> Result<Self> {
        ensure!(
            bytes.len() <= LIMIT,
            "operation status exceeds its size limit"
        );
        let status: Self = serde_json::from_slice(bytes)?;
        ensure!(
            status.schema == "locron.windows-status/v1"
                && status.operation_id == request.operation_id
                && status.sid == request.sid
                && same_path(&status.executable, &request.executable)?,
            "status differs from its owned operation request"
        );
        if !status.current_version.is_empty() {
            stable_version(&status.current_version)?;
        }
        stable_version(&status.new_version)?;
        ensure!(
            status.new_version == request.version,
            "status release differs from its request"
        );
        ensure!(
            status.updated
                == (status.phase == Phase::Completed
                    && matches!(
                        request.kind,
                        Kind::Install | Kind::SelfUpdate | Kind::Recover
                    )),
            "status fabricates or hides confirmed replacement"
        );
        ensure!(
            status.prepared == (status.phase == Phase::Prepared),
            "status fabricates or hides confirmed preparation"
        );
        ensure!(
            status.phase != Phase::Prepared || request.kind == Kind::MaintenancePrepare,
            "standalone operation cannot report package preparation"
        );
        ensure!(
            status.phase != Phase::Removed
                || matches!(request.kind, Kind::Uninstall | Kind::MaintenanceRemove),
            "operation cannot report unrelated removal"
        );
        ensure!(
            status.warnings.len() <= 64
                && status.warnings.iter().all(|warning| {
                    warning.len() <= 2048 && !warning.chars().any(|ch| ch == '\0')
                }),
            "operation warning inventory exceeds its limit"
        );
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const TARGET: &str = "x86_64-pc-windows-msvc";
    const OPERATION: &str = "e41c210d-c98d-47fb-9975-a5af66d01346";

    fn request() -> Value {
        json!({
            "schema": "locron.windows-operation/v1", "operation_id": OPERATION,
            "kind": "install", "sid": SID, "executable": r"C:\private\locron.exe",
            "target": TARGET, "version": "0.10.0", "archive_sha256": "ab".repeat(32),
            "helper_sha256": "cd".repeat(32), "caller_pid": null,
            "no_service": false, "dashboard": false, "add_to_path": false, "state_root": null,
        })
    }

    fn parse(value: &Value) -> Result<Request> {
        Request::parse(
            &serde_json::to_vec(value)?,
            SID,
            TARGET,
            Uuid::parse_str(OPERATION)?,
        )
    }

    #[test]
    fn request_refuses_ambiguous_fields_foreign_channel_and_architecture() {
        assert!(parse(&request()).is_ok());
        for (field, bad) in [
            ("schema", json!("locron.windows-operation/v0")),
            ("sid", json!("S-1-5-21-1-2-3-1002")),
            ("target", json!("aarch64-pc-windows-msvc")),
            ("kind", json!("arbitrary_task")),
            ("executable", json!(r"C:\private\locron.exe:stream")),
            ("executable", json!(r"C:\private\other.exe")),
            ("version", json!("0.9.6")),
            ("operation_id", json!(OPERATION.to_ascii_uppercase())),
            ("operation_id", json!(Uuid::nil().to_string())),
            ("caller_pid", json!(0)),
            ("caller_pid", json!("123")),
            ("state_root", json!(r"\\remote\state")),
        ] {
            let mut value = request();
            value[field] = bad;
            assert!(parse(&value).is_err(), "{field}: {value}");
        }
        for field in ["caller_pid", "state_root", "helper_sha256", "dashboard"] {
            let mut value = request();
            value.as_object_mut().unwrap().remove(field);
            assert!(parse(&value).is_err(), "missing {field}");
        }
        let mut value = request();
        value["script"] = json!("malicious.ps1");
        assert!(parse(&value).is_err());
        let bytes = serde_json::to_string(&request()).unwrap().replacen(
            "\"kind\":\"install\"",
            "\"kind\":\"install\",\"k\\u0069nd\":\"uninstall\"",
            1,
        );
        assert!(
            Request::parse(
                bytes.as_bytes(),
                SID,
                TARGET,
                Uuid::parse_str(OPERATION).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn package_completion_refuses_stale_location_source_and_archive_paths() {
        let mut value = request();
        value["kind"] = json!("maintenance_complete");
        value["executable"] = json!(format!(r"C:\WinGet\locron-v0.10.0-{TARGET}\locron.exe"));
        value["archive_file"] = json!(format!("archive-{OPERATION}.zip"));
        value["package"] = json!({
            "key": "WhiteKiwi.locron_source", "package_id": "WhiteKiwi.locron", "source_id": "winget-source",
            "install_location": r"C:\WinGet", "executable": value["executable"], "version": "0.10.0", "target": TARGET,
            "binary_sha256": "ef".repeat(32), "archive_sha256": "ab".repeat(32),
        });
        assert!(parse(&value).is_ok());
        for (field, bad) in [
            ("source_id", json!("")),
            ("package_id", json!("Other.locron")),
            ("key", json!(r"..\Other")),
            ("install_location", json!(r"C:\Other")),
            ("version", json!("0.10.1")),
            ("archive_sha256", json!("00".repeat(32))),
        ] {
            let mut changed = value.clone();
            changed["package"][field] = bad;
            assert!(parse(&changed).is_err(), "{field}");
        }
        for bad in [
            "archive.zip",
            "../archive.zip",
            "C:\\archive.zip",
            "archive-invalid.zip",
        ] {
            let mut changed = value.clone();
            changed["archive_file"] = json!(bad);
            assert!(parse(&changed).is_err());
        }
        let mut changed = value;
        changed["no_service"] = json!(true);
        assert!(parse(&changed).is_err());
    }

    #[test]
    fn status_never_turns_pending_preparation_or_removal_into_replacement() {
        let request = parse(&request()).unwrap();
        let value = json!({
            "schema": "locron.windows-status/v1", "operation_id": OPERATION, "sid": SID,
            "executable": request.executable, "phase": "accepted", "current_version": "",
            "new_version": "0.10.0", "updated": false, "prepared": false, "warnings": [],
        });
        let status = Status::parse(&serde_json::to_vec(&value).unwrap(), &request).unwrap();
        assert!(status.phase.pending());
        for (phase, updated, prepared, succeeds) in [
            ("accepted", true, false, false),
            ("completed", false, false, false),
            ("completed", true, false, true),
            ("prepared", false, true, false),
            ("removed", false, false, false),
            ("failed", false, false, true),
            ("rolled_back", true, false, false),
        ] {
            let mut changed = value.clone();
            changed["phase"] = json!(phase);
            changed["updated"] = json!(updated);
            changed["prepared"] = json!(prepared);
            assert_eq!(
                Status::parse(&serde_json::to_vec(&changed).unwrap(), &request).is_ok(),
                succeeds,
                "{phase}"
            );
        }
        for field in ["prepared", "warnings"] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(Status::parse(&serde_json::to_vec(&changed).unwrap(), &request).is_err());
        }
        let mut changed = value;
        changed["executable"] = json!(r"C:\other\locron.exe");
        assert!(Status::parse(&serde_json::to_vec(&changed).unwrap(), &request).is_err());
    }
}
