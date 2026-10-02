//! One guarded owner for registered retries, native I/O and cooperative teardown.
//!
//! A deadline closes admission, not an uncancellable syscall. The one worker retains all
//! ownership while that syscall finishes; timeout never authorizes replay or replacement.

use std::fs::{File, OpenOptions};
use std::future::{Future, poll_fn};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use locron_core::filesystem::{self, DirectoryGuard, GuardedFile};
use locron_engine::windows_child::{ChildWindow, OwnedChild, SpawnContainment, SpawnFailure};
use locron_store::{DaemonLock, LockMetadata, RoleLockMetadata, StatePaths};
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::oneshot;
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
    NotStarted { message: String },
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
struct NotStartedAttempt {
    lifetime: String,
    message: String,
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
    not_started: Vec<NotStartedAttempt>,
    cause: Option<Cause>,
}

#[derive(Debug)]
struct Failure {
    message: String,
    expired: bool,
}

impl Failure {
    fn io(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string().chars().take(512).collect(),
            expired: false,
        }
    }

    fn expired() -> Self {
        Self {
            message:
                "registered lifecycle deadline expired; ownership and queued I/O remain quarantined"
                    .into(),
            expired: true,
        }
    }

    fn service(self) -> ServiceError {
        ServiceError::Io(self.message)
    }
}

type WorkResult<T> = Result<T, Failure>;

struct Admission {
    open: AtomicBool,
    stop: Mutex<Option<Instant>>,
    cancellation: CancellationToken,
    control_abort: OnceLock<tokio::task::AbortHandle>,
    #[cfg(test)]
    gate: Mutex<Option<Arc<TestGate>>>,
    #[cfg(test)]
    retry_waiting: AtomicBool,
}

impl Admission {
    fn new(cancellation: CancellationToken) -> Self {
        Self {
            open: AtomicBool::new(true),
            stop: Mutex::new(None),
            cancellation,
            control_abort: OnceLock::new(),
            #[cfg(test)]
            gate: Mutex::new(None),
            #[cfg(test)]
            retry_waiting: AtomicBool::new(false),
        }
    }

    fn stop_at(&self, deadline: Instant) -> Instant {
        let mut stop = self
            .stop
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let deadline = stop.map_or(deadline, |old| old.min(deadline));
        *stop = Some(deadline);
        deadline
    }

    fn deadline(&self, requested: Instant) -> Instant {
        if self.cancellation.is_cancelled() {
            self.stop_at(Instant::now() + STOP_TIMEOUT);
        }
        self.stop
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .map_or(requested, |stop| requested.min(stop))
    }

    fn close(&self) {
        self.open.store(false, Ordering::Release);
        if let Some(abort) = self.control_abort.get() {
            abort.abort();
        }
    }

    fn stopping(&self) -> bool {
        self.stop
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }
}

#[derive(Clone)]
struct Budget {
    admission: Arc<Admission>,
    requested: Instant,
}

impl Budget {
    #[cfg(test)]
    fn withheld_io(&self, boundary: Boundary) {
        let gate = self.admission.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.withhold(boundary);
        }
    }
    fn check(&self) -> WorkResult<()> {
        if !self.admission.open.load(Ordering::Acquire)
            || Instant::now() >= self.admission.deadline(self.requested)
        {
            return Err(Failure::expired());
        }
        Ok(())
    }

    fn operation<T>(&self, operation: impl FnOnce() -> WorkResult<T>) -> WorkResult<T> {
        self.check()?;
        let result = operation();
        self.check()?;
        result
    }

    async fn await_response<F: Future>(&self, future: F) -> WorkResult<F::Output> {
        tokio::pin!(future);
        loop {
            self.check()?;
            let guarded = poll_fn(|cx| {
                if let Err(error) = self.check() {
                    return std::task::Poll::Ready(Err(error));
                }
                match future.as_mut().poll(cx) {
                    std::task::Poll::Pending => std::task::Poll::Pending,
                    std::task::Poll::Ready(value) => {
                        std::task::Poll::Ready(self.check().map(|()| value))
                    }
                }
            });
            tokio::select! {
                biased;
                () = self.admission.cancellation.cancelled(), if !self.admission.stopping() => {
                    self.admission.stop_at(Instant::now() + STOP_TIMEOUT);
                }
                response = tokio::time::timeout_at(tokio::time::Instant::from_std(self.admission.deadline(self.requested)), guarded) => {
                    return response.map_err(|_| Failure::expired())?;
                }
            }
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
    fn log_name(self) -> &'static str {
        match self {
            Self::Daemon => "service.daemon.log",
            Self::Dashboard => "service.dashboard.log",
        }
    }
}

fn metadata(lifetime: String) -> LockMetadata {
    LockMetadata {
        pid: std::process::id(),
        lifetime_id: lifetime,
        started_at_us: crate::now_us(),
        binary_version: env!("CARGO_PKG_VERSION").into(),
    }
}

struct ServiceLogs {
    stdout: GuardedFile,
    stderr: GuardedFile,
}

impl ServiceLogs {
    fn open(path: &Path, budget: &Budget) -> WorkResult<Self> {
        let open = || filesystem::open_private(path, OpenOptions::new().read(true).append(true));
        budget.check()?;
        let opened = open();
        budget.check()?;
        match opened {
            Ok(file) => drop(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                budget.operation(|| filesystem::create_private_new(path).map_err(Failure::io))?;
            }
            Err(error) => return Err(Failure::io(error)),
        }
        Ok(Self {
            stdout: budget.operation(|| open().map_err(Failure::io))?,
            stderr: budget.operation(|| open().map_err(Failure::io))?,
        })
    }

    fn apply(&self, command: &mut Command, budget: &Budget) -> WorkResult<()> {
        let stdout = budget.operation(|| self.stdout.try_clone().map_err(Failure::io))?;
        let stderr = budget.operation(|| self.stderr.try_clone().map_err(Failure::io))?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        Ok(())
    }

    fn sync(&self, budget: &Budget) -> WorkResult<()> {
        budget.operation(|| self.stdout.sync_all().map_err(Failure::io))?;
        budget.operation(|| self.stderr.sync_all().map_err(Failure::io))
    }
}

struct Context {
    paths: StatePaths,
    root: DirectoryGuard,
    executable: GuardedFile,
    logs: ServiceLogs,
    target: Target,
}

