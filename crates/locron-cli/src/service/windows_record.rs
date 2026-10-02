//! Typed maintenance facts and a pure complete-record capacity reservation.
//!
//! Validation here cannot establish live ownership. Effectful adapters must reconstruct existing
//! directory/task guards and verify object identities before accepting any saved transition.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{ServiceError, Target};

const RECORD_VERSION: u8 = 1;
const ROLE_LIMIT: usize = 256;
const RECORD_LIMIT: usize = 128 * 1024;
const PATH_UNITS: usize = 4096;
const FUTURE_PATH_JSON_BYTES: usize = 3 * PATH_UNITS + 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutableBinding {
    pub(super) path: String,
    pub(super) volume: String,
    pub(super) file: String,
}

impl ExecutableBinding {
    pub(super) fn guarded(
        file: &locron_core::filesystem::GuardedFile,
    ) -> Result<Self, ServiceError> {
        let path = file
            .normalized_path()
            .to_str()
            .ok_or_else(|| invalid("maintenance paths require valid Unicode"))?
            .to_owned();
        validate_path(&path)?;
        let identity = locron_core::filesystem::file_identity(file)
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(Self {
            path,
            volume: format!("{:016x}", identity.volume_serial_number),
            file: format!("{:032x}", identity.file_id),
        })
    }

    fn validate(&self) -> Result<(), ServiceError> {
        validate_path(&self.path)?;
        validate_hex(&self.volume, 16)?;
        validate_hex(&self.file, 32)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RestorePhase {
    Snapshot,
    Quiescing,
    Quiescent,
    Restoring,
    Restored,
    Removing,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RoleProgress {
    Original,
    DisableIntent,
    Disabled,
    RefreshIntent,
    Refreshed,
    EnableIntent,
    Restored,
    DeleteIntent,
    Removed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RestoreRole {
    pub(super) role: Target,
    pub(super) root: String,
    pub(super) instance: String,
    pub(super) task_name: String,
    pub(super) enabled: bool,
    pub(super) definition: String,
    pub(super) future_definition: Option<String>,
    pub(super) progress: RoleProgress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ForcedPhase {
    StopRequested,
    ExitConfirmed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ForcedInstance {
    pub(super) role_index: u16,
    pub(super) instance: String,
    pub(super) phase: ForcedPhase,
}

/// A private-journal value, never executable task source or live ownership authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceRestoreRecord {
    pub(super) version: u8,
    pub(super) sid: String,
    pub(super) previous: ExecutableBinding,
    pub(super) roles: Vec<RestoreRole>,
    pub(super) phase: RestorePhase,
    pub(super) next: Option<ExecutableBinding>,
    pub(super) forced: Vec<ForcedInstance>,
}

/// Maximum repeated service slot and finite callback counts for complete journal preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ServicePersistencePlan {
    pub(crate) max_record_bytes: usize,
    pub(crate) quiesce_callbacks: usize,
    pub(crate) restore_callbacks: usize,
    pub(crate) remove_callbacks: usize,
}

fn invalid(message: &str) -> ServiceError {
    ServiceError::Io(message.to_owned())
}

fn validate_hex(value: &str, width: usize) -> Result<(), ServiceError> {
    if value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(invalid(
            "maintenance identities require fixed-width lowercase hexadecimal",
        ))
    }
}

fn validate_sid(value: &str) -> Result<(), ServiceError> {
    let mut parts = value.split('-');
    if value.len() > 184 || parts.next() != Some("S") || parts.next() != Some("1") {
        return Err(invalid("maintenance SID is not canonical"));
    }
    let authority = parts
        .next()
        .ok_or_else(|| invalid("maintenance SID has no authority"))?;
    let authority_number = authority
        .parse::<u64>()
        .map_err(|_| invalid("maintenance SID authority is invalid"))?;
    if authority_number >= 1 << 48 || authority_number.to_string() != authority {
        return Err(invalid("maintenance SID authority is not canonical"));
    }
    let mut count = 0;
    for part in parts {
        let number = part
            .parse::<u32>()
            .map_err(|_| invalid("maintenance SID subauthority is invalid"))?;
        if number.to_string() != part {
            return Err(invalid("maintenance SID subauthority is not canonical"));
        }
        count += 1;
    }
    if !(1..=15).contains(&count) {
        return Err(invalid("maintenance SID subauthority count is invalid"));
    }
    Ok(())
}

/// Pure syntax boundary. Live adapters separately verify retained existing path identity.
pub(super) fn validate_path(value: &str) -> Result<(), ServiceError> {
    if value.encode_utf16().count() > PATH_UNITS || value.chars().any(|ch| ch <= '\u{1f}') {
        return Err(invalid(
            "maintenance path exceeds 4096 UTF-16 units or contains controls",
        ));
    }
    let drive = value.strip_prefix(r"\\?\").unwrap_or(value);
    let bytes = drive.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1..3] != *b":\\" {
        return Err(invalid(
            "maintenance requires a normalized absolute local drive path",
        ));
    }
    let tail = &drive[3..];
    if !tail.is_empty()
        && tail.split('\\').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component
                    .chars()
                    .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/'))
        })
    {
        return Err(invalid("maintenance path has an unsupported component"));
    }
    Ok(())
}

pub(super) fn task_name(instance: &str, role: Target) -> String {
    let role = match role {
        Target::Daemon => "daemon",
        Target::Dashboard => "dashboard",
    };
    format!("Locron-v1-{instance}-{role}")
}

impl ServiceRestoreRecord {
    pub(super) fn snapshot(
        sid: String,
        previous: ExecutableBinding,
        roles: Vec<RestoreRole>,
    ) -> Result<Self, ServiceError> {
        let record = Self {
            version: RECORD_VERSION,
            sid,
            previous,
            roles,
            phase: RestorePhase::Snapshot,
            next: None,
            forced: Vec::new(),
        };
        record.validate_for_sid(&record.sid)?;
        record.persistence_plan()?;
        Ok(record)
    }

