//! One fixed filesystem child, with bounded requests and kernel-owned crash cleanup.

use std::io;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

use base64::Engine;
use process_wrap::tokio::{
    ChildWrapper, CommandWrap, CommandWrapper, CreationFlags, JobObject, KillOnDrop,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc as queue;
use tokio::task::JoinHandle;
use win32job::{ExtendedLimitInfo, Job};

#[cfg(not(test))]
use super::capture_output;
use super::{OUTPUT_LIMIT, remaining, stock_powershell};

const CLEANUP: Duration = Duration::from_secs(3);
const IDLE: Duration = Duration::from_secs(60);
const SOURCE: &str = include_str!("filesystem_worker.ps1");
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DISPATCHER: OnceLock<io::Result<queue::Sender<Request>>> = OnceLock::new();
#[cfg(test)]
static LAST_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(test)]
static LAST_PHASES: std::sync::Mutex<Option<Arc<ChildPhases>>> = std::sync::Mutex::new(None);

struct Request {
    id: String,
    operation: &'static str,
    bytes: Vec<u8>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<io::Result<Value>>,
    #[cfg(test)]
    diagnostic: Arc<Diagnostic>,
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
struct ChildPhases(std::sync::Mutex<std::collections::VecDeque<(&'static str, Instant)>>);

#[cfg(test)]
impl ChildPhases {
    fn observe(&self, bytes: &[u8]) {
        let line = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        let phase = match line {
            b"locron-fs-phase:source-entry" => "child-source-entry",
            b"locron-fs-phase:encoding-ready" => "child-encoding-ready",
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
    let mut source = format!("{}\n{SOURCE}", phase_token("source-entry"));
    for (anchor, phase) in [
        (
            "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)",
            "encoding-ready",
        ),
        (
            "while ($null -ne ($line = [Console]::In.ReadLine())) {",
            "input-line",
        ),
        ("$request = $line | ConvertFrom-Json", "json-parsed"),
        (
            "$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User",
            "sid-resolved",
        ),
        ("[Console]::Out.Flush()", "reply-flushed"),
    ] {
        assert_eq!(source.matches(anchor).count(), 1);
        source = source.replace(anchor, &format!("{anchor}\n{}", phase_token(phase)));
    }
    let serialize = "(@{version=1; id=$request.id; pid=$PID; result=$result} | ConvertTo-Json -Compress -Depth 6)";
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
    #[cfg(test)]
    let entered = Instant::now();
    remaining(deadline)?;
    if !matches!(operation, "sid" | "create_directory" | "create_file") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown filesystem selector",
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
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(io::Error::other("filesystem worker stopped"))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::Release);
            // The operation already expired. Wait only for its separately bounded owned cleanup.
            receiver.recv_timeout(CLEANUP).unwrap_or_else(|_| {
                Err(io::Error::other(
                    "filesystem worker cleanup remains unconfirmed",
                ))
            })
        }
    }
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
        if quarantine {
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
            match Worker::spawn() {
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
                Err((uncertain, error)) => {
                    #[cfg(test)]
                    request.diagnostic.record("spawn-refused", 0);
                    quarantine = uncertain;
                    fail_queued(&mut receiver, &error);
                    let _ = request.reply.send(Err(error));
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
                continue;
            }
            #[cfg(test)]
            owned.phases.attach(&request.diagnostic, owned.pid);
            #[cfg(test)]
            request.diagnostic.record("cleanup-confirmed", owned.pid);
            worker = None;
            fail_queued(&mut receiver, error);
        }
        let _ = request.reply.send(result);
    }
    if let Some(owned) = worker.as_mut() {
        let _ = owned.cleanup().await;
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

#[derive(Debug)]
struct Enroll {
    job: Arc<Job>,
    created: Arc<AtomicBool>,
}

impl CommandWrapper for Enroll {
    fn post_spawn(
        &mut self,
        _command: &mut Command,
        child: &mut Child,
        _core: &CommandWrap,
    ) -> io::Result<()> {
        self.created.store(true, Ordering::Release);
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("suspended filesystem child has no handle"))?;
        if let Err(error) = self.job.assign_process(handle as isize) {
            let _ = child.start_kill();
            return Err(io::Error::other(error));
        }
        Ok(())
    }
}

struct Worker {
    child: Box<dyn ChildWrapper>,
    job: Arc<Job>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: JoinHandle<io::Result<Vec<u8>>>,
    pid: u32,
    #[cfg(test)]
    phases: Arc<ChildPhases>,
}

fn hidden_creation_flags() -> CreationFlags {
    let mut flags = CreationFlags(Default::default());
    flags.0.0 = 0x0800_0000;
    flags
}

impl Worker {
    fn spawn() -> Result<Self, (bool, io::Error)> {
        let executable = stock_powershell().map_err(|error| (false, error))?;
        #[cfg(not(test))]
        let source = SOURCE;
        #[cfg(test)]
        let source = instrumented_source();
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            source
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut limits = ExtendedLimitInfo::default();
        limits.limit_kill_on_job_close();
        let job = Arc::new(
            Job::create_with_limit_info(&limits)
                .map_err(|error| (false, io::Error::other(error)))?,
        );
        let created = Arc::new(AtomicBool::new(false));
        let mut command = Command::new(executable);
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded,
            ])
            .env_remove("PSModulePath")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut wrapped = CommandWrap::from(command);
        wrapped
            .wrap(hidden_creation_flags())
            .wrap(KillOnDrop)
            .wrap(JobObject)
            .wrap(Enroll {
                job: Arc::clone(&job),
                created: Arc::clone(&created),
            });
        let mut child = wrapped
            .spawn()
            .map_err(|error| (created.load(Ordering::Acquire), error))?;
        let pid = child
            .id()
            .ok_or_else(|| (true, io::Error::other("filesystem child has no PID")))?;
        let stdin = child
            .stdin()
            .take()
            .ok_or_else(|| (true, io::Error::other("filesystem child has no stdin")))?;
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| (true, io::Error::other("filesystem child has no stdout")))?;
        let stderr = child
            .stderr()
            .take()
            .ok_or_else(|| (true, io::Error::other("filesystem child has no stderr")))?;
        #[cfg(test)]
        let phases = Arc::new(ChildPhases::default());
        #[cfg(test)]
        let errors = tokio::spawn(capture_diagnostic_stderr(stderr, Arc::clone(&phases)));
        #[cfg(not(test))]
        let errors = tokio::spawn(capture_output(stderr));
        Ok(Self {
            child,
            job,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: errors,
            pid,
            #[cfg(test)]
            phases,
        })
    }