fn child_command(
    context: &Context,
    parent: &str,
    worker: &str,
    budget: &Budget,
) -> WorkResult<Command> {
    let mut command = Command::new(context.executable.normalized_path());
    command
        .arg("--state-dir")
        .arg(context.root.normalized_path());
    match context.target {
        Target::Daemon => {
            command.args(["daemon", "run"]);
        }
        Target::Dashboard => {
            command
                .args(["dashboard", "serve", "--port"])
                .arg(locron_server::DEFAULT_PORT.to_string());
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
        .current_dir(context.root.normalized_path());
    context.logs.apply(&mut command, budget)?;
    Ok(command)
}

fn write_facts(context: &Context, facts: &RuntimeFacts, budget: &Budget) -> WorkResult<()> {
    budget.check()?;
    let bytes = serde_json::to_vec(facts).map_err(Failure::io)?;
    if bytes.len() > FACT_LIMIT
        || facts.completed.len() + facts.not_started.len() > usize::from(MAX_ATTEMPTS)
    {
        return Err(Failure::io(
            "registered runtime facts exceed their fixed bound",
        ));
    }
    let destination = context.paths.root.join(context.target.facts_name());
    let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    let mut file = budget
        .operation(|| filesystem::create_private_new_exclusive(&temporary).map_err(Failure::io))?;
    budget.operation(|| file.write_all(&bytes).map_err(Failure::io))?;
    budget.operation(|| {
        let result = file.sync_all().map_err(Failure::io);
        #[cfg(test)]
        budget.withheld_io(Boundary::FactSync);
        result
    })?;
    budget.operation(|| {
        drop(file);
        Ok(())
    })?;
    // An already-started rename may finish after expiry; its result is never success proof.
    budget.operation(|| {
        let result = filesystem::rename_private_until(
            &temporary,
            &destination,
            budget.admission.deadline(budget.requested),
        )
        .map_err(Failure::io);
        #[cfg(test)]
        budget.withheld_io(Boundary::FactRename);
        result
    })
}

fn read_owner(file: &mut GuardedFile, budget: &Budget) -> WorkResult<RoleLockMetadata> {
    budget.operation(|| file.seek(SeekFrom::Start(0)).map_err(Failure::io))?;
    #[cfg(test)]
    budget.withheld_io(Boundary::BeforeMetadataRead);
    let mut bytes = Vec::new();
    budget.operation(|| {
        let result = (&mut **file)
            .take(FACT_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(Failure::io);
        #[cfg(test)]
        budget.withheld_io(Boundary::MetadataRead);
        result
    })?;
    if bytes.len() > FACT_LIMIT {
        return Err(Failure::io("role owner metadata exceeds its bound"));
    }
    serde_json::from_slice(&bytes).map_err(Failure::io)
}

fn held(file: &File, budget: &Budget) -> WorkResult<bool> {
    budget.operation(|| match file.try_lock() {
        Ok(()) => {
            File::unlock(file).map_err(Failure::io)?;
            Ok(false)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(std::fs::TryLockError::Error(error)) => Err(Failure::io(error)),
    })
}

#[derive(Default)]
struct Lease {
    permanent: Option<GuardedFile>,
    owner: Option<GuardedFile>,
}

impl Lease {
    fn owns(&mut self, path: &Path, pid: u32, lifetime: &str, budget: &Budget) -> WorkResult<bool> {
        let mut owner = if let Some(owner) = self.owner.take() {
            owner
        } else {
            budget.check()?;
            let opened = filesystem::open_private(
                &DaemonLock::owner_sidecar(path),
                OpenOptions::new().read(true),
            );
            budget.check()?;
            match opened {
                Ok(owner) => owner,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(Failure::io(error)),
            }
        };
        let before = read_owner(&mut owner, budget)?;
        if !before.service_mode
            || before.metadata.pid != pid
            || before.metadata.lifetime_id != lifetime
        {
            // Do not retain an unrelated manual owner's sidecar across its exit/publication.
            return Ok(false);
        }
        if self.permanent.is_none() {
            budget.check()?;
            let opened = filesystem::open_private(path, OpenOptions::new().read(true).write(true));
            self.permanent = match opened {
                Ok(file) => Some(file),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(Failure::io(error)),
            };
            budget.check()?;
        }
        let Some(permanent) = self.permanent.as_ref() else {
            return Ok(false);
        };
        let owns = held(permanent, budget)? && read_owner(&mut owner, budget)? == before;
        if owns {
            self.owner = Some(owner);
        }
        Ok(owns)
    }

    fn released(&mut self, path: &Path, budget: &Budget) -> WorkResult<bool> {
        if self.permanent.is_none() {
            budget.check()?;
            let opened = filesystem::open_private(path, OpenOptions::new().read(true).write(true));
            self.permanent = match opened {
                Ok(file) => Some(file),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(Failure::io(error)),
            };
            budget.check()?;
        }
        let released = if let Some(file) = &self.permanent {
            !held(file, budget)?
        } else {
            true
        };
        if released {
            self.owner = None;
        }
        Ok(released)
    }
}

enum Action {
    Initialize,
    Start,
    Observe { stopping: bool },
    Retry,
    Refuse(String),
    Finish { cancelled: bool },
}

#[derive(Debug)]
enum Reply {
    Initialized,
    Started,
    Pending { exit_deadline: Option<Instant> },
    Completed { code: i32, deadline: Instant },
    NotStarted { deadline: Instant },
    Refusing { child: bool, uncertain: bool },
    Finished,
}

struct Request {
    action: Action,
    budget: Budget,
    response: oneshot::Sender<WorkResult<Reply>>,
}

struct Lifecycle {
    target: Target,
    state_dir: Option<PathBuf>,
    admission: Arc<Admission>,
    main_runtime: tokio::runtime::Handle,
    facts: RuntimeFacts,
    // Quarantine drops containment before activation and the remaining context guards.
    child: Option<OwnedChild>,
    uncertain: Option<SpawnContainment>,
    worker_lease: Lease,
    role_lease: Lease,
    exit_deadline: Option<Instant>,
    shutdown_sent: bool,
    completion_recorded: bool,
    #[cfg(test)]
    settings: Option<TestSettings>,
    control: Option<tokio::task::JoinHandle<()>>,
    activation: Option<DaemonLock>,
    context: Option<Context>,
}

impl Lifecycle {
    fn new(
        target: Target,
        state_dir: Option<PathBuf>,
        admission: Arc<Admission>,
        main_runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            target,
            state_dir,
            admission,
            main_runtime,
            context: None,
            activation: None,
            control: None,
            facts: RuntimeFacts {
                version: 1,
                role: target.runtime_role(),
                supervisor_lifetime: uuid::Uuid::now_v7().to_string(),
                supervisor_pid: std::process::id(),
                worker_lifetime: None,
                worker_pid: None,
                attempts: 0,
                phase: Phase::Waiting,
                completed: Vec::new(),
                not_started: Vec::new(),
                cause: None,
            },
            child: None,
            uncertain: None,
            worker_lease: Lease::default(),
            role_lease: Lease::default(),
            exit_deadline: None,
            shutdown_sent: false,
            completion_recorded: false,
            #[cfg(test)]
            settings: None,
        }
    }

    fn context(&self) -> WorkResult<&Context> {
        self.context
            .as_ref()
            .ok_or_else(|| Failure::io("registered lifecycle is not initialized"))
    }
    fn persist(&self, budget: &Budget) -> WorkResult<()> {
        if self.activation.is_none() {
            return Err(Failure::io(
                "runtime facts require the owned activation lease",
            ));
        }
        write_facts(self.context()?, &self.facts, budget)
    }

    fn initialize(&mut self, budget: &Budget) -> WorkResult<Reply> {
        if self.context.is_some() || self.activation.is_some() {
            return Err(Failure::io("registered lifecycle is already initialized"));
        }
        #[cfg(test)]
        let executable_path = match &self.settings {
            Some(settings) => settings.executable.clone(),
            None => budget.operation(|| canonical_executable().map_err(Failure::io))?,
        };
        #[cfg(not(test))]
        let executable_path = budget.operation(|| canonical_executable().map_err(Failure::io))?;
        let executable = budget.operation(|| {
            filesystem::read_owned_executable(&executable_path).map_err(Failure::io)
        })?;
        let discovered = budget
            .operation(|| StatePaths::discover(self.state_dir.as_deref()).map_err(Failure::io))?;
        // Registration creates the managed root; its hidden activation must never recreate it.
        let root = budget.operation(|| {
            DirectoryGuard::existing_private(&discovered.root).map_err(Failure::io)
        })?;
        let paths = StatePaths::new(root.normalized_path().to_path_buf());
        let logs = ServiceLogs::open(&paths.root.join(self.target.log_name()), budget)?;
        self.context = Some(Context {
            paths,
            root,
            executable,
            logs,
            target: self.target,
        });
        budget.check()?;
        let activation = DaemonLock::acquire_role(
            self.target.activation_lock(&self.context()?.paths),
            &metadata(self.facts.supervisor_lifetime.clone()),
            true,
        );
        self.activation = Some(activation.map_err(Failure::io)?);
        budget.check()?;
        let control = {
            let _entered = self.main_runtime.enter();
            locron_engine::ipc::bind_role_control(
                self.context()?.root.normalized_path(),
                self.target.supervisor_role(),
                &self.facts.supervisor_lifetime,
                self.admission.cancellation.clone(),
            )
        }
        .map_err(Failure::io)?;
        let _ = self.admission.control_abort.set(control.abort_handle());
        self.control = Some(control);
        budget.check()?;
        self.persist(budget)?;
        Ok(Reply::Initialized)
    }

    fn start(&mut self, budget: &Budget) -> WorkResult<Reply> {
        budget.check()?;
        if self.child.is_some() || self.uncertain.is_some() || self.facts.attempts >= MAX_ATTEMPTS {
            return Err(Failure::io(
                "registered start refused while ownership or attempt bound remains",
            ));
        }
        if self.admission.cancellation.is_cancelled() {
            return Ok(Reply::Pending {
                exit_deadline: None,
            });
        }
        self.facts.attempts += 1;
        self.facts.worker_lifetime = Some(uuid::Uuid::now_v7().to_string());
        self.facts.worker_pid = None;
        self.facts.phase = Phase::Waiting;
        self.facts.cause = None;
        self.persist(budget)?;
        let command = child_command(
            self.context()?,
            &self.facts.supervisor_lifetime,
            self.facts.worker_lifetime.as_deref().unwrap_or_default(),
            budget,
        )?;
        budget.check()?;
        if self.admission.cancellation.is_cancelled() {
            return Ok(Reply::Pending {
                exit_deadline: None,
            });
        }
        #[cfg(test)]
        let command = if let Some(settings) = &self.settings {
            fixture_command(self.context()?, settings, &self.facts, budget)?
        } else {
            command
        };
        budget.check()?;
        if self.admission.cancellation.is_cancelled() {
            return Ok(Reply::Pending {
                exit_deadline: None,
            });
        }
        let spawned = OwnedChild::spawn(command, ChildWindow::Hidden);
        match spawned {
            Ok(child) => {
                self.facts.worker_pid = child.id();
                self.child = Some(child);
            }
            Err(SpawnFailure::ExecutionMayHaveStarted { error, containment }) => {
                self.uncertain = Some(containment);
                budget.check()?;
                return Err(Failure::io(error));
            }
            Err(SpawnFailure::NotStarted(error)) => {
                budget.check()?;
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                ) {
                    return Err(Failure::io(error));
                }
                let message = error.to_string().chars().take(512).collect::<String>();
                self.facts.not_started.push(NotStartedAttempt {
                    lifetime: self.facts.worker_lifetime.clone().unwrap_or_default(),
                    message: message.clone(),
                    finished_at_us: crate::now_us(),
                });
                self.facts.cause = Some(Cause::NotStarted { message });
                self.facts.phase = if self.facts.attempts == MAX_ATTEMPTS {
                    Phase::Exhausted
                } else {
                    Phase::Retry
                };
                self.persist(budget)?;
                return Ok(Reply::NotStarted {
                    deadline: budget.requested,
                });
            }
        }
        budget.check()?;
        if self.facts.worker_pid.is_none() {
            return Err(Failure::io("owned registered child has no native PID"));
        }
        self.worker_lease = Lease::default();
        self.role_lease = Lease::default();
        self.exit_deadline = None;
        self.shutdown_sent = false;
        self.completion_recorded = false;
        self.persist(budget)?;
        Ok(Reply::Started)
    }

    fn observe(&mut self, budget: &Budget, stopping: bool) -> WorkResult<Reply> {
        if self.child.is_none() {
            return Ok(Reply::Pending {
                exit_deadline: None,
            });
        }
        if stopping && self.facts.phase != Phase::Stopping && self.facts.phase != Phase::Error {
            self.facts.phase = Phase::Stopping;
            self.facts.cause = Some(Cause::Cancelled);
            self.persist(budget)?;
        }
        let status = budget.operation(|| {
            self.child
                .as_mut()
                .ok_or_else(|| Failure::io("missing owned child"))?
                .try_wait()
                .map_err(Failure::io)
        })?;
        if status.is_some() && self.exit_deadline.is_none() {
            self.exit_deadline = Some(Instant::now() + STOP_TIMEOUT);
        }
        let budget = Budget {
            admission: Arc::clone(&budget.admission),
            requested: self
                .exit_deadline
                .map_or(budget.requested, |deadline| deadline.min(budget.requested)),
        };
        budget.check()?;
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| Failure::io("missing owned context"))?;
        let pid = self
            .facts
            .worker_pid
            .ok_or_else(|| Failure::io("missing worker PID"))?;
        let lifetime = self
            .facts
            .worker_lifetime
            .as_deref()
            .ok_or_else(|| Failure::io("missing worker UUID"))?;
        if let Some(status) = status
            && budget.operation(|| {
                self.child
                    .as_ref()
                    .ok_or_else(|| Failure::io("missing owned child"))?
                    .tree_empty()
                    .map_err(Failure::io)
            })?
        {
            // Root reaping plus an empty retained Job proves the old processes closed their
            // role handles. Release its sidecar before I/O so a new manual owner can publish.
            budget.operation(|| {
                self.role_lease = Lease::default();
                Ok(())
            })?;
            if !self
                .worker_lease
                .released(self.target.worker_activation_lock(&context.paths), &budget)?
            {
                return Err(Failure::io(
                    "owned worker activation lease remains held after tree exit",
                ));
            }
            let code = status
                .code()
                .ok_or_else(|| Failure::io("registered worker has no actual i32 exit code"))?;
            context.logs.sync(&budget)?;
            if !self.completion_recorded {
                self.facts.completed.push(CompletedAttempt {
                    lifetime: lifetime.into(),
                    pid,
                    exit_code: code,
                    finished_at_us: crate::now_us(),
                });
                self.completion_recorded = true;
            }
            if self.facts.phase != Phase::Error {
                self.facts.cause = Some(if stopping {
                    Cause::Cancelled
                } else {
                    Cause::ChildExit { code }
                });
                self.facts.phase = if code != 0 && !stopping && self.facts.attempts == MAX_ATTEMPTS
                {
                    Phase::Exhausted
                } else {
                    Phase::Stopping
                };
            }
            self.persist(&budget)?;
            budget.operation(|| {
                self.child = None;
                self.worker_lease = Lease::default();
                self.role_lease = Lease::default();
                Ok(())
            })?;
            let deadline = self
                .exit_deadline
                .take()
                .ok_or_else(|| Failure::io("missing observed exit deadline"))?
                .min(budget.admission.deadline(budget.requested));
            return Ok(Reply::Completed { code, deadline });
        }
        if stopping {
            if !self.shutdown_sent
                && self.worker_lease.owns(
                    self.target.worker_activation_lock(&context.paths),
                    pid,
                    lifetime,
                    &budget,
                )?
            {
                self.shutdown_sent = true;
                budget.check()?;
                // Once queued, delivery is uncertain even when the receipt times out. Never replay.
                let _delivery = locron_core::notification::request_shutdown_guarded_until(
                    &context.root,
                    self.target.worker_role(),
                    lifetime,
                    budget.admission.deadline(budget.requested),
                );
                budget.check()?;
            }
        } else if self.facts.phase != Phase::Running
            && self.role_lease.owns(
                self.target.role_lock(&context.paths),
                pid,
                lifetime,
                &budget,
            )?
        {
            self.facts.phase = Phase::Running;
            self.persist(&budget)?;
        }
        Ok(Reply::Pending {
            exit_deadline: self.exit_deadline,
        })
    }

    async fn action(&mut self, action: Action, budget: &Budget) -> WorkResult<Reply> {
        budget.check()?;
        match action {
            Action::Initialize => self.initialize(budget),
            Action::Start => self.start(budget),
            Action::Observe { stopping } => self.observe(budget, stopping),
            Action::Retry => {
                if self.child.is_some() || self.uncertain.is_some() {
                    return Err(Failure::io(
                        "retry refused with unconfirmed child ownership",
                    ));
                }
                self.facts.phase = Phase::Retry;
                self.persist(budget)?;
                Ok(Reply::Pending {
                    exit_deadline: None,
                })
            }
            Action::Refuse(message) => {
                self.facts.phase = Phase::Error;
                self.facts.cause = Some(Cause::Infrastructure { message });
                if self.activation.is_some() {
                    self.persist(budget)?;
                }
                Ok(Reply::Refusing {
                    child: self.child.is_some(),
                    uncertain: self.uncertain.is_some(),
                })
            }
            Action::Finish { cancelled } => {
                if self.child.is_some() || self.uncertain.is_some() {
                    return Err(Failure::io(
                        "registered teardown refused with unconfirmed child ownership",
                    ));
                }
                if cancelled {
                    self.facts.phase = Phase::Stopping;
                    self.facts.cause = Some(Cause::Cancelled);
                    self.persist(budget)?;
                }
                if let Some(context) = &self.context {
                    context.logs.sync(budget)?;
                }
                budget.check()?;
                if let Some(control) = self.control.take() {
                    control.abort();
                    let joined = budget.await_response(control).await?;
                    if joined.as_ref().is_err_and(|error| !error.is_cancelled()) {
                        return Err(Failure::io("registered control listener teardown failed"));
                    }
                }
                // Retain activation until root/tree/leases, diagnostics and listener are confirmed.
                budget.operation(|| {
                    self.activation = None;
                    Ok(())
                })?;
                budget.operation(|| {
                    self.context = None;
                    Ok(())
                })?;
                Ok(Reply::Finished)
            }
        }
    }
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        if let Some(control) = &self.control {
            control.abort();
        }
        // Dropping retained Jobs is emergency containment, never graceful/root reaping proof.
    }
}

