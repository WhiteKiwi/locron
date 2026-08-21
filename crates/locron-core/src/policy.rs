//! Retry, overlap, missed-run, timeout, and concurrency policy values.

use serde::{Deserialize, Serialize};

use crate::{DurationMicros, ValidationError};

/// Behavior when a normal occurrence conflicts with active same-job work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    /// Explain the new occurrence as skipped.
    #[default]
    Skip,
    /// Terminate current work and retain only the newest replacement.
    Replace,
    /// Permit concurrency up to the configured bound.
    Allow,
}

/// Behavior for occurrences elapsed without reconciliation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedRunPolicy {
    /// Materialize no catch-up run.
    #[default]
    Skip,
    /// Materialize only the latest eligible occurrence.
    Latest,
    /// Materialize the newest bounded window, oldest first.
    All,
}

/// Retry delay mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffMode {
    /// Constant delay.
    Fixed,
    /// Doubling delay capped at the configured maximum.
    #[default]
    Exponential,
}

/// Complete normalized execution policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub overlap: OverlapPolicy,
    pub missed_run: MissedRunPolicy,
    pub start_deadline: Option<DurationMicros>,
    pub catch_up_limit: u16,
    pub retries: u8,
    pub retry_delay: DurationMicros,
    pub retry_cap: DurationMicros,
    pub backoff: BackoffMode,
    pub retry_timeout: bool,
    pub timeout: Option<DurationMicros>,
    pub termination_grace: DurationMicros,
    pub per_job_concurrency: u8,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            overlap: OverlapPolicy::Skip,
            missed_run: MissedRunPolicy::Skip,
            start_deadline: None,
            catch_up_limit: 100,
            retries: 0,
            retry_delay: DurationMicros::new(10_000_000),
            retry_cap: DurationMicros::new(300_000_000),
            backoff: BackoffMode::Exponential,
            retry_timeout: false,
            timeout: Some(DurationMicros::new(60_000_000)),
            termination_grace: DurationMicros::new(5_000_000),
            per_job_concurrency: 1,
        }
    }
}

impl ExecutionPolicy {
    /// Checks all policy bounds and cross-field constraints.
    pub fn validate(&self, global_concurrency: u8) -> Result<(), ValidationError> {
        if !(1..=64).contains(&global_concurrency) {
            return Err(ValidationError::new(
                "global_concurrency",
                "out_of_range",
                "must be from 1 through 64",
            ));
        }
        if !(1..=1000).contains(&self.catch_up_limit) {
            return Err(ValidationError::new(
                "catch_up_limit",
                "out_of_range",
                "must be from 1 through 1000",
            ));
        }
        if self.retries > 10 {
            return Err(ValidationError::new(
                "retries",
                "out_of_range",
                "must be from 0 through 10",
            ));
        }
        let allowed = match self.overlap {
            OverlapPolicy::Allow => (2..=global_concurrency).contains(&self.per_job_concurrency),
            OverlapPolicy::Skip | OverlapPolicy::Replace => self.per_job_concurrency == 1,
        };
        if !allowed {
            return Err(ValidationError::new(
                "per_job_concurrency",
                "invalid_for_overlap",
                "skip/replace require 1; allow requires 2 through global concurrency",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_matches_overlap_policy() {
        let mut policy = ExecutionPolicy::default();
        assert!(policy.validate(16).is_ok());
        policy.overlap = OverlapPolicy::Allow;
        assert!(policy.validate(16).is_err());
        policy.per_job_concurrency = 2;
        assert!(policy.validate(16).is_ok());
        policy.per_job_concurrency = 17;
        assert!(policy.validate(16).is_err());
    }
}
