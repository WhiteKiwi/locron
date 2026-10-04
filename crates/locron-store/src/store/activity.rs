//! Active-run observations independent of presentation-history limits.

use rusqlite::params;

use super::{RunRecord, Store, StoreError, StoreResult, map_run};

impl Store {
    /// Returns the exact active count and a newest-first sample for a canonical job id.
    ///
    /// Active means queued, starting, running or retry-wait, including a running
    /// termination-unconfirmed quarantine. The sample is capped at 100 rows;
    /// `limit == 0` only counts and does not decode any execution snapshots.
    /// Count and sample share one read transaction. Callers resolve live names
    /// before using this id-only helper; an unknown canonical id yields zero.
    /// These observations neither prove process liveness nor reserve capacity.
    pub fn active_runs_for_job(
        &self,
        job_id: &str,
        limit: usize,
    ) -> StoreResult<(usize, Vec<RunRecord>)> {
        crate::paths::validate_uuid(job_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM runs WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
            [job_id],
            |row| row.get(0),
        )?;
        let total = usize::try_from(count).map_err(|_| {
            StoreError::Conflict("active run count is outside supported range".into())
        })?;
        let runs = if limit == 0 || total == 0 {
            Vec::new()
        } else {
            let mut statement = tx.prepare(
                "SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait') ORDER BY requested_at_us DESC,id DESC LIMIT ?2",
            )?;
            statement
                .query_map(params![job_id, limit.min(100) as i64], map_run)?
                .collect::<Result<Vec<_>, _>>()?
        };
        tx.commit()?;
        Ok((total, runs))
    }
}