struct Worker {
    admission: Arc<Admission>,
    requests: Option<mpsc::SyncSender<Request>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn spawn(
        target: Target,
        state_dir: Option<PathBuf>,
        cancellation: CancellationToken,
    ) -> WorkResult<Self> {
        Self::spawn_config(
            target,
            state_dir,
            cancellation,
            #[cfg(test)]
            None,
        )
    }

    fn spawn_config(
        target: Target,
        state_dir: Option<PathBuf>,
        cancellation: CancellationToken,
        #[cfg(test)] settings: Option<TestSettings>,
    ) -> WorkResult<Self> {
        let admission = Arc::new(Admission::new(cancellation));
        let worker_admission = Arc::clone(&admission);
        let main_runtime = tokio::runtime::Handle::current();
        let (requests, receiver) = mpsc::sync_channel::<Request>(1);
        let thread = std::thread::Builder::new()
            .name("locron-role-owner".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let mut lifecycle = Lifecycle::new(
                    target,
                    state_dir,
                    Arc::clone(&worker_admission),
                    main_runtime,
                );
                #[cfg(test)]
                {
                    lifecycle.settings = settings;
                }
                let mut normally_finished = false;
                while let Ok(request) = receiver.recv() {
                    if !worker_admission.open.load(Ordering::Acquire) {
                        break;
                    }
                    let finish = matches!(request.action, Action::Finish { .. });
                    let result = match &runtime {
                        Ok(runtime) => {
                            runtime.block_on(lifecycle.action(request.action, &request.budget))
                        }
                        Err(error) => Err(Failure::io(error)),
                    };
                    let result = request.budget.check().and(result);
                    let expired = result.as_ref().is_err_and(|error| error.expired);
                    let finished = finish && result.is_ok();
                    let _ = request.response.send(result);
                    if expired || finished {
                        normally_finished = finished;
                        break;
                    }
                }
                if !normally_finished {
                    worker_admission.close();
                }
            })
            .map_err(Failure::io)?;
        Ok(Self {
            admission,
            requests: Some(requests),
            thread: Some(thread),
        })
    }

    fn budget(&self, deadline: Instant) -> Budget {
        Budget {
            admission: Arc::clone(&self.admission),
            requested: deadline,
        }
    }

    async fn request(&mut self, action: Action, deadline: Instant) -> WorkResult<Reply> {
        let budget = self.budget(deadline);
        budget.check()?;
        let (response, receipt) = oneshot::channel();
        self.requests
            .as_ref()
            .ok_or_else(Failure::expired)?
            .try_send(Request {
                action,
                budget: budget.clone(),
                response,
            })
            .map_err(Failure::io)?;
        let result = budget.await_response(receipt).await?.map_err(Failure::io)?;
        budget.check()?;
        result
    }

    fn quarantine(&mut self) {
        self.admission.close();
        self.requests = None;
    }

    async fn join_until(&mut self, deadline: Instant) -> WorkResult<()> {
        let budget = self.budget(deadline);
        while self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            budget.check()?;
            tokio::time::sleep(
                OBSERVE_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            )
            .await;
        }
        budget.check()?;
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| Failure::io("registered native owner panicked"))?;
        }
        budget.check()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.quarantine();
        // An unfinished std thread is deliberately not joined. It owns the quarantine until
        // its uncancellable operation returns. There is exactly one worker, never a replay.
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
    }
}

