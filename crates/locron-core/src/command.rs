//! Normalized application command values shared by all future surfaces.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::id::{JobId, RevisionNumber, RunId};
use crate::lifecycle::RunState;
use crate::policy::ExecutionPolicy;
use crate::schedule::Schedule;
use crate::target::{Environment, Target, is_valid_environment_name};
use crate::{DurationMicros, Timestamp, ValidationError};

/// Normalized application command with a statically paired result type.
///
/// Persistence adapters implement the corresponding operation through
/// [`crate::ports::PersistencePort`] without exposing their storage model.
pub trait ApplicationCommand: Send + 'static {
    type Result: ApplicationResult;
}

/// Stable result returned by an application command.
pub trait ApplicationResult: Send + 'static {}

/// Immutable normalized job definition stored in each revision and run snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobDefinition {
    pub schedule: Schedule,
    pub target: Target,
    pub cwd: PathBuf,
    pub environment: Environment,
    pub policy: ExecutionPolicy,
}

impl JobDefinition {
    /// Validates all independent and cross-field invariants.
    pub fn validate(&self, global_concurrency: u8) -> Result<(), ValidationError> {
        self.schedule.validate()?;
        self.target.validate()?;
        self.environment.validate()?;
        self.policy.validate(global_concurrency)?;
        if !self.cwd.is_absolute() {
            return Err(ValidationError::new(
                "cwd",
                "absolute_path_required",
                "working directory must be absolute",
            ));
        }
        Ok(())
    }
}

/// Create a live job and its first immutable revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddJob {
    pub id: JobId,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub definition: JobDefinition,
    pub created_at: Timestamp,
    pub cursor_at: Timestamp,
}

impl AddJob {
    /// Validates the normalized command before it crosses a persistence port.
    pub fn validate(&self, global_concurrency: u8) -> Result<(), ValidationError> {
        validate_job_name(&self.name)?;
        self.definition.validate(global_concurrency)
    }
}

/// Durable identity and revision produced by [`AddJob`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddJobResult {
    pub job_id: JobId,
    pub revision: RevisionNumber,
}

impl ApplicationResult for AddJobResult {}

impl ApplicationCommand for AddJob {
    type Result = AddJobResult;
}

/// Replace the editable job metadata and create a new immutable revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateJob {
    pub job_id: JobId,
    pub expected_revision: RevisionNumber,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub definition: JobDefinition,
    pub updated_at: Timestamp,
    pub cursor_at: Timestamp,
}

impl UpdateJob {
    /// Validates the normalized command before it crosses a persistence port.
    pub fn validate(&self, global_concurrency: u8) -> Result<(), ValidationError> {
        validate_job_name(&self.name)?;
        self.definition.validate(global_concurrency)
    }
}

/// Durable identity and revision produced by [`UpdateJob`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateJobResult {
    pub job_id: JobId,
    pub revision: RevisionNumber,
}

impl ApplicationResult for UpdateJobResult {}

impl ApplicationCommand for UpdateJob {
    type Result = UpdateJobResult;
}

/// Enable or disable a live job without creating another revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetJobEnabled {
    pub job_id: JobId,
    pub enabled: bool,
    pub changed_at: Timestamp,
}

/// Durable state produced by [`SetJobEnabled`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetJobEnabledResult {
    pub job_id: JobId,
    pub revision: RevisionNumber,
    pub enabled: bool,
}

impl ApplicationResult for SetJobEnabledResult {}

impl ApplicationCommand for SetJobEnabled {
    type Result = SetJobEnabledResult;
}

/// Soft-remove a live job while preserving its history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoveJob {
    pub job_id: JobId,
    pub removed_at: Timestamp,
}

/// Durable identity produced by [`RemoveJob`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoveJobResult {
    pub job_id: JobId,
}

impl ApplicationResult for RemoveJobResult {}

impl ApplicationCommand for RemoveJob {
    type Result = RemoveJobResult;
}

/// Durable manual-run request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManualRun {
    pub run_id: RunId,
    pub job_id: JobId,
    pub requested_at: Timestamp,
}

/// Durable run identity and initial policy decision produced by [`ManualRun`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManualRunResult {
    pub run_id: RunId,
    pub state: RunState,
}

impl ApplicationResult for ManualRunResult {}

impl ApplicationCommand for ManualRun {
    type Result = ManualRunResult;
}

/// Request cancellation or acknowledge an unconfirmed termination quarantine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelRun {
    pub run_id: RunId,
    pub requested_at: Timestamp,
    pub acknowledge_unconfirmed: bool,
}

/// Durable cancellation decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationDecision {
    CancelledBeforeExecution,
    CancellationRequested,
    AcknowledgedUnconfirmed,
}

/// Result produced by [`CancelRun`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelRunResult {
    pub run_id: RunId,
    pub decision: CancellationDecision,
}

impl ApplicationResult for CancelRunResult {}

impl ApplicationCommand for CancelRun {
    type Result = CancelRunResult;
}

/// Typed mutation of one global configuration value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "key", content = "value", rename_all = "snake_case")]
pub enum ConfigurationChange {
    GlobalConcurrency(u8),
    ExecutionPath(String),
    RunRetentionCount(u64),
    RunRetentionAge(DurationMicros),
    OutputLimitBytes(u64),
    PerRunOutputLimitBytes(u64),
    Environment { name: String, value: Option<String> },
}

impl ConfigurationChange {
    /// Checks bounds and environment safety before persistence.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::GlobalConcurrency(value) if !(1..=64).contains(value) => {
                Err(ValidationError::new(
                    "global_concurrency",
                    "out_of_range",
                    "must be from 1 through 64",
                ))
            }
            Self::Environment { name, value }
                if !is_valid_environment_name(name)
                    || name.starts_with("LOCRON_")
                    || value.as_deref().is_some_and(|item| item.contains('\0')) =>
            {
                Err(ValidationError::new(
                    "environment",
                    "invalid_value",
                    "environment names and values must be safe and non-reserved",
                ))
            }
            Self::ExecutionPath(value) if value.contains('\0') => Err(ValidationError::new(
                "execution_path",
                "invalid_value",
                "execution PATH cannot contain NUL",
            )),
            _ => Ok(()),
        }
    }
}

/// Apply one normalized global configuration mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateConfiguration {
    pub change: ConfigurationChange,
    pub changed_at: Timestamp,
}

/// Presentation- and storage-neutral global configuration snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub global_concurrency: u8,
    pub execution_path: String,
    pub run_retention_count: u64,
    pub run_retention_age: Option<DurationMicros>,
    pub output_limit_bytes: u64,
    pub per_run_output_limit_bytes: u64,
    pub environment: BTreeMap<String, String>,
}

impl ApplicationResult for Configuration {}

impl ApplicationCommand for UpdateConfiguration {
    type Result = Configuration;
}

/// Trigger identity for a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Scheduled,
    CatchUp,
    Manual,
}

fn validate_job_name(name: &str) -> Result<(), ValidationError> {
    if name.trim().is_empty() || name.contains('\0') {
        return Err(ValidationError::new(
            "name",
            "invalid_name",
            "job name must be non-empty and cannot contain NUL",
        ));
    }
    Ok(())
}
