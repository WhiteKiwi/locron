//! Explicit legal lifecycle transitions.

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Durable run lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    Starting,
    Running,
    RetryWait,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    SkippedOverlap,
    SkippedConcurrency,
    InterruptedUnknown,
}

impl RunState {
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
    Starting,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
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

    #[test]
    fn terminal_runs_cannot_transition() {
        for state in [
            RunState::Succeeded,
            RunState::Failed,
            RunState::TimedOut,
            RunState::Cancelled,
            RunState::SkippedOverlap,
            RunState::SkippedConcurrency,
            RunState::InterruptedUnknown,
        ] {
            assert!(state.transition(RunState::Queued).is_err());
        }
    }

    #[test]
    fn retry_wait_returns_only_to_starting() {
        assert_eq!(
            RunState::RetryWait.transition(RunState::Starting).unwrap(),
            RunState::Starting
        );
        assert!(RunState::RetryWait.transition(RunState::Running).is_err());
    }
}
