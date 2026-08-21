//! Long-lived daemon coordination independent of SQLite implementation details.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::runner::{AttemptContext, ExecutionOutcome, Runner, RunnerError, TargetSpec};

/// Daemon timing and resource limits.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    /// Maximum simultaneously running attempts.
    pub global_concurrency: usize,
    /// Safety reconciliation interval when no earlier event exists.
    pub safety_reconciliation: Duration,
    /// Natural completion window after shutdown begins.
    pub shutdown_drain: Duration,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            global_concurrency: 16,
            safety_reconciliation: Duration::from_secs(30),
            shutdown_drain: Duration::from_secs(30),
        }
    }
}

/// One atomically admitted durable attempt.
#[derive(Clone, Debug)]
pub struct AdmittedAttempt {
    /// Durable run ID.
    pub run_id: String,
    /// Immutable target snapshot.
    pub target: TargetSpec,
    /// Output and cancellation context.
    pub context: AttemptContext,
}

/// Result of a durable reconcile/admission pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickResult {
    /// Number of occurrences materialized or explained.
    pub reconciled: usize,
    /// Number of attempts admitted.
    pub admitted: usize,
}

/// Persistence boundary required by the daemon runtime.
#[async_trait]
pub trait DaemonStore: Send + Sync + 'static {
    /// Classifies stale prior-lifetime work and creates this lifetime.
    async fn begin_lifetime(&self) -> Result<(), String>;
    /// Reconciles due schedules and durable retry intents.
    async fn reconcile(&self) -> Result<usize, String>;
    /// Atomically admits up to `capacity` attempts using durable fairness.
    async fn admit(&self, capacity: usize) -> Result<Vec<AdmittedAttempt>, String>;
    /// Persists an attempt result and any durable retry intent.
    async fn complete(
        &self,
        attempt: &AdmittedAttempt,
        outcome: &ExecutionOutcome,
    ) -> Result<(), String>;
    /// Reads a durable user or replacement cancellation intent.
    async fn cancellation_requested(&self, run_id: &str) -> Result<bool, String>;
    /// Marks persistence degraded for diagnostics.
    async fn persistence_degraded(&self, reason: &str);
    /// Ends this scheduler lifetime after active transitions are durable.
    async fn end_lifetime(&self) -> Result<(), String>;
}

/// Daemon error category.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// Durable transition failed.
    #[error("durable daemon transition failed: {0}")]
    Store(String),
    /// Target execution infrastructure failed.
    #[error(transparent)]
    Runner(#[from] RunnerError),
    /// Global concurrency setting is outside the product range.
    #[error("global concurrency must be between 1 and 64")]
    InvalidConcurrency,
}

/// Owns reconciliation, admission, execution tasks, and graceful shutdown.
pub struct Daemon<S> {
    store: Arc<S>,
    runner: Runner,
    config: DaemonConfig,
    wake: Arc<tokio::sync::Notify>,
    cancellation: CancellationToken,
    tracker: TaskTracker,
}

impl<S: DaemonStore> Daemon<S> {
    /// Constructs a daemon after validating resource bounds.
    pub fn new(store: Arc<S>, runner: Runner, config: DaemonConfig) -> Result<Self, DaemonError> {
        if !(1..=64).contains(&config.global_concurrency) {
            return Err(DaemonError::InvalidConcurrency);
        }
        Ok(Self {
            store,
            runner,
            config,
            wake: Arc::new(tokio::sync::Notify::new()),
            cancellation: CancellationToken::new(),
            tracker: TaskTracker::new(),
        })
    }