    async fn exchange(&mut self, request: &Request) -> io::Result<Value> {
        remaining(request.deadline)?;
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
                    stdin.write_all(&request.bytes).await?;
                    stdin.flush().await?;
                    #[cfg(test)]
                    request.diagnostic.record("input-flushed", *pid);
                    let bytes = read_frame(stdout).await?;
                    #[cfg(test)]
                    request.diagnostic.record("reply-received", *pid);
                    validate_frame(&bytes, request, *pid)
                } => result,
                result = stderr => {
                    let bytes = result.map_err(io::Error::other)??;
                    Err(io::Error::other(format!("filesystem worker stderr closed: {}", String::from_utf8_lossy(&bytes).trim())))
                }
                () = cancelled(&request.cancelled) => Err(io::Error::new(io::ErrorKind::TimedOut, "filesystem caller deadline elapsed")),
            }
        };
        tokio::time::timeout_at(deadline, operation)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "stock Windows filesystem worker timed out",
                )
            })?
    }

    async fn cleanup(&mut self) -> io::Result<()> {
        self.stderr.abort();
        let _ = self.child.start_kill();
        let deadline = Instant::now() + CLEANUP;
        loop {
            let reaped = self.child.try_wait().is_ok_and(|status| status.is_some());
            let empty = self
                .job
                .query_process_id_list()
                .is_ok_and(|processes| processes.is_empty());
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
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other(
                    "filesystem worker termination remains unconfirmed",
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
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
    let reply: Reply = serde_json::from_slice(bytes).map_err(io::Error::other)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command as StdCommand, Stdio};

    #[test]
    fn fixed_phase_source_and_observations_are_bounded_and_do_not_render_inputs() {
        let source = instrumented_source();
        for phase in [
            "source-entry",
            "encoding-ready",
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

    #[test]
    fn job_wrapper_keeps_hidden_flags_without_explicit_suspension() {
        let mut wrapped = CommandWrap::from(Command::new("fixture.exe"));
        wrapped.wrap(hidden_creation_flags()).wrap(JobObject);
        assert!(wrapped.has_wrap::<JobObject>());
        let flags = wrapped.get_wrap::<CreationFlags>().unwrap().0.0;
        assert_eq!(flags, 0x0800_0000);
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

    fn run_isolated_fixture(mode: &str, confirmation: &str) {
        let temporary = tempfile::tempdir().unwrap();
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
        if mode == "phase-order" {
            phase_order_fixture();
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
        crate::windows::current_user_sid().unwrap();
        let pid = LAST_PID.load(Ordering::Acquire);
        let phases = Arc::clone(LAST_PHASES.lock().unwrap().as_ref().unwrap());
        wait_for_phases(
            &phases,
            &[
                "child-source-entry",
                "child-encoding-ready",
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
}
