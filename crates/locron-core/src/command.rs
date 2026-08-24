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
    /// Statically paired result type produced by applying this command.
    type Result: ApplicationResult;
}

/// Stable result returned by an application command.
pub trait ApplicationResult: Send + 'static {}

/// Immutable normalized job definition stored in each revision and run snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobDefinition {
    /// Recurrence rule that generates run occurrences.
    pub schedule: Schedule,
    /// Executable, shell, or HTTP destination for runs.
    pub target: Target,
    /// Absolute working directory used for executions.
    pub cwd: PathBuf,
    /// Environment values applied to executions.
    pub environment: Environment,
    /// Concurrency, overlap, and retry behavior for runs.
    pub policy: ExecutionPolicy,
    /// Action to apply when a one-time scheduled run reaches completion.
    #[serde(default)]
    pub completion_action: CompletionAction,
}

/// Lifetime action for a completed one-time schedule.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionAction {
    /// Keep the disabled job definition after its one-time occurrence resolves.
    #[default]
    Retain,
    /// Soft-remove the definition after its scheduled one-time run is terminal.
    Delete,
}

impl JobDefinition {
    /// Validates all independent and cross-field invariants.
    pub fn validate(&self, global_concurrency: u8) -> Result<(), ValidationError> {
        self.schedule.validate()?;
        self.target.validate()?;
        self.environment.validate()?;
        self.policy.validate(global_concurrency)?;
        if self.completion_action == CompletionAction::Delete
            && !matches!(self.schedule, Schedule::At { .. })
        {
            return Err(ValidationError::new(
                "completion_action",
                "one_time_schedule_required",
                "delete completion action requires a one-time schedule",
            ));
        }
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
    /// Globally unique identity for the new job.
    pub id: JobId,
    /// Human-readable unique name used to reference the job.
    pub name: String,
    /// Optional free-form description.
    pub description: Option<String>,
    /// Free-form labels used for grouping and selection.
    pub tags: Vec<String>,
    /// Whether the job accepts new runs immediately.
    pub enabled: bool,
    /// Immutable definition captured in the first revision.
    pub definition: JobDefinition,
    /// Durable creation instant.
    pub created_at: Timestamp,
    /// Durable schedule cursor position at creation.
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
    /// Identity assigned to the created job.
    pub job_id: JobId,
    /// Revision number of the first immutable definition.
    pub revision: RevisionNumber,
}

impl ApplicationResult for AddJobResult {}

impl ApplicationCommand for AddJob {
    type Result = AddJobResult;
}

/// Replace the editable job metadata and create a new immutable revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateJob {
    /// Identity of the job being updated.
    pub job_id: JobId,
    /// Revision the update must be based on (optimistic concurrency).
    pub expected_revision: RevisionNumber,
    /// New human-readable name for the job.
    pub name: String,
    /// New optional description.
    pub description: Option<String>,
    /// New label set.
    pub tags: Vec<String>,
    /// Whether the job accepts new runs.
    pub enabled: bool,
    /// New immutable definition recorded as the next revision.
    pub definition: JobDefinition,
    /// Durable update instant.
    pub updated_at: Timestamp,
    /// Durable schedule cursor position at update.
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
    /// Identity of the updated job.
    pub job_id: JobId,
    /// New revision number produced by the update.
    pub revision: RevisionNumber,
}

impl ApplicationResult for UpdateJobResult {}

impl ApplicationCommand for UpdateJob {
    type Result = UpdateJobResult;
}

/// Enable or disable a live job without creating another revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetJobEnabled {
    /// Identity of the job being enabled or disabled.
    pub job_id: JobId,
    /// Whether the job accepts new runs.
    pub enabled: bool,
    /// Durable instant of the change.
    pub changed_at: Timestamp,
}

/// Durable state produced by [`SetJobEnabled`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SetJobEnabledResult {
    /// Identity of the changed job.
    pub job_id: JobId,
    /// Revision recorded for the change.
    pub revision: RevisionNumber,
    /// Effective enabled state after the change.
    pub enabled: bool,
}

