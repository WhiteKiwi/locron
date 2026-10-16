//! One fixed filesystem child, with bounded requests and kernel-owned crash cleanup.

use std::io;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc as queue;
use tokio::task::JoinHandle;

#[cfg(not(test))]
use super::capture_output;
use super::owned_child::{OwnedChild, SpawnFailure};
use super::{
    OUTPUT_LIMIT, STOCK_JSON_BOOTSTRAP, StockAdapterGuard, StockModuleSet, adapter_poll_until,
    configure_stock_command, remaining,
};

const CLEANUP: Duration = Duration::from_secs(3);
const IDLE: Duration = Duration::from_secs(60);
const SOURCE: &str = include_str!("filesystem_worker.ps1");
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DISPATCHER: OnceLock<io::Result<queue::Sender<Request>>> = OnceLock::new();
#[cfg(test)]
static LAST_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(test)]
static LAST_PHASES: std::sync::Mutex<Option<Arc<ChildPhases>>> = std::sync::Mutex::new(None);
#[cfg(test)]
pub(super) mod eof_probe;

struct Request {
    id: String,
    operation: &'static str,
    bytes: Vec<u8>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<io::Result<Value>>,
    _ownership: RequestOwnership,
    #[cfg(test)]
    diagnostic: Arc<Diagnostic>,
}

enum RequestOwnership {
    None,
    Remove {
        _parent: crate::filesystem::DirectoryGuard,
    },
    Move {
        _owner: Arc<crate::filesystem::MoveOwnership>,
    },
}

impl Request {
    fn destructive(&self) -> bool {
        matches!(self.operation, "remove_file" | "move_empty_directory_new")
    }
}

#[cfg(test)]
static DIAGNOSTIC_HISTORY: std::sync::Mutex<
    std::collections::VecDeque<(String, &'static str, DiagnosticStage)>,
> = std::sync::Mutex::new(std::collections::VecDeque::new());

#[cfg(test)]
struct Diagnostic {
    id: String,
    operation: &'static str,
    entered: Instant,
    deadline: Instant,
    stages: std::sync::Mutex<std::collections::VecDeque<DiagnosticStage>>,
    child_stages: std::sync::Mutex<std::collections::VecDeque<DiagnosticStage>>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct DiagnosticStage {
    phase: &'static str,
    pid: u32,
    elapsed_ms: u128,
    remaining_ms: u128,
}

#[cfg(test)]
impl Diagnostic {
    fn new(id: String, operation: &'static str, entered: Instant, deadline: Instant) -> Self {
        Self {
            id,
            operation,
            entered,
            deadline,
            stages: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(12)),
            child_stages: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(16)),
        }
    }

    fn record(&self, phase: &'static str, pid: u32) {
        self.record_at(phase, pid, Instant::now());
    }

    fn record_at(&self, phase: &'static str, pid: u32, now: Instant) {
        let stage = DiagnosticStage {
            phase,
            pid,
            elapsed_ms: now.duration_since(self.entered).as_millis(),
            remaining_ms: self.deadline.saturating_duration_since(now).as_millis(),
        };
        let (target, capacity) = if phase.starts_with("child-") {
            (&self.child_stages, 16)
        } else {
            (&self.stages, 12)
        };
        if let Ok(mut stages) = target.try_lock() {
            if stages.len() == capacity {
                stages.pop_front();
            }
            stages.push_back(stage);
        }
        if let Ok(mut history) = DIAGNOSTIC_HISTORY.try_lock() {
            if history.len() == 48 {
                history.pop_front();
            }
            history.push_back((self.id.clone(), self.operation, stage));
        }
    }

    fn emit(&self) {
        if let Ok(history) = DIAGNOSTIC_HISTORY.try_lock() {
            for (id, operation, stage) in &*history {
                if id != &self.id {
                    Self::emit_stage(id, operation, stage);
                }
            }
        }
        if let Ok(stages) = self.stages.try_lock() {
            for stage in &*stages {
                Self::emit_stage(&self.id, self.operation, stage);
            }
        }
        if let Ok(stages) = self.child_stages.try_lock() {
            for stage in &*stages {
                Self::emit_stage(&self.id, self.operation, stage);
            }
        }
    }

    fn emit_stage(id: &str, operation: &str, stage: &DiagnosticStage) {
        eprintln!(
            "filesystem-stage request={id} operation={operation} phase={} pid={} elapsed_ms={} remaining_ms={}",
            stage.phase, stage.pid, stage.elapsed_ms, stage.remaining_ms
        );
    }
}

#[cfg(test)]
#[derive(Default)]
struct ChildPhases(
    std::sync::Mutex<std::collections::VecDeque<(&'static str, Instant)>>,
    AtomicBool,
);

#[cfg(test)]
impl ChildPhases {
    fn observe(&self, bytes: &[u8]) {
        let line = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        let phase = match line {
            b"locron-fs-phase:source-entry" => "child-source-entry",
            b"locron-fs-phase:encoding-ready" => "child-encoding-ready",
            b"locron-fs-phase:policy-confirmed" => {
                self.1.store(true, Ordering::Release);
                "child-policy-confirmed"
            }
            b"locron-fs-phase:utility-import-start" => "child-utility-import-start",
            b"locron-fs-phase:utility-import-ready" => "child-utility-import-ready",
            b"locron-fs-phase:input-line" => "child-input-line",
            b"locron-fs-phase:json-parsed" => "child-json-parsed",
            b"locron-fs-phase:sid-resolved" => "child-sid-resolved",
            b"locron-fs-phase:reply-serialized" => "child-reply-serialized",
            b"locron-fs-phase:reply-flushed" => "child-reply-flushed",
            _ => return,
        };
        if let Ok(mut phases) = self.0.try_lock() {
            if phases.len() == 16 {
                phases.pop_front();
            }
            phases.push_back((phase, Instant::now()));
        }
    }

    fn attach(&self, diagnostic: &Diagnostic, pid: u32) {
        if let Ok(phases) = self.0.try_lock() {
            for (phase, received) in &*phases {
                if *received >= diagnostic.entered {
                    diagnostic.record_at(phase, pid, *received);
                }
            }
        }
    }
}

/// Instrument only fixed source in test builds; caller data never selects executable text.
#[cfg(test)]
fn instrumented_source() -> String {
    instrumented_source_with_policy(super::loader_tests::restricted_child())
}

#[cfg(test)]
fn instrumented_source_with_policy(restricted: bool) -> String {
    let mut source = format!("{}\n{}", phase_token("source-entry"), worker_source());
    let import = STOCK_JSON_BOOTSTRAP;
    assert_eq!(source.matches(import).count(), 1);
    source = source.replace(
        import,
        &format!(
            "{}{}\n{import}",
            if restricted {
                format!(
                    "{}\n{}\n",
                    super::loader_tests::POLICY_ASSERTION,
                    phase_token("policy-confirmed")
                )
            } else {
                String::new()
            },
            phase_token("utility-import-start")
        ),
    );
    for (anchor, phase) in [
        (
            "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)",
            "encoding-ready",
        ),
        (import, "utility-import-ready"),
        (
            "while ($null -ne ($line = [Console]::In.ReadLine())) {",
            "input-line",
        ),
        (r"$request = $line | & $locronFromJson", "json-parsed"),
        (
            "$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User",
            "sid-resolved",
        ),
        ("[Console]::Out.Flush()", "reply-flushed"),
    ] {
        assert_eq!(source.matches(anchor).count(), 1);
        source = source.replace(anchor, &format!("{anchor}\n{}", phase_token(phase)));
    }
    let serialize = r"(@{version=1; id=$request.id; pid=$PID; result=$result} | & $locronToJson -Compress -Depth 6)";
    let anchor = format!("[Console]::Out.WriteLine({serialize})");
    assert_eq!(source.matches(&anchor).count(), 1);
    source.replace(
        &anchor,
        &format!(
            "$reply = {serialize}\n{}\n[Console]::Out.WriteLine($reply)",
            phase_token("reply-serialized")
        ),
    )
}

#[cfg(test)]
pub(super) fn policy_observation() -> (u32, bool) {
    let pid = LAST_PID.load(Ordering::Acquire);
    let confirmed = LAST_PHASES
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|phases| phases.1.load(Ordering::Acquire));
    (pid, confirmed)
}

