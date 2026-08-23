//! Explicit legal lifecycle transitions.

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Durable run lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Admitted to the durable queue but not yet started.
    Queued,
    /// Spawning or initializing its first attempt.
    Starting,
    /// Actively executing one attempt.
    Running,
    /// Waiting out the configured retry delay before the next attempt.
    RetryWait,
    /// A terminal attempt succeeded.
    Succeeded,
    /// A terminal attempt failed and no retry was scheduled.
    Failed,
    /// A terminal attempt exceeded its timeout.
    TimedOut,
    /// Cancellation was requested and accepted.
    Cancelled,
    /// Superseded or discarded due to an overlap conflict.
    SkippedOverlap,
    /// Not admitted because the per-job concurrency limit was reached.
    SkippedConcurrency,
    /// Termination could not be confirmed after an interruption.
    InterruptedUnknown,
}

impl RunState {
    /// Returns whether the state is terminal and no further transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::TimedOut
                | Self::Cancelled
                | Self::SkippedOverlap
                | Self::SkippedConcurrency
                | Self::InterruptedUnknown
        )
    }

    /// Enforces the documented run state machine.
    pub fn transition(self, next: Self) -> Result<Self, CoreError> {
        let legal = matches!(
            (self, next),
            (
                Self::Queued,
                Self::Starting | Self::Cancelled | Self::SkippedOverlap | Self::SkippedConcurrency
            ) | (
                Self::Starting,
                Self::Running | Self::Failed | Self::Cancelled | Self::InterruptedUnknown
            ) | (
                Self::Running,
                Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::RetryWait
                    | Self::InterruptedUnknown
            ) | (Self::RetryWait, Self::Starting | Self::Cancelled)
        );
        if legal {
            Ok(next)
        } else {
            Err(CoreError::InvalidTransition {
                from: format!("{self:?}"),
                to: format!("{next:?}"),
            })
        }
    }
}

/// Durable attempt lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    /// Spawned but not yet confirmed running.
    Starting,
    /// Actively executing the target.
    Running,
    /// The target completed successfully.
    Succeeded,
    /// The target failed.
    Failed,
    /// The target exceeded its timeout.
    TimedOut,
    /// Cancellation was accepted.
    Cancelled,
    /// Termination could not be confirmed after an interruption.
    InterruptedUnknown,
}

impl AttemptState {
    /// Enforces the documented attempt state machine.
    pub fn transition(self, next: Self) -> Result<Self, CoreError> {
        let legal = matches!(
            (self, next),
            (
                Self::Starting,
                Self::Running | Self::Failed | Self::Cancelled | Self::InterruptedUnknown
            ) | (
                Self::Running,
                Self::Succeeded
                    | Self::Failed
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::InterruptedUnknown
            )
        );
        if legal {
            Ok(next)
        } else {
            Err(CoreError::InvalidTransition {
                from: format!("{self:?}"),
                to: format!("{next:?}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_STATES: [RunState; 11] = [
        RunState::Queued,
        RunState::Starting,
        RunState::Running,
        RunState::RetryWait,
        RunState::Succeeded,
        RunState::Failed,
        RunState::TimedOut,
        RunState::Cancelled,
        RunState::SkippedOverlap,
        RunState::SkippedConcurrency,
        RunState::InterruptedUnknown,
    ];

    const LEGAL_RUN_TRANSITIONS: [(RunState, RunState); 16] = [
        (RunState::Queued, RunState::Starting),
        (RunState::Queued, RunState::Cancelled),
        (RunState::Queued, RunState::SkippedOverlap),
        (RunState::Queued, RunState::SkippedConcurrency),
        (RunState::Starting, RunState::Running),
        (RunState::Starting, RunState::Failed),
        (RunState::Starting, RunState::Cancelled),
        (RunState::Starting, RunState::InterruptedUnknown),
        (RunState::Running, RunState::Succeeded),
        (RunState::Running, RunState::Failed),
        (RunState::Running, RunState::TimedOut),
        (RunState::Running, RunState::Cancelled),
        (RunState::Running, RunState::RetryWait),
        (RunState::Running, RunState::InterruptedUnknown),
        (RunState::RetryWait, RunState::Starting),
        (RunState::RetryWait, RunState::Cancelled),
    ];

    const ATTEMPT_STATES: [AttemptState; 7] = [
        AttemptState::Starting,
        AttemptState::Running,
        AttemptState::Succeeded,
        AttemptState::Failed,
        AttemptState::TimedOut,
        AttemptState::Cancelled,
        AttemptState::InterruptedUnknown,
    ];

    const LEGAL_ATTEMPT_TRANSITIONS: [(AttemptState, AttemptState); 9] = [
        (AttemptState::Starting, AttemptState::Running),
        (AttemptState::Starting, AttemptState::Failed),
        (AttemptState::Starting, AttemptState::Cancelled),
        (AttemptState::Starting, AttemptState::InterruptedUnknown),
        (AttemptState::Running, AttemptState::Succeeded),
        (AttemptState::Running, AttemptState::Failed),
        (AttemptState::Running, AttemptState::TimedOut),
        (AttemptState::Running, AttemptState::Cancelled),
        (AttemptState::Running, AttemptState::InterruptedUnknown),
    ];

    #[test]
    fn every_run_transition_is_classified() {
        for from in RUN_STATES {
            for to in RUN_STATES {
                let expected = LEGAL_RUN_TRANSITIONS.contains(&(from, to));
                let actual = from.transition(to);
                assert_eq!(
                    actual.is_ok(),
                    expected,
                    "unexpected run transition classification: {from:?} -> {to:?}"
                );
                if expected {
                    assert_eq!(actual.unwrap(), to);
                } else {
                    assert!(matches!(actual, Err(CoreError::InvalidTransition { .. })));
                }
            }
        }
    }

    #[test]
    fn every_attempt_transition_is_classified() {
        for from in ATTEMPT_STATES {
            for to in ATTEMPT_STATES {
                let expected = LEGAL_ATTEMPT_TRANSITIONS.contains(&(from, to));
                let actual = from.transition(to);
                assert_eq!(
                    actual.is_ok(),
                    expected,
                    "unexpected attempt transition classification: {from:?} -> {to:?}"
                );
                if expected {
                    assert_eq!(actual.unwrap(), to);
                } else {
                    assert!(matches!(actual, Err(CoreError::InvalidTransition { .. })));
                }
            }
        }
    }
}