async fn finish(worker: &mut Worker, cancelled: bool, deadline: Instant) -> WorkResult<()> {
    if !matches!(
        worker
            .request(Action::Finish { cancelled }, deadline)
            .await?,
        Reply::Finished
    ) {
        return Err(Failure::io("unexpected registered teardown reply"));
    }
    worker.join_until(deadline).await
}

async fn refuse(worker: &mut Worker, failure: Failure) -> ServiceError {
    if worker.thread.is_none() {
        // A fully confirmed NotStarted exhaustion already released every owned resource.
        return failure.service();
    }
    if failure.expired {
        worker.quarantine();
        return failure.service();
    }
    let deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
    match worker
        .request(Action::Refuse(failure.message.clone()), deadline)
        .await
    {
        Ok(Reply::Refusing {
            child: true,
            uncertain: false,
        }) => loop {
            match worker
                .request(Action::Observe { stopping: true }, deadline)
                .await
            {
                Ok(Reply::Completed { .. }) => break,
                Ok(Reply::Pending { .. }) => {
                    tokio::time::sleep(
                        OBSERVE_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                    )
                    .await
                }
                _ => {
                    worker.quarantine();
                    return failure.service();
                }
            }
        },
        Ok(Reply::Refusing {
            child: false,
            uncertain: false,
        }) => {}
        _ => {
            worker.quarantine();
            return failure.service();
        }
    }
    if finish(worker, false, deadline).await.is_err() {
        worker.quarantine();
    }
    failure.service()
}

async fn run(worker: &mut Worker) -> WorkResult<i32> {
    for attempt in 1..=MAX_ATTEMPTS {
        if worker.admission.cancellation.is_cancelled() {
            let deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
            finish(worker, true, deadline).await?;
            return Ok(0);
        }
        let mut deadline = Instant::now() + STOP_TIMEOUT;
        let mut end = worker.request(Action::Start, deadline).await?;
        if matches!(end, Reply::Started) {
            loop {
                let stopping = worker.admission.cancellation.is_cancelled();
                if stopping {
                    deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
                }
                end = worker
                    .request(Action::Observe { stopping }, deadline)
                    .await?;
                match end {
                    Reply::Pending { exit_deadline } => {
                        deadline = exit_deadline.unwrap_or_else(|| Instant::now() + STOP_TIMEOUT);
                        tokio::select! {
                            () = worker.admission.cancellation.cancelled(), if !stopping => {},
                            () = tokio::time::sleep(OBSERVE_INTERVAL) => {},
                        }
                    }
                    Reply::Completed { .. } => break,
                    _ => return Err(Failure::io("unexpected registered observation reply")),
                }
            }
        }
        match end {
            Reply::Completed { code, deadline } => {
                if worker.admission.cancellation.is_cancelled()
                    || code == 0
                    || attempt == MAX_ATTEMPTS
                {
                    let cancelled = worker.admission.cancellation.is_cancelled();
                    finish(worker, cancelled, deadline).await?;
                    return Ok(if cancelled { 0 } else { code });
                }
                worker.request(Action::Retry, deadline).await?;
            }
            Reply::NotStarted { deadline } => {
                if attempt == MAX_ATTEMPTS {
                    finish(worker, false, deadline).await?;
                    return Err(Failure::io(
                        "known-not-started registered attempts exhausted without a child exit code",
                    ));
                }
            }
            Reply::Pending { .. } if worker.admission.cancellation.is_cancelled() => {
                let deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
                finish(worker, true, deadline).await?;
                return Ok(0);
            }
            _ => return Err(Failure::io("unexpected registered start reply")),
        }
        #[cfg(test)]
        worker
            .admission
            .retry_waiting
            .store(true, Ordering::Release);
        tokio::select! {
            () = worker.admission.cancellation.cancelled() => {},
            () = tokio::time::sleep(RETRY_DELAY) => {},
        }
        #[cfg(test)]
        worker
            .admission
            .retry_waiting
            .store(false, Ordering::Release);
    }
    Err(Failure::io("registered attempt bound exceeded"))
}

