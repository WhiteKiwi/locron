//! Registered activation ownership and bounded ordinary-role retries.
//!
//! Saved facts describe observed exits. Only retained native child/Job handles and actual
//! role locks prove exit; neither these facts nor a control acknowledgement is authority.

use std::io::Write;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use locron_core::filesystem::{self, DirectoryGuard, GuardedFile};
use locron_engine::windows_child::{ChildWindow, OwnedChild, SpawnContainment, SpawnFailure};
use locron_store::{DaemonLock, LockMetadata, LockProbe, StatePaths};
use serde::Serialize;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::{ServiceError, Target, canonical_executable};

const RETRY_DELAY: Duration = Duration::from_secs(60);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
const OBSERVE_INTERVAL: Duration = Duration::from_millis(25);
const MAX_ATTEMPTS: u8 = 4;
const FACT_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Waiting,
    Running,
    Retry,
    Stopping,
    Exhausted,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Cause {
    ChildExit { code: i32 },
    Cancelled,
    Infrastructure { message: String },
}

#[derive(Debug, Serialize)]
struct CompletedAttempt {
    lifetime: String,
    pid: u32,
    exit_code: i32,
    finished_at_us: i64,
}

#[derive(Debug, Serialize)]
struct RuntimeFacts {
    version: u8,
    role: &'static str,
    supervisor_lifetime: String,
    supervisor_pid: u32,
    worker_lifetime: Option<String>,
    worker_pid: Option<u32>,
    attempts: u8,
    phase: Phase,
    completed: Vec<CompletedAttempt>,
    cause: Option<Cause>,
}

struct Context {
    paths: StatePaths,
    root: DirectoryGuard,
    executable: GuardedFile,
    target: Target,
}

enum RefusalOwnership {
    Child(OwnedChild),
    Uncertain(SpawnContainment),
}

impl RefusalOwnership {
    fn release(self) {
        // Kernel containment is emergency cleanup only. The caller retains this value through
        // failure facts and control-listener teardown, and always returns an explicit error.
        match self {
            Self::Child(child) => drop(child),
            Self::Uncertain(containment) => drop(containment),
        }
    }
}

impl Target {
    fn supervisor_role(self) -> &'static str {
        match self {
            Self::Daemon => "daemon-activation",
            Self::Dashboard => "dashboard-activation",
        }
    }

    fn worker_role(self) -> &'static str {
        match self {
            Self::Daemon => "daemon-worker",
            Self::Dashboard => "dashboard-worker",
        }
    }

    fn runtime_role(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Dashboard => "dashboard",
        }
    }

    fn role_lock(self, paths: &StatePaths) -> &Path {
        match self {
            Self::Daemon => &paths.daemon_lock,
            Self::Dashboard => &paths.dashboard_lock,
        }
    }

    fn facts_name(self) -> &'static str {
        match self {
            Self::Daemon => "service.daemon.runtime.json",
            Self::Dashboard => "service.dashboard.runtime.json",
        }
    }
}

fn service_error(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Io(error.to_string())
}

fn metadata(lifetime: String) -> LockMetadata {
    LockMetadata {
        pid: std::process::id(),
        lifetime_id: lifetime,
        started_at_us: crate::now_us(),
        binary_version: env!("CARGO_PKG_VERSION").into(),
    }
}