    /// Checks only bounded typed syntax/phase consistency and exact supplied account identity.
    /// It performs no filesystem access, task lookup, directory creation or SID initialization.
    pub(crate) fn validate_for_sid(&self, sid: &str) -> Result<(), ServiceError> {
        if self.version != RECORD_VERSION || self.sid != sid || self.roles.len() > ROLE_LIMIT {
            return Err(invalid(
                "maintenance record version/account/inventory does not match",
            ));
        }
        validate_sid(&self.sid)?;
        self.previous.validate()?;
        let restoring = matches!(self.phase, RestorePhase::Restoring | RestorePhase::Restored);
        if restoring != self.next.is_some() {
            return Err(invalid(
                "maintenance future binding is invalid for its phase",
            ));
        }
        if let Some(binding) = &self.next {
            binding.validate()?;
        }
        let mut names = BTreeSet::new();
        for role in &self.roles {
            validate_path(&role.root)?;
            validate_hex(&role.instance, 64)?;
            validate_hex(&role.definition, 64)?;
            if role.task_name != task_name(&role.instance, role.role)
                || !names.insert(&role.task_name)
                || restoring != role.future_definition.is_some()
            {
                return Err(invalid(
                    "maintenance role name/definition/inventory is invalid",
                ));
            }
            if let Some(definition) = &role.future_definition {
                validate_hex(definition, 64)?;
            }
            let permitted = match self.phase {
                RestorePhase::Snapshot => matches!(role.progress, RoleProgress::Original),
                RestorePhase::Quiescing => matches!(
                    role.progress,
                    RoleProgress::Original | RoleProgress::DisableIntent | RoleProgress::Disabled
                ),
                RestorePhase::Quiescent => matches!(role.progress, RoleProgress::Disabled),
                RestorePhase::Restoring => matches!(
                    role.progress,
                    RoleProgress::Disabled
                        | RoleProgress::RefreshIntent
                        | RoleProgress::Refreshed
                        | RoleProgress::EnableIntent
                        | RoleProgress::Restored
                ),
                RestorePhase::Restored => matches!(role.progress, RoleProgress::Restored),
                RestorePhase::Removing => matches!(
                    role.progress,
                    RoleProgress::Disabled | RoleProgress::DeleteIntent | RoleProgress::Removed
                ),
                RestorePhase::Removed => matches!(role.progress, RoleProgress::Removed),
            };
            if !permitted {
                return Err(invalid(
                    "maintenance role progress is invalid for its phase",
                ));
            }
        }
        let mut forced_roles = BTreeSet::new();
        let mut forced_instances = BTreeSet::new();
        for fact in &self.forced {
            let index = usize::from(fact.role_index);
            let lifetime = uuid::Uuid::parse_str(&fact.instance)
                .map_err(|_| invalid("forced task instance is not a UUID"))?;
            if index >= self.roles.len()
                || !forced_roles.insert(index)
                || !forced_instances.insert(&fact.instance)
                || lifetime.is_nil()
                || lifetime.hyphenated().to_string() != fact.instance
            {
                return Err(invalid(
                    "forced task instance/index is not canonical and unique",
                ));
            }
            if self.phase == RestorePhase::Snapshot
                || self.roles[index].progress == RoleProgress::Original
                || self.phase != RestorePhase::Quiescing && fact.phase != ForcedPhase::ExitConfirmed
            {
                return Err(invalid(
                    "forced completion fact is inconsistent with the record phase",
                ));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|error| invalid(&error.to_string()))?
            .len()
            > RECORD_LIMIT
        {
            return Err(invalid("complete service record exceeds 128 KiB"));
        }
        Ok(())
    }

    /// True only after a durably recorded actual all-role stop; it does not re-prove live exit.
    pub(crate) fn is_quiesced(&self) -> bool {
        matches!(
            self.phase,
            RestorePhase::Quiescent | RestorePhase::Removing | RestorePhase::Removed
        )
    }

    pub(crate) fn phase(&self) -> &'static str {
        match self.phase {
            RestorePhase::Snapshot => "snapshot",
            RestorePhase::Quiescing => "quiescing",
            RestorePhase::Quiescent => "quiescent",
            RestorePhase::Restoring => "restoring",
            RestorePhase::Restored => "restored",
            RestorePhase::Removing => "removing",
            RestorePhase::Removed => "removed",
        }
    }

