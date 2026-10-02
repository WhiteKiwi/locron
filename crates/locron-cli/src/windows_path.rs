//! Read-only persistent PATH facts and pure conditional insertion/rollback plans.
//! No registry writer or effect authority is supplied by this module.

use std::path::Path;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::windows_receipt::{
    PathKind, UserPath, appended_path, local_path, required_nullable, same_path,
};

const INPUT_LIMIT: usize = 64 * 1024;
const OUTPUT_LIMIT: usize = 128 * 1024;
const QUERY_SCHEMA: &str = "locron.windows-path-query/v1";
const SNAPSHOT_SCHEMA: &str = "locron.windows-path-snapshot/v1";
const WRITE_SCHEMA: &str = "locron.windows-path-write/v1";

// Read existing HKCU state only, preserving absent/empty and unexpanded strings.
const READ_PATH: &str = r"
if ($request.schema -cne 'locron.windows-path-query/v1') { throw 'invalid PATH query' }
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if ($request.sid -cne $sid) { throw 'PATH query belongs to another account' }
$value = $null
$kind = $null
$hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,
    [Microsoft.Win32.RegistryView]::Registry64)
try {
    $key = $hive.OpenSubKey('Environment', $false)
    if ($null -ne $key) {
        try {
            if ($key.GetValueNames() -contains 'Path') {
                $storedKind = $key.GetValueKind('Path')
                if ($storedKind -ne [Microsoft.Win32.RegistryValueKind]::String -and
                    $storedKind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) {
                    throw 'persistent PATH must be a string value'
                }
                $kind = $storedKind.ToString()
                $value = $key.GetValue('Path', $null,
                    [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                if ($value -isnot [string]) { throw 'persistent PATH changed type during read' }
                if ($key.GetValueKind('Path') -ne $storedKind) { throw 'persistent PATH kind changed during read' }
            }
        } finally { $key.Dispose() }
    }
} finally { $hive.Dispose() }
[ordered]@{ schema = 'locron.windows-path-snapshot/v1'; sid = $sid; value = $value; kind = $kind } |
    ConvertTo-Json -Compress -Depth 4
";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PathValue {
    #[serde(deserialize_with = "required_nullable")]
    pub value: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub kind: Option<PathKind>,
}

impl PathValue {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.value.is_some() == self.kind.is_some(),
            "persistent PATH confuses an absent and present value"
        );
        ensure!(
            self.value
                .as_ref()
                .is_none_or(|value| !value.contains('\0')),
            "persistent PATH contains a NUL"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= OUTPUT_LIMIT,
            "persistent PATH exceeds the fixed reader bound"
        );
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: String,
    sid: String,
    #[serde(deserialize_with = "required_nullable")]
    value: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    kind: Option<PathKind>,
}

fn parse_snapshot(value: &Value, sid: &str) -> Result<PathValue> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= OUTPUT_LIMIT,
        "PATH reader exceeded its bound"
    );
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    ensure!(
        snapshot.schema == SNAPSHOT_SCHEMA && snapshot.sid == sid,
        "PATH snapshot belongs to another account/protocol"
    );
    let value = PathValue {
        value: snapshot.value,
        kind: snapshot.kind,
    };
    value.validate()?;
    Ok(value)
}

pub(super) fn read_current() -> Result<PathValue> {
    let sid = locron_core::windows::current_user_sid()?;
    let value = locron_core::windows::run_script_json(
        READ_PATH,
        &json!({"schema":QUERY_SCHEMA,"sid":sid}),
    )?;
    parse_snapshot(&value, &sid)
}

/// Complete precondition and desired value for a later fixed, journaled adapter.
#[derive(Serialize)]
pub(super) struct WriteIntent {
    schema: &'static str,
    sid: String,
    pub expected: PathValue,
    pub desired: PathValue,
}

impl WriteIntent {
    fn new(sid: &str, expected: PathValue, desired: PathValue) -> Result<Self> {
        ensure!(
            !sid.is_empty() && sid.len() <= 184 && sid.is_ascii(),
            "PATH intent account is invalid"
        );
        expected.validate()?;
        desired.validate()?;
        let intent = Self {
            schema: WRITE_SCHEMA,
            sid: sid.to_owned(),
            expected,
            desired,
        };
        ensure!(
            serde_json::to_vec(&intent)?.len() <= INPUT_LIMIT,
            "complete conditional PATH intent exceeds the fixed adapter input bound"
        );
        Ok(intent)
    }
}

