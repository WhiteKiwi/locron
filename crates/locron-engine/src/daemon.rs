//! Long-lived daemon coordination independent of SQLite implementation details.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::runner::{AttemptContext, ExecutionOutcome, Runner, RunnerError, TargetSpec};

const MAX_GLOBAL_CONCURRENCY: usize = 64;

/// Daemon timing and resource limits.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    /// Maximum simultaneously running attempts.
    pub global_concurrency: usize,
    /// Safety reconciliation interval when no earlier event exists.
    pub safety_reconciliation: Duration,
    /// Natural completion window after shutdown begins.
    pub shutdown_drain: Duration,
    /// Initial retry delay for the required pre-spawn durable transition.
    pub pre_spawn_retry_initial: Duration,
    /// Maximum retry delay for the required pre-spawn durable transition.
    pub pre_spawn_retry_cap: Duration,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            global_concurrency: 16,
            safety_reconciliation: Duration::from_secs(30),
            shutdown_drain: Duration::from_secs(30),
            pre_spawn_retry_initial: Duration::from_millis(50),
            pre_spawn_retry_cap: Duration::from_secs(1),
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
    /// Atomically applies durable concurrency and admits attempts, additionally
    /// bounded by the process-local hard-guard permits currently available.
    async fn admit(&self, hard_guard_available: usize) -> Result<Vec<AdmittedAttempt>, String>;
    /// Acknowledges the final pre-execution boundary immediately before the
    /// runner is allowed to create external side effects.
    async fn mark_running(&self, attempt: &AdmittedAttempt) -> Result<bool, String>;
    /// Samples the immutable completion instant once after target outcome.
    fn completion_instant_us(&self) -> i64;
    /// Persists an attempt result and any durable retry intent.
    async fn complete(
        &self,
        attempt: &AdmittedAttempt,
        outcome: &ExecutionOutcome,
        completed_at_us: i64,
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
    /// Required-transition retry timing is invalid.
    #[error("pre-spawn retry delay must be positive and no greater than its cap")]
    InvalidRetryTiming,
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
        if !(1..=MAX_GLOBAL_CONCURRENCY).contains(&config.global_concurrency) {
            return Err(DaemonError::InvalidConcurrency);
        }
        if config.pre_spawn_retry_initial.is_zero()
            || config.pre_spawn_retry_initial > config.pre_spawn_retry_cap
        {
            return Err(DaemonError::InvalidRetryTiming);
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
        let available = semaphore.available_permits();
        if available > MAX_GLOBAL_CONCURRENCY {
            return Err(DaemonError::InvalidConcurrency);
        }
        if available == 0 {
            return Ok(TickResult {
                reconciled,
                admitted: 0,
            });
        }
        let attempts = match self.store.admit(available).await {
            Ok(attempts) => attempts,
            Err(error) => {
                self.store.persistence_degraded(&error).await;
                return Err(DaemonError::Store(error));
            }
        };
        let admitted = attempts.len();
        for mut attempt in attempts {
            let shutdown = self.cancellation.child_token();
            attempt.context.cancellation = self.cancellation.child_token();
            let permit = Arc::clone(semaphore)
                .acquire_owned()
                .await
                .map_err(|_| DaemonError::Store("admission semaphore closed".into()))?;
            let store = Arc::clone(&self.store);
            let runner = self.runner.clone();
            let retry_initial = self.config.pre_spawn_retry_initial;
            let retry_cap = self.config.pre_spawn_retry_cap;
            self.tracker.spawn(async move {
                let mut retry_delay = retry_initial;
                loop {
                    let decision = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => {
                            drop(permit);
                            return;
                        }
                        decision = store.mark_running(&attempt) => decision,
                    };
                    match decision {
                        Ok(true) => break,
                        Ok(false) => {
                            drop(permit);
                            return;
                        }
                        Err(error) => store.persistence_degraded(&error).await,
                    }
                    tokio::select! {
                        biased;
                        () = shutdown.cancelled() => {
                            drop(permit);
                            return;
                        }
                        () = tokio::time::sleep(retry_delay) => {}
                    }
                    retry_delay = next_retry_delay(retry_delay, retry_cap);
                }
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
                        let completed_at_us = store.completion_instant_us();
                        let mut retry_delay = retry_initial;
                        let mut first_completion = true;
                        loop {
                            let completion = if first_completion {
                                first_completion = false;
                                store.complete(&attempt, &outcome, completed_at_us).await
                            } else {
                                tokio::select! {
                                    biased;
                                    () = shutdown.cancelled() => break,
                                    completion = store.complete(&attempt, &outcome, completed_at_us) => completion,
                                }
                            };
                            match completion {
                                Ok(()) => break,
                                Err(error) => store.persistence_degraded(&error).await,
                            }
                            tokio::select! {
                                biased;
                                () = shutdown.cancelled() => break,
                                () = tokio::time::sleep(retry_delay) => {}
                            }
                            retry_delay = next_retry_delay(retry_delay, retry_cap);
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
        self.run_until(external_cancel, os_shutdown_signal()).await
    }

    async fn run_until<F>(
        self,
        external_cancel: CancellationToken,
        shutdown_signal: F,
    ) -> Result<(), DaemonError>
    where
        F: Future<Output = ()> + Send,
    {
        self.store
            .begin_lifetime()
            .await
            .map_err(DaemonError::Store)?;
        let semaphore = Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY));
        tokio::pin!(shutdown_signal);
        let mut first = true;
        loop {
            if !first {
                tokio::select! {
                    biased;
                    () = external_cancel.cancelled() => break,
                    () = &mut shutdown_signal => break,
                    () = self.wake.notified() => {}
                    () = tokio::time::sleep(self.config.safety_reconciliation) => {}
                }
            }
            first = false;

            let tick = tokio::select! {
                biased;
                () = external_cancel.cancelled() => break,
                () = &mut shutdown_signal => break,
                tick = self.tick(&semaphore) => tick,
            };
            if let Err(error) = tick {
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
            wait.await;
        }
        self.store.end_lifetime().await.map_err(DaemonError::Store)
    }
}

async fn wait_for_ctrl_c() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(%error, "failed to listen for ctrl-c");
        std::future::pending().await
    }
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{SignalKind, signal};

    match signal(SignalKind::terminate()) {
        Ok(mut signals) => {
            if signals.recv().await.is_none() {
                tracing::warn!("SIGTERM signal stream closed");
                std::future::pending().await
            }
        }
        Err(error) => {
            tracing::warn!(%error, "failed to listen for SIGTERM");
            std::future::pending().await
        }
    }
}