fn write_facts(context: &Context, facts: &RuntimeFacts) -> Result<(), ServiceError> {
    let bytes = serde_json::to_vec(facts).map_err(service_error)?;
    if bytes.len() > FACT_LIMIT || facts.completed.len() > usize::from(MAX_ATTEMPTS) {
        return Err(service_error(
            "registered runtime facts exceed their fixed bound",
        ));
    }
    let destination = context.paths.root.join(context.target.facts_name());
    let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    let result = (|| {
        let mut file = filesystem::create_private_new_exclusive(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        filesystem::rename_private(&temporary, &destination)
    })();
    if result.is_err() {
        let _ = filesystem::remove_private_file(&temporary);
    }
    result.map_err(service_error)
}

fn child_command(context: &Context, parent: &str, worker: &str) -> Command {
    let mut command = Command::new(context.executable.normalized_path());
    command
        .arg("--state-dir")
        .arg(context.root.normalized_path());
    match context.target {
        Target::Daemon => {
            command.args(["daemon", "run"]);
        }
        Target::Dashboard => {
            command.args(["dashboard", "serve", "--port"]);
            command.arg(locron_server::DEFAULT_PORT.to_string());
        }
    }
    command
        .args([
            "--service-mode",
            "--supervisor-lifetime",
            parent,
            "--worker-lifetime",
            worker,
        ])
        .current_dir(context.root.normalized_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// A Held observation is accepted only between two equal exact-lifetime sidecar reads.
fn owned_lease(path: &Path, pid: u32, lifetime: &str) -> Result<bool, ServiceError> {
    let before = DaemonLock::read_role_metadata(path).map_err(service_error)?;
    if DaemonLock::probe_existing(path).map_err(service_error)? != LockProbe::Held {
        return Ok(false);
    }
    let after = DaemonLock::read_role_metadata(path).map_err(service_error)?;
    Ok(before.as_ref().is_some_and(|owner| {
        owner.service_mode
            && owner.metadata.pid == pid
            && owner.metadata.lifetime_id == lifetime
            && after.as_ref() == Some(owner)
    }))
}

/// A child cannot authorize stopping an unrelated holder of the worker lease.
fn confirm_leases_released(
    context: &Context,
    pid: u32,
    lifetime: &str,
) -> Result<(), ServiceError> {
    let worker = context.target.worker_activation_lock(&context.paths);
    if DaemonLock::probe_existing(worker).map_err(service_error)? == LockProbe::Held {
        return Err(service_error(
            "supervised worker activation remains held after tree exit",
        ));
    }
    let role = context.target.role_lock(&context.paths);
    if owned_lease(role, pid, lifetime)? {
        return Err(service_error(
            "supervised role remains held after tree exit",
        ));
    }
    // An unrelated manual role may still hold this lock. It is never signalled or killed.
    Ok(())
}

async fn stop_child(
    context: &Context,
    child: &mut OwnedChild,
    pid: u32,
    lifetime: &str,
    deadline: Instant,
    sent: &mut bool,
) -> Result<ExitStatus, ServiceError> {
    loop {
        if child.try_wait().map_err(service_error)?.is_some()
            && child.tree_empty().map_err(service_error)?
        {
            let status = child
                .confirm_exit_until(deadline)
                .await
                .map_err(service_error)?;
            confirm_leases_released(context, pid, lifetime)?;
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(service_error(
                "supervised shutdown is unconfirmed at its shared deadline",
            ));
        }
        if !*sent
            && owned_lease(
                context.target.worker_activation_lock(&context.paths),
                pid,
                lifetime,
            )?
        {
            // A timeout may leave OS delivery queued. Never replay this exact request.
            *sent = true;
            if let Err(error) = locron_core::notification::request_shutdown_guarded_until(
                &context.root,
                context.target.worker_role(),
                lifetime,
                deadline,
            ) {
                tracing::warn!(%error, "supervised cancellation delivery remains uncertain; observing actual exit");
            }
        }
        tokio::time::sleep(
            OBSERVE_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        )
        .await;
    }
}

enum AttemptEnd {
    Completed(ExitStatus),
    Cancelled(ExitStatus),
}

#[derive(Default)]
struct StopState {
    deadline: Option<Instant>,
    sent: bool,
}

async fn observe_attempt(
    context: &Context,
    child: &mut OwnedChild,
    pid: u32,
    lifetime: &str,
    cancellation: &CancellationToken,
    facts: &mut RuntimeFacts,
    stop: &mut StopState,
) -> Result<AttemptEnd, ServiceError> {
    loop {
        if cancellation.is_cancelled() {
            let deadline = Instant::now() + STOP_TIMEOUT;
            stop.deadline = Some(deadline);
            facts.phase = Phase::Stopping;
            facts.cause = Some(Cause::Cancelled);
            // Even a failed diagnostic write must perform owned shutdown under this budget.
            let persisted = write_facts(context, facts);
            let status =
                stop_child(context, child, pid, lifetime, deadline, &mut stop.sent).await?;
            persisted?;
            return Ok(AttemptEnd::Cancelled(status));
        }
        if child.try_wait().map_err(service_error)?.is_some() {
            let deadline = Instant::now() + STOP_TIMEOUT;
            stop.deadline = Some(deadline);
            let status = child
                .confirm_exit_until(deadline)
                .await
                .map_err(service_error)?;
            confirm_leases_released(context, pid, lifetime)?;
            return Ok(AttemptEnd::Completed(status));
        }
        if facts.phase != Phase::Running
            && owned_lease(context.target.role_lock(&context.paths), pid, lifetime)?
        {
            facts.phase = Phase::Running;
            write_facts(context, facts)?;
        }
        tokio::select! {
            () = cancellation.cancelled() => {},
            () = tokio::time::sleep(OBSERVE_INTERVAL) => {},
        }
    }
}

async fn run(
    context: &Context,
    cancellation: &CancellationToken,
    facts: &mut RuntimeFacts,
    refusal: &mut Option<RefusalOwnership>,
) -> Result<i32, ServiceError> {
    for attempt in 1..=MAX_ATTEMPTS {
        if cancellation.is_cancelled() {
            facts.phase = Phase::Stopping;
            facts.cause = Some(Cause::Cancelled);
            write_facts(context, facts)?;
            return Ok(0);
        }
        let lifetime = uuid::Uuid::now_v7().to_string();
        facts.attempts = attempt;
        facts.worker_lifetime = Some(lifetime.clone());
        facts.worker_pid = None;
        facts.phase = Phase::Waiting;
        facts.cause = None;
        write_facts(context, facts)?;
        let mut child = match OwnedChild::spawn(
            child_command(context, &facts.supervisor_lifetime, &lifetime),
            ChildWindow::Hidden,
        ) {
            Ok(child) => child,
            Err(failure) => {
                facts.phase = Phase::Error;
                facts.cause = Some(Cause::Infrastructure {
                    message: failure.to_string().chars().take(512).collect(),
                });
                // Keep uncertain containment alive through diagnostic flush. Its drop is emergency
                // kernel cleanup, never confirmation of root reaping or permission to retry.
                let persisted = write_facts(context, facts);
                let message = match failure {
                    SpawnFailure::NotStarted(_) => "registered worker could not be started",
                    SpawnFailure::ExecutionMayHaveStarted { containment, .. } => {
                        *refusal = Some(RefusalOwnership::Uncertain(containment));
                        "registered worker spawn is quarantined with unconfirmed cleanup"
                    }
                };
                persisted?;
                return Err(service_error(message));
            }
        };
        let Some(pid) = child.id() else {
            *refusal = Some(RefusalOwnership::Child(child));
            return Err(service_error(
                "owned registered child has no native PID; cleanup is unconfirmed",
            ));
        };
        facts.worker_pid = Some(pid);
        let mut stop = StopState::default();
        if let Err(error) = write_facts(context, facts) {
            let shutdown = stop_child(
                context,
                &mut child,
                pid,
                &lifetime,
                Instant::now() + STOP_TIMEOUT,
                &mut stop.sent,
            )
            .await;
            *refusal = Some(RefusalOwnership::Child(child));
            return Err(shutdown.err().unwrap_or(error));
        }
        let end = match observe_attempt(
            context,
            &mut child,
            pid,
            &lifetime,
            cancellation,
            facts,
            &mut stop,
        )
        .await
        {
            Ok(end) => end,
            Err(error) => {
                // A diagnostic/observer failure also cancels only this owned child. Reuse any
                // deadline already established by exit/cancellation; never reset an expired one.
                let deadline = stop
                    .deadline
                    .unwrap_or_else(|| Instant::now() + STOP_TIMEOUT);
                let stopped = stop_child(
                    context,
                    &mut child,
                    pid,
                    &lifetime,
                    deadline,
                    &mut stop.sent,
                )
                .await;
                let error = stopped.err().unwrap_or(error);
                facts.phase = Phase::Error;
                facts.cause = Some(Cause::Infrastructure {
                    message: error.to_string().chars().take(512).collect(),
                });
                let _ = write_facts(context, facts);
                *refusal = Some(RefusalOwnership::Child(child));
                return Err(error);
            }
        };
        let (status, cancelled) = match end {
            AttemptEnd::Completed(status) => (status, false),
            AttemptEnd::Cancelled(status) => (status, true),
        };
        let code = status
            .code()
            .ok_or_else(|| service_error("registered worker has no actual exit code"))?;
        facts.completed.push(CompletedAttempt {
            lifetime,
            pid,
            exit_code: code,
            finished_at_us: crate::now_us(),
        });
        if cancelled || code == 0 {
            facts.phase = Phase::Stopping;
            facts.cause = Some(if cancelled {
                Cause::Cancelled
            } else {
                Cause::ChildExit { code }
            });
            write_facts(context, facts)?;
            return Ok(0);
        }
        facts.cause = Some(Cause::ChildExit { code });
        if attempt == MAX_ATTEMPTS {
            facts.phase = Phase::Exhausted;
            write_facts(context, facts)?;
            return Ok(code);
        }
        facts.phase = Phase::Retry;
        write_facts(context, facts)?;
        tokio::select! {
            () = cancellation.cancelled() => {},
            () = tokio::time::sleep(RETRY_DELAY) => {},
        }
    }
    Err(service_error("registered attempt bound was exceeded"))
}

/// Runs only the hidden registered entry. The caller preserves returned real child exit codes
/// and maps infrastructure errors to the separately documented internal exit status 70.
pub(crate) async fn supervise(
    state_dir: Option<std::path::PathBuf>,
    target: Target,
) -> Result<i32, ServiceError> {
    let executable =
        filesystem::read_owned_executable(&canonical_executable()?).map_err(service_error)?;
    let discovered = StatePaths::discover(state_dir.as_deref()).map_err(service_error)?;
    let root = DirectoryGuard::private(&discovered.root).map_err(service_error)?;
    let paths = StatePaths::new(root.normalized_path().to_path_buf());
    let lifetime = uuid::Uuid::now_v7().to_string();
    let activation = DaemonLock::acquire_role(
        target.activation_lock(&paths),
        &metadata(lifetime.clone()),
        true,
    )
    .map_err(service_error)?;
    let context = Context {
        paths,
        root,
        executable,
        target,
    };
    let cancellation = CancellationToken::new();
    let control = locron_engine::ipc::bind_role_control(
        context.root.normalized_path(),
        target.supervisor_role(),
        &lifetime,
        cancellation.clone(),
    )
    .map_err(service_error)?;
    let mut facts = RuntimeFacts {
        version: 1,
        role: target.runtime_role(),
        supervisor_lifetime: lifetime,
        supervisor_pid: std::process::id(),
        worker_lifetime: None,
        worker_pid: None,
        attempts: 0,
        phase: Phase::Waiting,
        completed: Vec::with_capacity(usize::from(MAX_ATTEMPTS)),
        cause: None,
    };
    let mut refusal = None;
    let outcome = run(&context, &cancellation, &mut facts, &mut refusal).await;
    if let Err(error) = &outcome {
        facts.phase = Phase::Error;
        facts.cause = Some(Cause::Infrastructure {
            message: error.to_string().chars().take(512).collect(),
        });
        let _ = write_facts(&context, &facts);
    }
    control.abort();
    let _ = control.await;
    if let Some(ownership) = refusal {
        ownership.release();
    }
    drop(activation);
    drop(context);
    outcome
}

#[cfg(test)]
mod tests {
    use super::{
        Cause, CompletedAttempt, Context, Phase, RuntimeFacts, child_command,
        confirm_leases_released, metadata, owned_lease, write_facts,
    };
    use crate::service::Target;
    use locron_core::filesystem::{self, DirectoryGuard};
    use locron_store::{DaemonLock, LockProbe, StatePaths};
    use std::io::{Read, Write};

    struct Fixture {
        context: Context,
        _temporary: tempfile::TempDir,
    }

    fn fixture(target: Target) -> Fixture {
        let temporary = tempfile::tempdir().unwrap();
        let root = DirectoryGuard::private(&temporary.path().join("private 状態 % #")).unwrap();
        let paths = StatePaths::new(root.normalized_path().to_path_buf());
        let executable_path = paths.root.join("fixture program.exe");
        let mut file = filesystem::create_private_new_exclusive(&executable_path).unwrap();
        file.write_all(b"fixture identity bytes").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let executable = filesystem::read_owned_executable(&executable_path).unwrap();
        Fixture {
            context: Context {
                paths,
                root,
                executable,
                target,
            },
            _temporary: temporary,
        }
    }

    #[test]
    fn worker_lease_requires_exact_pid_uuid_and_registration_without_stopping_manual_role() {
        let fixture = fixture(Target::Daemon);
        let context = &fixture.context;
        let lifetime = uuid::Uuid::now_v7().to_string();
        let expected = metadata(lifetime.clone());
        let worker = DaemonLock::acquire_role(
            &context.paths.daemon_worker_activation_lock,
            &expected,
            true,
        )
        .unwrap();
        assert!(
            owned_lease(
                &context.paths.daemon_worker_activation_lock,
                expected.pid,
                &lifetime
            )
            .unwrap()
        );
        assert!(
            !owned_lease(
                &context.paths.daemon_worker_activation_lock,
                expected.pid + 1,
                &lifetime
            )
            .unwrap()
        );
        assert!(
            !owned_lease(
                &context.paths.daemon_worker_activation_lock,
                expected.pid,
                &uuid::Uuid::now_v7().to_string()
            )
            .unwrap()
        );
        assert!(confirm_leases_released(context, expected.pid, &lifetime).is_err());
        let manual_metadata = metadata(uuid::Uuid::now_v7().to_string());
        let manual =
            DaemonLock::acquire_role(&context.paths.daemon_lock, &manual_metadata, false).unwrap();
        drop(worker);
        confirm_leases_released(context, expected.pid, &lifetime).unwrap();
        assert_eq!(
            DaemonLock::probe_existing(&context.paths.daemon_lock).unwrap(),
            LockProbe::Held
        );
        assert_eq!(
            DaemonLock::read_role_metadata(&context.paths.daemon_lock)
                .unwrap()
                .unwrap()
                .metadata,
            manual_metadata
        );
        drop(manual);
    }

    #[test]
    fn fixed_child_argv_keeps_unicode_paths_and_distinct_lifetimes_as_data() {
        let fixture = fixture(Target::Dashboard);
        let context = &fixture.context;
        let parent = uuid::Uuid::now_v7().to_string();
        let worker = uuid::Uuid::now_v7().to_string();
        let command = child_command(context, &parent, &worker);
        let args = command
            .as_std()
            .get_args()
            .map(|value| value.to_os_string())
            .collect::<Vec<_>>();
        let expected: [std::ffi::OsString; 11] = [
            "--state-dir".into(),
            context.paths.root.clone().into_os_string(),
            "dashboard".into(),
            "serve".into(),
            "--port".into(),
            locron_server::DEFAULT_PORT.to_string().into(),
            "--service-mode".into(),
            "--supervisor-lifetime".into(),
            parent.into(),
            "--worker-lifetime".into(),
            worker.into(),
        ];
        assert_eq!(args, expected);
        assert_eq!(
            command.as_std().get_program(),
            context.executable.normalized_path().as_os_str()
        );
        assert_eq!(
            command.as_std().get_current_dir(),
            Some(context.root.normalized_path())
        );
    }

    #[test]
    fn private_facts_preserve_actual_exit_seventy_separately_from_infrastructure_failure() {
        let fixture = fixture(Target::Daemon);
        let context = &fixture.context;
        let lifetime = uuid::Uuid::now_v7().to_string();
        let mut facts = RuntimeFacts {
            version: 1,
            role: "daemon",
            supervisor_lifetime: uuid::Uuid::now_v7().to_string(),
            supervisor_pid: std::process::id(),
            worker_lifetime: Some(lifetime.clone()),
            worker_pid: Some(19),
            attempts: 1,
            phase: Phase::Exhausted,
            completed: vec![CompletedAttempt {
                lifetime,
                pid: 19,
                exit_code: 70,
                finished_at_us: crate::now_us(),
            }],
            cause: Some(Cause::ChildExit { code: 70 }),
        };
        write_facts(context, &facts).unwrap();
        facts.phase = Phase::Error;
        facts.cause = Some(Cause::Infrastructure {
            message: "fixture failed diagnostic boundary".into(),
        });
        write_facts(context, &facts).unwrap();
        let path = context.paths.root.join("service.daemon.runtime.json");
        assert!(filesystem::is_private(&path, false).unwrap());
        let mut file =
            filesystem::open_private(&path, std::fs::OpenOptions::new().read(true)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(actual["cause"]["kind"], "infrastructure");
        assert_eq!(actual["completed"][0]["exit_code"], 70);
        assert_eq!(actual["completed"].as_array().unwrap().len(), 1);
    }
}