pub(super) struct Insertion {
    pub intent: Option<WriteIntent>,
    pub ownership: Option<UserPath>,
}

/// A spelling comparison cannot grant ownership of an existing PATH field.
pub(super) fn plan_insertion(
    current: &PathValue,
    directory: &str,
    previous: Option<&UserPath>,
    sid: &str,
) -> Result<Insertion> {
    current.validate()?;
    let directory = local_path(directory)?;
    ensure!(
        !directory.contains(';'),
        "directory cannot be one PATH field"
    );
    let kind = current.kind.unwrap_or(PathKind::String);
    ensure!(
        kind != PathKind::ExpandString || !directory.contains('%'),
        "literal directory would expand in the existing PATH format"
    );
    if let Some(record) = previous {
        record.validate(&directory)?;
    }
    if current.value.as_deref().is_some_and(|value| {
        value.split(';').any(|entry| {
            // Variables and unsupported syntax are never expanded or rewritten.
            !(kind == PathKind::ExpandString && entry.contains('%'))
                && same_path(entry, &directory).unwrap_or(false)
        })
    }) {
        return Ok(Insertion {
            intent: None,
            ownership: previous.cloned(),
        });
    }
    let after = appended_path(current.value.as_deref(), &directory);
    let record = UserPath {
        before: current.value.clone(),
        after: after.clone(),
        before_kind: current.kind,
        after_kind: kind,
    };
    record.validate(&directory)?;
    let intent = WriteIntent::new(
        sid,
        current.clone(),
        PathValue {
            value: Some(after),
            kind: Some(kind),
        },
    )?;
    Ok(Insertion {
        intent: Some(intent),
        ownership: Some(record),
    })
}

/// A concurrently edited raw value or kind is retained without a rollback write.
pub(super) fn plan_rollback(
    current: &PathValue,
    record: &UserPath,
    directory: &str,
    sid: &str,
) -> Result<Option<WriteIntent>> {
    current.validate()?;
    let directory = local_path(directory)?;
    record.validate(&directory)?;
    if current.value.as_deref() != Some(record.after.as_str())
        || current.kind != Some(record.after_kind)
    {
        return Ok(None);
    }
    Ok(Some(WriteIntent::new(
        sid,
        current.clone(),
        PathValue {
            value: record.before.clone(),
            kind: record.before_kind,
        },
    )?))
}

