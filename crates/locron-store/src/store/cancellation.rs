//! Cancellation admission shared by the live transaction and read-only preview.

use rusqlite::{Connection, OptionalExtension};

use super::{CancelOutcome, Store, StoreError, StoreResult};

/// A prospective cancellation decision from one durable row observation.
/// It neither reserves the result nor proves that a process has stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationPreview {
    /// Observed run state before any prospective cancellation effect.
    pub state: String,
    /// Outcome the same row and explicit option would admit in the live transaction.
    pub outcome: CancelOutcome,
    /// Whether the observed row already has a durable cancellation request.
    pub already_requested: bool,
}

pub(super) fn decision(
    id: &str,
    state: &str,
    reason: Option<&str>,
    acknowledge_unconfirmed: bool,
) -> StoreResult<CancelOutcome> {
    let quarantined = state == "running" && reason == Some("termination_unconfirmed");
    if acknowledge_unconfirmed {
        if !quarantined {
            return Err(StoreError::Conflict(format!(
                "run {id} is not an active termination-unconfirmed quarantine"
            )));
        }
        return Ok(CancelOutcome::AcknowledgedUnconfirmed);
    }
    if quarantined {
        return Err(StoreError::Conflict(format!(
            "run {id} termination is unconfirmed; repeat cancel with --acknowledge-unconfirmed to accept the risk and release the quarantine"
        )));
    }
    match state {
        "queued" | "retry_wait" => Ok(CancelOutcome::CancelledBeforeExecution),
        "starting" | "running" => Ok(CancelOutcome::CancellationRequested),
        terminal => Err(StoreError::Conflict(format!(
            "run {id} is already terminal ({terminal})"
        ))),
    }
}

fn observe(
    connection: &Connection,
    id: &str,
    acknowledge_unconfirmed: bool,
) -> StoreResult<CancellationPreview> {
    let row: Option<(String, Option<i64>, Option<String>)> = connection
        .query_row(
            "SELECT state,cancellation_requested_at_us,reason FROM runs WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((state, requested_at, reason)) = row else {
        return Err(StoreError::NotFound(id.into()));
    };
    let outcome = decision(id, &state, reason.as_deref(), acknowledge_unconfirmed)?;
    Ok(CancellationPreview {
        state,
        outcome,
        already_requested: requested_at.is_some(),
    })
}

impl Store {
    /// Observes prospective cancellation admission without writing, waking a
    /// runner, or acquiring a write transaction. All decision facts are read
    /// by one SELECT. A later live request rechecks its own current row under
    /// the existing immediate transaction; this value grants no authority.
    pub fn preview_cancellation(
        &self,
        id: &str,
        acknowledge_unconfirmed: bool,
    ) -> StoreResult<CancellationPreview> {
        observe(&self.conn()?, id, acknowledge_unconfirmed)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    #[test]
    fn admission_preserves_live_state_and_acknowledgement_policy() {
        for state in ["queued", "retry_wait", "starting", "running", "succeeded", "failed", "timed_out", "cancelled", "skipped_overlap", "skipped_concurrency", "interrupted_unknown", "unknown"] {
            for reason in [None, Some("ordinary"), Some("termination_unconfirmed")] {
                for acknowledge in [false, true] {
                    let actual = decision("run", state, reason, acknowledge);
                    let quarantine = state == "running" && reason == Some("termination_unconfirmed");
                    if acknowledge && quarantine {
                        assert_eq!(actual.unwrap(), CancelOutcome::AcknowledgedUnconfirmed);
                    } else if acknowledge {
                        assert_eq!(actual.unwrap_err().to_string(), "durable conflict: run run is not an active termination-unconfirmed quarantine");
                    } else if quarantine {
                        assert_eq!(actual.unwrap_err().to_string(), "durable conflict: run run termination is unconfirmed; repeat cancel with --acknowledge-unconfirmed to accept the risk and release the quarantine");
                    } else if ["queued", "retry_wait"].contains(&state) {
                        assert_eq!(actual.unwrap(), CancelOutcome::CancelledBeforeExecution);
                    } else if ["starting", "running"].contains(&state) {
                        assert_eq!(actual.unwrap(), CancelOutcome::CancellationRequested);
                    } else {
                        assert_eq!(actual.unwrap_err().to_string(), format!("durable conflict: run run is already terminal ({state})"));
                    }
                }
            }
        }
    }

    #[test]
    fn one_statement_preview_operates_with_query_only_and_does_not_change_rows() {
        // This narrow SQL fixture qualifies observation only. Full migrated
        // Store and actual MCP lifecycle fixtures live in the CLI test suite.
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE runs(id TEXT PRIMARY KEY,state TEXT NOT NULL,cancellation_requested_at_us INTEGER,reason TEXT)").unwrap();
        for state in ["queued", "starting", "running", "retry_wait", "succeeded"] {
            for requested in [None, Some(123)] {
                for reason in [None, Some("termination_unconfirmed")] {
                    connection.pragma_update(None, "query_only", false).unwrap();
                    connection.execute("INSERT OR REPLACE INTO runs VALUES('run',?1,?2,?3)", params![state, requested, reason]).unwrap();
                    connection.pragma_update(None, "query_only", true).unwrap();
                    let before = connection.total_changes();
                    for acknowledge in [false, true] {
                        match decision("run", state, reason, acknowledge) {
                            Ok(outcome) => {
                                let preview = observe(&connection, "run", acknowledge).unwrap();
                                assert_eq!(preview.state, state);
                                assert_eq!(preview.outcome, outcome);
                                assert_eq!(preview.already_requested, requested.is_some());
                            }
                            Err(expected) => assert_eq!(observe(&connection, "run", acknowledge).unwrap_err().to_string(), expected.to_string()),
                        }
                    }
                    let after: (String, Option<i64>, Option<String>) = connection.query_row("SELECT state,cancellation_requested_at_us,reason FROM runs WHERE id='run'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
                    assert_eq!(after, (state.into(), requested, reason.map(str::to_owned)));
                    assert_eq!(connection.total_changes(), before);
                }
            }
        }
        assert!(matches!(observe(&connection, "missing", false), Err(StoreError::NotFound(_))));
    }
}
