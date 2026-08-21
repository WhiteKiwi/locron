//! Normalized application command values shared by all future surfaces.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::id::{JobId, RunId};
use crate::lifecycle::RunState;
use crate::policy::ExecutionPolicy;
use crate::schedule::Schedule;
use crate::target::{Environment, Target};
use crate::{Timestamp, ValidationError};

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
}

/// Durable identity and revision produced by [`AddJob`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddJobResult {
    pub job_id: JobId,
    pub revision: u64,
}

impl ApplicationResult for AddJobResult {}

impl ApplicationCommand for AddJob {
    type Result = AddJobResult;
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

/// Trigger identity for a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Scheduled,
    CatchUp,
    Manual,
}