#[cfg(test)]
pub(super) fn observed_pid() -> u32 {
    LAST_PID.load(Ordering::Acquire)
}

fn worker_source() -> String {
    SOURCE.replace("# LOCRON_STOCK_JSON_BOOTSTRAP", STOCK_JSON_BOOTSTRAP)
}

#[cfg(test)]
fn phase_token(phase: &str) -> String {
    format!("[Console]::Error.WriteLine('locron-fs-phase:{phase}'); [Console]::Error.Flush()")
}

#[cfg(test)]
async fn capture_diagnostic_stderr(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    phases: Arc<ChildPhases>,
) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    let mut scanned = 0;
    let mut buffer = [0_u8; 4096];
    loop {
        let room = usize::try_from(OUTPUT_LIMIT + 1).expect("small capture limit") - bytes.len();
        let capacity = room.min(buffer.len());
        let count = stream.read(&mut buffer[..capacity]).await?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > OUTPUT_LIMIT as usize {
            return Err(io::Error::other(
                "Windows adapter exceeded its output limit",
            ));
        }
        while let Some(newline) = bytes[scanned..].iter().position(|byte| *byte == b'\n') {
            phases.observe(&bytes[scanned..scanned + newline]);
            scanned += newline + 1;
        }
    }
}

/// The public caller supplies no source; selectors are a private fixed allowlist.
pub(super) fn request(
    operation: &'static str,
    path: Option<&str>,
    deadline: Instant,
) -> io::Result<Value> {
    remaining(deadline)?;
    if !matches!(operation, "sid" | "create_directory" | "create_file") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown filesystem selector",
        ));
    }
    request_owned(operation, &json!(path), RequestOwnership::None, deadline)
}

pub(super) fn remove_file(
    path: &str,
    parent: crate::filesystem::DirectoryGuard,
    deadline: Instant,
) -> io::Result<()> {
    let reply = request_owned(
        "remove_file",
        &json!(path),
        RequestOwnership::Remove { _parent: parent },
        deadline,
    )
    .map_err(|error| owned_exchange_error(&error))?;
    finish_mutation(&reply, deadline)
}

pub(super) fn move_empty_directory_new(
    owner: Arc<crate::filesystem::MoveOwnership>,
    deadline: Instant,
) -> io::Result<()> {
    let path = json!({
        "source":super::adapter_path(owner.source())?,
        "destination":super::adapter_path(owner.destination())?,
    });
    let reply = request_owned(
        "move_empty_directory_new",
        &path,
        RequestOwnership::Move { _owner: owner },
        deadline,
    )
    .map_err(|error| owned_exchange_error(&error))?;
    finish_mutation(&reply, deadline)
}

fn owned_exchange_error(error: &io::Error) -> io::Error {
    // Only a validated native-refusal frame may supply an OS code to the caller.
    io::Error::new(error.kind(), "owned filesystem exchange failed")
}

fn request_owned(
    operation: &'static str,
    path: &Value,
    ownership: RequestOwnership,
    deadline: Instant,
) -> io::Result<Value> {
    #[cfg(test)]
    let entered = Instant::now();
    remaining(deadline)?;
    if !matches!(
        (operation, &ownership),
        (
            "sid" | "create_directory" | "create_file",
            RequestOwnership::None
        ) | ("remove_file", RequestOwnership::Remove { .. })
            | ("move_empty_directory_new", RequestOwnership::Move { .. })
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "filesystem request ownership mismatch",
        ));
    }
    let id = NEXT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| io::Error::other("filesystem request identity exhausted"))?
        .to_string();
    let mut bytes = serde_json::to_vec(&json!({
        "version": 1, "id": id, "operation": operation, "path": path,
    }))
    .map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows adapter input is too large",
        ));
    }
    let sender = DISPATCHER
        .get_or_init(start_dispatcher)
        .as_ref()
        .map_err(copy_error)?;
    let (reply, receiver) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    #[cfg(test)]
    let diagnostic = Arc::new(Diagnostic::new(id.clone(), operation, entered, deadline));
    #[cfg(test)]
    diagnostic.record("entry", 0);
    let request = Request {
        id,
        operation,
        bytes,
        deadline,
        cancelled: Arc::clone(&cancelled),
        reply,
        _ownership: ownership,
        #[cfg(test)]
        diagnostic: Arc::clone(&diagnostic),
    };
    let result = enqueue(sender, request).and_then(|()| {
        #[cfg(test)]
        diagnostic.record("queue-admitted", 0);
        wait_for_reply(&receiver, &cancelled, deadline)
    });
    #[cfg(test)]
    if result.is_err() {
        diagnostic.record("caller-error", 0);
        diagnostic.emit();
    }
    result
}

fn wait_for_reply(
    receiver: &mpsc::Receiver<io::Result<Value>>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> io::Result<Value> {
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => refuse_late_success(result, deadline),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(io::Error::other("filesystem worker stopped"))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::Release);
            // The operation already expired. Wait only for its separately bounded owned cleanup.
            let result = receiver.recv_timeout(CLEANUP).unwrap_or_else(|_| {
                Err(io::Error::other(
                    "filesystem worker cleanup remains unconfirmed",
                ))
            });
            refuse_late_success(result, deadline)
        }
    }
}

fn refuse_late_success(result: io::Result<Value>, deadline: Instant) -> io::Result<Value> {
    if result.is_ok() {
        remaining(deadline)?;
    }
    result
}

fn enqueue(sender: &queue::Sender<Request>, mut request: Request) -> io::Result<()> {
    loop {
        remaining(request.deadline)?;
        match sender.try_send(request) {
            Ok(()) => return Ok(()),
            Err(queue::error::TrySendError::Full(returned)) => {
                request = returned;
                std::thread::sleep(remaining(request.deadline)?.min(Duration::from_millis(1)));
            }
            Err(queue::error::TrySendError::Closed(_)) => {
                return Err(io::Error::other("filesystem worker stopped"));
            }
        }
    }
}

fn start_dispatcher() -> io::Result<queue::Sender<Request>> {
    let (sender, receiver) = queue::channel(16);
    std::thread::Builder::new()
        .name("locron-filesystem-owner".to_owned())
        .spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(own_worker(receiver)),
                Err(error) => {
                    let mut receiver = receiver;
                    while let Some(request) = receiver.blocking_recv() {
                        let _ = request.reply.send(Err(copy_error(&error)));
                    }
                }
            }
        })?;
    Ok(sender)
}

