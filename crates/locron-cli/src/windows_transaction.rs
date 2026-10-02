//! Concrete existing-installation records. Parsing never authorizes an OS effect.
//!
//! The original request and immutable inventory are qualified through retained
//! live guards by the caller. Fresh role creation requires its own real service
//! registration/rollback record and is deliberately outside this envelope.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use anyhow::{Result, ensure};
use locron_core::filesystem::FileIdentity;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::service::ServiceRestoreRecord;

use super::sha256_hex;
use super::windows_journal::{Reservation, object_growth, preflight_with_growth};
use super::windows_ownership::{RetainedPayload, Retention as OwnedRetention};
use super::windows_path::{PathValue, WriteIntent};
use super::windows_protocol::{Kind, MAINTENANCE_PATH_JSON_BYTES, PackageBinding, Request};
use super::windows_receipt::{
    PAYLOADS, RECEIPT, Receipt, required_nullable, same_path, valid_hash,
};

const SCHEMA: &str = "locron.windows-transaction/v1";
const PAYLOAD_LIMIT: u64 = 64 * 1024 * 1024;
const RECEIPT_LIMIT: u64 = 128 * 1024;

/// Hex preserves the complete identity across Rust/PowerShell/JSON boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    volume: String,
    file: String,
}

impl Identity {
    pub(super) fn from_file(identity: FileIdentity) -> Self {
        Self {
            volume: format!("{:016x}", identity.volume_serial_number),
            file: format!("{:032x}", identity.file_id),
        }
    }

    pub(super) fn file_identity(&self) -> Result<FileIdentity> {
        ensure!(
            fixed_hex(&self.volume, 16) && fixed_hex(&self.file, 32),
            "transaction requires complete canonical file identities"
        );
        Ok(FileIdentity {
            volume_serial_number: u64::from_str_radix(&self.volume, 16)?,
            file_id: u128::from_str_radix(&self.file, 16)?,
        })
    }
}

fn fixed_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Content {
    pub bytes: u64,
    pub sha256: String,
}