    /// Returns a coalescing in-process wake handle for composition adapters.
    #[must_use]
    pub fn wake_handle(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.wake)
    }

    /// Performs one deterministic reconcile/admission pass.
    pub async fn tick(&self, semaphore: &Arc<Semaphore>) -> Result<TickResult, DaemonError> {
        let reconciled = match self.store.reconcile().await {
            Ok(count) => count,
            Err(error) => {
                self.store.persistence_degraded(&error).await;
                return Err(DaemonError::Store(error));
            }
        };
        let capacity = semaphore.available_permits();
        if capacity == 0 {
            return Ok(TickResult {
                reconciled,
                admitted: 0,
            });
        }
        let attempts = self
            .store
            .admit(capacity)
            .await
            .map_err(DaemonError::Store)?;
        let admitted = attempts.len();
        for mut attempt in attempts {
            attempt.context.cancellation = self.cancellation.child_token();
            let permit = Arc::clone(semaphore)
                .acquire_owned()
                .await
                .map_err(|_| DaemonError::Store("admission semaphore closed".into()))?;
            let store = Arc::clone(&self.store);
            let runner = self.runner.clone();
            self.tracker.spawn(async move {
                let cancellation = attempt.context.cancellation.clone();
                let execution = runner.execute(&attempt.target, &attempt.context);
                tokio::pin!(execution);
                let outcome = loop {
                    tokio::select! {
                        outcome = &mut execution => break outcome,
                        () = tokio::time::sleep(Duration::from_millis(200)) => {
                            match store.cancellation_requested(&attempt.run_id).await {
                                Ok(true) => cancellation.cancel(),
                                Ok(false) => {}
                                Err(error) => store.persistence_degraded(&error).await,
                            }
                        }
                    }
                };
                match outcome {
                    Ok(outcome) => {
                        if let Err(error) = store.complete(&attempt, &outcome).await {
                            store.persistence_degraded(&error).await;
                        }
                    }
                    Err(error) => {
                        store.persistence_degraded(&error.to_string()).await;
                    }
                }
                drop(permit);
            });
        }
        Ok(TickResult {
            reconciled,
            admitted,
        })
    }

    /// Runs until the supplied cancellation token or an OS termination signal.
    pub async fn run(self, external_cancel: CancellationToken) -> Result<(), DaemonError> {
        self.store
            .begin_lifetime()
            .await
            .map_err(DaemonError::Store)?;
        let semaphore = Arc::new(Semaphore::new(self.config.global_concurrency));
        let mut first = true;
        loop {
            if !first {
                tokio::select! {
                    () = tokio::time::sleep(self.config.safety_reconciliation) => {}
                    () = self.wake.notified() => {}
                    () = external_cancel.cancelled() => break,
                    result = tokio::signal::ctrl_c() => {
                        if let Err(error) = result {
                            tracing::warn!(%error, "failed to listen for ctrl-c");
                        }
                        break;
                    }
                }
            }
            first = false;
            if external_cancel.is_cancelled() {
                break;
            }
            if let Err(error) = self.tick(&semaphore).await {
                tracing::error!(%error, "daemon tick failed; admission paused until next reconciliation");
            }
        }

        self.tracker.close();
        let wait = self.tracker.wait();
        tokio::pin!(wait);
        if tokio::time::timeout(self.config.shutdown_drain, &mut wait)
            .await
            .is_err()
        {
            tracing::warn!("shutdown drain elapsed with active attempts");
            self.cancellation.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(10), &mut wait).await;
        }
        self.store.end_lifetime().await.map_err(DaemonError::Store)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeStore {
        calls: Mutex<Vec<&'static str>>,
    }

    #[async_trait]
    impl DaemonStore for FakeStore {
        async fn begin_lifetime(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("begin");
            Ok(())
        }
        async fn reconcile(&self) -> Result<usize, String> {
            self.calls.lock().unwrap().push("reconcile");
            Ok(2)
        }
        async fn admit(&self, _: usize) -> Result<Vec<AdmittedAttempt>, String> {
            self.calls.lock().unwrap().push("admit");
            Ok(Vec::new())
        }
        async fn complete(&self, _: &AdmittedAttempt, _: &ExecutionOutcome) -> Result<(), String> {
            Ok(())
        }
        async fn cancellation_requested(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }
        async fn persistence_degraded(&self, _: &str) {}
        async fn end_lifetime(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("end");
            Ok(())
        }
    }

    #[tokio::test]
    async fn zero_capacity_never_calls_admission() {
        let store = Arc::new(FakeStore::default());
        let daemon = Daemon::new(
            store.clone(),
            Runner::new(Default::default()).unwrap(),
            Default::default(),
        )
        .unwrap();
        let result = daemon.tick(&Arc::new(Semaphore::new(0))).await.unwrap();
        assert_eq!(
            result,
            TickResult {
                reconciled: 2,
                admitted: 0
            }
        );
        assert_eq!(*store.calls.lock().unwrap(), ["reconcile"]);
    }

    #[test]
    fn rejects_invalid_global_limit() {
        let config = DaemonConfig {
            global_concurrency: 0,
            ..DaemonConfig::default()
        };
        assert!(matches!(
            Daemon::new(
                Arc::new(FakeStore::default()),
                Runner::new(Default::default()).unwrap(),
                config
            ),
            Err(DaemonError::InvalidConcurrency)
        ));
    }
}