#[cfg(unix)]
async fn os_shutdown_signal() {
    tokio::select! {
        () = wait_for_ctrl_c() => {}
        () = wait_for_sigterm() => {}
    }
}

#[cfg(not(unix))]
async fn os_shutdown_signal() {
    wait_for_ctrl_c().await;
}

fn next_retry_delay(current: Duration, cap: Duration) -> Duration {
    current.checked_mul(2).unwrap_or(Duration::MAX).min(cap)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Default)]
    struct FakeStore {
        calls: Mutex<Vec<&'static str>>,
    }

    struct ScriptedStore {
        attempt: Mutex<Option<AdmittedAttempt>>,
        admission_error: Mutex<Option<String>>,
        marks: Mutex<VecDeque<Result<bool, String>>>,
        mark_calls: AtomicUsize,
        completions: AtomicUsize,
        successful_completions: AtomicUsize,
        degradations: AtomicUsize,
        completion_results: Mutex<VecDeque<Result<(), String>>>,
        mark_notify: tokio::sync::Notify,
        complete_notify: tokio::sync::Notify,
    }

    struct DynamicCapacityStore {
        global: AtomicUsize,
        capacities: Mutex<Vec<usize>>,
    }

    struct ShutdownStore {
        attempt: Mutex<Option<AdmittedAttempt>>,
        events: Arc<Mutex<Vec<&'static str>>>,
        admit_calls: AtomicUsize,
        mark_calls: AtomicUsize,
        outcome: Mutex<Option<crate::runner::OutcomeKind>>,
        admit_notify: tokio::sync::Notify,
        mark_notify: tokio::sync::Notify,
    }

    impl DynamicCapacityStore {
        fn new(global: usize) -> Self {
            Self {
                global: AtomicUsize::new(global),
                capacities: Mutex::new(Vec::new()),
            }
        }

        fn set_global(&self, value: usize) {
            self.global.store(value, Ordering::Release);
        }
    }

    impl ShutdownStore {
        fn empty() -> Self {
            Self {
                attempt: Mutex::new(None),
                events: Arc::new(Mutex::new(Vec::new())),
                admit_calls: AtomicUsize::new(0),
                mark_calls: AtomicUsize::new(0),
                outcome: Mutex::new(None),
                admit_notify: tokio::sync::Notify::new(),
                mark_notify: tokio::sync::Notify::new(),
            }
        }

        fn process(temp: &tempfile::TempDir, script: String) -> Self {
            let store = Self::empty();
            *store.attempt.lock().unwrap() = Some(AdmittedAttempt {
                run_id: "shutdown-run".into(),
                target: TargetSpec::Process(crate::runner::ProcessSpec {
                    executable: "/bin/sh".into(),
                    args: vec!["-c".into(), script],
                    cwd: temp.path().into(),
                    env: BTreeMap::new(),
                }),
                context: AttemptContext {
                    run_id: "shutdown-run".into(),
                    attempt: 1,
                    partial_output: temp.path().join("shutdown.partial"),
                    final_output: temp.path().join("shutdown.log"),
                    output_limit: 1024,
                    timeout: None,
                    cancellation: CancellationToken::new(),
                },
            });
            store
        }

        async fn wait_for_admits(&self, count: usize) {
            while self.admit_calls.load(Ordering::Acquire) < count {
                self.admit_notify.notified().await;
            }
        }

        async fn wait_for_marks(&self, count: usize) {
            while self.mark_calls.load(Ordering::Acquire) < count {
                self.mark_notify.notified().await;
            }
        }
    }

    impl ScriptedStore {
        fn new(temp: &tempfile::TempDir, marks: Vec<Result<bool, String>>) -> Self {
            Self {
                attempt: Mutex::new(Some(AdmittedAttempt {
                    run_id: "run".into(),
                    target: TargetSpec::Process(crate::runner::ProcessSpec {
                        executable: "/bin/sh".into(),
                        args: vec![
                            "-c".into(),
                            format!(
                                "printf x >> '{}'",
                                temp.path().join("side-effect").display()
                            ),
                        ],
                        cwd: temp.path().into(),
                        env: BTreeMap::new(),
                    }),
                    context: AttemptContext {
                        run_id: "run".into(),
                        attempt: 1,
                        partial_output: temp.path().join("1.partial"),
                        final_output: temp.path().join("1.log"),
                        output_limit: 1024,
                        timeout: Some(Duration::from_secs(1)),
                        cancellation: CancellationToken::new(),
                    },
                })),
                admission_error: Mutex::new(None),
                marks: Mutex::new(marks.into()),
                mark_calls: AtomicUsize::new(0),
                completions: AtomicUsize::new(0),
                successful_completions: AtomicUsize::new(0),
                degradations: AtomicUsize::new(0),
                completion_results: Mutex::new(VecDeque::new()),
                mark_notify: tokio::sync::Notify::new(),
                complete_notify: tokio::sync::Notify::new(),
            }
        }

        fn with_completion_results(self, results: Vec<Result<(), String>>) -> Self {
            *self.completion_results.lock().unwrap() = results.into();
            self
        }

        fn with_admission_error(self, error: &str) -> Self {
            *self.admission_error.lock().unwrap() = Some(error.into());
            self
        }

        async fn wait_for_marks(&self, count: usize) {
            while self.mark_calls.load(Ordering::Acquire) < count {
                self.mark_notify.notified().await;
            }
        }
    }

    #[async_trait]
    impl DaemonStore for ScriptedStore {
        async fn begin_lifetime(&self) -> Result<(), String> {
            Ok(())
        }
        async fn reconcile(&self) -> Result<usize, String> {
            Ok(0)
        }
        async fn admit(&self, _: usize) -> Result<Vec<AdmittedAttempt>, String> {
            if let Some(error) = self.admission_error.lock().unwrap().take() {
                return Err(error);
            }
            Ok(self.attempt.lock().unwrap().take().into_iter().collect())
        }
        async fn mark_running(&self, _: &AdmittedAttempt) -> Result<bool, String> {
            self.mark_calls.fetch_add(1, Ordering::AcqRel);
            self.mark_notify.notify_one();
            self.marks
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("persistent mark-running failure".into()))
        }
        fn completion_instant_us(&self) -> i64 {
            100
        }
        async fn complete(
            &self,
            _: &AdmittedAttempt,
            _: &ExecutionOutcome,
            _: i64,
        ) -> Result<(), String> {
            self.completions.fetch_add(1, Ordering::AcqRel);
            self.complete_notify.notify_one();
            let result = self
                .completion_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(()));
            if result.is_ok() {
                self.successful_completions.fetch_add(1, Ordering::AcqRel);
            }
            result
        }
        async fn cancellation_requested(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }
        async fn persistence_degraded(&self, _: &str) {
            self.degradations.fetch_add(1, Ordering::AcqRel);
        }
        async fn end_lifetime(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn retry_test_config() -> DaemonConfig {
        DaemonConfig {
            pre_spawn_retry_initial: Duration::from_millis(1),
            pre_spawn_retry_cap: Duration::from_millis(2),
            ..DaemonConfig::default()
        }
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
        async fn mark_running(&self, _: &AdmittedAttempt) -> Result<bool, String> {
            self.calls.lock().unwrap().push("running");
            Ok(true)
        }
        fn completion_instant_us(&self) -> i64 {
            100
        }
        async fn complete(
            &self,
            _: &AdmittedAttempt,
            _: &ExecutionOutcome,
            _: i64,
        ) -> Result<(), String> {
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

    #[async_trait]
    impl DaemonStore for DynamicCapacityStore {
        async fn begin_lifetime(&self) -> Result<(), String> {
            Ok(())
        }
        async fn reconcile(&self) -> Result<usize, String> {
            Ok(0)
        }
        async fn admit(&self, hard_guard_available: usize) -> Result<Vec<AdmittedAttempt>, String> {
            let configured = self.global.load(Ordering::Acquire);
            if !(1..=MAX_GLOBAL_CONCURRENCY).contains(&configured) {
                return Err("global concurrency must be between 1 and 64".into());
            }
            let active = MAX_GLOBAL_CONCURRENCY.saturating_sub(hard_guard_available);
            let capacity = configured.saturating_sub(active).min(hard_guard_available);
            self.capacities.lock().unwrap().push(capacity);
            Ok(Vec::new())
        }
        async fn mark_running(&self, _: &AdmittedAttempt) -> Result<bool, String> {
            unreachable!("capacity-only store never admits")
        }
        fn completion_instant_us(&self) -> i64 {
            0
        }
        async fn complete(
            &self,
            _: &AdmittedAttempt,
            _: &ExecutionOutcome,
            _: i64,
        ) -> Result<(), String> {
            unreachable!("capacity-only store never admits")
        }
        async fn cancellation_requested(&self, _: &str) -> Result<bool, String> {
            unreachable!("capacity-only store never admits")
        }
        async fn persistence_degraded(&self, _: &str) {}
        async fn end_lifetime(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[async_trait]
    impl DaemonStore for ShutdownStore {
        async fn begin_lifetime(&self) -> Result<(), String> {
            self.events.lock().unwrap().push("begin");
            Ok(())
        }

        async fn reconcile(&self) -> Result<usize, String> {
            self.events.lock().unwrap().push("reconcile");
            Ok(0)
        }

        async fn admit(&self, _: usize) -> Result<Vec<AdmittedAttempt>, String> {
            self.events.lock().unwrap().push("admit");
            self.admit_calls.fetch_add(1, Ordering::AcqRel);
            self.admit_notify.notify_one();
            Ok(self.attempt.lock().unwrap().take().into_iter().collect())
        }

        async fn mark_running(&self, _: &AdmittedAttempt) -> Result<bool, String> {
            self.events.lock().unwrap().push("running");
            self.mark_calls.fetch_add(1, Ordering::AcqRel);
            self.mark_notify.notify_one();
            Ok(true)
        }

        fn completion_instant_us(&self) -> i64 {
            100
        }

        async fn complete(
            &self,
            _: &AdmittedAttempt,
            outcome: &ExecutionOutcome,
            _: i64,
        ) -> Result<(), String> {
            self.events.lock().unwrap().push("complete");
            *self.outcome.lock().unwrap() = Some(outcome.kind.clone());
            Ok(())
        }

        async fn cancellation_requested(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }

        async fn persistence_degraded(&self, _: &str) {}

        async fn end_lifetime(&self) -> Result<(), String> {
            self.events.lock().unwrap().push("end");
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

    #[tokio::test]
    async fn durable_limit_changes_apply_to_next_admission_without_resizing_or_cancellation() {
        let store = Arc::new(DynamicCapacityStore::new(1));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            DaemonConfig::default(),
        )
        .unwrap();
        let semaphore = Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY));

        daemon.tick(&semaphore).await.unwrap();
        assert_eq!(*store.capacities.lock().unwrap(), [1]);

        let active = Arc::clone(&semaphore).acquire_many_owned(2).await.unwrap();
        store.set_global(3);
        daemon.tick(&semaphore).await.unwrap();
        assert_eq!(*store.capacities.lock().unwrap(), [1, 1]);
        assert_eq!(semaphore.available_permits(), 62);

        store.set_global(1);
        let reduced = daemon.tick(&semaphore).await.unwrap();
        assert_eq!(reduced.admitted, 0);
        assert_eq!(*store.capacities.lock().unwrap(), [1, 1, 0]);
        assert_eq!(semaphore.available_permits(), 62);

        store.set_global(3);
        daemon.tick(&semaphore).await.unwrap();
        assert_eq!(*store.capacities.lock().unwrap(), [1, 1, 0, 1]);

        drop(active);
        assert_eq!(semaphore.available_permits(), MAX_GLOBAL_CONCURRENCY);
    }

    #[tokio::test]
    async fn durable_limit_above_hard_max_is_rejected_before_store_admission() {
        let store = Arc::new(DynamicCapacityStore::new(65));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            DaemonConfig::default(),
        )
        .unwrap();
        let error = daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap_err();
        assert!(matches!(error, DaemonError::Store(_)));
        assert!(store.capacities.lock().unwrap().is_empty());
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

    #[tokio::test]
    async fn injected_shutdown_signal_stops_admission() {
        let store = Arc::new(ShutdownStore::empty());
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            DaemonConfig::default(),
        )
        .unwrap();
        let signal = CancellationToken::new();
        let signal_for_wait = signal.clone();
        let events = Arc::clone(&store.events);
        let task = tokio::spawn(daemon.run_until(CancellationToken::new(), async move {
            signal_for_wait.cancelled().await;
            events.lock().unwrap().push("signal");
        }));

        store.wait_for_admits(1).await;
        signal.cancel();
        task.await.unwrap().unwrap();

        let events = store.events.lock().unwrap();
        let signal_position = events.iter().position(|event| *event == "signal").unwrap();
        assert_eq!(store.admit_calls.load(Ordering::Acquire), 1);
        assert!(!events[signal_position + 1..].contains(&"admit"));
        assert_eq!(events.last(), Some(&"end"));
    }

    #[tokio::test]
    async fn shutdown_drain_allows_natural_completion_before_lifetime_end() {
        let temp = tempfile::tempdir().unwrap();
        let done = temp.path().join("done");
        let term = temp.path().join("term");
        let store = Arc::new(ShutdownStore::process(
            &temp,
            "trap 'printf term > term' TERM; sleep 0.05; printf done > done".into(),
        ));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(crate::runner::RunnerConfig {
                termination_grace: Duration::from_millis(10),
                ..Default::default()
            })
            .unwrap(),
            DaemonConfig {
                shutdown_drain: Duration::from_secs(1),
                ..Default::default()
            },
        )
        .unwrap();
        let signal = CancellationToken::new();
        let signal_for_wait = signal.clone();
        let task = tokio::spawn(daemon.run_until(CancellationToken::new(), async move {
            signal_for_wait.cancelled().await;
        }));

        store.wait_for_marks(1).await;
        signal.cancel();
        task.await.unwrap().unwrap();

        assert_eq!(
            *store.outcome.lock().unwrap(),
            Some(crate::runner::OutcomeKind::Succeeded)
        );
        assert!(done.is_file());
        assert!(!term.exists());
        assert_eq!(store.events.lock().unwrap().last(), Some(&"end"));
    }

    #[tokio::test]
    async fn elapsed_shutdown_drain_cancels_runner_before_lifetime_end() {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let term = temp.path().join("term");
        let store = Arc::new(ShutdownStore::process(
            &temp,
            "trap 'printf term > term; exit 0' TERM; printf ready > ready; while :; do sleep 1; done"
                .into(),
        ));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(crate::runner::RunnerConfig {
                termination_grace: Duration::from_millis(20),
                ..Default::default()
            })
            .unwrap(),
            DaemonConfig {
                shutdown_drain: Duration::from_millis(10),
                ..Default::default()
            },
        )
        .unwrap();
        let signal = CancellationToken::new();
        let signal_for_wait = signal.clone();
        let task = tokio::spawn(daemon.run_until(CancellationToken::new(), async move {
            signal_for_wait.cancelled().await;
        }));

        for _ in 0..100 {
            if ready.is_file() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(ready.is_file());
        signal.cancel();
        task.await.unwrap().unwrap();

        assert_eq!(
            *store.outcome.lock().unwrap(),
            Some(crate::runner::OutcomeKind::Cancelled)
        );
        assert!(term.is_file());
        let events = store.events.lock().unwrap();
        assert_eq!(events.last(), Some(&"end"));
        assert!(
            events
                .iter()
                .position(|event| *event == "complete")
                .unwrap()
                < events.len() - 1
        );
    }

    #[tokio::test]
    async fn transient_mark_running_failure_retries_before_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(ScriptedStore::new(
            &temp,
            vec![Err("busy".into()), Ok(true)],
        ));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();
        daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap();
        while store.completions.load(Ordering::Acquire) == 0 {
            store.complete_notify.notified().await;
        }
        daemon.tracker.close();
        daemon.tracker.wait().await;
        assert_eq!(store.mark_calls.load(Ordering::Acquire), 2);
        assert_eq!(store.degradations.load(Ordering::Acquire), 1);
        assert_eq!(store.completions.load(Ordering::Acquire), 1);
        assert!(temp.path().join("1.log").is_file());
        assert_eq!(
            std::fs::read(temp.path().join("side-effect")).unwrap(),
            b"x"
        );
    }

    #[tokio::test]
    async fn admission_failure_marks_persistence_degraded_without_starting_target() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            ScriptedStore::new(&temp, vec![Ok(true)])
                .with_admission_error("injected failure before admission"),
        );
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();

        let error = daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap_err();

        assert!(matches!(error, DaemonError::Store(_)));
        assert_eq!(store.degradations.load(Ordering::Acquire), 1);
        assert_eq!(store.mark_calls.load(Ordering::Acquire), 0);
        assert_eq!(store.completions.load(Ordering::Acquire), 0);
        assert!(!temp.path().join("side-effect").exists());
    }

    #[tokio::test]
    async fn transient_failure_then_durable_cancellation_never_spawns() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(ScriptedStore::new(
            &temp,
            vec![Err("busy".into()), Ok(false)],
        ));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();
        daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap();
        store.wait_for_marks(2).await;
        daemon.tracker.close();
        daemon.tracker.wait().await;
        assert_eq!(store.completions.load(Ordering::Acquire), 0);
        assert!(!temp.path().join("1.log").exists());
        assert!(!temp.path().join("1.partial").exists());
        assert!(!temp.path().join("side-effect").exists());
    }

    #[tokio::test]
    async fn persistent_mark_running_failure_waits_for_shutdown_without_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(ScriptedStore::new(&temp, Vec::new()));
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();
        daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap();
        store.wait_for_marks(3).await;
        daemon.cancellation.cancel();
        daemon.tracker.close();
        daemon.tracker.wait().await;
        assert!(store.mark_calls.load(Ordering::Acquire) >= 3);
        assert_eq!(store.completions.load(Ordering::Acquire), 0);
        assert!(!temp.path().join("1.log").exists());
        assert!(!temp.path().join("1.partial").exists());
        assert!(!temp.path().join("side-effect").exists());
    }

    #[tokio::test]
    async fn transient_completion_failure_retries_without_reexecuting_target() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            ScriptedStore::new(&temp, vec![Ok(true)])
                .with_completion_results(vec![Err("busy after target exit".into()), Ok(())]),
        );
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();
        daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap();
        while store.successful_completions.load(Ordering::Acquire) == 0 {
            store.complete_notify.notified().await;
        }
        daemon.tracker.close();
        daemon.tracker.wait().await;
        assert_eq!(store.completions.load(Ordering::Acquire), 2);
        assert_eq!(store.degradations.load(Ordering::Acquire), 1);
        assert_eq!(
            std::fs::read(temp.path().join("side-effect")).unwrap(),
            b"x"
        );
    }

    #[tokio::test]
    async fn shutdown_after_target_outcome_leaves_completion_uncommitted_without_reexecution() {
        let temp = tempfile::tempdir().unwrap();
        let completion_failures = (0..100)
            .map(|_| Err("injected failure after target outcome".into()))
            .collect();
        let store = Arc::new(
            ScriptedStore::new(&temp, vec![Ok(true)]).with_completion_results(completion_failures),
        );
        let daemon = Daemon::new(
            Arc::clone(&store),
            Runner::new(Default::default()).unwrap(),
            retry_test_config(),
        )
        .unwrap();
        daemon
            .tick(&Arc::new(Semaphore::new(MAX_GLOBAL_CONCURRENCY)))
            .await
            .unwrap();
        while store.completions.load(Ordering::Acquire) < 3 {
            store.complete_notify.notified().await;
        }

        daemon.cancellation.cancel();
        daemon.tracker.close();
        daemon.tracker.wait().await;

        assert!(store.completions.load(Ordering::Acquire) >= 3);
        assert_eq!(store.successful_completions.load(Ordering::Acquire), 0);
        assert_eq!(
            std::fs::read(temp.path().join("side-effect")).unwrap(),
            b"x"
        );
    }
}
