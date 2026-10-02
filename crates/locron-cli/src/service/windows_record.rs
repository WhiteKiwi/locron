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

    /// Reads the future path only; validate account/phase and live guarded bytes separately.
    pub(crate) fn next_path(&self) -> Option<&Path> {
        self.next.as_ref().map(|binding| Path::new(&binding.path))
    }

    /// Compares every future volume/file identity bit without granting effect authority.
    pub(crate) fn matches_next_identity(
        &self,
        identity: &locron_core::filesystem::FileIdentity,
    ) -> bool {
        self.next.as_ref().is_some_and(|binding| {
            binding.volume == format!("{:016x}", identity.volume_serial_number)
                && binding.file == format!("{:032x}", identity.file_id)
        })
    }

    /// Compares the complete ordered frozen origin, independently of recovery progress.
    /// This pure equality check never validates a transition or proves live ownership.
    pub(crate) fn same_original(&self, other: &Self) -> bool {
        self.version == other.version
            && self.sid == other.sid
            && self.previous == other.previous
            && self.roles.len() == other.roles.len()
            && self.roles.iter().zip(&other.roles).all(|(left, right)| {
                left.role == right.role
                    && left.root == right.root
                    && left.instance == right.instance
                    && left.task_name == right.task_name
                    && left.enabled == right.enabled
                    && left.definition == right.definition
            })
    }

    /// Compares both full original identity components with a typed executable inventory fact.
    /// Parsing, bytes/path equality and this pure check do not authorize an external effect.
    pub(crate) fn matches_previous_identity(
        &self,
        identity: &locron_core::filesystem::FileIdentity,
    ) -> bool {
        self.previous.volume == format!("{:016x}", identity.volume_serial_number)
            && self.previous.file == format!("{:032x}", identity.file_id)
    }

    /// Accepts only an unchanged observation or one exact forward journal transition.
    /// Native ownership/readback and effect authorization remain separate requirements.
    pub(crate) fn validate_successor(&self, previous: &Self) -> Result<(), ServiceError> {
        previous.validate_for_sid(&previous.sid)?;
        self.validate_for_sid(&previous.sid)?;
        previous.validate_transition_shape()?;
        self.validate_transition_shape()?;
        if !self.same_original(previous) {
            return Err(invalid("service transition changed its frozen origin"));
        }
        if self == previous {
            return Ok(());
        }
        let entering_restore =
            previous.phase == RestorePhase::Quiescent && self.phase == RestorePhase::Restoring;
        if !entering_restore
            && (self.next != previous.next
                || self
                    .roles
                    .iter()
                    .zip(&previous.roles)
                    .any(|(next, prior)| next.future_definition != prior.future_definition))
        {
            return Err(invalid("service transition rebound a future definition"));
        }
        let same_progress = self
            .roles
            .iter()
            .zip(&previous.roles)
            .all(|(next, prior)| next.progress == prior.progress);
        if self.forced != previous.forced {
            if self.phase != RestorePhase::Quiescing
                || previous.phase != RestorePhase::Quiescing
                || !same_progress
                || !self.all_progress(RoleProgress::Disabled)
            {
                return Err(invalid(
                    "forced fact changed outside an all-disabled observation",
                ));
            }
            return self.validate_forced_successor(previous);
        }
        let permitted = match (previous.phase, self.phase) {
            (RestorePhase::Snapshot, RestorePhase::Quiescing) => self.ordered_progress_step(
                previous,
                &[(RoleProgress::Original, RoleProgress::DisableIntent)],
                RoleProgress::Disabled,
                RoleProgress::Original,
            ),
            (RestorePhase::Snapshot, RestorePhase::Quiescent) => self.roles.is_empty(),
            (RestorePhase::Quiescing, RestorePhase::Quiescing) => self.ordered_progress_step(
                previous,
                &[
                    (RoleProgress::Original, RoleProgress::DisableIntent),
                    (RoleProgress::DisableIntent, RoleProgress::Disabled),
                ],
                RoleProgress::Disabled,
                RoleProgress::Original,
            ),
            (RestorePhase::Quiescing, RestorePhase::Quiescent) => {
                same_progress && self.all_progress(RoleProgress::Disabled)
            }
            (RestorePhase::Quiescent, RestorePhase::Restoring | RestorePhase::Removing) => {
                same_progress && self.all_progress(RoleProgress::Disabled)
            }
            (RestorePhase::Restoring, RestorePhase::Restoring) => self.ordered_progress_step(
                previous,
                &[
                    (RoleProgress::Disabled, RoleProgress::RefreshIntent),
                    (RoleProgress::RefreshIntent, RoleProgress::Refreshed),
                    (RoleProgress::Refreshed, RoleProgress::EnableIntent),
                    (RoleProgress::EnableIntent, RoleProgress::Restored),
                ],
                RoleProgress::Restored,
                RoleProgress::Disabled,
            ),
            (RestorePhase::Restoring, RestorePhase::Restored) => {
                same_progress && self.all_progress(RoleProgress::Restored)
            }
            (RestorePhase::Removing, RestorePhase::Removing) => self.ordered_progress_step(
                previous,
                &[
                    (RoleProgress::Disabled, RoleProgress::DeleteIntent),
                    (RoleProgress::DeleteIntent, RoleProgress::Removed),
                ],
                RoleProgress::Removed,
                RoleProgress::Disabled,
            ),
            (RestorePhase::Removing, RestorePhase::Removed) => {
                same_progress && self.all_progress(RoleProgress::Removed)
            }
            _ => false,
        };
        if permitted {
            Ok(())
        } else {
            Err(invalid("service transition is not one exact forward step"))
        }
    }

    fn all_progress(&self, progress: RoleProgress) -> bool {
        self.roles.iter().all(|role| role.progress == progress)
    }

    fn validate_transition_shape(&self) -> Result<(), ServiceError> {
        let ordered = match self.phase {
            RestorePhase::Quiescing => {
                !self.roles.is_empty()
                    && !self.all_progress(RoleProgress::Original)
                    && self.ordered_progress_shape(
                        RoleProgress::Disabled,
                        RoleProgress::Original,
                        &[RoleProgress::Original, RoleProgress::DisableIntent],
                    )
                    && (self.forced.is_empty() || self.all_progress(RoleProgress::Disabled))
            }
            RestorePhase::Restoring => self.ordered_progress_shape(
                RoleProgress::Restored,
                RoleProgress::Disabled,
                &[
                    RoleProgress::Disabled,
                    RoleProgress::RefreshIntent,
                    RoleProgress::Refreshed,
                    RoleProgress::EnableIntent,
                ],
            ),
            RestorePhase::Removing => self.ordered_progress_shape(
                RoleProgress::Removed,
                RoleProgress::Disabled,
                &[RoleProgress::Disabled, RoleProgress::DeleteIntent],
            ),
            _ => true,
        };
        if ordered {
            Ok(())
        } else {
            Err(invalid(
                "service progress is not an ordered reachable state",
            ))
        }
    }

    fn ordered_progress_shape(
        &self,
        prefix: RoleProgress,
        suffix: RoleProgress,
        active: &[RoleProgress],
    ) -> bool {
        let mut roles = self.roles.iter().skip_while(|role| role.progress == prefix);
        roles.next().is_none_or(|role| {
            active.contains(&role.progress) && roles.all(|role| role.progress == suffix)
        })
    }

    fn ordered_progress_step(
        &self,
        previous: &Self,
        edges: &[(RoleProgress, RoleProgress)],
        prefix: RoleProgress,
        suffix: RoleProgress,
    ) -> bool {
        let mut differences = self
            .roles
            .iter()
            .zip(&previous.roles)
            .enumerate()
            .filter(|(_, (next, prior))| next.progress != prior.progress);
        let Some((index, (next, prior))) = differences.next() else {
            return false;
        };
        differences.next().is_none()
            && edges.contains(&(prior.progress, next.progress))
            && self.roles[..index]
                .iter()
                .all(|role| role.progress == prefix)
            && self.roles[index + 1..]
                .iter()
                .all(|role| role.progress == suffix)
    }

    fn validate_forced_successor(&self, previous: &Self) -> Result<(), ServiceError> {
        let appended = self.forced.len() == previous.forced.len() + 1
            && self.forced[..previous.forced.len()] == previous.forced
            && self
                .forced
                .last()
                .is_some_and(|fact| fact.phase == ForcedPhase::StopRequested);
        if appended {
            return Ok(());
        }
        let mut confirmations = 0;
        if self.forced.len() == previous.forced.len() {
            for (next, prior) in self.forced.iter().zip(&previous.forced) {
                if next.role_index != prior.role_index || next.instance != prior.instance {
                    return Err(invalid("forced fact rebound its ordered role or GUID"));
                }
                if next.phase != prior.phase {
                    if prior.phase != ForcedPhase::StopRequested
                        || next.phase != ForcedPhase::ExitConfirmed
                    {
                        return Err(invalid("forced fact rewound its exit confirmation"));
                    }
                    confirmations += 1;
                }
            }
        }
        if confirmations == 1 {
            Ok(())
        } else {
            Err(invalid("forced facts require one append or confirmation"))
        }
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

    #[test]
    fn future_readbacks_preserve_none_path_and_all_identity_bits() {
        let original = record(1);
        let mut identity = locron_core::filesystem::FileIdentity {
            volume_serial_number: u64::MAX,
            file_id: u128::MAX,
        };
        original.validate_for_sid(SID).unwrap();
        assert_eq!(original.next_path(), None);
        assert!(!original.matches_next_identity(&identity));
        let mut next = original.clone();
        next.phase = RestorePhase::Restoring;
        next.next = Some(ExecutableBinding {
            path: r"\\?\C:\new 子\locron.exe".into(),
            volume: "f".repeat(16),
            file: "f".repeat(32),
        });
        next.roles[0].progress = RoleProgress::Disabled;
        next.roles[0].future_definition = Some("f".repeat(64));
        next.validate_for_sid(SID).unwrap();
        assert_eq!(
            next.next_path(),
            Some(std::path::Path::new(r"\\?\C:\new 子\locron.exe"))
        );
        assert_ne!(next.next_path(), Some(original.previous_path()));
        assert!(next.matches_next_identity(&identity));
        for bit in [0, 63] {
            identity.volume_serial_number = u64::MAX ^ (1 << bit);
            assert!(!next.matches_next_identity(&identity));
        }
        identity.volume_serial_number = u64::MAX;
        for bit in [0, 127] {
            identity.file_id = u128::MAX ^ (1 << bit);
            assert!(!next.matches_next_identity(&identity));
        }
    }

    fn push_state(
        states: &mut Vec<ServiceRestoreRecord>,
        update: impl FnOnce(&mut ServiceRestoreRecord),
    ) {
        let previous = states.last().unwrap();
        let mut next = previous.clone();
        update(&mut next);
        next.validate_successor(previous).unwrap();
        next.validate_successor(&next).unwrap();
        states.push(next);
    }

    fn restore_walk() -> Vec<ServiceRestoreRecord> {
        let mut states = vec![record(2)];
        for index in 0..2 {
            push_state(&mut states, |next| {
                next.phase = RestorePhase::Quiescing;
                next.roles[index].progress = RoleProgress::DisableIntent;
            });
            push_state(&mut states, |next| {
                next.roles[index].progress = RoleProgress::Disabled
            });
        }
        for index in 0..2 {
            push_state(&mut states, |next| {
                next.forced.push(ForcedInstance {
                    role_index: index,
                    instance: uuid::Uuid::now_v7().to_string(),
                    phase: ForcedPhase::StopRequested,
                })
            });
            push_state(&mut states, |next| {
                next.forced[usize::from(index)].phase = ForcedPhase::ExitConfirmed;
            });
        }
        push_state(&mut states, |next| next.phase = RestorePhase::Quiescent);
        push_state(&mut states, |next| {
            next.phase = RestorePhase::Restoring;
            next.next = Some(ExecutableBinding {
                path: r"C:\new\locron.exe".into(),
                volume: "e".repeat(16),
                file: "e".repeat(32),
            });
            for role in &mut next.roles {
                role.future_definition = Some("e".repeat(64));
            }
        });
        for index in 0..2 {
            for progress in [
                RoleProgress::RefreshIntent,
                RoleProgress::Refreshed,
                RoleProgress::EnableIntent,
                RoleProgress::Restored,
            ] {
                push_state(&mut states, |next| next.roles[index].progress = progress);
            }
        }
        push_state(&mut states, |next| next.phase = RestorePhase::Restored);
        states
    }

    #[test]
    fn successors_permit_only_single_forward_edges_and_unchanged_recovery_observations() {
        let states = restore_walk();
        for (prior_index, previous) in states.iter().enumerate() {
            for (next_index, next) in states.iter().enumerate() {
                assert_eq!(
                    next.validate_successor(previous).is_ok(),
                    next_index == prior_index || next_index == prior_index + 1,
                    "unexpected edge {prior_index}->{next_index}"
                );
            }
        }
        let quiescent = states
            .iter()
            .find(|state| state.phase == RestorePhase::Quiescent)
            .unwrap();
        let mut removal = vec![quiescent.clone()];
        push_state(&mut removal, |next| next.phase = RestorePhase::Removing);
        for index in 0..2 {
            push_state(&mut removal, |next| {
                next.roles[index].progress = RoleProgress::DeleteIntent
            });
            push_state(&mut removal, |next| {
                next.roles[index].progress = RoleProgress::Removed
            });
        }
        push_state(&mut removal, |next| next.phase = RestorePhase::Removed);
        for (prior_index, previous) in removal.iter().enumerate() {
            for (next_index, next) in removal.iter().enumerate() {
                assert_eq!(
                    next.validate_successor(previous).is_ok(),
                    next_index == prior_index || next_index == prior_index + 1
                );
            }
        }
    }

    #[test]
    fn zero_role_restore_and_remove_preserve_exact_phase_boundaries() {
        let mut states = vec![record(0)];
        push_state(&mut states, |next| next.phase = RestorePhase::Quiescent);
        let quiescent = states.last().unwrap().clone();
        push_state(&mut states, |next| {
            next.phase = RestorePhase::Restoring;
            next.next = Some(next.previous.clone());
        });
        push_state(&mut states, |next| next.phase = RestorePhase::Restored);
        let mut removal = vec![quiescent];
        push_state(&mut removal, |next| next.phase = RestorePhase::Removing);
        push_state(&mut removal, |next| next.phase = RestorePhase::Removed);
        assert!(
            states
                .last()
                .unwrap()
                .validate_successor(&states[0])
                .is_err()
        );
        assert!(
            removal
                .last()
                .unwrap()
                .validate_successor(&removal[0])
                .is_err()
        );
    }

    #[test]
    fn successors_refuse_valid_future_rebinding_and_forced_fact_rewrites() {
        let states = restore_walk();
        let restoring = states
            .iter()
            .find(|state| state.phase == RestorePhase::Restoring)
            .unwrap();
        let changes: [fn(&mut ServiceRestoreRecord); 7] = [
            |value| value.next.as_mut().unwrap().path = r"C:\different\locron.exe".into(),
            |value| value.next.as_mut().unwrap().volume = "d".repeat(16),
            |value| value.next.as_mut().unwrap().file = "d".repeat(32),
            |value| value.roles[0].future_definition = Some("d".repeat(64)),
            |value| {
                value.forced.pop().unwrap();
            },
            |value| value.forced[0].instance = uuid::Uuid::now_v7().to_string(),
            |value| value.forced.swap(0, 1),
        ];
        for change in changes {
            let mut next = restoring.clone();
            change(&mut next);
            next.validate_for_sid(SID).unwrap();
            assert!(next.same_original(restoring));
            assert!(next.validate_successor(restoring).is_err());
        }
        let mut pending = restoring.clone();
        pending.phase = RestorePhase::Quiescing;
        pending.next = None;
        for role in &mut pending.roles {
            role.future_definition = None;
        }
        for fact in &mut pending.forced {
            fact.phase = ForcedPhase::StopRequested;
        }
        pending.validate_for_sid(SID).unwrap();
        let mut batched = pending.clone();
        for fact in &mut batched.forced {
            fact.phase = ForcedPhase::ExitConfirmed;
        }
        assert!(batched.validate_successor(&pending).is_err());
        let mut cleared = restoring.clone();
        cleared.next = None;
        assert!(cleared.validate_successor(restoring).is_err());
    }

    #[test]
    fn unchanged_parse_valid_but_unreachable_role_order_is_refused() {
        let mut unordered = record(2);
        unordered.phase = RestorePhase::Quiescing;
        unordered.roles[1].progress = RoleProgress::DisableIntent;
        unordered.validate_for_sid(SID).unwrap();
        assert!(unordered.validate_successor(&unordered).is_err());
        unordered.roles[0].progress = RoleProgress::DisableIntent;
        unordered.roles[1].progress = RoleProgress::Original;
        unordered.forced.push(ForcedInstance {
            role_index: 0,
            instance: uuid::Uuid::now_v7().to_string(),
            phase: ForcedPhase::StopRequested,
        });
        unordered.validate_for_sid(SID).unwrap();
        assert!(unordered.validate_successor(&unordered).is_err());
    }

    #[test]
    fn frozen_origin_refuses_each_tampered_original_field_and_role_order() {
        let original = record(2);
        let changes: [fn(&mut ServiceRestoreRecord); 12] = [
            |value| value.version = 2,
            |value| value.sid = "S-1-5-21-4-5-6-1001".into(),
            |value| value.previous.path = r"C:\other\locron.exe".into(),
            |value| value.previous.volume = "e".repeat(16),
            |value| value.previous.file = "e".repeat(32),
            |value| value.roles[0].role = Target::Dashboard,
            |value| value.roles[0].root = r"C:\other\state".into(),
            |value| value.roles[0].instance = "e".repeat(64),
            |value| value.roles[0].task_name.push('x'),
            |value| value.roles[0].enabled = !value.roles[0].enabled,
            |value| value.roles[0].definition = "e".repeat(64),
            |value| value.roles.swap(0, 1),
        ];
        for change in changes {
            let mut altered = original.clone();
            change(&mut altered);
            assert!(!original.same_original(&altered));
            assert!(!altered.same_original(&original));
        }
        let mut future = original.clone();
        future.phase = RestorePhase::Restoring;
        future.next = Some(ExecutableBinding {
            path: r"C:\new\locron.exe".into(),
            volume: "e".repeat(16),
            file: "e".repeat(32),
        });
        for (index, role) in future.roles.iter_mut().enumerate() {
            role.progress = RoleProgress::RefreshIntent;
            role.future_definition = Some("e".repeat(64));
            future.forced.push(ForcedInstance {
                role_index: u16::try_from(index).unwrap(),
                instance: uuid::Uuid::now_v7().to_string(),
                phase: ForcedPhase::ExitConfirmed,
            });
        }
        future.validate_for_sid(SID).unwrap();
        assert!(original.same_original(&future));
        assert!(future.same_original(&original));
        future.roles.pop();
        assert!(!original.same_original(&future));
    }

    #[test]
    fn original_binding_compares_the_complete_typed_volume_and_file_identity() {
        let original = record(1);
        let mut identity = locron_core::filesystem::FileIdentity {
            volume_serial_number: u64::MAX,
            file_id: u128::MAX,
        };
        assert!(original.matches_previous_identity(&identity));
        identity.volume_serial_number ^= 1 << 63;
        assert!(!original.matches_previous_identity(&identity));
        identity.volume_serial_number = u64::MAX;
        identity.file_id ^= 1 << 127;
        assert!(!original.matches_previous_identity(&identity));
        identity.file_id = u128::MAX ^ 1;
        assert!(!original.matches_previous_identity(&identity));
    }

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