fn copy_error(error: &io::Error) -> io::Error {
    if let Some(code) = error.raw_os_error() {
        io::Error::from_raw_os_error(code)
    } else {
        io::Error::new(error.kind(), error.to_string())
    }
}

async fn own_worker(mut receiver: queue::Receiver<Request>) {
    let mut worker: Option<Worker> = None;
    let mut quarantine = false;
    let mut quarantined_spawn = None;
    let mut quarantined_request = None;
    loop {
        let request = if worker.is_some() && !quarantine {
            if let Ok(request) = tokio::time::timeout(IDLE, receiver.recv()).await {
                request
            } else {
                if let Some(owned) = worker.as_mut() {
                    if owned.cleanup().await.is_ok() {
                        worker = None;
                    } else {
                        quarantine = true;
                    }
                }
                continue;
            }
        } else {
            receiver.recv().await
        };
        let Some(request) = request else { break };
        #[cfg(test)]
        request.diagnostic.record("dispatch-received", 0);
        if quarantine || quarantined_spawn.is_some() {
            #[cfg(test)]
            request.diagnostic.record("quarantined", 0);
            let _ = request.reply.send(Err(io::Error::other(
                "filesystem worker termination remains unconfirmed",
            )));
            continue;
        }
        if let Err(error) = remaining(request.deadline) {
            #[cfg(test)]
            request
                .diagnostic
                .record("deadline-expired-before-spawn", 0);
            let _ = request.reply.send(Err(error));
            continue;
        }
        if worker.is_none() {
            #[cfg(test)]
            request.diagnostic.record("spawn-start", 0);
            match Worker::spawn(request.deadline) {
                Ok(owned) => {
                    #[cfg(test)]
                    LAST_PID.store(owned.pid, Ordering::Release);
                    #[cfg(test)]
                    if let Ok(mut phases) = LAST_PHASES.lock() {
                        *phases = Some(Arc::clone(&owned.phases));
                    }
                    #[cfg(test)]
                    request.diagnostic.record("spawn-complete", owned.pid);
                    worker = Some(owned);
                }
                Err(failure) => {
                    let (ownership, error) = match failure {
                        FailedSpawn::NotStarted(error) => (None, error),
                        FailedSpawn::Retained { error, ownership } => (Some(ownership), error),
                    };
                    #[cfg(test)]
                    request.diagnostic.record("spawn-refused", 0);
                    quarantine = ownership.is_some();
                    quarantined_spawn = ownership;
                    fail_queued(&mut receiver, &error);
                    let _ = request.reply.send(Err(error));
                    if quarantine {
                        quarantined_request = Some(request);
                    }
                    continue;
                }
            }
        }
        let result = worker
            .as_mut()
            .expect("owned worker")
            .exchange(&request)
            .await;
        if let Err(error) = &result {
            let owned = worker.as_mut().expect("owned worker");
            #[cfg(test)]
            request.diagnostic.record("exchange-refused", owned.pid);
            #[cfg(test)]
            request.diagnostic.record("cleanup-start", owned.pid);
            if let Err(cleanup) = owned.cleanup().await {
                #[cfg(test)]
                owned.phases.attach(&request.diagnostic, owned.pid);
                #[cfg(test)]
                request.diagnostic.record("cleanup-unconfirmed", owned.pid);
                quarantine = true;
                fail_queued(&mut receiver, &cleanup);
                let _ = request.reply.send(Err(cleanup));
                quarantined_request = Some(request);
                #[cfg(test)]
                if let Some(probe) = &owned.mutation_probe {
                    probe.event("owner-quarantined");
                }
                continue;
            }
            #[cfg(test)]
            owned.phases.attach(&request.diagnostic, owned.pid);
            #[cfg(test)]
            request.diagnostic.record("cleanup-confirmed", owned.pid);
            worker = None;
            fail_queued(&mut receiver, error);
        }
        if request.destructive() {
            // The command has returned (or cleanup was confirmed). Release its
            // namespace pins before publishing the result; a move caller still
            // retains its separate Arc through destination admission.
            let reply = request.reply.clone();
            drop(request);
            let _ = reply.send(result);
        } else {
            let _ = request.reply.send(result);
        }
    }
    if !quarantine
        && let Some(owned) = worker.as_mut()
        && owned.cleanup().await.is_err()
    {
        quarantine = true;
    }
    if quarantine || quarantined_spawn.is_some() || quarantined_request.is_some() {
        #[cfg(test)]
        if let Some(probe) = worker.as_ref().and_then(|owned| owned.eof_probe.as_ref()) {
            probe.quarantined();
        }
        // Even a private/fixture channel EOF cannot release uncertain native ownership.
        // Active request ancestry and child/Job/stock handles survive until process exit.
        loop {
            std::thread::park();
        }
    }
}

fn fail_queued(receiver: &mut queue::Receiver<Request>, error: &io::Error) {
    // Drain the bounded queue snapshot; fresh arrivals cannot indefinitely delay cleanup delivery.
    for _ in 0..16 {
        let Ok(request) = receiver.try_recv() else {
            break;
        };
        #[cfg(test)]
        request.diagnostic.record("queue-invalidated", 0);
        let _ = request.reply.send(Err(copy_error(error)));
    }
}

enum FailedSpawn {
    NotStarted(io::Error),
    Retained {
        error: io::Error,
        ownership: Box<SpawnOwnership>,
    },
}

enum SpawnOwnership {
    LostWait {
        _failure: SpawnFailure,
        _guard: StockAdapterGuard,
    },
    Child {
        _child: OwnedChild,
        _guard: StockAdapterGuard,
    },
}

struct Worker {
    child: OwnedChild,
    _stock: StockAdapterGuard,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: JoinHandle<io::Result<Vec<u8>>>,
    pid: u32,
    #[cfg(test)]
    phases: Arc<ChildPhases>,
    #[cfg(test)]
    eof_probe: Option<eof_probe::Probe>,
    #[cfg(test)]
    mutation_probe: Option<MutationProbe>,
}

