//! Normalized application command values shared by all future surfaces.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::id::{JobId, RunId};
use crate::policy::ExecutionPolicy;
use crate::schedule::Schedule;
use crate::target::{Environment, Target};
use crate::{Timestamp, ValidationError};

/// Immutable normalized job definition stored in each revision and run snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddJob {
    pub id: JobId,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub definition: JobDefinition,
    pub created_at: Timestamp,
}

/// Durable manual-run request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualRun {
    pub run_id: RunId,
    pub job_id: JobId,
    pub requested_at: Timestamp,
}

/// Trigger identity for a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Scheduled,
    CatchUp,
    Manual,
}