impl Content {
    fn validate(&self, name: &str) -> Result<()> {
        let limit = if name == RECEIPT {
            RECEIPT_LIMIT
        } else {
            PAYLOAD_LIMIT
        };
        ensure!(
            self.bytes <= limit && valid_hash(&self.sha256),
            "transaction leaf exceeds its exact content bound"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileFact {
    pub identity: Identity,
    pub content: Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LeafPhase {
    Original,
    BackupIntent,
    BackedUp,
    DeleteIntent,
    Absent,
    CreateIntent,
    Created,
    Verified,
    RollbackDeleteIntent,
    RollbackAbsent,
    RollbackCreateIntent,
    RollbackCreated,
    RolledBack,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Leaf {
    pub original: FileFact,
    #[serde(deserialize_with = "required_nullable")]
    pub desired: Option<Content>,
    #[serde(deserialize_with = "required_nullable")]
    backup: Option<FileFact>,
    #[serde(deserialize_with = "required_nullable")]
    created: Option<Identity>,
    #[serde(deserialize_with = "required_nullable")]
    rollback: Option<Identity>,
    phase: LeafPhase,
}

impl Leaf {
    pub(super) fn original(original: FileFact, desired: Option<Content>) -> Self {
        Self {
            original,
            desired,
            backup: None,
            created: None,
            rollback: None,
            phase: LeafPhase::Original,
        }
    }

    fn validate(&self, name: &str) -> Result<()> {
        self.original.identity.file_identity()?;
        self.original.content.validate(name)?;
        if let Some(desired) = &self.desired {
            desired.validate(name)?;
        }
        if let Some(backup) = &self.backup {
            backup.identity.file_identity()?;
            ensure!(
                backup.identity != self.original.identity
                    && backup.content == self.original.content,
                "backup is not a separate exact copy of the original"
            );
        }
        if let Some(created) = &self.created {
            created.file_identity()?;
            ensure!(
                self.desired.is_some()
                    && created != &self.original.identity
                    && self
                        .backup
                        .as_ref()
                        .is_none_or(|backup| created != &backup.identity),
                "new leaf identity conflicts with immutable original/backup facts"
            );
        }
        if let Some(rollback) = &self.rollback {
            rollback.file_identity()?;
            ensure!(
                self.created.as_ref() != Some(rollback)
                    && self
                        .backup
                        .as_ref()
                        .is_none_or(|backup| rollback != &backup.identity),
                "restored leaf cannot reuse a deleted new object or its backup"
            );
        }
        let (backup_required, created_required, rollback_required) = match self.phase {
            LeafPhase::Original | LeafPhase::BackupIntent => (false, false, false),
            LeafPhase::BackedUp
            | LeafPhase::DeleteIntent
            | LeafPhase::Absent
            | LeafPhase::CreateIntent
            | LeafPhase::RollbackAbsent
            | LeafPhase::RollbackCreateIntent => (true, false, false),
            LeafPhase::Created | LeafPhase::Verified | LeafPhase::RollbackDeleteIntent => {
                (true, true, false)
            }
            LeafPhase::RollbackCreated => (true, false, true),
            LeafPhase::RolledBack => (false, false, true),
        };
        ensure!(
            (!backup_required || self.backup.is_some())
                && (!created_required || self.created.is_some())
                && (!rollback_required || self.rollback.is_some()),
            "transaction phase lacks a durable exact identity"
        );
        if matches!(self.phase, LeafPhase::Original | LeafPhase::BackupIntent) {
            ensure!(
                self.backup.is_none() && self.created.is_none() && self.rollback.is_none(),
                "unmodified leaf contains future identity facts"
            );
        }
        if matches!(
            self.phase,
            LeafPhase::CreateIntent | LeafPhase::Created | LeafPhase::Verified
        ) {
            ensure!(
                self.desired.is_some(),
                "removal cannot create a replacement"
            );
        }
        if !matches!(
            self.phase,
            LeafPhase::Created
                | LeafPhase::Verified
                | LeafPhase::RollbackDeleteIntent
                | LeafPhase::RollbackAbsent
                | LeafPhase::RollbackCreateIntent
                | LeafPhase::RollbackCreated
                | LeafPhase::RolledBack
        ) {
            ensure!(self.created.is_none(), "premature newly created identity");
        }
        if !matches!(
            self.phase,
            LeafPhase::RollbackCreated | LeafPhase::RolledBack
        ) {
            ensure!(self.rollback.is_none(), "premature restored identity");
        }
        Ok(())
    }

    fn validate_successor(&self, previous: &Self) -> Result<()> {
        ensure!(
            self.original == previous.original && self.desired == previous.desired,
            "transaction changed a frozen original/replacement content fact"
        );
        ensure!(
            previous.backup.is_none() || previous.backup == self.backup,
            "durable backup binding changed"
        );
        for (old, next) in [
            (&previous.created, &self.created),
            (&previous.rollback, &self.rollback),
        ] {
            ensure!(
                old.is_none() || old == next,
                "durable leaf identity changed"
            );
        }
        if self.phase == previous.phase {
            ensure!(
                self == previous,
                "identity was added without its phase boundary"
            );
            return Ok(());
        }
        let allowed = matches!(
            (previous.phase, self.phase),
            (LeafPhase::Original, LeafPhase::BackupIntent)
                | (LeafPhase::BackupIntent, LeafPhase::BackedUp)
                | (LeafPhase::BackedUp, LeafPhase::DeleteIntent)
                | (LeafPhase::DeleteIntent, LeafPhase::Absent)
                | (LeafPhase::Absent, LeafPhase::CreateIntent)
                | (LeafPhase::CreateIntent, LeafPhase::Created)
                | (LeafPhase::Created, LeafPhase::Verified)
                | (
                    LeafPhase::Created | LeafPhase::Verified,
                    LeafPhase::RollbackDeleteIntent
                )
                | (LeafPhase::RollbackDeleteIntent, LeafPhase::RollbackAbsent)
                | (
                    LeafPhase::Absent | LeafPhase::CreateIntent | LeafPhase::RollbackAbsent,
                    LeafPhase::RollbackCreateIntent
                )
                | (LeafPhase::RollbackCreateIntent, LeafPhase::RollbackCreated)
                | (LeafPhase::RollbackCreated, LeafPhase::RolledBack)
                | (
                    LeafPhase::Original
                        | LeafPhase::BackupIntent
                        | LeafPhase::BackedUp
                        | LeafPhase::DeleteIntent,
                    LeafPhase::RolledBack
                )
        );
        ensure!(allowed, "transaction skipped or rewound a leaf phase");
        if previous.backup.is_none() && self.backup.is_some() {
            ensure!(
                previous.phase == LeafPhase::BackupIntent && self.phase == LeafPhase::BackedUp,
                "backup binding appeared without a recorded copy intent"
            );
        }
        if previous.created.is_none() && self.created.is_some() {
            ensure!(
                previous.phase == LeafPhase::CreateIntent && self.phase == LeafPhase::Created,
                "new identity appeared without its CreateNew intent"
            );
        }
        if self.phase == LeafPhase::RolledBack && previous.phase != LeafPhase::RollbackCreated {
            ensure!(
                self.rollback.as_ref() == Some(&self.original.identity),
                "untouched rollback must preserve the original exact object"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Retention {
    Changed,
    Unverifiable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Accepted,
    BackingUp,
    Quiescing,
    Ready,
    Replacing,
    Restoring,
    Prepared,
    Removing,
    Removed,
    Completed,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PathPhase {
    Planned,
    WriteIntent,
    Written,
    RollbackIntent,
    Restored,
    Retained,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathEdit {
    expected: PathValue,
    desired: PathValue,
    phase: PathPhase,
}

/// Pure inputs from the caller's guarded existing source and selected payloads.
pub(super) struct ExistingInputs<'a> {
    pub request: &'a Request,
    pub protected_request_sha256: String,
    pub original_executable: Identity,
    pub service: ServiceRestoreRecord,
    pub original_receipt: Option<Receipt>,
    pub new_receipt: Option<Receipt>,
    pub leaves: BTreeMap<String, Leaf>,
    pub retained: Vec<RetainedPayload>,
    pub path_edit: Option<(PathValue, PathValue)>,
}

/// Every nullable key is emitted, including both actual typed service objects.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema: String,
    operation_id: String,
    kind: Kind,
    protected_request_sha256: String,
    sequence: u16,
    phase: Phase,
    original_executable: Identity,
    #[serde(deserialize_with = "required_nullable")]
    original_service: Option<ServiceRestoreRecord>,
    #[serde(deserialize_with = "required_nullable")]
    current_service: Option<ServiceRestoreRecord>,
    #[serde(deserialize_with = "required_nullable")]
    original_receipt: Option<Receipt>,
    #[serde(deserialize_with = "required_nullable")]
    new_receipt: Option<Receipt>,
    #[serde(deserialize_with = "required_nullable")]
    original_package: Option<PackageBinding>,
    #[serde(deserialize_with = "required_nullable")]
    completion: Option<PackageBinding>,
    #[serde(deserialize_with = "required_nullable")]
    completion_identity: Option<Identity>,
    #[serde(deserialize_with = "strict_leaves")]
    leaves: BTreeMap<String, Leaf>,
    #[serde(deserialize_with = "strict_retained")]
    retained: BTreeMap<String, Retention>,
    #[serde(deserialize_with = "required_nullable")]
    path_edit: Option<PathEdit>,
}

fn strict_named_map<'de, D, T>(
    deserializer: D,
    receipt: bool,
) -> Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Names<T>(bool, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Names<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("unique exact distribution leaf names")
        }
        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut entries = BTreeMap::new();
            while let Some((name, value)) = map.next_entry::<String, T>()? {
                if !(PAYLOADS.contains(&name.as_str()) || self.0 && name == RECEIPT)
                    || entries.insert(name, value).is_some()
                {
                    return Err(serde::de::Error::custom(
                        "unknown or duplicate transaction leaf",
                    ));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Names(receipt, std::marker::PhantomData))
}

fn strict_leaves<'de, D>(deserializer: D) -> Result<BTreeMap<String, Leaf>, D::Error>
where
    D: Deserializer<'de>,
{
    strict_named_map(deserializer, true)
}

fn strict_retained<'de, D>(deserializer: D) -> Result<BTreeMap<String, Retention>, D::Error>
where
    D: Deserializer<'de>,
{
    strict_named_map(deserializer, false)
}

fn same<T: Serialize>(left: &T, right: &T) -> Result<bool> {
    Ok(serde_json::to_vec(left)? == serde_json::to_vec(right)?)
}

impl Record {
    /// Pure preparation, with an already guarded complete existing inventory.
    /// This constructor neither creates a journal nor qualifies live ownership.
    pub(super) fn existing(input: ExistingInputs<'_>) -> Result<Self> {
        let request = input.request;
        let mut retained = BTreeMap::new();
        for entry in input.retained {
            let reason = match entry.reason {
                OwnedRetention::Changed => Retention::Changed,
                OwnedRetention::Unverifiable => Retention::Unverifiable,
            };
            ensure!(
                retained.insert(entry.name, reason).is_none(),
                "duplicate retention fact"
            );
        }
        let record = Self {
            schema: SCHEMA.into(),
            operation_id: request.operation_id.to_string(),
            kind: request.kind,
            protected_request_sha256: input.protected_request_sha256,
            sequence: 0,
            phase: Phase::Accepted,
            original_executable: input.original_executable,
            original_service: Some(input.service.clone()),
            current_service: Some(input.service),
            original_receipt: input.original_receipt,
            new_receipt: input.new_receipt,
            original_package: request.package.clone(),
            completion: None,
            completion_identity: None,
            leaves: input.leaves,
            retained,
            path_edit: input.path_edit.map(|(expected, desired)| PathEdit {
                expected,
                desired,
                phase: PathPhase::Planned,
            }),
        };
        record.validate(None, request, &record.protected_request_sha256)?;
        Ok(record)
    }

    /// Validate a whole frame and its immediate predecessor without effects.
    /// The request/digest must come from the retained protected original object.
    pub(super) fn validate(
        &self,
        previous: Option<&Self>,
        request: &Request,
        protected_request_sha256: &str,
    ) -> Result<()> {
        request.validate(&request.sid, &request.target, request.operation_id)?;
        ensure!(
            !matches!(request.kind, Kind::Recover | Kind::MaintenanceComplete)
                && self.schema == SCHEMA
                && self.operation_id == request.operation_id.to_string()
                && self.kind == request.kind
                && self.protected_request_sha256 == protected_request_sha256
                && valid_hash(protected_request_sha256)
                && self.sequence < 128,
            "transaction does not bind the protected original operation"
        );
        let original = self.original_service.as_ref().ok_or_else(|| {
            anyhow::anyhow!("existing transaction requires its real original service record")
        })?;
        let current = self.current_service.as_ref().ok_or_else(|| {
            anyhow::anyhow!("existing transaction requires its real current service record")
        })?;
        original.validate_for_sid(&request.sid)?;
        current.validate_for_sid(&request.sid)?;
        ensure!(
            original.phase() == "snapshot"
                && original.same_original(current)
                && original.matches_previous_identity(&self.original_executable.file_identity()?)
                && same_path(
                    original
                        .previous_path()
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("non-Unicode service path"))?,
                    &request.executable,
                )?,
            "service record differs from the frozen exact executable origin"
        );
        self.validate_inventory(request)?;
        self.validate_next_binding(request)?;
        let allowed_service = match self.phase {
            Phase::Accepted | Phase::BackingUp => current.phase() == "snapshot",
            Phase::Quiescing => matches!(current.phase(), "snapshot" | "quiescing" | "quiescent"),
            Phase::Ready | Phase::Replacing | Phase::Prepared => current.phase() == "quiescent",
            Phase::Removing => matches!(current.phase(), "quiescent" | "removing" | "removed"),
            Phase::Restoring => matches!(current.phase(), "quiescent" | "restoring" | "restored"),
            // Registration/enable completion does not confirm Task.Run. Until
            // the provider's actual activation record is integrated this slice
            // cannot admit a confirmed completed/activated rollback terminal.
            Phase::Completed => false,
            Phase::Removed => current.phase() == "removed",
            Phase::RollingBack => matches!(
                current.phase(),
                "snapshot" | "quiescent" | "restoring" | "restored"
            ),
            Phase::RolledBack => current.phase() == "snapshot",
        };
        ensure!(allowed_service, "outer/service phase mismatch");
        self.validate_leaf_phases(request.kind)?;
        if let Some(previous) = previous {
            self.validate_successor(previous)?;
        } else {
            ensure!(
                self.sequence == 0
                    && self.phase == Phase::Accepted
                    && same(&self.original_service, &self.current_service)?
                    && self.completion.is_none()
                    && self
                        .leaves
                        .values()
                        .all(|leaf| leaf.phase == LeafPhase::Original)
                    && self
                        .path_edit
                        .as_ref()
                        .is_none_or(|path| path.phase == PathPhase::Planned),
                "initial transaction is not a complete unmodified snapshot"
            );
        }
        Ok(())
    }

    fn validate_inventory(&self, request: &Request) -> Result<()> {
        ensure!(
            self.leaves
                .keys()
                .all(|name| PAYLOADS.contains(&name.as_str()) || name == RECEIPT),
            "unknown logical leaf name"
        );
        if request.kind.maintenance() {
            ensure!(
                self.original_receipt.is_none()
                    && self.new_receipt.is_none()
                    && self.leaves.is_empty()
                    && self.retained.is_empty()
                    && self.path_edit.is_none()
                    && same(&self.original_package, &request.package)?,
                "package transaction cannot authorize standalone artifacts"
            );
        } else {
            ensure!(
                self.original_package.is_none(),
                "standalone adopted package authority"
            );
            let receipt = self.original_receipt.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "fresh installation requires its separate typed registration rollback plan"
                )
            })?;
            receipt.validate(&request.sid, &receipt.directory, &request.target)?;
            ensure!(
                same_path(&receipt.executable, &request.executable)?,
                "foreign original receipt"
            );
            ensure!(
                self.leaves.contains_key("locron.exe")
                    && self.leaves.contains_key(RECEIPT)
                    && self.leaves.len() <= 7
                    && self.retained.len() <= 5
                    && !self.retained.contains_key("locron.exe"),
                "existing transaction lacks its mandatory executable/receipt inventory"
            );
            for name in PAYLOADS {
                let leaf = self.leaves.get(name);
                ensure!(
                    leaf.is_some() != self.retained.contains_key(name),
                    "payload missing or both owned and retained"
                );
                if let Some(leaf) = leaf {
                    ensure!(
                        receipt.files.get(name) == Some(&leaf.original.content.sha256),
                        "original receipt and payload disagree"
                    );
                }
            }
            ensure!(
                self.leaves["locron.exe"].original.identity == self.original_executable,
                "original executable full identity differs from inventory"
            );
            if matches!(request.kind, Kind::Install | Kind::SelfUpdate) {
                ensure!(
                    self.retained.is_empty() && !request.no_service && !request.dashboard,
                    "existing update accepts no fresh service choices or incomplete inventory"
                );
                let new = self
                    .new_receipt
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("replacement receipt missing"))?;
                new.validate(&request.sid, &receipt.directory, &request.target)?;
                ensure!(
                    new.version == request.version && new.archive_sha256 == request.archive_sha256,
                    "new receipt differs from the protected verified release choice"
                );
                if self.path_edit.is_none() {
                    ensure!(
                        new.user_path == receipt.user_path,
                        "update changed a frozen PATH ownership claim without a write plan"
                    );
                } else {
                    ensure!(
                        request.kind == Kind::Install
                            && request.add_to_path
                            && receipt.user_path.is_none(),
                        "existing PATH ownership cannot be rewritten by an update"
                    );
                }
                let encoded = serde_json::to_vec(new)?;
                for (name, leaf) in &self.leaves {
                    let desired = leaf.desired.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("replacement inventory lacks desired bytes")
                    })?;
                    let hash = if name == RECEIPT {
                        sha256_hex(&encoded)
                    } else {
                        new.files[name].clone()
                    };
                    ensure!(
                        desired.sha256 == hash
                            && (name != RECEIPT || desired.bytes == encoded.len() as u64),
                        "new receipt and desired leaf content disagree"
                    );
                }
            } else {
                ensure!(
                    request.kind == Kind::Uninstall
                        && self.new_receipt.is_none()
                        && self.leaves.values().all(|leaf| leaf.desired.is_none())
                        && receipt.version == request.version
                        && receipt.archive_sha256 == request.archive_sha256,
                    "removal cannot stage replacement content"
                );
            }
        }
        let mut originals = BTreeSet::new();
        let mut backups = BTreeSet::new();
        let mut created = BTreeSet::new();
        let mut restored = BTreeSet::new();
        for (name, leaf) in &self.leaves {
            ensure!(
                PAYLOADS.contains(&name.as_str()) || name == RECEIPT,
                "unknown logical leaf name"
            );
            leaf.validate(name)?;
            ensure!(
                originals.insert(&leaf.original.identity),
                "duplicate original full object identity"
            );
            if let Some(backup) = &leaf.backup {
                ensure!(
                    backups.insert(&backup.identity),
                    "duplicate durable backup identity"
                );
            }
            if let Some(identity) = &leaf.created {
                ensure!(
                    created.insert(identity),
                    "duplicate new full object identity"
                );
            }
            if let Some(identity) = &leaf.rollback {
                ensure!(
                    restored.insert(identity),
                    "duplicate restored full object identity"
                );
            }
        }
        ensure!(
            originals.is_disjoint(&backups)
                && created.is_disjoint(&originals)
                && created.is_disjoint(&backups)
                && restored.is_disjoint(&backups)
                && restored.is_disjoint(&created),
            "future/backup identity aliases another recorded object"
        );
        for leaf in self.leaves.values() {
            if let Some(identity) = &leaf.rollback {
                ensure!(
                    !originals.contains(identity)
                        || identity == &leaf.original.identity
                            && leaf.phase == LeafPhase::RolledBack
                            && leaf.created.is_none(),
                    "restored identity adopted another original or an unconfirmed old leaf"
                );
            }
        }
        ensure!(
            self.retained
                .keys()
                .all(|name| PAYLOADS.contains(&name.as_str())),
            "unknown retention fact"
        );
        ensure!(
            self.completion.is_some() == self.completion_identity.is_some(),
            "incomplete future package binding"
        );
        if let Some(completion) = &self.completion {
            completion.validate()?;
            self.completion_identity.as_ref().unwrap().file_identity()?;
            let original = self
                .original_package
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("completion has no original package"))?;
            ensure!(
                request.kind == Kind::MaintenancePrepare
                    && completion.package_id == original.package_id
                    && completion.source_id == original.source_id
                    && completion.target == original.target
                    && matches!(self.phase, Phase::Restoring | Phase::Completed),
                "completion changed channel/source or precedes preparation"
            );
        }
        if let Some(path) = &self.path_edit {
            // Reuse the exact typed fixed-adapter 64 KiB input check, including
            // SID/schema/punctuation. Journal capacity alone is insufficient.
            WriteIntent::new(&request.sid, path.expected.clone(), path.desired.clone())?;
            let (receipt, removal) = if request.kind == Kind::Uninstall {
                (self.original_receipt.as_ref().unwrap(), true)
            } else {
                (
                    self.new_receipt
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("PATH edit has no owned receipt"))?,
                    false,
                )
            };
            let claim = receipt
                .user_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("PATH edit has no literal owned insertion"))?;
            let before = PathValue {
                value: claim.before.clone(),
                kind: claim.before_kind,
            };
            let after = PathValue {
                value: Some(claim.after.clone()),
                kind: Some(claim.after_kind),
            };
            ensure!(
                path.expected
                    == if removal {
                        after.clone()
                    } else {
                        before.clone()
                    }
                    && path.desired == if removal { before } else { after },
                "PATH plan differs from its exact raw value/kind receipt"
            );
        }
        Ok(())
    }

    fn validate_next_binding(&self, request: &Request) -> Result<()> {
        let current = self.current_service.as_ref().unwrap();
        if !matches!(current.phase(), "restoring" | "restored") {
            return Ok(());
        }
        let (path, identity) = match self.phase {
            Phase::Restoring if request.kind == Kind::MaintenancePrepare => (
                self.completion
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!("service restoration has no verified package completion")
                    })?
                    .executable
                    .as_str(),
                self.completion_identity.as_ref().unwrap(),
            ),
            Phase::Restoring => (
                self.new_receipt
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!("service restoration has no durable new receipt")
                    })?
                    .executable
                    .as_str(),
                self.leaves["locron.exe"].created.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "service restoration has no durable created executable identity"
                    )
                })?,
            ),
            Phase::RollingBack => {
                ensure!(
                    self.leaves
                        .values()
                        .all(|leaf| leaf.phase == LeafPhase::RolledBack),
                    "old-role restoration precedes complete verified original payloads/receipt"
                );
                if let Some(path) = &self.path_edit {
                    ensure!(
                        matches!(path.phase, PathPhase::Restored | PathPhase::Retained),
                        "old-role restoration precedes confirmed conditional PATH rollback"
                    );
                }
                (
                    self.original_receipt
                        .as_ref()
                        .ok_or_else(|| {
                            anyhow::anyhow!("rollback has no verified original receipt")
                        })?
                        .executable
                        .as_str(),
                    self.leaves["locron.exe"].rollback.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("rollback has no verified restored executable identity")
                    })?,
                )
            }
            _ => anyhow::bail!("service next binding has no matching restoration phase"),
        };
        let next = current
            .next_path()
            .ok_or_else(|| anyhow::anyhow!("service restoration lacks its actual typed next path"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-Unicode next service path"))?;
        ensure!(
            same_path(next, path)? && current.matches_next_identity(&identity.file_identity()?),
            "service next path/full identity differs from the verified restoration inventory"
        );
        Ok(())
    }

    fn validate_leaf_phases(&self, kind: Kind) -> Result<()> {
        let backed_up = self
            .leaves
            .values()
            .all(|leaf| leaf.phase == LeafPhase::BackedUp);
        let verified = self
            .leaves
            .values()
            .all(|leaf| leaf.phase == LeafPhase::Verified);
        let absent = self
            .leaves
            .values()
            .all(|leaf| leaf.phase == LeafPhase::Absent);
        let rollback = self
            .leaves
            .values()
            .all(|leaf| leaf.phase == LeafPhase::RolledBack);
        match self.phase {
            Phase::Accepted => ensure!(
                self.leaves
                    .values()
                    .all(|leaf| leaf.phase == LeafPhase::Original),
                "premature leaf changes"
            ),
            Phase::BackingUp => ensure!(
                self.leaves.values().all(|leaf| matches!(
                    leaf.phase,
                    LeafPhase::Original | LeafPhase::BackupIntent | LeafPhase::BackedUp
                )),
                "backup phase contains destructive work"
            ),
            Phase::Quiescing | Phase::Ready => ensure!(
                backed_up,
                "quiescence began before complete durable backups"
            ),
            Phase::Replacing => ensure!(
                matches!(kind, Kind::Install | Kind::SelfUpdate)
                    && self.leaves.values().all(|leaf| matches!(
                        leaf.phase,
                        LeafPhase::BackedUp
                            | LeafPhase::DeleteIntent
                            | LeafPhase::Absent
                            | LeafPhase::CreateIntent
                            | LeafPhase::Created
                            | LeafPhase::Verified
                    )),
                "wrong replacement channel/leaf phase"
            ),
            Phase::Restoring | Phase::Completed => ensure!(
                matches!(kind, Kind::Install | Kind::SelfUpdate) && verified
                    || kind == Kind::MaintenancePrepare && self.completion.is_some(),
                "restoration precedes verified complete new bytes/receipt"
            ),
            Phase::Prepared => ensure!(
                kind == Kind::MaintenancePrepare && self.completion.is_none(),
                "wrong preparation terminal"
            ),
            Phase::Removing | Phase::Removed => {
                ensure!(
                    matches!(kind, Kind::Uninstall | Kind::MaintenanceRemove),
                    "wrong removal channel"
                );
                ensure!(
                    self.leaves.values().all(|leaf| matches!(
                        leaf.phase,
                        LeafPhase::BackedUp | LeafPhase::DeleteIntent | LeafPhase::Absent
                    )),
                    "removal contains replacement/rollback work"
                );
                if self.phase == Phase::Removed {
                    ensure!(absent, "removal terminal retains an owned leaf");
                }
            }
            Phase::RollingBack => {}
            Phase::RolledBack => ensure!(
                rollback,
                "rollback terminal lacks verified original objects"
            ),
        }
        if let Some(path) = &self.path_edit {
            let allowed = match path.phase {
                PathPhase::Planned => !matches!(
                    self.phase,
                    Phase::Restoring | Phase::Completed | Phase::Removed | Phase::RolledBack
                ),
                PathPhase::WriteIntent => matches!(
                    self.phase,
                    Phase::Replacing | Phase::Removing | Phase::RollingBack
                ),
                PathPhase::Written => matches!(
                    self.phase,
                    Phase::Replacing
                        | Phase::Removing
                        | Phase::Restoring
                        | Phase::Completed
                        | Phase::Removed
                        | Phase::RollingBack
                ),
                PathPhase::RollbackIntent | PathPhase::Restored => {
                    matches!(self.phase, Phase::RollingBack | Phase::RolledBack)
                }
                PathPhase::Retained => matches!(
                    self.phase,
                    Phase::Removing | Phase::Removed | Phase::RollingBack | Phase::RolledBack
                ),
            };
            ensure!(allowed, "PATH phase does not match transaction phase");
            if matches!(self.phase, Phase::Completed | Phase::Removed) {
                ensure!(
                    matches!(path.phase, PathPhase::Written | PathPhase::Retained),
                    "PATH terminal is uncertain"
                );
            }
        }
        Ok(())
    }

    fn validate_successor(&self, previous: &Self) -> Result<()> {
        ensure!(
            self.sequence
                == previous
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("sequence overflow"))?
                && self.schema == previous.schema
                && self.operation_id == previous.operation_id
                && self.kind == previous.kind
                && self.protected_request_sha256 == previous.protected_request_sha256
                && self.original_executable == previous.original_executable
                && same(&self.original_service, &previous.original_service)?
                && same(&self.original_receipt, &previous.original_receipt)?
                && same(&self.new_receipt, &previous.new_receipt)?
                && same(&self.original_package, &previous.original_package)?
                && self.retained == previous.retained
                && self.leaves.keys().eq(previous.leaves.keys()),
            "transaction sequence or frozen origin changed"
        );
        let current = self.current_service.as_ref().unwrap();
        let prior_service = previous
            .current_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("previous frame lacks its real typed service record"))?;
        current.validate_successor(prior_service)?;
        let service_changed = current != prior_service;
        if previous.completion.is_some() {
            ensure!(
                same(&self.completion, &previous.completion)?
                    && self.completion_identity == previous.completion_identity,
                "verified completion binding changed"
            );
        } else if self.completion.is_some() {
            ensure!(
                previous.phase == Phase::Prepared && self.phase == Phase::Restoring,
                "package completion skipped prepared state"
            );
        }
        let allowed = self.phase == previous.phase
            && !matches!(
                self.phase,
                Phase::Completed | Phase::Removed | Phase::RolledBack
            )
            || matches!(
                (previous.phase, self.phase),
                (Phase::Accepted, Phase::BackingUp | Phase::Quiescing)
                    | (Phase::BackingUp, Phase::Quiescing)
                    | (Phase::Quiescing, Phase::Ready)
                    | (
                        Phase::Ready,
                        Phase::Replacing | Phase::Removing | Phase::Prepared
                    )
                    | (Phase::Replacing, Phase::Restoring)
                    | (Phase::Prepared, Phase::Restoring)
                    | (Phase::Restoring, Phase::Completed)
                    | (Phase::Removing, Phase::Removed)
                    | (
                        Phase::Accepted
                            | Phase::BackingUp
                            | Phase::Quiescing
                            | Phase::Ready
                            | Phase::Replacing,
                        Phase::RollingBack
                    )
                    | (Phase::RollingBack, Phase::RolledBack)
            );
        ensure!(allowed, "transaction skipped or rewound its outer phase");
        let mut changed = 0;
        for (name, leaf) in &self.leaves {
            let old = &previous.leaves[name];
            leaf.validate_successor(old)?;
            changed += usize::from(leaf != old);
        }
        ensure!(
            changed <= 1
                && (changed == 0 || self.phase == previous.phase)
                && (!service_changed || changed == 0 && self.phase == previous.phase),
            "multiple leaf effects in one frame"
        );
        match (&previous.path_edit, &self.path_edit) {
            (None, None) => {}
            (Some(old), Some(next)) => {
                ensure!(
                    next.expected == old.expected && next.desired == old.desired,
                    "PATH plan changed"
                );
                ensure!(
                    next.phase == old.phase
                        || matches!(
                            (old.phase, next.phase),
                            (
                                PathPhase::Planned,
                                PathPhase::WriteIntent | PathPhase::Retained
                            ) | (PathPhase::WriteIntent, PathPhase::Written)
                                | (PathPhase::Written, PathPhase::RollbackIntent)
                                | (
                                    PathPhase::RollbackIntent,
                                    PathPhase::Restored | PathPhase::Retained
                                )
                        ),
                    "PATH skipped or rewound its phase"
                );
                ensure!(
                    next.phase == old.phase
                        || changed == 0 && !service_changed && self.phase == previous.phase,
                    "combined PATH and file/phase effects"
                );
            }
            _ => anyhow::bail!("frozen optional PATH plan changed"),
        }
        Ok(())
    }

    /// Pure complete forward plus rollback reservation, before Journal::create.
    pub(super) fn preflight(&self, request: &Request) -> Result<Reservation<Self>> {
        self.validate(None, request, &self.protected_request_sha256)?;
        let service = self.original_service.as_ref().unwrap();
        let plan = service.persistence_plan()?;
        let mut worst = self.clone();
        // This conservative size projection deliberately combines future
        // nullable identities. It is never an admissible replay frame.
        worst.sequence = 127;
        worst.phase = Phase::RollingBack;
        let identity = Identity::from_file(FileIdentity {
            volume_serial_number: u64::MAX,
            file_id: u128::MAX,
        });
        for leaf in worst.leaves.values_mut() {
            leaf.backup = Some(FileFact {
                identity: identity.clone(),
                content: leaf.original.content.clone(),
            });
            leaf.created = Some(identity.clone());
            leaf.rollback = Some(identity.clone());
            leaf.phase = LeafPhase::RollbackDeleteIntent;
        }
        if let Some(path) = &mut worst.path_edit {
            path.phase = PathPhase::RollbackIntent;
        }
        // Both fixed nullable service keys contain actual serde objects. Future
        // object growth comes only from the real provider's checked bound.
        let mut growth = object_growth(worst.original_service.as_ref(), plan.max_record_bytes, 1)?
            .checked_add(object_growth(
                worst.current_service.as_ref(),
                plan.max_record_bytes,
                1,
            )?)
            .ok_or_else(|| anyhow::anyhow!("repeated service reservation overflow"))?;
        if request.kind == Kind::MaintenancePrepare {
            let mut future = worst.original_package.as_ref().unwrap().clone();
            future.key = "\"".repeat(255); // At most 255 UTF-8 bytes, each at most two JSON bytes.
            future.version = format!("{0}.{0}.{0}", u64::MAX);
            future.install_location = r"\\?\C:\x".into();
            future.executable = r"\\?\C:\x\locron.exe".into();
            for path in [&future.install_location, &future.executable] {
                growth = growth
                    .checked_add(
                        MAINTENANCE_PATH_JSON_BYTES
                            .checked_sub(serde_json::to_vec(path)?.len())
                            .ok_or_else(|| {
                                anyhow::anyhow!("future path baseline exceeds maximum")
                            })?,
                    )
                    .ok_or_else(|| anyhow::anyhow!("future package reservation overflow"))?;
            }
            future.binary_sha256 = "f".repeat(64);
            future.archive_sha256 = "f".repeat(64);
            worst.completion = Some(future);
            worst.completion_identity = Some(identity);
        }
        let restore = plan.restore_callbacks;
        let service_frames = if matches!(request.kind, Kind::Uninstall | Kind::MaintenanceRemove) {
            plan.quiesce_callbacks
                .checked_add(plan.remove_callbacks)
                .and_then(|count| count.checked_add(restore))
        } else {
            plan.quiesce_callbacks.checked_add(
                restore
                    .checked_mul(2)
                    .ok_or_else(|| anyhow::anyhow!("restore count overflow"))?,
            )
        }
        .ok_or_else(|| anyhow::anyhow!("service callback reservation overflow"))?;
        let per_leaf = if self.new_receipt.is_some() {
            12_usize
        } else {
            9_usize
        };
        let frames = self.leaves.len().checked_mul(per_leaf)
            .and_then(|count| count.checked_add(service_frames))
            .and_then(|count| count.checked_add(10)) // Initial and every forward/rollback outer boundary.
            .and_then(|count| count.checked_add(if self.path_edit.is_some() { 4 } else { 0 }))
            .ok_or_else(|| anyhow::anyhow!("complete transaction callback reservation overflow"))?;
        preflight_with_growth(&worst, frames, growth)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::super::windows_protocol::native_target;
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const TARGET: &str = "x86_64-pc-windows-msvc";
    const DIRECTORY: &str = r"C:\test-owned\locron";

    fn identity(number: u128) -> Identity {
        Identity::from_file(FileIdentity {
            volume_serial_number: u64::MAX,
            file_id: number,
        })
    }

    fn service(executable: &str, roles: usize) -> ServiceRestoreRecord {
        serde_json::from_value(json!({
            "version":1, "sid":SID,
            "previous":{"path":executable, "volume":"ffffffffffffffff", "file":format!("{:032x}", 1_u128)},
            "roles":(0..roles).map(|index| {
                let instance = format!("{index:064x}");
                json!({"role":"daemon", "root":format!(r"C:\test-owned\state{index}"),
                    "instance":instance, "task_name":format!("Locron-v1-{instance}-daemon"),
                    "enabled":true, "definition":"a".repeat(64), "future_definition":null,
                    "progress":"original"})
            }).collect::<Vec<_>>(),
            "phase":"snapshot", "next":null, "forced":[],
        })).unwrap()
    }

    fn receipt(version: &str, hash: &str) -> Receipt {
        Receipt {
            schema: "locron.install/windows-v1".into(),
            sid: SID.into(),
            channel: "standalone".into(),
            directory: DIRECTORY.into(),
            executable: format!(r"{DIRECTORY}\locron.exe"),
            target: TARGET.into(),
            version: version.into(),
            archive_url: format!(
                "https://github.com/WhiteKiwi/locron/releases/download/v{version}/locron-v{version}-{TARGET}.zip"
            ),
            archive_sha256: hash.into(),
            binary_sha256: hash.into(),
            files: PAYLOADS
                .into_iter()
                .map(|name| (name.to_owned(), hash.to_owned()))
                .collect(),
            user_path: None,
        }
    }

    fn request(kind: Kind, executable: String) -> Request {
        Request {
            schema: "locron.windows-operation/v1".into(),
            operation_id: Uuid::now_v7(),
            kind,
            sid: SID.into(),
            executable,
            target: TARGET.into(),
            version: "0.10.1".into(),
            archive_sha256: "b".repeat(64),
            helper_sha256: "c".repeat(64),
            caller_pid: Some(42),
            no_service: false,
            dashboard: false,
            add_to_path: false,
            state_root: None,
            package: None,
            archive_file: None,
        }
    }

    fn update(roles: usize) -> (Request, Record) {
        let old = receipt("0.10.0", &"a".repeat(64));
        let new = receipt("0.10.1", &"b".repeat(64));
        let request = request(Kind::SelfUpdate, old.executable.clone());
        let old_bytes = serde_json::to_vec(&old).unwrap();
        let new_bytes = serde_json::to_vec(&new).unwrap();
        let mut leaves = BTreeMap::new();
        for (index, name) in PAYLOADS.into_iter().chain([RECEIPT]).enumerate() {
            let (old_hash, old_size, new_hash, new_size) = if name == RECEIPT {
                (
                    sha256_hex(&old_bytes),
                    old_bytes.len() as u64,
                    sha256_hex(&new_bytes),
                    new_bytes.len() as u64,
                )
            } else {
                (old.files[name].clone(), 512, new.files[name].clone(), 1024)
            };
            leaves.insert(
                name.to_owned(),
                Leaf::original(
                    FileFact {
                        identity: identity(index as u128 + 1),
                        content: Content {
                            bytes: old_size,
                            sha256: old_hash,
                        },
                    },
                    Some(Content {
                        bytes: new_size,
                        sha256: new_hash,
                    }),
                ),
            );
        }
        let record = Record::existing(ExistingInputs {
            request: &request,
            protected_request_sha256: "d".repeat(64),
            original_executable: leaves["locron.exe"].original.identity.clone(),
            service: service(&request.executable, roles),
            original_receipt: Some(old),
            new_receipt: Some(new),
            leaves,
            retained: Vec::new(),
            path_edit: None,
        })
        .unwrap();
        (request, record)
    }

    fn maintenance(roles: usize) -> (Request, Record) {
        let mut request = request(Kind::MaintenancePrepare, format!(r"{DIRECTORY}\locron.exe"));
        request.package = Some(PackageBinding {
            key: "WhiteKiwi.locron_test".into(),
            package_id: "WhiteKiwi.locron".into(),
            source_id: "Microsoft.Winget.Source_8wekyb3d8bbwe".into(),
            install_location: DIRECTORY.into(),
            executable: request.executable.clone(),
            version: request.version.clone(),
            target: TARGET.into(),
            binary_sha256: "a".repeat(64),
            archive_sha256: request.archive_sha256.clone(),
        });
        let record = Record::existing(ExistingInputs {
            request: &request,
            protected_request_sha256: "d".repeat(64),
            original_executable: identity(1),
            service: service(&request.executable, roles),
            original_receipt: None,
            new_receipt: None,
            leaves: BTreeMap::new(),
            retained: Vec::new(),
            path_edit: None,
        })
        .unwrap();
        (request, record)
    }

    fn valid(record: &Record, previous: Option<&Record>, request: &Request) -> Result<()> {
        record.validate(previous, request, &"d".repeat(64))
    }

    #[test]
    fn concrete_original_slots_and_capacity_are_checked_without_effects() {
        native_target(TARGET).unwrap();
        let (request, original) = update(2);
        original.preflight(&request).unwrap();
        let value = serde_json::to_value(&original).unwrap();
        assert!(value["original_service"].is_object());
        assert!(value["current_service"].is_object());
        assert!(value["completion"].is_null());
        assert!(value["completion_identity"].is_null());
        assert!(value["path_edit"].is_null());
        assert_eq!(value["leaves"].as_object().unwrap().len(), 7);
        let (request, too_many_callbacks) = update(3);
        assert!(too_many_callbacks.preflight(&request).is_err());
        let (request, package) = maintenance(2);
        package.preflight(&request).unwrap();
        let mut escaped_large_key = package.clone();
        escaped_large_key.original_package.as_mut().unwrap().key = "\"".repeat(255);
        let mut matching_request = request.clone();
        matching_request.package = escaped_large_key.original_package.clone();
        escaped_large_key.preflight(&matching_request).unwrap();
    }

    #[test]
    fn unknown_duplicate_missing_and_foreign_origin_records_refuse() {
        let (request, original) = update(1);
        let value = serde_json::to_value(&original).unwrap();
        for field in [
            "original_service",
            "current_service",
            "completion",
            "path_edit",
        ] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Record>(changed).is_err(),
                "{field}"
            );
        }
        let mut extra = value.clone();
        extra["arbitrary_script"] = json!("never execute");
        assert!(serde_json::from_value::<Record>(extra).is_err());
        let text = serde_json::to_string(&original).unwrap();
        let leaf = serde_json::to_string(&original.leaves["README.md"]).unwrap();
        for altered in [
            text.replace("README.md", "README.MD"),
            text.replacen(
                r#""leaves":{"#,
                &format!(r#""leaves":{{"README.md":{leaf},"#),
                1,
            ),
            text.replace("README.md", "../outside"),
        ] {
            assert!(serde_json::from_str::<Record>(&altered).is_err());
        }
        let changes: [fn(&mut Record); 6] = [
            |record: &mut Record| record.protected_request_sha256 = "e".repeat(64),
            |record: &mut Record| record.original_executable.volume = "7fffffffffffffff".into(),
            |record: &mut Record| record.operation_id = Uuid::now_v7().to_string(),
            |record: &mut Record| record.schema = "locron.windows-transaction/v2".into(),
            |record: &mut Record| record.original_service = None,
            |record: &mut Record| record.original_receipt = None,
        ];
        for change in changes {
            let mut changed = original.clone();
            change(&mut changed);
            assert!(valid(&changed, None, &request).is_err());
        }
        let mut unknown = original.clone();
        unknown
            .leaves
            .insert("foreign".into(), original.leaves["README.md"].clone());
        assert!(valid(&unknown, None, &request).is_err()); // Result, never a map-index panic.
    }

    #[test]
    fn consecutive_frames_freeze_originals_and_allow_one_backup_boundary() {
        let (request, original) = update(1);
        let mut backing = original.clone();
        backing.sequence = 1;
        backing.phase = Phase::BackingUp;
        valid(&backing, Some(&original), &request).unwrap();
        let mut intent = backing.clone();
        intent.sequence = 2;
        intent.leaves.get_mut("README.md").unwrap().phase = LeafPhase::BackupIntent;
        valid(&intent, Some(&backing), &request).unwrap();
        let mut copied = intent.clone();
        copied.sequence = 3;
        let leaf = copied.leaves.get_mut("README.md").unwrap();
        leaf.backup = Some(FileFact {
            identity: identity(100),
            content: leaf.original.content.clone(),
        });
        leaf.phase = LeafPhase::BackedUp;
        valid(&copied, Some(&intent), &request).unwrap();
        let mut skipped = copied.clone();
        skipped.sequence = 5;
        assert!(valid(&skipped, Some(&copied), &request).is_err());
        let mut combined = backing.clone();
        combined.sequence = 2;
        for name in ["README.md", "LICENSE-MIT"] {
            combined.leaves.get_mut(name).unwrap().phase = LeafPhase::BackupIntent;
        }
        assert!(valid(&combined, Some(&backing), &request).is_err());
        let mut rewritten = copied.clone();
        rewritten.sequence = 4;
        rewritten
            .leaves
            .get_mut("README.md")
            .unwrap()
            .backup
            .as_mut()
            .unwrap()
            .identity = identity(101);
        assert!(valid(&rewritten, Some(&copied), &request).is_err());
    }

    #[test]
    fn actual_quiesce_and_bound_restoration_pass_but_activation_completion_refuses() {
        let (request, original) = maintenance(0);
        let mut quiescing = original.clone();
        quiescing.sequence = 1;
        quiescing.phase = Phase::Quiescing;
        valid(&quiescing, Some(&original), &request).unwrap();
        let mut stopped = quiescing.clone();
        stopped.sequence = 2;
        let mut typed = serde_json::to_value(stopped.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("quiescent");
        stopped.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&stopped, Some(&quiescing), &request).unwrap();
        let mut ready = stopped.clone();
        ready.sequence = 3;
        ready.phase = Phase::Ready;
        valid(&ready, Some(&stopped), &request).unwrap();
        let mut prepared = ready.clone();
        prepared.sequence = 4;
        prepared.phase = Phase::Prepared;
        valid(&prepared, Some(&ready), &request).unwrap();
        let mut restoring = prepared.clone();
        restoring.sequence = 5;
        restoring.phase = Phase::Restoring;
        restoring.completion = restoring.original_package.clone();
        restoring.completion_identity = Some(identity(200));
        valid(&restoring, Some(&prepared), &request).unwrap();
        let mut next = restoring.clone();
        next.sequence = 6;
        let mut typed = serde_json::to_value(next.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("restoring");
        typed["next"] = json!({"path":request.executable,"volume":"ffffffffffffffff","file":format!("{:032x}",200_u128)});
        next.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&next, Some(&restoring), &request).unwrap();
        for (field, wrong) in [
            ("path", json!(r"C:\foreign\locron.exe")),
            ("volume", json!("7fffffffffffffff")),
            ("file", json!(format!("{:032x}", 200_u128 ^ (1 << 127)))),
            ("file", json!(format!("{:032x}", 201_u128))),
        ] {
            let mut unbound = next.clone();
            let mut typed =
                serde_json::to_value(unbound.current_service.as_ref().unwrap()).unwrap();
            typed["next"][field] = wrong;
            unbound.current_service = Some(serde_json::from_value(typed).unwrap());
            assert!(valid(&unbound, Some(&restoring), &request).is_err());
        }
        let mut restored = next.clone();
        restored.sequence = 7;
        let mut typed = serde_json::to_value(restored.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("restored");
        restored.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&restored, Some(&next), &request).unwrap();
        let mut optimistic = restored.clone();
        optimistic.phase = Phase::Completed;
        optimistic.sequence = 8;
        assert!(valid(&optimistic, Some(&restored), &request).is_err());
        let mut rollback = restoring.clone();
        rollback.phase = Phase::RollingBack;
        rollback.sequence = 6;
        assert!(
            rollback
                .validate_successor(&restoring)
                .unwrap_err()
                .to_string()
                .contains("outer phase")
        );
    }

    #[test]
    fn standalone_restoration_binds_the_created_object_after_every_payload_and_receipt() {
        let (request, original) = update(0);
        let mut frame = original.clone();
        frame.sequence += 1;
        frame.phase = Phase::BackingUp;
        valid(&frame, Some(&original), &request).unwrap();
        let names = frame.leaves.keys().cloned().collect::<Vec<_>>();
        for (index, name) in names.iter().enumerate() {
            let previous = frame.clone();
            frame.sequence += 1;
            frame.leaves.get_mut(name).unwrap().phase = LeafPhase::BackupIntent;
            valid(&frame, Some(&previous), &request).unwrap();
            let previous = frame.clone();
            frame.sequence += 1;
            let leaf = frame.leaves.get_mut(name).unwrap();
            leaf.backup = Some(FileFact {
                identity: identity(100 + index as u128),
                content: leaf.original.content.clone(),
            });
            leaf.phase = LeafPhase::BackedUp;
            valid(&frame, Some(&previous), &request).unwrap();
        }
        let previous = frame.clone();
        frame.sequence += 1;
        frame.phase = Phase::Quiescing;
        valid(&frame, Some(&previous), &request).unwrap();
        let previous = frame.clone();
        frame.sequence += 1;
        let mut typed = serde_json::to_value(frame.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("quiescent");
        frame.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&frame, Some(&previous), &request).unwrap();
        for phase in [Phase::Ready, Phase::Replacing] {
            let previous = frame.clone();
            frame.sequence += 1;
            frame.phase = phase;
            valid(&frame, Some(&previous), &request).unwrap();
        }
        let rollback_base = frame.clone();
        for (index, name) in names.iter().enumerate() {
            for phase in [
                LeafPhase::DeleteIntent,
                LeafPhase::Absent,
                LeafPhase::CreateIntent,
                LeafPhase::Created,
                LeafPhase::Verified,
            ] {
                let previous = frame.clone();
                frame.sequence += 1;
                let leaf = frame.leaves.get_mut(name).unwrap();
                leaf.phase = phase;
                if phase == LeafPhase::Created {
                    leaf.created = Some(identity(200 + index as u128));
                }
                valid(&frame, Some(&previous), &request).unwrap();
            }
        }
        let previous = frame.clone();
        frame.sequence += 1;
        frame.phase = Phase::Restoring;
        valid(&frame, Some(&previous), &request).unwrap();
        let previous = frame.clone();
        frame.sequence += 1;
        let mut typed = serde_json::to_value(frame.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("restoring");
        typed["next"] = json!({"path":request.executable,
            "volume":frame.leaves["locron.exe"].created.as_ref().unwrap().volume,
            "file":frame.leaves["locron.exe"].created.as_ref().unwrap().file});
        frame.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&frame, Some(&previous), &request).unwrap();
        let mut early = frame.clone();
        early.leaves.get_mut(RECEIPT).unwrap().phase = LeafPhase::Created;
        assert!(valid(&early, Some(&previous), &request).is_err());
        let mut alias = frame.clone();
        let aliased_identity = alias.leaves["locron.exe"].created.clone();
        alias.leaves.get_mut("README.md").unwrap().created = aliased_identity;
        assert!(valid(&alias, Some(&previous), &request).is_err());

        let mut rollback = rollback_base.clone();
        rollback.sequence += 1;
        rollback.phase = Phase::RollingBack;
        valid(&rollback, Some(&rollback_base), &request).unwrap();
        let original_next = json!({"path":request.executable,
            "volume":rollback.original_executable.volume,"file":rollback.original_executable.file});
        let mut early_roles = rollback.clone();
        early_roles.sequence += 1;
        let mut typed =
            serde_json::to_value(early_roles.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("restoring");
        typed["next"] = original_next.clone();
        early_roles.current_service = Some(serde_json::from_value(typed).unwrap());
        assert!(valid(&early_roles, Some(&rollback), &request).is_err());
        for name in &names {
            let previous = rollback.clone();
            rollback.sequence += 1;
            let leaf = rollback.leaves.get_mut(name).unwrap();
            leaf.phase = LeafPhase::RolledBack;
            leaf.rollback = Some(leaf.original.identity.clone());
            valid(&rollback, Some(&previous), &request).unwrap();
        }
        let previous = rollback.clone();
        rollback.sequence += 1;
        let mut typed = serde_json::to_value(rollback.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("restoring");
        typed["next"] = original_next;
        rollback.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&rollback, Some(&previous), &request).unwrap();
    }

    #[test]
    fn actual_empty_inventory_remove_successors_do_not_skip_intent_or_terminal() {
        let (mut request, template) = maintenance(0);
        request.kind = Kind::MaintenanceRemove;
        let original = Record::existing(ExistingInputs {
            request: &request,
            protected_request_sha256: "d".repeat(64),
            original_executable: template.original_executable,
            service: template.original_service.unwrap(),
            original_receipt: None,
            new_receipt: None,
            leaves: BTreeMap::new(),
            retained: Vec::new(),
            path_edit: None,
        })
        .unwrap();
        let mut quiescing = original.clone();
        quiescing.sequence = 1;
        quiescing.phase = Phase::Quiescing;
        valid(&quiescing, Some(&original), &request).unwrap();
        let mut stopped = quiescing.clone();
        stopped.sequence = 2;
        let mut typed = serde_json::to_value(stopped.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("quiescent");
        stopped.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&stopped, Some(&quiescing), &request).unwrap();
        let mut ready = stopped.clone();
        ready.sequence = 3;
        ready.phase = Phase::Ready;
        valid(&ready, Some(&stopped), &request).unwrap();
        let mut removing = ready.clone();
        removing.sequence = 4;
        removing.phase = Phase::Removing;
        valid(&removing, Some(&ready), &request).unwrap();
        let mut intent = removing.clone();
        intent.sequence = 5;
        let mut typed = serde_json::to_value(intent.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("removing");
        intent.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&intent, Some(&removing), &request).unwrap();
        let mut finished = intent.clone();
        finished.sequence = 6;
        let mut typed = serde_json::to_value(finished.current_service.as_ref().unwrap()).unwrap();
        typed["phase"] = json!("removed");
        finished.current_service = Some(serde_json::from_value(typed).unwrap());
        valid(&finished, Some(&intent), &request).unwrap();
        assert!(valid(&finished, Some(&removing), &request).is_err());
        let mut terminal = finished.clone();
        terminal.sequence = 7;
        terminal.phase = Phase::Removed;
        valid(&terminal, Some(&finished), &request).unwrap();
        let mut duplicate = terminal.clone();
        duplicate.sequence = 8;
        assert!(valid(&duplicate, Some(&terminal), &request).is_err());
    }

    #[test]
    fn leaf_identity_boundaries_never_adopt_unknown_or_rewritten_objects() {
        let (_, original) = update(0);
        let mut leaf = original.leaves["README.md"].clone();
        let mut predecessor = leaf.clone();
        leaf.phase = LeafPhase::BackedUp;
        leaf.backup = Some(FileFact {
            identity: identity(20),
            content: leaf.original.content.clone(),
        });
        assert!(leaf.validate_successor(&predecessor).is_err());
        predecessor.phase = LeafPhase::BackupIntent;
        leaf.validate("README.md").unwrap();
        leaf.validate_successor(&predecessor).unwrap();
        predecessor = leaf.clone();
        leaf.phase = LeafPhase::Created;
        leaf.created = Some(identity(21));
        assert!(leaf.validate_successor(&predecessor).is_err());
        predecessor = leaf.clone();
        predecessor.phase = LeafPhase::CreateIntent;
        predecessor.created = None;
        leaf.validate_successor(&predecessor).unwrap();
        let mut future = leaf.clone();
        future.phase = LeafPhase::Verified;
        future.created = Some(identity(22));
        assert!(future.validate_successor(&leaf).is_err());
        let mut premature = original.leaves["README.md"].clone();
        premature.phase = LeafPhase::BackupIntent;
        premature.created = Some(identity(99));
        assert!(premature.validate("README.md").is_err());
        let mut short_identity = identity(1);
        short_identity.file = "f".repeat(16);
        assert!(short_identity.file_identity().is_err());
        let full = Identity::from_file(FileIdentity {
            volume_serial_number: u64::MAX,
            file_id: u128::MAX,
        });
        assert_eq!(full.file_identity().unwrap().file_id, u128::MAX);
    }

    #[test]
    fn partial_removal_retains_companions_without_turning_them_into_leaf_authority() {
        let (_, update) = update(0);
        let old = update.original_receipt.as_ref().unwrap().clone();
        let mut request = request(Kind::Uninstall, old.executable.clone());
        request.version = old.version.clone();
        request.archive_sha256 = old.archive_sha256.clone();
        let mut leaves = update.leaves.clone();
        for leaf in leaves.values_mut() {
            leaf.desired = None;
        }
        leaves.remove("README.md");
        let record = Record::existing(ExistingInputs {
            request: &request,
            protected_request_sha256: "d".repeat(64),
            original_executable: update.original_executable.clone(),
            service: update.original_service.as_ref().unwrap().clone(),
            original_receipt: Some(old),
            new_receipt: None,
            leaves,
            retained: vec![RetainedPayload {
                name: "README.md".into(),
                reason: OwnedRetention::Changed,
            }],
            path_edit: None,
        })
        .unwrap();
        record.preflight(&request).unwrap();
        assert!(!record.leaves.contains_key("README.md"));
        assert_eq!(record.retained.len(), 1);
        let mut invented = record.clone();
        invented
            .retained
            .insert("foreign.txt".into(), Retention::Unverifiable);
        assert!(valid(&invented, None, &request).is_err());
        let mut ambiguous = record.clone();
        ambiguous
            .leaves
            .insert("README.md".into(), update.leaves["README.md"].clone());
        assert!(valid(&ambiguous, None, &request).is_err());
        let mut alias = record.clone();
        alias
            .leaves
            .get_mut("LICENSE-MIT")
            .unwrap()
            .original
            .identity = alias.original_executable.clone();
        assert!(valid(&alias, None, &request).is_err());
    }
}