impl Worker {
    fn spawn(deadline: Instant) -> Result<Self, FailedSpawn> {
        let guard = StockAdapterGuard::acquire_until(StockModuleSet::Utility, deadline)
            .map_err(FailedSpawn::NotStarted)?;
        #[cfg(not(test))]
        let source = worker_source();
        #[cfg(test)]
        let source = instrumented_source();
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            source
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        remaining(deadline).map_err(FailedSpawn::NotStarted)?;
        let mut command = Command::new(guard.executable());
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_stock_command(&mut command, &guard);
        let mut child = match OwnedChild::spawn(command) {
            Ok(child) => child,
            Err(SpawnFailure::NotStarted(error)) => return Err(FailedSpawn::NotStarted(error)),
            Err(failure @ SpawnFailure::Unconfirmed { .. }) => {
                let SpawnFailure::Unconfirmed { ref error, .. } = failure else {
                    unreachable!()
                };
                return Err(FailedSpawn::Retained {
                    error: copy_error(error),
                    ownership: Box::new(SpawnOwnership::LostWait {
                        _failure: failure,
                        _guard: guard,
                    }),
                });
            }
        };
        let (Some(pid), Some(stdin), Some(stdout), Some(stderr)) = (
            child.id(),
            child.take_stdin(),
            child.take_stdout(),
            child.take_stderr(),
        ) else {
            let _ = child.start_kill();
            return Err(FailedSpawn::Retained {
                error: io::Error::other("filesystem child capture failed"),
                ownership: Box::new(SpawnOwnership::Child {
                    _child: child,
                    _guard: guard,
                }),
            });
        };
        #[cfg(test)]
        let phases = Arc::new(ChildPhases::default());
        #[cfg(test)]
        let errors = tokio::spawn(capture_diagnostic_stderr(stderr, Arc::clone(&phases)));
        #[cfg(not(test))]
        let errors = tokio::spawn(capture_output(stderr));
        Ok(Self {
            child,
            _stock: guard,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: errors,
            pid,
            #[cfg(test)]
            phases,
            #[cfg(test)]
            eof_probe: eof_probe::take(),
            #[cfg(test)]
            mutation_probe: None,
        })
    }

    async fn exchange(&mut self, request: &Request) -> io::Result<Value> {
        remaining(request.deadline)?;
        #[cfg(test)]
        if request.destructive() && self.mutation_probe.is_none() {
            self.mutation_probe = NEXT_MUTATION_PROBE.lock().unwrap().take();
        }
        #[cfg(test)]
        let mutation_probe = self.mutation_probe.as_ref();
        let deadline = tokio::time::Instant::from_std(request.deadline);
        let Self {
            stdin,
            stdout,
            stderr,
            pid,
            ..
        } = self;
        let operation = async {
            tokio::select! {
                result = async {
                    adapter_poll_until(request.deadline, stdin.write_all(&request.bytes)).await?;
                    adapter_poll_until(request.deadline, stdin.flush()).await?;
                    #[cfg(test)]
                    request.diagnostic.record("input-flushed", *pid);
                    let bytes = read_frame(stdout).await?;
                    #[cfg(test)]
                    request.diagnostic.record("reply-received", *pid);
                    let result = validate_frame(&bytes, request, *pid)?;
                    #[cfg(test)]
                    if let Some(probe) = mutation_probe
                        && request.operation == "remove_file" {
                        if !matches!(MutationOutcome::parse(&result)?, MutationOutcome::Completed) {
                            return Err(io::Error::other("owned mutation fixture did not complete"));
                        }
                        probe.event("native-removal-completed");
                        return Err(io::Error::other("owned fixture withheld mutation reply"));
                    }
                    Ok(result)
                } => result,
                result = stderr => {
                    let bytes = result.map_err(io::Error::other)??;
                    if request.destructive() {
                        Err(io::Error::other("owned filesystem worker stderr closed"))
                    } else {
                        Err(io::Error::other(format!("filesystem worker stderr closed: {}", String::from_utf8_lossy(&bytes).trim())))
                    }
                }
                () = cancelled(&request.cancelled) => Err(io::Error::new(io::ErrorKind::TimedOut, "filesystem caller deadline elapsed")),
            }
        };
        let result = tokio::time::timeout_at(deadline, operation)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "stock Windows filesystem worker timed out",
                )
            })?;
        remaining(request.deadline)?;
        result
    }

    async fn cleanup(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + CLEANUP;
        #[cfg(test)]
        if let Some(probe) = &mut self.mutation_probe {
            probe.start();
        }
        #[cfg(test)]
        if let Some(probe) = self.eof_probe.as_mut() {
            probe.start(deadline);
        }
        self.stderr.abort();
        remaining(deadline)?;
        let _ = self.child.start_kill();
        remaining(deadline)?;
        loop {
            remaining(deadline)?;
            let reaped = self.child.try_wait().is_ok_and(|status| status.is_some());
            remaining(deadline)?;
            let empty = self.child.tree_empty().is_ok_and(|empty| empty);
            remaining(deadline)?;
            if reaped && empty {
                if !self.stderr.is_finished()
                    && tokio::time::timeout_at(
                        tokio::time::Instant::from_std(deadline),
                        &mut self.stderr,
                    )
                    .await
                    .is_err()
                {
                    return Err(io::Error::other(
                        "filesystem pipe cleanup remains unconfirmed",
                    ));
                }
                remaining(deadline)?;
                #[cfg(test)]
                if let Some(probe) = &mut self.mutation_probe {
                    probe.confirm_until(deadline).await?;
                }
                #[cfg(test)]
                if let Some(probe) = self.eof_probe.as_mut() {
                    probe.confirm_tree_and_read_until(deadline).await?;
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other(
                    "filesystem worker termination remains unconfirmed",
                ));
            }
            tokio::time::sleep(remaining(deadline)?.min(Duration::from_millis(10))).await;
        }
    }
}