    /// Reserves the entire future typed service slot, including every possible forced stop.
    /// The caller must add all outer-envelope fields/overhead and every repeated service slot.
    pub(crate) fn persistence_plan(&self) -> Result<ServicePersistencePlan, ServiceError> {
        self.validate_for_sid(&self.sid)?;
        let mut worst = self.clone();
        worst.phase = RestorePhase::Restoring;
        let future_path = r"\\?\C:\x".to_owned();
        let actual_path_bytes = serde_json::to_vec(&future_path)
            .map_err(|error| invalid(&error.to_string()))?
            .len();
        worst.next = Some(ExecutableBinding {
            path: future_path,
            volume: "f".repeat(16),
            file: "f".repeat(32),
        });
        worst.forced.clear();
        for (index, role) in worst.roles.iter_mut().enumerate() {
            role.progress = RoleProgress::RefreshIntent;
            role.future_definition = Some("f".repeat(64));
            worst.forced.push(ForcedInstance {
                role_index: u16::try_from(index).map_err(|_| invalid("role index overflow"))?,
                instance: "ffffffff-ffff-ffff-ffff-ffffffffffff".into(),
                phase: ForcedPhase::StopRequested,
            });
        }
        let max_record_bytes = serde_json::to_vec(&worst)
            .map_err(|error| invalid(&error.to_string()))?
            .len()
            .checked_add(FUTURE_PATH_JSON_BYTES - actual_path_bytes)
            .ok_or_else(|| invalid("service reservation overflow"))?;
        if max_record_bytes > RECORD_LIMIT {
            return Err(invalid(
                "all reachable service records cannot fit the 128 KiB bound",
            ));
        }
        let roles = self.roles.len();
        Ok(ServicePersistencePlan {
            max_record_bytes,
            quiesce_callbacks: 4 * roles + 2,
            restore_callbacks: 4 * roles + 2,
            remove_callbacks: 2 * roles + 2,
        })
    }

    /// Existing-only recovery can require an old binding without opening it during pure decode.
    pub(crate) fn previous_path(&self) -> &Path {
        Path::new(&self.previous.path)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutableBinding, ForcedInstance, ForcedPhase, RestorePhase, RestoreRole, RoleProgress,
        ServiceRestoreRecord, task_name, validate_path,
    };
    use crate::service::Target;

    const SID: &str = "S-1-5-21-1-2-3-1001";