/// The hidden caller propagates genuine i32 outcomes; every error bypasses the public renderer
/// and exits 70. No caller joins the quarantined native worker after deadline expiry.
pub(crate) async fn supervise(
    state_dir: Option<PathBuf>,
    target: Target,
) -> Result<i32, ServiceError> {
    let entry_deadline = Instant::now() + STOP_TIMEOUT;
    let cancellation = CancellationToken::new();
    let mut worker = Worker::spawn(target, state_dir, cancellation).map_err(Failure::service)?;
    if let Err(failure) = worker.request(Action::Initialize, entry_deadline).await {
        return Err(refuse(&mut worker, failure).await);
    }
    match run(&mut worker).await {
        Ok(code) => Ok(code),
        Err(failure) => Err(refuse(&mut worker, failure).await),
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    BeforeMetadataRead,
    MetadataRead,
    FactSync,
    FactRename,
}

#[cfg(test)]
struct TestGate {
    boundary: Boundary,
    entered: AtomicBool,
    release: (Mutex<bool>, std::sync::Condvar),
}

#[cfg(test)]
impl TestGate {
    fn withhold(&self, boundary: Boundary) {
        if boundary != self.boundary || self.entered.swap(true, Ordering::AcqRel) {
            return;
        }
        let (lock, signal) = &self.release;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = signal.wait(released).unwrap();
        }
    }

    fn release(&self) {
        *self.release.0.lock().unwrap() = true;
        self.release.1.notify_all();
    }
}

#[cfg(test)]
struct ReleaseGate(Arc<TestGate>);

#[cfg(test)]
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[cfg(test)]
#[derive(Clone)]
struct TestSettings {
    executable: PathBuf,
    codes: Vec<i32>,
    cooperative: bool,
    missing: bool,
}