/// Pure path resolution; no state object is opened, created or adopted.
pub(super) fn normalize_state_override(path: &Path) -> Result<String> {
    let absolute = std::path::absolute(path)?;
    local_path(
        absolute
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("selected state directory is not Unicode"))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const DIRECTORY: &str = r"C:\test-owned\Unicode 한글";

    fn value(raw: Option<&str>, kind: Option<PathKind>) -> PathValue {
        PathValue {
            value: raw.map(str::to_owned),
            kind,
        }
    }

    #[test]
    fn insertion_preserves_raw_fields_kind_absence_empty_and_trailing_separator() {
        for (raw, kind, expected) in [
            (None, None, DIRECTORY.to_owned()),
            (Some(""), Some(PathKind::String), DIRECTORY.to_owned()),
            (
                Some(r"%USERPROFILE%\bin"),
                Some(PathKind::ExpandString),
                format!(r"%USERPROFILE%\bin;{DIRECTORY}"),
            ),
            (
                Some(r"C:\other;"),
                Some(PathKind::String),
                format!(r"C:\other;{DIRECTORY}"),
            ),
            (
                Some(r#""C:\quoted";relative;%VARIABLE%;"#),
                Some(PathKind::String),
                format!(r#""C:\quoted";relative;%VARIABLE%;{DIRECTORY}"#),
            ),
        ] {
            let current = value(raw, kind);
            let plan = plan_insertion(&current, DIRECTORY, None, SID).unwrap();
            let intent = plan.intent.unwrap();
            assert_eq!(intent.expected, current);
            assert_eq!(intent.desired.value.as_deref(), Some(expected.as_str()));
            assert_eq!(intent.desired.kind, Some(kind.unwrap_or(PathKind::String)));
            let rollback = plan_rollback(&intent.desired, &plan.ownership.unwrap(), DIRECTORY, SID)
                .unwrap()
                .unwrap();
            assert_eq!(rollback.desired, current);
        }
    }

    #[test]
    fn literal_duplicates_do_not_create_new_ownership_and_previous_claim_is_preserved() {
        let current = value(
            Some(&format!(r"C:\other;{}\", DIRECTORY.to_ascii_lowercase())),
            Some(PathKind::String),
        );
        let plan = plan_insertion(&current, DIRECTORY, None, SID).unwrap();
        assert!(plan.intent.is_none() && plan.ownership.is_none());
        let original = value(None, None);
        let first = plan_insertion(&original, DIRECTORY, None, SID).unwrap();
        let record = first.ownership.unwrap();
        let current = first.intent.unwrap().desired;
        let repeated = plan_insertion(&current, DIRECTORY, Some(&record), SID).unwrap();
        assert!(repeated.intent.is_none());
        assert_eq!(repeated.ownership.unwrap(), record);
    }

    #[test]
    fn percent_semicolon_unsupported_format_and_adapter_overflow_refuse_before_effects() {
        let percent = r"C:\literal%name\locron";
        let string = value(Some(r"C:\other"), Some(PathKind::String));
        assert!(plan_insertion(&string, percent, None, SID).is_ok());
        assert!(
            plan_insertion(
                &value(Some(percent), Some(PathKind::String)),
                percent,
                None,
                SID
            )
            .unwrap()
            .intent
            .is_none()
        );
        let expand = value(Some(r"%USERPROFILE%\bin"), Some(PathKind::ExpandString));
        assert!(plan_insertion(&expand, percent, None, SID).is_err());
        assert!(plan_insertion(&string, r"C:\a;b", None, SID).is_err());
        assert!(
            plan_insertion(&value(None, Some(PathKind::String)), DIRECTORY, None, SID).is_err()
        );
        assert!(
            plan_insertion(
                &value(Some("a\0b"), Some(PathKind::String)),
                DIRECTORY,
                None,
                SID
            )
            .is_err()
        );
        let oversize = value(Some(&"x".repeat(INPUT_LIMIT)), Some(PathKind::String));
        assert!(plan_insertion(&oversize, DIRECTORY, None, SID).is_err());
        assert!(serde_json::from_value::<PathValue>(json!({"value":"x","kind":"Binary"})).is_err());
        assert!(serde_json::from_value::<PathValue>(json!({"value":null})).is_err());
    }

    #[test]
    fn rollback_refuses_an_intervening_raw_or_kind_edit_and_state_normalization_creates_nothing() {
        let initial = value(Some(r"%USERPROFILE%\bin"), Some(PathKind::ExpandString));
        let plan = plan_insertion(&initial, DIRECTORY, None, SID).unwrap();
        let record = plan.ownership.unwrap();
        let current = plan.intent.unwrap().desired;
        for edited in [
            value(Some(&format!("{};edited", record.after)), current.kind),
            value(current.value.as_deref(), Some(PathKind::String)),
            value(None, None),
        ] {
            assert!(
                plan_rollback(&edited, &record, DIRECTORY, SID)
                    .unwrap()
                    .is_none()
            );
        }
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-state");
        let normalized = normalize_state_override(&missing).unwrap();
        assert!(same_path(&normalized, missing.to_str().unwrap()).unwrap());
        assert!(!missing.exists());
        assert!(normalize_state_override(Path::new(r"\\remote\share\state")).is_err());
    }

    #[test]
    fn native_readonly_snapshot_is_strict_and_binds_the_current_sid() {
        let sid = locron_core::windows::current_user_sid().unwrap();
        read_current().unwrap().validate().unwrap();
        let foreign_sid = if sid == "S-1-5-18" {
            "S-1-5-19"
        } else {
            "S-1-5-18"
        };
        assert!(
            locron_core::windows::run_script_json(
                READ_PATH,
                &json!({"schema":QUERY_SCHEMA,"sid":foreign_sid})
            )
            .is_err()
        );
        for (field, replacement) in [
            ("schema", json!("foreign")),
            ("sid", json!("foreign")),
            ("kind", json!("Binary")),
            ("value", json!(1)),
        ] {
            let mut snapshot = json!({"schema":SNAPSHOT_SCHEMA,"sid":sid,"value":null,"kind":null});
            snapshot[field] = replacement;
            assert!(parse_snapshot(&snapshot, &sid).is_err());
        }
        let mut snapshot = json!({"schema":SNAPSHOT_SCHEMA,"sid":sid,"value":null,"kind":null});
        snapshot["unexpected"] = true.into();
        assert!(parse_snapshot(&snapshot, &sid).is_err());
    }
}