impl ApplicationResult for SetJobEnabledResult {}

impl ApplicationCommand for SetJobEnabled {
    type Result = SetJobEnabledResult;
}

/// Soft-remove a live job while preserving its history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoveJob {
    /// Identity of the job being removed.
    pub job_id: JobId,
    /// Durable instant of removal.
    pub removed_at: Timestamp,
}

/// Durable identity produced by [`RemoveJob`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoveJobResult {
    /// Identity of the removed job.
    pub job_id: JobId,
}

impl ApplicationResult for RemoveJobResult {}

impl ApplicationCommand for RemoveJob {
    type Result = RemoveJobResult;
}

/// Durable manual-run request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManualRun {
    /// Identity of the new run.
    pub run_id: RunId,
    /// Identity of the job the run belongs to.
    pub job_id: JobId,
    /// Durable instant the run was requested.
    pub requested_at: Timestamp,
}

/// Durable run identity and initial policy decision produced by [`ManualRun`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManualRunResult {
    /// Identity of the created run.
    pub run_id: RunId,
    /// Initial lifecycle state assigned by the adapter.
    pub state: RunState,
}

impl ApplicationResult for ManualRunResult {}

impl ApplicationCommand for ManualRun {
    type Result = ManualRunResult;
}

/// Request cancellation or acknowledge an unconfirmed termination quarantine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelRun {
    /// Identity of the run being cancelled.
    pub run_id: RunId,
    /// Durable instant of the cancellation request.
    pub requested_at: Timestamp,
    /// Whether an unconfirmed-termination quarantine is being acknowledged.
    pub acknowledge_unconfirmed: bool,
}

/// Durable cancellation decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationDecision {
    /// The run was cancelled before it started.
    CancelledBeforeExecution,
    /// The cancellation request was recorded for the run.
    CancellationRequested,
    /// The unconfirmed termination was acknowledged.
    AcknowledgedUnconfirmed,
}

/// Result produced by [`CancelRun`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelRunResult {
    /// Identity of the run the decision applies to.
    pub run_id: RunId,
    /// Decision produced by the cancellation request.
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
    /// Global ceiling on concurrent attempts across all jobs.
    GlobalConcurrency(u8),
    /// Execution PATH passed to every process.
    ExecutionPath(String),
    /// Number of finished runs retained per job.
    RunRetentionCount(u64),
    /// Maximum age of retained finished runs.
    RunRetentionAge(DurationMicros),
    /// Global cap on total retained output bytes.
    OutputLimitBytes(u64),
    /// Cap on output bytes retained for a single run.
    PerRunOutputLimitBytes(u64),
    /// Set or clear one environment variable.
    Environment {
        /// Environment variable name.
        name: String,
        /// New value; `None` unsets the variable.
        value: Option<String>,
    },
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
    /// The configuration mutation to apply.
    pub change: ConfigurationChange,
    /// Durable instant of the change.
    pub changed_at: Timestamp,
}

/// Presentation- and storage-neutral global configuration snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    /// Global ceiling on concurrent attempts across all jobs.
    pub global_concurrency: u8,
    /// Execution PATH passed to every process.
    pub execution_path: String,
    /// Number of finished runs retained per job.
    pub run_retention_count: u64,
    /// Maximum age of retained finished runs (unbounded when `None`).
    pub run_retention_age: Option<DurationMicros>,
    /// Global cap on total retained output bytes.
    pub output_limit_bytes: u64,
    /// Cap on output bytes retained for a single run.
    pub per_run_output_limit_bytes: u64,
    /// Global environment snapshot applied to every execution.
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
    /// Occurrence produced by the recurring schedule.
    Scheduled,
    /// Occurrence materialized for an elapsed missed range.
    CatchUp,
    /// Occurrence requested by an operator.
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