#[cfg(test)]
fn fixture_command(
    context: &Context,
    settings: &TestSettings,
    facts: &RuntimeFacts,
    budget: &Budget,
) -> WorkResult<Command> {
    let program = if settings.missing {
        context
            .paths
            .root
            .join("never-installed-locron-fixture.exe")
    } else {
        settings.executable.clone()
    };
    let mut command = Command::new(program);
    command
        .args([
            "--exact",
            "service::windows_supervisor::tests::native_role_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            "LOCRON_SUPERVISOR_FIXTURE",
            if settings.cooperative {
                "cooperative"
            } else {
                "codes"
            },
        )
        .env("LOCRON_SUPERVISOR_ROOT", &context.paths.root)
        .env("LOCRON_SUPERVISOR_PARENT", &facts.supervisor_lifetime)
        .env(
            "LOCRON_SUPERVISOR_WORKER",
            facts.worker_lifetime.as_deref().unwrap_or_default(),
        )
        .env("LOCRON_SUPERVISOR_ATTEMPT", facts.attempts.to_string())
        .env(
            "LOCRON_SUPERVISOR_CODE",
            settings
                .codes
                .get(usize::from(facts.attempts - 1))
                .copied()
                .unwrap_or(0)
                .to_string(),
        )
        .current_dir(&context.paths.root);
    context.logs.apply(&mut command, budget)?;
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::{
        Action, Admission, Boundary, Budget, MAX_ATTEMPTS, RETRY_DELAY, ReleaseGate, Reply,
        STOP_TIMEOUT, TestGate, TestSettings, Worker, finish, metadata, refuse, run,
    };
    use crate::service::{self, Target};
    use locron_core::filesystem::{self, DirectoryGuard};
    use locron_engine::windows_child::{ChildWindow, OwnedChild};
    use locron_store::{DaemonLock, LockProbe, StatePaths};
    use std::future::poll_fn;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        settings: TestSettings,
    }

    impl Fixture {
        fn new(codes: Vec<i32>, cooperative: bool, missing: bool) -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("private 状態 % #");
            let guard = DirectoryGuard::private(&root).unwrap();
            let executable = guard.normalized_path().join("native role fixture.exe");
            let mut source =
                filesystem::open_read_no_follow(&std::env::current_exe().unwrap()).unwrap();
            let mut destination = filesystem::create_private_new(&executable).unwrap();
            std::io::copy(&mut *source, &mut *destination).unwrap();
            destination.sync_all().unwrap();
            drop(destination);
            drop(source);
            let root = guard.normalized_path().to_path_buf();
            drop(guard);
            Self {
                _temporary: temporary,
                root,
                settings: TestSettings {
                    executable,
                    codes,
                    cooperative,
                    missing,
                },
            }
        }

        async fn worker(&self, cancellation: CancellationToken) -> Worker {
            let mut worker = Worker::spawn_config(
                Target::Daemon,
                Some(self.root.clone()),
                cancellation,
                Some(self.settings.clone()),
            )
            .unwrap();
            assert!(matches!(
                worker
                    .request(Action::Initialize, Instant::now() + STOP_TIMEOUT)
                    .await
                    .unwrap(),
                Reply::Initialized
            ));
            worker
        }

        fn facts(&self) -> serde_json::Value {
            let mut file = filesystem::open_private(
                &self.root.join("service.daemon.runtime.json"),
                std::fs::OpenOptions::new().read(true),
            )
            .unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        fn log(&self) -> String {
            let path = self.root.join("service.daemon.log");
            assert!(filesystem::is_private(&path, false).unwrap());
            let mut file =
                filesystem::open_private(&path, std::fs::OpenOptions::new().read(true)).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            String::from_utf8(bytes).unwrap()
        }
    }

    fn marker(path: &Path, bytes: &[u8]) {
        let mut file = filesystem::create_private_new(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    async fn wait_for(path: &Path) {
        wait_for_until(path, Instant::now() + STOP_TIMEOUT).await;
    }

    async fn wait_for_until(path: &Path, deadline: Instant) {
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "native role did not publish {path:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn native_role_fixture() {
        let Ok(mode) = std::env::var("LOCRON_SUPERVISOR_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("LOCRON_SUPERVISOR_ROOT").unwrap());
        let attempt = std::env::var("LOCRON_SUPERVISOR_ATTEMPT").unwrap();
        if mode == "descendant" {
            std::thread::sleep(Duration::from_millis(400));
            marker(&root.join(format!("descendant-done-{attempt}")), b"done");
            std::process::exit(0);
        }
        if mode == "manual" {
            let lifetime = std::env::var("LOCRON_SUPERVISOR_MANUAL_LIFETIME").unwrap();
            let paths = StatePaths::new(root.clone());
            let role =
                DaemonLock::acquire_role(&paths.daemon_lock, &metadata(lifetime.clone()), false)
                    .unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let cancellation = CancellationToken::new();
                let control = locron_engine::ipc::bind_role_control(
                    &root,
                    "daemon",
                    &lifetime,
                    cancellation.clone(),
                )
                .unwrap();
                marker(&root.join("manual-owner-ready"), b"ready");
                cancellation.cancelled().await;
                control.abort();
                let _ = control.await;
                drop(role);
            });
            std::process::exit(0);
        }
        let code: i32 = std::env::var("LOCRON_SUPERVISOR_CODE")
            .unwrap()
            .parse()
            .unwrap();
        let parent = std::env::var("LOCRON_SUPERVISOR_PARENT").unwrap();
        let worker = std::env::var("LOCRON_SUPERVISOR_WORKER").unwrap();
        let paths = StatePaths::new(root.clone());
        let _parent_guard = service::validate_supervisor(&paths, Target::Daemon, &parent).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let cancellation = CancellationToken::new();
            let lease = DaemonLock::acquire_role(
                &paths.daemon_worker_activation_lock,
                &metadata(worker.clone()),
                true,
            )
            .unwrap();
            let control = locron_engine::ipc::bind_role_control(
                &root,
                "daemon-worker",
                &worker,
                cancellation.clone(),
            )
            .unwrap();
            marker(&root.join(format!("worker-ready-{attempt}")), b"ready");
            println!("stdout attempt={attempt} code={code}");
            eprintln!("stderr attempt={attempt} code={code}");
            let role = loop {
                if cancellation.is_cancelled() {
                    break None;
                }
                match DaemonLock::acquire_role(&paths.daemon_lock, &metadata(worker.clone()), true)
                {
                    Ok(role) => break Some(role),
                    Err(locron_store::StoreError::DaemonAlreadyRunning) => {
                        tokio::time::sleep(Duration::from_millis(20)).await
                    }
                    Err(error) => panic!("native fixture role failed: {error}"),
                }
            };
            if mode == "cooperative" {
                while !cancellation.is_cancelled() {
                    let path = root.join("heartbeat");
                    let mut file = match filesystem::open_private(
                        &path,
                        std::fs::OpenOptions::new().append(true),
                    ) {
                        Ok(file) => file,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            filesystem::create_private_new(&path).unwrap()
                        }
                        Err(error) => panic!("native heartbeat failed: {error}"),
                    };
                    file.write_all(b"x").unwrap();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                marker(&root.join("cooperative-stop"), b"stopped");
            } else {
                let ordinal: u8 = attempt.parse().unwrap();
                for earlier in 1..ordinal {
                    assert!(
                        root.join(format!("descendant-done-{earlier}")).exists(),
                        "next native role overlapped an old descendant"
                    );
                }
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "service::windows_supervisor::tests::native_role_fixture",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("LOCRON_SUPERVISOR_FIXTURE", "descendant")
                    .spawn()
                    .unwrap();
                // The external owned Job must confirm this deliberately surviving descendant.
                let _descendant = std::os::windows::io::OwnedHandle::from(child);
                std::process::exit(code);
            }
            drop(role);
            control.abort();
            let _ = control.await;
            drop(lease);
        });
        std::process::exit(0);
    }

    #[tokio::test]
    async fn late_ready_response_is_refused_before_driver_dispatch() {
        let admission = Arc::new(Admission::new(CancellationToken::new()));
        let budget = Budget {
            admission,
            requested: Instant::now() + Duration::from_millis(20),
        };
        let polls = AtomicUsize::new(0);
        let response = poll_fn(|_| {
            polls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(45));
            std::task::Poll::Ready(Reply::Initialized)
        });
        let error = budget.await_response(response).await.unwrap_err();
        assert!(error.expired);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(
            budget
                .operation(|| -> Result<(), Failure> {
                    panic!("expired response admitted another operation")
                })
                .is_err()
        );
    }

    #[tokio::test]
    async fn native_io_expiry_quarantines_activation_child_and_all_later_admission() {
        for boundary in [
            Boundary::BeforeMetadataRead,
            Boundary::MetadataRead,
            Boundary::FactSync,
            Boundary::FactRename,
        ] {
            let fixture = Fixture::new(vec![0], true, false);
            let mut worker = fixture.worker(CancellationToken::new()).await;
            assert!(matches!(
                worker
                    .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                    .await
                    .unwrap(),
                Reply::Started
            ));
            wait_for(&fixture.root.join("worker-ready-1")).await;
            wait_for(&fixture.root.join("heartbeat")).await;
            let gate = Arc::new(TestGate {
                boundary,
                entered: AtomicBool::new(false),
                release: (Mutex::new(false), std::sync::Condvar::new()),
            });
            let release = ReleaseGate(Arc::clone(&gate));
            *worker.admission.gate.lock().unwrap() = Some(Arc::clone(&gate));
            let action = if matches!(
                boundary,
                Boundary::BeforeMetadataRead | Boundary::MetadataRead
            ) {
                Action::Observe { stopping: false }
            } else {
                Action::Refuse("native withheld I/O fixture".into())
            };
            let started = Instant::now();
            let error = worker
                .request(action, started + Duration::from_millis(100))
                .await
                .unwrap_err();
            assert!(error.expired);
            assert!(started.elapsed() < Duration::from_millis(250));
            assert!(
                gate.entered.load(Ordering::Acquire),
                "the actual native I/O boundary was not entered"
            );
            worker.quarantine();
            assert!(!worker.thread.as_ref().unwrap().is_finished());
            assert!(
                worker
                    .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                    .await
                    .unwrap_err()
                    .expired
            );
            let paths = StatePaths::new(fixture.root.clone());
            assert_eq!(
                DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
                LockProbe::Held
            );
            let heartbeat = fixture.root.join("heartbeat");
            let before = std::fs::metadata(&heartbeat).unwrap().len();
            tokio::time::sleep(Duration::from_millis(80)).await;
            assert!(
                std::fs::metadata(&heartbeat).unwrap().len() > before,
                "driver timeout discarded the live Job"
            );
            assert!(
                !fixture.root.join("cooperative-stop").exists(),
                "expired observation delivered a late shutdown"
            );
            drop(release);
            let cleanup = Instant::now() + STOP_TIMEOUT;
            while !worker.thread.as_ref().unwrap().is_finished() {
                assert!(
                    Instant::now() < cleanup,
                    "released native I/O did not return its ownership"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            drop(worker);
            assert_eq!(
                DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
                LockProbe::Free
            );
            assert_eq!(fixture.facts()["attempts"], 1);
            assert!(!fixture.root.join("worker-ready-2").exists());
        }
    }

    #[tokio::test]
    async fn genuine_four_codes_and_three_real_waits_keep_private_logs_and_no_tree_overlap() {
        let codes = vec![17, 70, 23, -1_073_741_510];
        let fixture = Fixture::new(codes.clone(), false, false);
        let mut worker = fixture.worker(CancellationToken::new()).await;
        let started = Instant::now();
        assert_eq!(run(&mut worker).await.unwrap(), -1_073_741_510);
        assert!(started.elapsed() >= RETRY_DELAY * u32::from(MAX_ATTEMPTS - 1));
        let facts = fixture.facts();
        assert_eq!(facts["phase"], "exhausted");
        assert_eq!(facts["completed"].as_array().unwrap().len(), 4);
        assert!(facts["not_started"].as_array().unwrap().is_empty());
        for (index, expected) in codes.iter().enumerate() {
            assert_eq!(facts["completed"][index]["exit_code"], *expected);
            assert!(
                fixture
                    .root
                    .join(format!("descendant-done-{}", index + 1))
                    .exists()
            );
        }
        let log = fixture.log();
        for (index, code) in codes.iter().enumerate() {
            assert!(log.contains(&format!("stdout attempt={} code={code}", index + 1)));
            assert!(log.contains(&format!("stderr attempt={} code={code}", index + 1)));
        }
        let paths = StatePaths::new(fixture.root.clone());
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
            LockProbe::Free
        );
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_worker_activation_lock).unwrap(),
            LockProbe::Free
        );
        assert!(!fixture.root.join("worker-ready-5").exists());
    }

    #[tokio::test]
    async fn actual_not_started_exhaustion_has_the_same_ceiling_without_inventing_exit_codes() {
        let fixture = Fixture::new(Vec::new(), false, true);
        let mut worker = fixture.worker(CancellationToken::new()).await;
        let started = Instant::now();
        let error = run(&mut worker).await.unwrap_err();
        assert!(!error.expired);
        assert!(started.elapsed() >= RETRY_DELAY * u32::from(MAX_ATTEMPTS - 1));
        let facts = fixture.facts();
        assert_eq!(facts["phase"], "exhausted");
        assert_eq!(facts["cause"]["kind"], "not_started");
        assert_eq!(facts["not_started"].as_array().unwrap().len(), 4);
        assert!(facts["completed"].as_array().unwrap().is_empty());
        assert!(!fixture.root.join("worker-ready-1").exists());
    }

    #[tokio::test]
    async fn cooperative_waiting_cancellation_preserves_the_unrelated_manual_owner() {
        let fixture = Fixture::new(vec![0], true, false);
        let paths = StatePaths::new(fixture.root.clone());
        let expected = metadata(uuid::Uuid::now_v7().to_string());
        let manual = DaemonLock::acquire_role(&paths.daemon_lock, &expected, false).unwrap();
        let cancellation = CancellationToken::new();
        let mut worker = fixture.worker(cancellation.clone()).await;
        assert!(matches!(
            worker
                .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap(),
            Reply::Started
        ));
        wait_for(&fixture.root.join("worker-ready-1")).await;
        cancellation.cancel();
        let deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
        loop {
            match worker
                .request(Action::Observe { stopping: true }, deadline)
                .await
                .unwrap()
            {
                Reply::Completed { code, .. } => {
                    assert_eq!(code, 0);
                    break;
                }
                Reply::Pending { .. } => tokio::time::sleep(Duration::from_millis(10)).await,
                other => panic!("unexpected owned stop reply {other:?}"),
            }
        }
        finish(&mut worker, true, deadline).await.unwrap();
        assert!(fixture.root.join("cooperative-stop").exists());
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_lock).unwrap(),
            LockProbe::Held
        );
        assert_eq!(
            DaemonLock::read_role_metadata(&paths.daemon_lock)
                .unwrap()
                .unwrap()
                .metadata,
            expected
        );
        assert_eq!(fixture.facts()["cause"]["kind"], "cancelled");
        drop(manual);
    }

    #[tokio::test]
    async fn startup_cancellation_admits_no_child_after_its_withheld_fact_response() {
        let fixture = Fixture::new(vec![0], true, false);
        let cancellation = CancellationToken::new();
        let mut worker = fixture.worker(cancellation.clone()).await;
        let gate = Arc::new(TestGate {
            boundary: Boundary::FactSync,
            entered: AtomicBool::new(false),
            release: (Mutex::new(false), std::sync::Condvar::new()),
        });
        let release = ReleaseGate(Arc::clone(&gate));
        *worker.admission.gate.lock().unwrap() = Some(Arc::clone(&gate));
        let canceller = tokio::spawn(async move {
            let deadline = Instant::now() + STOP_TIMEOUT;
            while !gate.entered.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "startup did not enter fact sync");
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            cancellation.cancel();
            gate.release();
        });
        assert!(matches!(
            worker
                .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap(),
            Reply::Pending { .. }
        ));
        canceller.await.unwrap();
        assert_eq!(run(&mut worker).await.unwrap(), 0);
        drop(release);
        let facts = fixture.facts();
        assert_eq!(facts["attempts"], 1);
        assert!(facts["worker_pid"].is_null());
        assert!(facts["completed"].as_array().unwrap().is_empty());
        assert!(facts["not_started"].as_array().unwrap().is_empty());
        assert_eq!(facts["cause"]["kind"], "cancelled");
        assert!(!fixture.root.join("worker-ready-1").exists());
    }

    #[tokio::test]
    async fn running_cancellation_confirms_the_real_tree_and_both_owned_leases() {
        let fixture = Fixture::new(vec![0], true, false);
        let cancellation = CancellationToken::new();
        let mut worker = fixture.worker(cancellation.clone()).await;
        assert!(matches!(
            worker
                .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap(),
            Reply::Started
        ));
        wait_for(&fixture.root.join("heartbeat")).await;
        worker
            .request(
                Action::Observe { stopping: false },
                Instant::now() + STOP_TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(fixture.facts()["phase"], "running");
        cancellation.cancel();
        let deadline = worker.admission.stop_at(Instant::now() + STOP_TIMEOUT);
        loop {
            match worker
                .request(Action::Observe { stopping: true }, deadline)
                .await
                .unwrap()
            {
                Reply::Completed { code, .. } => {
                    assert_eq!(code, 0);
                    break;
                }
                Reply::Pending { .. } => tokio::time::sleep(Duration::from_millis(10)).await,
                other => panic!("unexpected running stop reply {other:?}"),
            }
        }
        finish(&mut worker, true, deadline).await.unwrap();
        let paths = StatePaths::new(fixture.root.clone());
        for lock in [
            &paths.daemon_activation_lock,
            &paths.daemon_worker_activation_lock,
            &paths.daemon_lock,
        ] {
            assert_eq!(DaemonLock::probe_existing(lock).unwrap(), LockProbe::Free);
        }
        assert!(fixture.root.join("cooperative-stop").exists());
        assert_eq!(fixture.facts()["completed"][0]["exit_code"], 0);
        assert!(fixture.log().contains("stdout attempt=1 code=0"));
        assert!(fixture.log().contains("stderr attempt=1 code=0"));
    }

    #[tokio::test]
    async fn cancellation_interrupts_the_real_retry_delay_without_another_start() {
        let fixture = Fixture::new(vec![17], false, false);
        let cancellation = CancellationToken::new();
        let mut worker = fixture.worker(cancellation.clone()).await;
        let admission = Arc::clone(&worker.admission);
        let canceller = tokio::spawn(async move {
            let deadline = Instant::now() + STOP_TIMEOUT;
            while !admission.retry_waiting.load(Ordering::Acquire) {
                assert!(
                    Instant::now() < deadline,
                    "native first exit did not reach retry wait"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let retry_started = Instant::now();
            tokio::time::sleep(Duration::from_millis(25)).await;
            cancellation.cancel();
            retry_started
        });
        assert_eq!(run(&mut worker).await.unwrap(), 0);
        let retry_started = canceller.await.unwrap();
        assert!(retry_started.elapsed() < STOP_TIMEOUT);
        let facts = fixture.facts();
        assert_eq!(facts["attempts"], 1);
        assert_eq!(facts["completed"][0]["exit_code"], 17);
        assert_eq!(facts["cause"]["kind"], "cancelled");
        assert!(fixture.root.join("descendant-done-1").exists());
        assert!(!fixture.root.join("worker-ready-2").exists());
        assert!(fixture.log().contains("stderr attempt=1 code=17"));
    }

    #[tokio::test]
    async fn a_real_diagnostic_write_refusal_never_admits_a_child_or_retry() {
        let fixture = Fixture::new(vec![17], false, false);
        let mut worker = fixture.worker(CancellationToken::new()).await;
        let destination = fixture.root.join("service.daemon.runtime.json");
        filesystem::remove_private_file(&destination).unwrap();
        drop(DirectoryGuard::private(&destination).unwrap());
        let error = worker
            .request(Action::Start, Instant::now() + STOP_TIMEOUT)
            .await
            .unwrap_err();
        assert!(!error.expired);
        let _ = refuse(&mut worker, error).await;
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !worker.thread.as_ref().unwrap().is_finished() {
            assert!(
                Instant::now() < deadline,
                "refused facts kept an idle owner alive"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(worker);
        let paths = StatePaths::new(fixture.root.clone());
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
            LockProbe::Free
        );
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_worker_activation_lock).unwrap(),
            LockProbe::Missing
        );
        assert!(destination.is_dir());
        assert!(!fixture.root.join("worker-ready-1").exists());
        assert!(!fixture.root.join("worker-ready-2").exists());
        assert!(fixture.log().is_empty());
    }

    #[tokio::test]
    async fn an_owned_invalid_executable_is_an_infrastructure_refusal_without_retry() {
        let mut fixture = Fixture::new(vec![17], false, false);
        let executable = fixture.root.join("invalid owned image.exe");
        marker(&executable, b"This fixture is not a PE image.");
        fixture.settings.executable = executable;
        let mut worker = fixture.worker(CancellationToken::new()).await;
        let error = worker
            .request(Action::Start, Instant::now() + STOP_TIMEOUT)
            .await
            .unwrap_err();
        assert!(!error.expired);
        let _ = refuse(&mut worker, error).await;
        assert!(
            worker.thread.is_none(),
            "ordinary refusal did not finish ownership"
        );
        let facts = fixture.facts();
        assert_eq!(facts["attempts"], 1);
        assert_eq!(facts["phase"], "error");
        assert_eq!(facts["cause"]["kind"], "infrastructure");
        assert!(facts["completed"].as_array().unwrap().is_empty());
        assert!(facts["not_started"].as_array().unwrap().is_empty());
        assert!(!fixture.root.join("worker-ready-1").exists());
        assert!(!fixture.root.join("worker-ready-2").exists());
    }

    #[tokio::test]
    async fn duplicate_supervisor_refusal_preserves_the_active_owners_runtime_facts() {
        let fixture = Fixture::new(vec![0], false, false);
        let mut owner = fixture.worker(CancellationToken::new()).await;
        let expected = fixture.facts();
        let mut duplicate = Worker::spawn_config(
            Target::Daemon,
            Some(fixture.root.clone()),
            CancellationToken::new(),
            Some(fixture.settings.clone()),
        )
        .unwrap();
        let error = duplicate
            .request(Action::Initialize, Instant::now() + STOP_TIMEOUT)
            .await
            .unwrap_err();
        assert!(!error.expired);
        let _ = refuse(&mut duplicate, error).await;
        assert!(duplicate.thread.is_none());
        assert_eq!(fixture.facts(), expected);
        let paths = StatePaths::new(fixture.root.clone());
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
            LockProbe::Held
        );
        assert_eq!(
            DaemonLock::read_role_metadata(&paths.daemon_activation_lock)
                .unwrap()
                .unwrap()
                .metadata
                .lifetime_id,
            expected["supervisor_lifetime"].as_str().unwrap()
        );
        finish(&mut owner, false, Instant::now() + STOP_TIMEOUT)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn completion_fact_gap_preserves_a_new_native_manual_owner_and_its_publication() {
        let fixture = Fixture::new(vec![0], true, false);
        let mut worker = fixture.worker(CancellationToken::new()).await;
        assert!(matches!(
            worker
                .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap(),
            Reply::Started
        ));
        wait_for(&fixture.root.join("heartbeat")).await;
        worker
            .request(
                Action::Observe { stopping: false },
                Instant::now() + STOP_TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(fixture.facts()["phase"], "running");
        let old_lifetime = fixture.facts()["worker_lifetime"]
            .as_str()
            .unwrap()
            .to_owned();
        let gate = Arc::new(TestGate {
            boundary: Boundary::FactSync,
            entered: AtomicBool::new(false),
            release: (Mutex::new(false), std::sync::Condvar::new()),
        });
        let release = ReleaseGate(Arc::clone(&gate));
        *worker.admission.gate.lock().unwrap() = Some(Arc::clone(&gate));
        locron_core::notification::request_shutdown(&fixture.root, "daemon-worker", &old_lifetime)
            .unwrap();
        let deadline = Instant::now() + STOP_TIMEOUT;
        let completion = tokio::spawn(async move {
            loop {
                match worker
                    .request(Action::Observe { stopping: false }, deadline)
                    .await
                    .unwrap()
                {
                    Reply::Pending { .. } => tokio::time::sleep(Duration::from_millis(10)).await,
                    Reply::Completed { code, deadline } => return (worker, code, deadline),
                    other => panic!("unexpected normal completion reply {other:?}"),
                }
            }
        });
        while !gate.entered.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "native completion did not reach fact sync"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let paths = StatePaths::new(fixture.root.clone());
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
            LockProbe::Held
        );
        let lifetime = uuid::Uuid::now_v7().to_string();
        let mut command = tokio::process::Command::new(&fixture.settings.executable);
        command
            .args([
                "--exact",
                "service::windows_supervisor::tests::native_role_fixture",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("LOCRON_SUPERVISOR_FIXTURE", "manual")
            .env("LOCRON_SUPERVISOR_ROOT", &fixture.root)
            .env("LOCRON_SUPERVISOR_ATTEMPT", "manual")
            .env("LOCRON_SUPERVISOR_MANUAL_LIFETIME", &lifetime)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        let mut manual = OwnedChild::spawn(command, ChildWindow::Hidden).unwrap();
        let pid = manual.id().unwrap();
        wait_for_until(&fixture.root.join("manual-owner-ready"), deadline).await;
        assert!(
            Instant::now() < deadline,
            "manual publication exceeded the completion budget"
        );
        let published = DaemonLock::read_role_metadata(&paths.daemon_lock)
            .unwrap()
            .unwrap();
        assert_eq!(published.metadata.pid, pid);
        assert_eq!(published.metadata.lifetime_id, lifetime);
        assert!(!published.service_mode);
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_lock).unwrap(),
            LockProbe::Held
        );
        drop(release);
        let (mut worker, code, deadline) = completion.await.unwrap();
        assert_eq!(code, 0);
        finish(&mut worker, false, deadline).await.unwrap();
        assert!(manual.try_wait().unwrap().is_none());
        assert_eq!(
            DaemonLock::read_role_metadata(&paths.daemon_lock)
                .unwrap()
                .unwrap(),
            published
        );
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_lock).unwrap(),
            LockProbe::Held
        );
        assert_eq!(
            DaemonLock::probe_existing(&paths.daemon_activation_lock).unwrap(),
            LockProbe::Free
        );
        // Only the independent fixture owner stops its manual process after the assertion.
        locron_core::notification::request_shutdown(&fixture.root, "daemon", &lifetime).unwrap();
        assert_eq!(
            manual
                .confirm_exit_until(Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap()
                .code(),
            Some(0)
        );
    }

    #[tokio::test]
    async fn recovered_fact_failure_records_the_real_child_exit_once_and_never_retries() {
        let fixture = Fixture::new(vec![0], false, false);
        let mut worker = fixture.worker(CancellationToken::new()).await;
        assert!(matches!(
            worker
                .request(Action::Start, Instant::now() + STOP_TIMEOUT)
                .await
                .unwrap(),
            Reply::Started
        ));
        wait_for(&fixture.root.join("descendant-done-1")).await;
        let destination = fixture.root.join("service.daemon.runtime.json");
        filesystem::remove_private_file(&destination).unwrap();
        drop(DirectoryGuard::private(&destination).unwrap());
        let deadline = Instant::now() + STOP_TIMEOUT;
        let error = loop {
            match worker
                .request(Action::Observe { stopping: false }, deadline)
                .await
            {
                Err(error) => break error,
                Ok(Reply::Pending { .. }) => tokio::time::sleep(Duration::from_millis(10)).await,
                other => panic!("unexpected completion fault reply {other:?}"),
            }
        };
        assert!(!error.expired);
        std::fs::remove_dir(&destination).unwrap();
        let _ = refuse(&mut worker, error).await;
        assert!(worker.thread.is_none());
        let facts = fixture.facts();
        assert_eq!(facts["completed"].as_array().unwrap().len(), 1);
        assert_eq!(facts["completed"][0]["exit_code"], 0);
        assert_eq!(facts["attempts"], 1);
        assert_eq!(facts["phase"], "error");
        assert_eq!(facts["cause"]["kind"], "infrastructure");
        assert!(!fixture.root.join("worker-ready-2").exists());
    }

    #[tokio::test]
    async fn missing_registered_root_is_not_created_by_hidden_activation() {
        let fixture = Fixture::new(vec![0], false, false);
        let missing = fixture.root.join("missing registered state");
        let mut worker = Worker::spawn_config(
            Target::Daemon,
            Some(missing.clone()),
            CancellationToken::new(),
            Some(fixture.settings.clone()),
        )
        .unwrap();
        let error = worker
            .request(Action::Initialize, Instant::now() + STOP_TIMEOUT)
            .await
            .unwrap_err();
        assert!(!error.expired);
        let _ = refuse(&mut worker, error).await;
        assert!(!missing.exists());
    }
}