async fn cancelled(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn read_frame(reader: &mut (impl tokio::io::AsyncBufRead + Unpin)) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "filesystem reply ended before its frame",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |position| position + 1);
        if bytes.len() + count > OUTPUT_LIMIT as usize {
            return Err(io::Error::other(
                "Windows adapter exceeded its output limit",
            ));
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(bytes);
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    version: u32,
    id: String,
    pid: u32,
    result: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Creation {
    created: bool,
    #[serde(default)]
    win32_error: Option<i32>,
}

fn validate_frame(bytes: &[u8], request: &Request, pid: u32) -> io::Result<Value> {
    let reply: Reply = serde_json::from_slice(bytes).map_err(|error| {
        if request.destructive() {
            invalid_mutation()
        } else {
            io::Error::other(error)
        }
    })?;
    if reply.version != 1 || reply.id != request.id || reply.pid != pid {
        return Err(io::Error::other("filesystem reply identity mismatch"));
    }
    if request.operation == "sid" {
        let sid = reply
            .result
            .as_str()
            .ok_or_else(|| io::Error::other("filesystem SID reply is invalid"))?;
        if !sid.starts_with("S-1-")
            || !sid
                .split('-')
                .skip(1)
                .all(|part| part.parse::<u64>().is_ok())
        {
            return Err(io::Error::other("filesystem SID reply is invalid"));
        }
    } else if request.destructive() {
        MutationOutcome::parse(&reply.result)?;
    } else {
        let created: Creation =
            serde_json::from_value(reply.result.clone()).map_err(io::Error::other)?;
        if (created.created && created.win32_error.is_some())
            || (!created.created
                && (request.operation != "create_file"
                    || !matches!(created.win32_error, Some(80 | 183))))
        {
            return Err(io::Error::other("filesystem creation reply is invalid"));
        }
    }
    Ok(reply.result)
}

enum MutationOutcome {
    Completed,
    NativeRefusal(i32),
    AdapterRefusal,
}

fn invalid_mutation() -> io::Error {
    io::Error::other("invalid owned filesystem result")
}

impl MutationOutcome {
    fn parse(value: &Value) -> io::Result<Self> {
        let object = value.as_object().ok_or_else(invalid_mutation)?;
        if object.len() != 3
            || !["outcome", "hresult", "win32_error"]
                .iter()
                .all(|field| object.contains_key(*field))
        {
            return Err(invalid_mutation());
        }
        match object["outcome"].as_str() {
            Some("completed") if object["hresult"].is_null() && object["win32_error"].is_null() => {
                Ok(Self::Completed)
            }
            Some("adapter_refusal")
                if object["hresult"].is_null() && object["win32_error"].is_null() =>
            {
                Ok(Self::AdapterRefusal)
            }
            Some("native_refusal") => {
                let hresult = object["hresult"]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(invalid_mutation)?;
                let code = object["win32_error"]
                    .as_u64()
                    .and_then(|value| u16::try_from(value).ok())
                    .filter(|value| *value != 0)
                    .ok_or_else(invalid_mutation)?;
                if u32::from_ne_bytes(hresult.to_ne_bytes()) != 0x8007_0000 | u32::from(code) {
                    return Err(invalid_mutation());
                }
                Ok(Self::NativeRefusal(i32::from(code)))
            }
            _ => Err(invalid_mutation()),
        }
    }
}

fn finish_mutation(reply: &Value, deadline: Instant) -> io::Result<()> {
    let outcome = MutationOutcome::parse(reply)?;
    remaining(deadline)?;
    match outcome {
        MutationOutcome::Completed => Ok(()),
        MutationOutcome::NativeRefusal(code) => Err(io::Error::from_raw_os_error(code)),
        MutationOutcome::AdapterRefusal => {
            Err(io::Error::other("filesystem adapter refused operation"))
        }
    }
}

#[cfg(test)]
static NEXT_MUTATION_PROBE: std::sync::Mutex<Option<MutationProbe>> = std::sync::Mutex::new(None);

#[cfg(test)]
struct MutationProbe {
    reader: Option<io::PipeReader>,
    pending: Option<JoinHandle<io::Result<()>>>,
    events: mpsc::SyncSender<&'static str>,
}

#[cfg(test)]
impl MutationProbe {
    fn event(&self, phase: &'static str) {
        self.events.try_send(phase).unwrap();
    }

    fn start(&mut self) {
        use std::io::Read as _;
        let Some(mut reader) = self.reader.take() else {
            return;
        };
        let events = self.events.clone();
        self.pending = Some(tokio::task::spawn_blocking(move || {
            events.try_send("native-cleanup-entered").unwrap();
            reader.read_exact(&mut [0_u8; 1])?;
            events.try_send("native-cleanup-completed").unwrap();
            Ok(())
        }));
    }

    async fn confirm_until(&mut self, deadline: Instant) -> io::Result<()> {
        self.event("tree-confirmed");
        let pending = self.pending.as_mut().unwrap();
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "owned cleanup read remains pending",
                )
            })?
            .map_err(io::Error::other)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command as StdCommand, Stdio};

    #[test]
    fn owned_mutation_frames_preserve_only_consistent_native_codes() {
        let request = fixture_request("remove_file");
        for code in [2_u16, 3, 5, 15, 32, 33, 183, u16::MAX] {
            let hresult = i32::from_ne_bytes((0x8007_0000 | u32::from(code)).to_ne_bytes());
            let result = json!({"outcome":"native_refusal","hresult":hresult,"win32_error":code});
            let bytes = serde_json::to_vec(&json!({"version":1,"id":"1","pid":42,"result":result}))
                .unwrap();
            let reply = validate_frame(&bytes, &request, 42).unwrap();
            assert_eq!(
                finish_mutation(&reply, super::super::filesystem_deadline())
                    .unwrap_err()
                    .raw_os_error(),
                Some(i32::from(code))
            );
        }
        for result in [
            json!({"outcome":"completed","hresult":null,"win32_error":null}),
            json!({"outcome":"adapter_refusal","hresult":null,"win32_error":null}),
        ] {
            assert!(MutationOutcome::parse(&result).is_ok());
        }
        let secret = "private-path-SID-SQL-body-must-never-render";
        for result in [
            json!({"outcome":"completed","win32_error":null}),
            json!({"outcome":"completed","hresult":null,"win32_error":null,"secret":secret}),
            json!({"outcome":secret,"hresult":null,"win32_error":null}),
            json!({"outcome":"completed","hresult":-2_147_024_891_i32,"win32_error":5}),
            json!({"outcome":"native_refusal","hresult":-2_147_024_891_i32,"win32_error":32}),
            json!({"outcome":"native_refusal","hresult":null,"win32_error":5}),
            json!({"outcome":"native_refusal","hresult":-2_147_024_891_i32,"win32_error":0}),
            json!({"outcome":"native_refusal","hresult":-2_147_024_891_i32,"win32_error":65_536}),
            json!({"outcome":"native_refusal","hresult":-2_147_024_891_i32,"win32_error":5.0}),
            json!({"outcome":"native_refusal","hresult":2_147_942_405_u64,"win32_error":5}),
            json!({"outcome":"native_refusal","hresult":-2_146_233_079_i32,"win32_error":5}),
        ] {
            let error = MutationOutcome::parse(&result).err().unwrap();
            assert_eq!(error.to_string(), "invalid owned filesystem result");
            assert!(!error.to_string().contains(secret));
        }
        let mut bad = json!({"version":1,"id":"1","pid":42,"result":null});
        bad[secret] = json!(secret);
        let error = validate_frame(&serde_json::to_vec(&bad).unwrap(), &request, 42).unwrap_err();
        assert_eq!(error.to_string(), "invalid owned filesystem result");
        let error = owned_exchange_error(&io::Error::from_raw_os_error(5));
        assert_eq!(error.raw_os_error(), None);
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        for operation in ["remove_file", "move_empty_directory_new"] {
            let error =
                super::request(operation, None, super::super::filesystem_deadline()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }

    #[test]
    fn late_validated_mutation_delivery_never_supplies_success_or_native_refusal() {
        for result in [
            json!({"outcome":"completed","hresult":null,"win32_error":null}),
            json!({"outcome":"native_refusal","hresult":-2_147_024_891_i32,"win32_error":5}),
        ] {
            let request = fixture_request("remove_file");
            let bytes = serde_json::to_vec(&json!({"version":1,"id":"1","pid":42,"result":result}))
                .unwrap();
            let validated = validate_frame(&bytes, &request, 42).unwrap();
            let (reply, receiver) = mpsc::sync_channel(1);
            let cancelled = Arc::new(AtomicBool::new(false));
            let observed = Arc::clone(&cancelled);
            let deadline = Instant::now() + Duration::from_millis(40);
            let worker = std::thread::spawn(move || {
                while !observed.load(Ordering::Acquire) {
                    assert!(Instant::now() < deadline + CLEANUP);
                    std::thread::yield_now();
                }
                reply.send(Ok(validated)).unwrap();
            });
            let error = wait_for_reply(&receiver, &cancelled, deadline).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert_eq!(error.raw_os_error(), None);
            if worker.is_finished() {
                worker.join().unwrap();
            }
            let expired = deadline.checked_sub(CLEANUP).unwrap();
            let error = finish_mutation(&result, expired).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert_eq!(error.raw_os_error(), None);
        }
    }

    #[test]
    fn strict_removal_preserves_native_refusals_and_warm_worker() {
        run_isolated_fixture("strict-remove", "fixed-worker-strict-remove-confirmed");
    }

    #[test]
    fn uncertain_active_removal_retains_sole_ancestry_until_process_disposal() {
        let mut temporary = tempfile::tempdir().unwrap();
        temporary.disable_cleanup(true);
        let positive = temporary.path().join("unguarded");
        let renamed = temporary.path().join("positive");
        std::fs::create_dir(&positive).unwrap();
        std::fs::rename(&positive, &renamed).unwrap();
        std::fs::remove_dir(&renamed).unwrap();
        let stdout = temporary.path().join("stdout");
        let stderr = temporary.path().join("stderr");
        let root = temporary.path().join("private");
        let destination = temporary.path().join("moved");
        let ready = temporary.path().join("strict-quarantine-ready");
        let mut child = FixtureChild(
            StdCommand::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "windows::filesystem_worker::tests::worker_fixture_child",
                    "--nocapture",
                ])
                .env("LOCRON_FIXED_WORKER_FIXTURE", "strict-remove-quarantine")
                .current_dir(temporary.path())
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(std::fs::File::create(&stdout).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            remaining(deadline).unwrap();
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "owned removal helper exited before quarantine"
            );
            if ready.is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read(&ready).unwrap(), b"active-ancestry-retained");
        assert_eq!(
            std::fs::rename(&root, &destination)
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        assert!(!destination.exists());
        child.0.kill().unwrap();
        loop {
            remaining(deadline).unwrap();
            if child.0.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::rename(&root, &destination).unwrap();
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 0);
        std::fs::remove_dir(&destination).unwrap();
        for path in [&stdout, &stderr, &ready] {
            std::fs::remove_file(path).unwrap();
        }
        temporary.close().unwrap();
    }

    #[test]
    fn fixed_policy_assertion_precedes_binary_binding_without_a_request_selector() {
        let source = instrumented_source_with_policy(true);
        let policy = source
            .find(super::super::loader_tests::POLICY_ASSERTION)
            .unwrap();
        assert!(policy < source.find(STOCK_JSON_BOOTSTRAP).unwrap());
        assert_eq!(source.matches(&phase_token("policy-confirmed")).count(), 1);
        assert!(!worker_source().contains("stock child effective policy"));
        assert!(!source.contains("Set-ExecutionPolicy"));
    }

    #[test]
    fn observed_policy_receipt_survives_ring_overflow_and_never_crosses_children() {
        let phases = ChildPhases::default();
        for unknown in [
            b"locron-fs-phase:policy-confirmed suffix".as_slice(),
            b"locron-fs-phase:policy-confirmed-other".as_slice(),
            b"Restricted".as_slice(),
        ] {
            phases.observe(unknown);
        }
        assert!(!phases.1.load(Ordering::Acquire));
        phases.observe(b"locron-fs-phase:policy-confirmed\r");
        for _ in 0..4 {
            for phase in [
                b"locron-fs-phase:input-line".as_slice(),
                b"locron-fs-phase:json-parsed".as_slice(),
                b"locron-fs-phase:sid-resolved".as_slice(),
                b"locron-fs-phase:reply-serialized".as_slice(),
                b"locron-fs-phase:reply-flushed".as_slice(),
            ] {
                phases.observe(phase);
            }
        }
        assert!(phases.1.load(Ordering::Acquire));
        let recent = phases.0.lock().unwrap();
        assert_eq!(recent.len(), 16);
        assert!(
            recent
                .iter()
                .all(|(phase, _)| *phase != "child-policy-confirmed")
        );
        let next = ChildPhases::default();
        next.observe(b"locron-fs-phase:sid-resolved");
        assert!(!next.1.load(Ordering::Acquire));
        assert!(phases.1.load(Ordering::Acquire));
    }

    #[test]
    fn fixed_phase_source_and_observations_are_bounded_and_do_not_render_inputs() {
        let source = instrumented_source();
        for phase in [
            "source-entry",
            "encoding-ready",
            "utility-import-start",
            "utility-import-ready",
            "input-line",
            "json-parsed",
            "sid-resolved",
            "reply-serialized",
            "reply-flushed",
        ] {
            assert_eq!(source.matches(&phase_token(phase)).count(), 1);
        }
        let phases = ChildPhases::default();
        phases.observe(b"unrecognized private stderr");
        phases.observe(b"locron-fs-phase:sid-resolved private suffix");
        assert!(phases.0.lock().unwrap().is_empty());
        for _ in 0..32 {
            phases.observe(b"locron-fs-phase:sid-resolved\r");
        }
        assert_eq!(phases.0.lock().unwrap().len(), 16);
        let request = fixture_request("sid");
        let before_entry = request
            .diagnostic
            .entered
            .checked_sub(Duration::from_millis(1))
            .expect("fixture clock has an earlier instant");
        for (_, received) in phases.0.lock().unwrap().iter_mut() {
            *received = before_entry;
        }
        phases.attach(&request.diagnostic, 42);
        // Old phases precede this new request and cannot be attributed to its exchange.
        assert!(request.diagnostic.child_stages.lock().unwrap().is_empty());
        phases.observe(b"locron-fs-phase:reply-flushed");
        phases.attach(&request.diagnostic, 42);
        let observed = request.diagnostic.child_stages.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].phase, "child-reply-flushed");
        assert_eq!(observed[0].pid, 42);
    }

    #[tokio::test]
    async fn diagnostic_stderr_preserves_bytes_and_fails_at_limit_without_eof() {
        use tokio::io::AsyncWriteExt;
        let phases = Arc::new(ChildPhases::default());
        let input = b"locron-fs-phase:input-line\r\nunknown\nlocron-fs-phase:json-parsed\n";
        let bytes = capture_diagnostic_stderr(&input[..], Arc::clone(&phases))
            .await
            .unwrap();
        assert_eq!(bytes, input);
        {
            let observed = phases.0.lock().unwrap();
            assert_eq!(observed.len(), 2);
            assert_eq!(observed[0].0, "child-input-line");
            assert_eq!(observed[1].0, "child-json-parsed");
        }
        let limit = usize::try_from(OUTPUT_LIMIT + 1).unwrap();
        let (reader, mut writer) = tokio::io::duplex(limit);
        writer.write_all(&vec![b'x'; limit]).await.unwrap();
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            capture_diagnostic_stderr(reader, Arc::clone(&phases)),
        )
        .await
        .expect("limit must fail before the open writer reaches EOF")
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Windows adapter exceeded its output limit"
        );
        assert_eq!(phases.0.lock().unwrap().len(), 2);
        drop(writer);
    }

    fn fixture_request(operation: &'static str) -> Request {
        let (reply, _) = mpsc::sync_channel(1);
        let entered = Instant::now();
        let deadline = entered + Duration::from_secs(30);
        Request {
            id: "1".to_owned(),
            operation,
            bytes: Vec::new(),
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            reply,
            _ownership: RequestOwnership::None,
            diagnostic: Arc::new(Diagnostic::new(
                "1".to_owned(),
                operation,
                entered,
                deadline,
            )),
        }
    }

    #[test]
    fn reply_validation_refuses_wrong_id_version_pid_and_creation_shape() {
        let request = fixture_request("sid");
        for bytes in [
            br#"{"version":1,"id":"2","pid":42,"result":"S-1-5-18"}"#.as_slice(),
            br#"{"version":2,"id":"1","pid":42,"result":"S-1-5-18"}"#,
            br#"{"version":1,"id":"1","pid":41,"result":"S-1-5-18"}"#,
            br#"{"version":1,"id":"1","pid":42,"result":"environment username"}"#,
            br#"{"version":1,"id":"1","pid":42,"result":"S-1-5-18","source":"ignored"}"#,
        ] {
            assert!(validate_frame(bytes, &request, 42).is_err());
        }
        let directory = fixture_request("create_directory");
        assert!(
            validate_frame(
                br#"{"version":1,"id":"1","pid":42,"result":{"created":false,"win32_error":80}}"#,
                &directory,
                42
            )
            .is_err()
        );
        let file = fixture_request("create_file");
        assert!(
            validate_frame(
                br#"{"version":1,"id":"1","pid":42,"result":{"created":false,"win32_error":80}}"#,
                &file,
                42
            )
            .is_ok()
        );
        assert!(
            validate_frame(
                br#"{"version":1,"id":"1","pid":42,"result":{"created":true,"win32_error":80}}"#,
                &file,
                42
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn frame_cap_refuses_before_newline_and_eof() {
        let bytes = vec![b'x'; OUTPUT_LIMIT as usize + 1];
        let mut input = BufReader::new(bytes.as_slice());
        let error = read_frame(&mut input).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "Windows adapter exceeded its output limit"
        );
        let mut input = BufReader::new(&b"{partial"[..]);
        assert_eq!(
            read_frame(&mut input).await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn queued_expired_request_is_not_sent_and_failures_wake_every_pending_caller() {
        let (sender, mut receiver) = queue::channel(16);
        let mut expired = fixture_request("sid");
        expired.deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("fixture monotonic clock has an earlier instant");
        assert_eq!(
            enqueue(&sender, expired).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(receiver.try_recv().is_err());
        let mut replies = Vec::new();
        for _ in 0..16 {
            let (reply, response) = mpsc::sync_channel(1);
            let mut request = fixture_request("sid");
            request.reply = reply;
            sender.try_send(request).ok().unwrap();
            replies.push(response);
        }
        fail_queued(&mut receiver, &io::Error::other("bad fixed reply"));
        for response in replies {
            assert_eq!(
                response
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap()
                    .unwrap_err()
                    .to_string(),
                "bad fixed reply"
            );
        }
    }

    #[test]
    fn success_delivered_only_after_timeout_is_refused_during_cleanup_receive() {
        for success in [true, false] {
            let (reply, receiver) = mpsc::sync_channel(1);
            let cancelled = Arc::new(AtomicBool::new(false));
            let observed = Arc::clone(&cancelled);
            let deadline = Instant::now() + Duration::from_millis(40);
            let delivery = std::thread::spawn(move || {
                let last = deadline + CLEANUP;
                while !observed.load(Ordering::Acquire) {
                    assert!(Instant::now() < last, "fixture must observe caller timeout");
                    std::thread::sleep(Duration::from_millis(1));
                }
                let result = if success {
                    Ok(json!({"computed_before_delivery": true}))
                } else {
                    Err(io::Error::other("owned cleanup reported failure"))
                };
                reply.send(result).unwrap();
            });
            let error = wait_for_reply(&receiver, &cancelled, deadline).unwrap_err();
            assert!(cancelled.load(Ordering::Acquire));
            if success {
                assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            } else {
                assert_eq!(error.to_string(), "owned cleanup reported failure");
            }
            if delivery.is_finished() {
                delivery.join().unwrap();
            }
        }
    }

    struct FixtureChild(Child);

    impl Drop for FixtureChild {
        fn drop(&mut self) {
            if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            let _ = self.0.kill();
            let deadline = Instant::now() + CLEANUP;
            while Instant::now() < deadline {
                if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn bounded_text(path: &std::path::Path) -> String {
        let mut text = String::new();
        std::fs::File::open(path)
            .unwrap()
            .take(32 * 1024)
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    #[test]
    fn cold_fixed_worker_serves_concurrent_callers_without_repeated_bootstrap() {
        run_isolated_fixture("reuse", "fixed-worker-reuse-confirmed");
    }

    #[test]
    fn owned_sid_and_create_requests_report_ordered_child_phases() {
        run_isolated_fixture("phase-order", "fixed-worker-phase-order-confirmed");
    }

    #[test]
    fn native_process_sid_matches_the_isolated_fixed_worker() {
        run_isolated_fixture("sid-identity", "native-stock-sid-identity-confirmed");
    }

    fn run_isolated_fixture(mode: &str, confirmation: &str) {
        let temporary = tempfile::tempdir().unwrap();
        let hostile_modules = temporary.path().join("hostile modules");
        let forged_utility = hostile_modules.join("Microsoft.PowerShell.Utility");
        std::fs::create_dir_all(&forged_utility).unwrap();
        std::fs::write(
            forged_utility.join("Microsoft.PowerShell.Utility.psm1"),
            "throw 'filesystem worker loaded an inherited module'",
        )
        .unwrap();
        let stdout = temporary.path().join("stdout.txt");
        let stderr = temporary.path().join("stderr.txt");
        let mut child = FixtureChild(
            StdCommand::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "windows::filesystem_worker::tests::worker_fixture_child",
                    "--nocapture",
                ])
                .env("LOCRON_FIXED_WORKER_FIXTURE", mode)
                .env("PSModulePath", &hostile_modules)
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(std::fs::File::create(&stdout).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(90);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "isolated filesystem fixture timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "stdout={}\nstderr={}",
            bounded_text(&stdout),
            bounded_text(&stderr)
        );
        assert!(bounded_text(&stdout).contains(confirmation));
    }

    #[test]
    fn worker_fixture_child() {
        let Ok(mode) = std::env::var("LOCRON_FIXED_WORKER_FIXTURE") else {
            return;
        };
        assert!(DISPATCHER.get().is_none());
        if mode == "strict-remove" {
            strict_remove_fixture();
            return;
        }
        if mode == "strict-remove-quarantine" {
            strict_remove_quarantine_fixture();
            return;
        }
        if mode == "phase-order" {
            phase_order_fixture();
            return;
        }
        if mode == "sid-identity" {
            sid_identity_fixture();
            return;
        }
        assert_eq!(mode, "reuse");
        let barrier = std::sync::Barrier::new(9);
        let cold = Instant::now();
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        let result =
                            request("sid", None, Instant::now() + Duration::from_secs(30)).unwrap();
                        (result, LAST_PID.load(Ordering::Acquire))
                    })
                })
                .collect();
            barrier.wait();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert!(cold.elapsed() < Duration::from_secs(30));
        let (first, pid) = &results[0];
        assert_ne!(*pid, 0);
        assert!(results.iter().all(|result| result == &results[0]));
        let started = Instant::now();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..8 {
                handles.push(scope.spawn(|| {
                    request("sid", None, Instant::now() + Duration::from_secs(30)).unwrap()
                }));
            }
            for handle in handles {
                assert_eq!(&handle.join().unwrap(), first);
            }
        });
        assert_eq!(LAST_PID.load(Ordering::Acquire), *pid);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "warm requests repeated a cold bootstrap"
        );
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private 한글");
        let guard = crate::filesystem::DirectoryGuard::private(&root).unwrap();
        let path = guard.normalized_path().join("fresh.txt");
        drop(crate::filesystem::create_private_new(&path).unwrap());
        assert!(crate::filesystem::is_private(&path, false).unwrap());
        assert_eq!(LAST_PID.load(Ordering::Acquire), *pid);
        println!("fixed-worker-reuse-confirmed");
    }

    #[allow(clippy::permissions_set_readonly_false)]
    fn strict_remove_fixture() {
        use crate::filesystem::{create_private_new, file_identity, open_private};
        use std::io::Write as _;
        use std::os::windows::fs::MetadataExt as _;
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("private 한글").join("retained.txt");
        let mut file = create_private_new(&path).unwrap();
        file.write_all(b"private-native-refusal-data").unwrap();
        let identity = file_identity(&file).unwrap();
        drop(file);
        let pid = LAST_PID.load(Ordering::Acquire);
        assert_ne!(pid, 0);
        let attributes = std::fs::metadata(&path).unwrap().file_attributes();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let error = crate::filesystem::remove_private_file_strict(&path).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let mut reader = open_private(&path, std::fs::OpenOptions::new().read(true)).unwrap();
        (file_identity(&reader).unwrap() == identity)
            .then_some(())
            .expect("strict refusal must retain the original file identity");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        (bytes == b"private-native-refusal-data")
            .then_some(())
            .expect("strict refusal must retain the original private bytes");
        assert_eq!(reader.metadata().unwrap().file_attributes(), attributes | 1);
        drop(reader);
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().file_attributes(),
            attributes
        );
        crate::filesystem::remove_private_file_strict(&path).unwrap();
        assert!(!path.exists());
        crate::filesystem::remove_private_file_strict(&path).unwrap();
        let absent = path.parent().unwrap().join("absent").join("none");
        crate::filesystem::remove_private_file_strict(&absent).unwrap();
        assert!(!absent.parent().unwrap().exists());
        drop(create_private_new(&path).unwrap());
        let holder = open_private(&path, std::fs::OpenOptions::new().read(true)).unwrap();
        let identity = file_identity(&holder).unwrap();
        let error = crate::filesystem::remove_private_file_strict(&path).unwrap_err();
        assert!(matches!(error.raw_os_error(), Some(32 | 33)));
        assert_eq!(
            error.kind(),
            io::Error::from_raw_os_error(error.raw_os_error().unwrap()).kind()
        );
        (file_identity(&holder).unwrap() == identity)
            .then_some(())
            .expect("sharing refusal must retain the original file identity");
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        drop(holder);
        crate::filesystem::remove_private_file_strict(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        let file = create_private_new(&path).unwrap();
        let (leaf, parent) = file.into_parts();
        drop(leaf);
        let next = NEXT_ID.load(Ordering::Relaxed);
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        let error = remove_file(path.to_str().unwrap(), parent, expired).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.raw_os_error(), None);
        assert_eq!(NEXT_ID.load(Ordering::Relaxed), next);
        assert!(path.is_file());
        crate::filesystem::remove_private_file_strict(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
        println!("fixed-worker-strict-remove-confirmed");
    }

    fn strict_remove_quarantine_fixture() {
        use std::io::Write as _;
        let path = std::env::current_dir()
            .unwrap()
            .join("private")
            .join("leaf");
        let file = crate::filesystem::create_private_new(&path).unwrap();
        let (leaf, parent) = file.into_parts();
        drop(leaf);
        let (reader, mut writer) = io::pipe().unwrap();
        let (events, receipts) = mpsc::sync_channel(8);
        *NEXT_MUTATION_PROBE.lock().unwrap() = Some(MutationProbe {
            reader: Some(reader),
            pending: None,
            events,
        });
        let deadline = super::super::filesystem_deadline();
        let error = remove_file(path.to_str().unwrap(), parent, deadline).unwrap_err();
        assert_eq!(error.raw_os_error(), None);
        let mut phases = Vec::new();
        while !phases.contains(&"owner-quarantined") {
            phases.push(receipts.recv_timeout(remaining(deadline).unwrap()).unwrap());
        }
        assert_eq!(phases.first(), Some(&"native-removal-completed"));
        assert!(phases.contains(&"native-cleanup-entered"));
        assert!(phases.contains(&"tree-confirmed"));
        assert!(!phases.contains(&"native-cleanup-completed"));
        assert!(!path.exists());
        let pid = LAST_PID.load(Ordering::Acquire);
        assert!(super::request("sid", None, deadline).is_err());
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        writer.write_all(&[1]).unwrap();
        drop(writer);
        assert_eq!(
            receipts.recv_timeout(remaining(deadline).unwrap()).unwrap(),
            "native-cleanup-completed"
        );
        std::fs::write("strict-quarantine-ready", b"active-ancestry-retained").unwrap();
        loop {
            std::thread::park();
        }
    }

    fn wait_for_phases(phases: &ChildPhases, expected: &[&str], deadline: Instant) {
        loop {
            {
                let observed = phases.0.lock().unwrap();
                if observed.len() >= expected.len() {
                    let names: Vec<_> = observed.iter().map(|(phase, _)| *phase).collect();
                    assert_eq!(names, expected);
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "owned child phase receipt timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn phase_order_fixture() {
        let deadline = Instant::now() + Duration::from_secs(30);
        request("sid", None, deadline).unwrap();
        let pid = LAST_PID.load(Ordering::Acquire);
        let phases = Arc::clone(LAST_PHASES.lock().unwrap().as_ref().unwrap());
        wait_for_phases(
            &phases,
            &[
                "child-source-entry",
                "child-encoding-ready",
                "child-utility-import-start",
                "child-utility-import-ready",
                "child-input-line",
                "child-json-parsed",
                "child-sid-resolved",
                "child-reply-serialized",
                "child-reply-flushed",
            ],
            deadline,
        );
        phases.0.lock().unwrap().clear();
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private Unicode 한글 # %");
        let operation_phases = [
            "child-input-line",
            "child-json-parsed",
            "child-sid-resolved",
            "child-reply-serialized",
            "child-reply-flushed",
        ];
        let deadline = Instant::now() + Duration::from_secs(30);
        let guard = crate::filesystem::DirectoryGuard::private(&root).unwrap();
        wait_for_phases(&phases, &operation_phases, deadline);
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        phases.0.lock().unwrap().clear();
        let deadline = Instant::now() + Duration::from_secs(30);
        let path = guard.normalized_path().join("fresh.txt");
        drop(crate::filesystem::create_private_new(&path).unwrap());
        wait_for_phases(&phases, &operation_phases, deadline);
        assert!(crate::filesystem::is_private(&path, false).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        println!("fixed-worker-phase-order-confirmed");
    }

    fn sid_identity_fixture() {
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(
            crate::windows::cached_current_user_sid()
                .is_err_and(|error| error.kind() == io::ErrorKind::NotConnected)
        );
        let native = crate::windows::current_user_sid_until(deadline).unwrap();
        assert!(DISPATCHER.get().is_none());
        let stock = request("sid", None, deadline).unwrap();
        assert!(
            stock.as_str().is_some_and(|stock| stock == native),
            "native and stock process identities differed"
        );
        assert_ne!(LAST_PID.load(Ordering::Acquire), 0);
        remaining(deadline).unwrap();
        println!("native-stock-sid-identity-confirmed");
    }
}