    fn record(count: usize) -> ServiceRestoreRecord {
        ServiceRestoreRecord::snapshot(
            SID.into(),
            ExecutableBinding {
                path: r"\\?\C:\owned\locron.exe".into(),
                volume: "f".repeat(16),
                file: "f".repeat(32),
            },
            (0..count)
                .map(|index| {
                    let instance = format!("{index:064x}");
                    RestoreRole {
                        role: Target::Daemon,
                        root: format!(r"\\?\C:\private\state{index}"),
                        task_name: task_name(&instance, Target::Daemon),
                        instance,
                        enabled: index.is_multiple_of(2),
                        definition: "0".repeat(64),
                        future_definition: None,
                        progress: RoleProgress::Original,
                    }
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn strict_decode_and_pure_validation_refuse_foreign_unknown_and_inconsistent_records() {
        let original = record(2);
        assert!(original.validate_for_sid("S-1-5-21-9-8-7-1001").is_err());
        let mut json = serde_json::to_value(&original).unwrap();
        json["arbitrary_command"] = "do not execute".into();
        assert!(serde_json::from_value::<ServiceRestoreRecord>(json).is_err());
        let mut changed = original.clone();
        let duplicate_name = changed.roles[0].task_name.clone();
        changed.roles[1].task_name = duplicate_name;
        assert!(changed.validate_for_sid(SID).is_err());
        changed = original.clone();
        changed.phase = RestorePhase::Quiescent;
        assert!(changed.validate_for_sid(SID).is_err());
        changed = original;
        changed.forced.push(ForcedInstance {
            role_index: 0,
            instance: uuid::Uuid::nil().to_string(),
            phase: ForcedPhase::StopRequested,
        });
        assert!(changed.validate_for_sid(SID).is_err());
    }

    #[test]
    fn maintenance_path_bound_counts_utf16_and_preserves_long_unicode() {
        let prefix = r"\\?\C:\";
        let path = format!(
            "{prefix}{}",
            "界".repeat(4096 - prefix.encode_utf16().count())
        );
        validate_path(&path).unwrap();
        assert!(validate_path(&format!("{path}界")).is_err());
        let astral = format!(
            "{prefix}{}",
            "𠮷".repeat((4096 - prefix.encode_utf16().count()) / 2)
        );
        validate_path(&astral).unwrap();
        for invalid in [
            r"C:relative",
            r"\root",
            r"\\server\share\x",
            r"\\?\UNC\server\share\x",
            r"C:\x\..\y",
            r"C:\x\a?b",
            "C:\\x\\line\n",
            r"C:\x\",
        ] {
            assert!(validate_path(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn reservation_covers_every_future_phase_and_rejects_oversize_before_effects() {
        let original = record(3);
        let plan = original.persistence_plan().unwrap();
        assert_eq!(plan.quiesce_callbacks, 14);
        assert_eq!(plan.restore_callbacks, 14);
        assert_eq!(plan.remove_callbacks, 8);
        let mut future = original;
        future.phase = RestorePhase::Restoring;
        future.next = Some(ExecutableBinding {
            path: format!(r"\\?\C:\{}", "界".repeat(4096 - 7)),
            volume: "f".repeat(16),
            file: "f".repeat(32),
        });
        for (index, role) in future.roles.iter_mut().enumerate() {
            role.progress = RoleProgress::RefreshIntent;
            role.future_definition = Some("f".repeat(64));
            future.forced.push(ForcedInstance {
                role_index: u16::try_from(index).unwrap(),
                instance: uuid::Uuid::now_v7().to_string(),
                phase: ForcedPhase::ExitConfirmed,
            });
        }
        future.validate_for_sid(SID).unwrap();
        assert!(serde_json::to_vec(&future).unwrap().len() <= plan.max_record_bytes);
        for (phase, progress) in [
            (RestorePhase::Snapshot, RoleProgress::Original),
            (RestorePhase::Quiescing, RoleProgress::DisableIntent),
            (RestorePhase::Quiescent, RoleProgress::Disabled),
            (RestorePhase::Restoring, RoleProgress::EnableIntent),
            (RestorePhase::Restored, RoleProgress::Restored),
            (RestorePhase::Removing, RoleProgress::DeleteIntent),
            (RestorePhase::Removed, RoleProgress::Removed),
        ] {
            let mut state = future.clone();
            state.phase = phase;
            if !matches!(phase, RestorePhase::Restoring | RestorePhase::Restored) {
                state.next = None;
            }
            if phase == RestorePhase::Snapshot {
                state.forced.clear();
            }
            for role in &mut state.roles {
                role.progress = progress;
                if state.next.is_none() {
                    role.future_definition = None;
                }
            }
            state.validate_for_sid(SID).unwrap();
            assert!(serde_json::to_vec(&state).unwrap().len() <= plan.max_record_bytes);
        }
        let mut oversized = record(1);
        let prototype = oversized.roles[0].clone();
        oversized.roles = (0..256)
            .map(|index| {
                let mut role = prototype.clone();
                role.instance = format!("{index:064x}");
                role.task_name = task_name(&role.instance, role.role);
                role.root = format!(r"\\?\C:\{}", "界".repeat(4096 - 7));
                role
            })
            .collect();
        assert!(oversized.persistence_plan().is_err());
    }
}
