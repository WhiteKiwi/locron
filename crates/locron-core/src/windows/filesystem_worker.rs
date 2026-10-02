//! One fixed filesystem child, with bounded requests and kernel-owned crash cleanup.

use std::io;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

use base64::Engine;
use process_wrap::tokio::{ChildWrapper, CommandWrap, CommandWrapper, JobObject, KillOnDrop};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc as queue;
use tokio::task::JoinHandle;
use win32job::{ExtendedLimitInfo, Job};

use super::{OUTPUT_LIMIT, capture_output, remaining, stock_powershell};

const CLEANUP: Duration = Duration::from_secs(3);
const IDLE: Duration = Duration::from_secs(60);
const SOURCE: &str = include_str!("filesystem_worker.ps1");
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DISPATCHER: OnceLock<io::Result<queue::Sender<Request>>> = OnceLock::new();
#[cfg(test)]
static LAST_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct Request {
    id: String,
    operation: &'static str,
    bytes: Vec<u8>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<io::Result<Value>>,
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
    let request = Request {
        id,
        operation,
        bytes,
        deadline,
        cancelled: Arc::clone(&cancelled),
        reply,
    };
    enqueue(sender, request)?;
    wait_for_reply(&receiver, &cancelled, deadline)
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
            match tokio::time::timeout(IDLE, receiver.recv()).await {
                Ok(request) => request,
                Err(_) => {
                    if let Some(owned) = worker.as_mut() {
                        if owned.cleanup().await.is_ok() {
                            worker = None;
                        } else {
                            quarantine = true;
                        }
                    }
                    continue;
                }
            }
        } else {
            receiver.recv().await
        };
        let Some(request) = request else { break };
        if quarantine {
            let _ = request.reply.send(Err(io::Error::other(
                "filesystem worker termination remains unconfirmed",
            )));
            continue;
        }
        if let Err(error) = remaining(request.deadline) {
            let _ = request.reply.send(Err(error));
            continue;
        }
        if worker.is_none() {
            match Worker::spawn() {
                Ok(owned) => {
                    #[cfg(test)]
                    LAST_PID.store(owned.pid, Ordering::Release);
                    worker = Some(owned);
                }
                Err((uncertain, error)) => {
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
            if let Err(cleanup) = owned.cleanup().await {
                quarantine = true;
                fail_queued(&mut receiver, &cleanup);
                let _ = request.reply.send(Err(cleanup));
                continue;
            }
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
}

impl Worker {
    fn spawn() -> Result<Self, (bool, io::Error)> {
        let executable = stock_powershell().map_err(|error| (false, error))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            SOURCE
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
            .creation_flags(0x0800_0000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(KillOnDrop).wrap(JobObject).wrap(Enroll {
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
        Ok(Self {
            child,
            job,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: tokio::spawn(capture_output(stderr)),
            pid,
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
                    let bytes = read_frame(stdout).await?;
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
                if !self.stderr.is_finished() {
                    if tokio::time::timeout_at(
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

    fn fixture_request(operation: &'static str) -> Request {
        let (reply, _) = mpsc::sync_channel(1);
        Request {
            id: "1".to_owned(),
            operation,
            bytes: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            cancelled: Arc::new(AtomicBool::new(false)),
            reply,
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
        expired.deadline = Instant::now() - Duration::from_millis(1);
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
                .env("LOCRON_FIXED_WORKER_FIXTURE", "reuse")
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
        assert!(bounded_text(&stdout).contains("fixed-worker-reuse-confirmed"));
    }

    #[test]
    fn worker_fixture_child() {
        let Ok(mode) = std::env::var("LOCRON_FIXED_WORKER_FIXTURE") else {
            return;
        };
        assert_eq!(mode, "reuse");
        let first = request("sid", None, Instant::now() + Duration::from_secs(30)).unwrap();
        let pid = LAST_PID.load(Ordering::Acquire);
        assert_ne!(pid, 0);
        let started = Instant::now();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..8 {
                handles.push(scope.spawn(|| {
                    request("sid", None, Instant::now() + Duration::from_secs(30)).unwrap()
                }));
            }
            for handle in handles {
                assert_eq!(handle.join().unwrap(), first);
            }
        });
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
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
        assert_eq!(LAST_PID.load(Ordering::Acquire), pid);
        println!("fixed-worker-reuse-confirmed");
    }
}
