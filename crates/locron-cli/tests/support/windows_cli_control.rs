//! Real native CLI control fixtures; auxiliary child roles are not acceptance tests.

use super::private_state::{PrivateState, private_state_fixture};
use interprocess::os::windows::named_pipe::{PipeStream, pipe_mode};
use locron_core::filesystem::{DirectoryGuard, create_private_new, open_read_no_follow};
use locron_core::notification::{ACK_MESSAGE, WAKE_MESSAGE, endpoint_name_guarded};
use locron_store::{DaemonLock, LockProbe};
use std::fmt;
use std::future::{Future, poll_fn};
use std::io::{self, Read, Write};
use std::os::windows::io::{AsHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::task::Poll;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, PipeMode, ServerOptions};
use tokio::runtime::{Builder, Runtime};

const GUARD: u8 = 1;
const CLIENT: u8 = 2;
const QUERIED: u8 = 4;
const MATCHED: u8 = 8;
const ACK: u8 = 16;
const REAPED: u8 = 32;
const CLEANED: u8 = 64;
const READ_LIMIT: u64 = 64 * 1024;
const OUTER_NANOS: u64 = 30_000_000_000;
const TARGET_SELECTOR: &str = "windows_cli_control::native_cancel_target";
const PEER_SELECTOR: &str = "windows_cli_control::native_wake_peer_target";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Code {
    Success,
    Expired,
    Native,
    ChildExited,
    Role,
    ForeignPeer,
    WrongDirection,
    BadAck,
    Cli,
    Run,
    Progress,
    Cleanup,
    Panicked,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum Phase {
    Setup,
    Guard,
    Role,
    Connect,
    Query,
    Frame,
    Ack,
    Receipt,
    Add,
    Run,
    History,
    Progress,
    Cancel,
    Cleanup,
}

impl Phase {
    fn from_slot(slot: u8) -> Self {
        match slot {
            1 => Self::Guard,
            2 => Self::Role,
            3 => Self::Connect,
            4 => Self::Query,
            5 => Self::Frame,
            6 => Self::Ack,
            7 => Self::Receipt,
            8 => Self::Add,
            9 => Self::Run,
            10 => Self::History,
            11 => Self::Progress,
            12 => Self::Cancel,
            13 => Self::Cleanup,
            _ => Self::Setup,
        }
    }
}

/// Fixed non-sensitive result; a late result cannot qualify successful cleanup.
#[derive(Debug)]
pub struct CaseResult {
    code: Code,
    phase: Phase,
    flags: u8,
    frame_bytes: u32,
    elapsed_us: u128,
}

impl CaseResult {
    /// True only for work and confirmed owned cleanup inside the caller's clock.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.code == Code::Success && self.flags & (REAPED | CLEANED) == (REAPED | CLEANED)
    }
}

impl fmt::Display for CaseResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "code={:?} phase={:?} flags={} frame_bytes={} elapsed_us={}",
            self.code, self.phase, self.flags, self.frame_bytes, self.elapsed_us
        )
    }
}

struct Control {
    deadline: Instant,
    entered: Instant,
    admitted: AtomicBool,
    phase: AtomicU8,
    flags: AtomicU8,
    frame_bytes: AtomicU32,
    case_expiry: AtomicU64,
    probe_expiry: AtomicU64,
    probe_complete: AtomicU64,
}

impl Control {
    fn new(entered: Instant, deadline: Instant) -> Arc<Self> {
        Arc::new(Self {
            deadline,
            entered,
            admitted: AtomicBool::new(true),
            phase: AtomicU8::new(Phase::Setup as u8),
            flags: AtomicU8::new(0),
            frame_bytes: AtomicU32::new(0),
            case_expiry: AtomicU64::new(0),
            probe_expiry: AtomicU64::new(0),
            probe_complete: AtomicU64::new(0),
        })
    }

    fn refuse(&self, code: Code) -> Code {
        self.admitted.store(false, Ordering::Release);
        code
    }

    fn encode(&self, instant: Instant) -> Result<u64, Code> {
        let nanos = instant
            .checked_duration_since(self.entered)
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
            .filter(|nanos| *nanos <= OUTER_NANOS)
            .ok_or_else(|| self.refuse(Code::Native))?;
        Ok(nanos + 1)
    }

    fn decode(&self, slot: u64) -> Result<Instant, Code> {
        let nanos = slot
            .checked_sub(1)
            .filter(|nanos| *nanos <= OUTER_NANOS)
            .ok_or_else(|| self.refuse(Code::Native))?;
        self.entered
            .checked_add(Duration::from_nanos(nanos))
            .filter(|deadline| *deadline <= self.deadline)
            .ok_or_else(|| self.refuse(Code::Native))
    }

    fn publish_case(&self, deadline: Instant) -> Result<(), Code> {
        let slot = self.encode(deadline)?;
        self.case_expiry
            .compare_exchange(0, slot, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        self.check(self.deadline)
    }

    fn effective_deadline(&self) -> Result<Instant, Code> {
        let slot = self.case_expiry.load(Ordering::Acquire);
        if slot == 0 {
            Ok(self.deadline)
        } else {
            self.decode(slot)
        }
    }

    fn publish_probe(&self, deadline: Instant) -> Result<(), Code> {
        let case = self.effective_deadline()?;
        if self.case_expiry.load(Ordering::Acquire) == 0 || deadline > case {
            return Err(self.refuse(Code::Native));
        }
        let slot = self.encode(deadline)?;
        self.probe_expiry
            .compare_exchange(0, slot, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        self.check(deadline)
    }

    fn observation_deadline(&self, requested: Instant) -> Result<Instant, Code> {
        let case = self.effective_deadline()?;
        let mut deadline = requested.min(case).min(self.deadline);
        let seal = self.probe_complete.load(Ordering::Acquire);
        // Acquiring the commit first also observes its preceding expiry write.
        let expiry = self.probe_expiry.load(Ordering::Acquire);
        if expiry == 0 {
            if seal != 0 {
                return Err(self.refuse(Code::Native));
            }
        } else {
            let probe = self.decode(expiry)?;
            if probe > case {
                return Err(self.refuse(Code::Native));
            }
            if seal == 0 {
                deadline = deadline.min(probe);
            } else if self.decode(seal)? >= probe {
                return Err(self.refuse(Code::Expired));
            }
        }
        Ok(deadline)
    }

    fn seal_probe(&self, completed: Instant) -> Result<(), Code> {
        self.check(self.deadline)?;
        let expiry = self.probe_expiry.load(Ordering::Acquire);
        if expiry == 0 || completed >= self.decode(expiry)? {
            return Err(self.refuse(Code::Expired));
        }
        let seal = self.encode(completed)?;
        self.probe_complete
            .compare_exchange(0, seal, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        // A seal never reopens admission after the driver has observed expiry.
        self.check(self.deadline)
    }

    fn check(&self, deadline: Instant) -> Result<(), Code> {
        let observed_deadline = self.observation_deadline(deadline)?;
        if !self.admitted.load(Ordering::Acquire) || Instant::now() >= observed_deadline {
            return Err(self.refuse(Code::Expired));
        }
        Ok(())
    }

    fn step(&self, phase: Phase, deadline: Instant) -> Result<(), Code> {
        self.phase.store(phase as u8, Ordering::Release);
        self.check(deadline)
    }

    fn flag(&self, flag: u8) {
        self.flags.fetch_or(flag, Ordering::Release);
    }

    fn snapshot(&self, code: Code) -> CaseResult {
        CaseResult {
            code,
            phase: Phase::from_slot(self.phase.load(Ordering::Acquire)),
            flags: self.flags.load(Ordering::Acquire),
            frame_bytes: self.frame_bytes.load(Ordering::Acquire),
            elapsed_us: self.entered.elapsed().as_micros(),
        }
    }
}

// The only driver-visible state is bounded counters/result. Exact native owners
// remain on this thread, including while a synchronous operation cannot return.
struct CaseAdmission {
    control: Arc<Control>,
    command: Option<SyncSender<CaseKind>>,
    result: Receiver<CaseResult>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CaseAdmission {
    fn dispatch(&mut self, kind: CaseKind) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        let sender = self
            .command
            .take()
            .ok_or_else(|| self.control.refuse(Code::Native))?;
        // The sole payload is a fixed scalar. Full/disconnect owns no fixture.
        let sent = sender.try_send(kind);
        self.control.check(self.control.deadline)?;
        sent.map_err(|_| self.control.refuse(Code::Native))
    }

    fn receive_until(&self, deadline: Instant) -> CaseResult {
        loop {
            if let Err(code) = self.control.check(deadline) {
                return self.control.snapshot(code);
            }
            let limit = match self.control.observation_deadline(deadline) {
                Ok(limit) => limit,
                Err(code) => return self.control.snapshot(code),
            };
            let slice =
                Duration::from_millis(1).min(limit.saturating_duration_since(Instant::now()));
            let observed = self.result.recv_timeout(slice);
            if let Err(code) = self.control.check(deadline) {
                return self.control.snapshot(code);
            }
            match observed {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return self.control.snapshot(self.control.refuse(Code::Native));
                }
            }
        }
    }

    fn wait_cleanup_until(&self, deadline: Instant) -> CaseResult {
        self.control.admitted.store(false, Ordering::Release);
        let limit = deadline.min(self.control.deadline);
        loop {
            if Instant::now() >= limit {
                return self.control.snapshot(Code::Expired);
            }
            let slice =
                Duration::from_millis(1).min(limit.saturating_duration_since(Instant::now()));
            let observed = self.result.recv_timeout(slice);
            if Instant::now() >= limit {
                return self.control.snapshot(Code::Expired);
            }
            match observed {
                Ok(mut result) => {
                    if result.code == Code::Success {
                        result.code = Code::Expired;
                    }
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return self.control.snapshot(Code::Cleanup);
                }
            }
        }
    }

    fn finish_if_returned(mut self) {
        // No polling/joining an unfinished owner after the 200 ms refusal.
        if self
            .thread
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
        // Otherwise detach: the same worker still owns its exact native context.
    }
}

impl Drop for CaseAdmission {
    fn drop(&mut self) {
        self.control.admitted.store(false, Ordering::Release);
    }
}

struct Owner {
    control: Arc<Control>,
    state: Option<PrivateState>,
    children: [Option<Child>; 2],
    active_cli: Option<Child>,
    guard: Option<DirectoryGuard>,
    runtime: Option<Runtime>,
    client: Option<NamedPipeClient>,
    metadata: Option<MetadataPipe>,
    query_handle: Option<OwnedHandle>,
    cli_stdout: Option<ChildStdout>,
    reaped: bool,
    uncertain_cleanup: bool,
}

impl Owner {
    fn empty(control: Arc<Control>) -> Self {
        Self {
            control,
            state: None,
            // Reserved before any actual spawn; anchoring cannot allocate.
            children: [None, None],
            active_cli: None,
            guard: None,
            runtime: None,
            client: None,
            metadata: None,
            query_handle: None,
            cli_stdout: None,
            reaped: false,
            uncertain_cleanup: false,
        }
    }

    fn root(&self) -> Result<&Path, Code> {
        self.state
            .as_ref()
            .map(PrivateState::path)
            .ok_or(Code::Cleanup)
    }

    fn initialize_state(&mut self) -> Result<(), Code> {
        self.control.step(Phase::Setup, self.control.deadline)?;
        // The unchanged factory is invoked only on this already-admitted worker.
        self.state = Some(private_state_fixture());
        self.control.check(self.control.deadline)
    }

    fn spawn_child(
        &mut self,
        index: usize,
        command: &mut Command,
        post_spawn: Option<Duration>,
    ) -> Result<(), Code> {
        self.control.step(Phase::Setup, self.control.deadline)?;
        let slot = self.children.get_mut(index).ok_or(Code::Native)?;
        if slot.is_some() {
            return Err(Code::Native);
        }
        let spawned = command.spawn();
        let returned = Instant::now();
        match spawned {
            Ok(child) => *slot = Some(child),
            Err(_) => {
                self.control.check(self.control.deadline)?;
                return Err(Code::Native);
            }
        }
        // Anchor the actual handle before publication/checks or any allocation.
        if let Some(duration) = post_spawn {
            self.control
                .publish_case((returned + duration).min(self.control.deadline))?;
        }
        self.control.check(self.control.deadline)
    }

    fn daemon(&mut self, index: usize, post_spawn: Option<Duration>) -> Result<(), Code> {
        self.spawn_child(
            index,
            self.command()?
                .args(["daemon", "run"])
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
            post_spawn,
        )
    }

    fn retain_guard(&mut self) -> Result<(), Code> {
        self.control.step(Phase::Guard, self.control.deadline)?;
        let guarded = DirectoryGuard::existing_private(self.root()?);
        let guard = match guarded {
            Ok(guard) => guard,
            Err(_) => {
                self.control.check(self.control.deadline)?;
                return Err(Code::Native);
            }
        };
        self.guard = Some(guard);
        self.control.flag(GUARD);
        self.control.check(self.control.deadline)
    }

    fn live(&mut self, index: usize, deadline: Instant) -> Result<(), Code> {
        let child = self
            .children
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or(Code::Native)?;
        child_live(&self.control, child, deadline)
    }

    fn expected_role(&mut self, index: usize) -> Result<(), Code> {
        let path = self.root()?.join("daemon.lock");
        loop {
            self.control.step(Phase::Role, self.control.deadline)?;
            self.live(index, self.control.deadline)?;
            let metadata = DaemonLock::read_role_metadata(&path);
            self.control.check(self.control.deadline)?;
            let metadata = metadata.map_err(|_| Code::Role)?;
            if let Some(metadata) = metadata {
                let child = self
                    .children
                    .get(index)
                    .and_then(Option::as_ref)
                    .ok_or(Code::Native)?;
                if metadata.metadata.pid != child.id() {
                    return Err(Code::Role);
                }
                self.control.check(self.control.deadline)?;
                let held = DaemonLock::probe_existing(&path);
                self.control.check(self.control.deadline)?;
                let held = held.map_err(|_| Code::Role)?;
                if held != LockProbe::Held {
                    return Err(Code::Role);
                }
                let repeated = DaemonLock::read_role_metadata(&path);
                self.control.check(self.control.deadline)?;
                let repeated = repeated.map_err(|_| Code::Role)?;
                if repeated.as_ref() != Some(&metadata) {
                    return Err(Code::Role);
                }
                self.live(index, self.control.deadline)?;
                return Ok(());
            }
            pause(
                &self.control,
                self.control.deadline,
                Duration::from_millis(5),
            )?;
        }
    }

    fn command(&self) -> Result<Command, Code> {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
        command.arg("--state-dir").arg(self.root()?);
        Ok(command)
    }

    fn output(&mut self, phase: Phase, command: &mut Command) -> Result<Output, Code> {
        self.control.step(phase, self.control.deadline)?;
        if self.active_cli.is_some() || self.cli_stdout.is_some() {
            return Err(Code::Native);
        }
        // Retain the actual control CLI too: Command::output would conceal its
        // Child on an I/O failure. The command/JSON/status under test is unchanged.
        let child = command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let child = match child {
            Ok(child) => child,
            Err(_) => {
                self.control.check(self.control.deadline)?;
                return Err(Code::Cli);
            }
        };
        self.active_cli = Some(child);
        self.control.check(self.control.deadline)?;
        self.cli_stdout = self.active_cli.as_mut().ok_or(Code::Cli)?.stdout.take();
        self.control.check(self.control.deadline)?;
        let mut bytes = Vec::new();
        let read = self
            .cli_stdout
            .as_mut()
            .ok_or(Code::Cli)?
            .read_to_end(&mut bytes);
        self.control.check(self.control.deadline)?;
        read.map_err(|_| Code::Cli)?;
        self.control.check(self.control.deadline)?;
        let status = self.active_cli.as_mut().ok_or(Code::Cli)?.wait();
        self.control.check(self.control.deadline)?;
        let status = status.map_err(|_| Code::Cli)?;
        // The actual control CLI has been reaped before its pipe is released.
        drop(self.cli_stdout.take());
        drop(self.active_cli.take());
        self.control.check(self.control.deadline)?;
        if !status.success() {
            return Err(Code::Cli);
        }
        Ok(Output {
            status,
            stdout: bytes,
            stderr: Vec::new(),
        })
    }

    fn history(&mut self, name: &str, run_id: &str) -> Result<String, Code> {
        self.live(0, self.control.deadline)?;
        let output = self.output(
            Phase::History,
            self.command()?.args(["--json", "history", name]),
        )?;
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Code::Run)?;
        self.control.check(self.control.deadline)?;
        envelope["data"]
            .as_array()
            .and_then(|runs| runs.iter().find(|run| run["id"] == run_id))
            .and_then(|run| run["state"].as_str())
            .map(str::to_owned)
            .ok_or(Code::Run)
    }

    fn submit(&mut self, name: &str) -> Result<String, Code> {
        let output = self.output(Phase::Run, self.command()?.args(["--json", "run", name]))?;
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Code::Run)?;
        self.control.check(self.control.deadline)?;
        envelope["data"]["run_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or(Code::Run)
    }

    fn probe(&mut self, index: usize, hook: Option<ReturnGate>) -> Result<(), Code> {
        // This entry is before runtime creation, open, duplication, native query or I/O.
        let entry = Instant::now();
        let deadline = self
            .control
            .effective_deadline()?
            .min(entry + Duration::from_millis(200));
        self.control.publish_probe(deadline)?;
        self.control.step(Phase::Connect, deadline)?;
        self.live(index, deadline)?;
        let guard = self.guard.as_ref().ok_or(Code::Native)?;
        let endpoint = native(&self.control, deadline, Code::Native, || {
            endpoint_name_guarded(guard, "wake", None)
        })?;
        self.control.check(deadline)?;
        let built = Builder::new_current_thread().enable_all().build();
        self.runtime = match built {
            Ok(runtime) => Some(runtime),
            Err(_) => {
                self.control.check(deadline)?;
                return Err(Code::Native);
            }
        };
        self.control.check(deadline)?;
        let gate_root = self.root()?.to_path_buf();
        let runtime = self.runtime.as_ref().ok_or(Code::Native)?;
        let control = &self.control;
        let child = self
            .children
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or(Code::Native)?;
        let client = &mut self.client;
        let metadata = &mut self.metadata;
        let query_handle = &mut self.query_handle;
        let result = runtime.block_on(async {
            loop {
                control.check(deadline)?;
                let opened = ClientOptions::new()
                    .read(true)
                    .write(true)
                    .pipe_mode(PipeMode::Byte)
                    .security_qos_flags(0x0001_0000)
                    .open(&endpoint);
                match opened {
                    Ok(opened) => {
                        *client = Some(opened);
                        control.flag(CLIENT);
                        control.check(deadline)?;
                        break;
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound
                            || error.raw_os_error() == Some(231) =>
                    {
                        control.check(deadline)?;
                        gated(control, deadline, async {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            Ok(())
                        })
                        .await?;
                    }
                    Err(_) => {
                        control.check(deadline)?;
                        return Err(Code::Native);
                    }
                }
            }
            child_live(control, child, deadline)?;
            control.step(Phase::Query, deadline)?;
            let original = client.as_mut().ok_or(Code::Native)?;
            control.check(deadline)?;
            let duplicated = original.as_handle().try_clone_to_owned();
            match duplicated {
                Ok(handle) => *query_handle = Some(handle),
                Err(_) => {
                    control.check(deadline)?;
                    return Err(Code::Native);
                }
            }
            control.check(deadline)?;
            let handle = query_handle.take().ok_or(Code::Native)?;
            *metadata = match PipeStream::<pipe_mode::Bytes, pipe_mode::Bytes>::try_from(handle) {
                Ok(wrapper) => Some(MetadataPipe(Some(wrapper))),
                Err(error) => {
                    // Plain returned handle only; never enter interprocess default limbo.
                    *query_handle = error.source;
                    control.check(deadline)?;
                    return Err(Code::Native);
                }
            };
            control.check(deadline)?;
            let pipe = metadata.as_ref().and_then(|wrapper| wrapper.0.as_ref()).ok_or(Code::Native)?;
            let is_client = pipe.is_client();
            control.check(deadline)?;
            let actual_peer = pipe.server_process_id();
            control.flag(QUERIED);
            control.check(deadline)?;
            child_live(control, child, deadline)?;
            if let Some(hook) = hook {
                if !is_client
                    || !matches!(actual_peer.as_ref(), Ok(peer) if *peer != 0 && *peer == child.id())
                {
                    return Err(Code::Native);
                }
                // The actual native query has returned. Deliberately withhold its
                // result/return; the real original client/root/child remain owned.
                let notice = HeldNotice {
                    deadline,
                    root: gate_root,
                };
                let sent = hook.entered.try_send(notice);
                control.check(deadline)?;
                sent.map_err(|_| Code::Native)?;
                let released = hook.release
                    .recv_timeout(control.deadline.saturating_duration_since(Instant::now()));
                control.check(deadline)?;
                released.map_err(|_| Code::Expired)?;
            }
            // Metadata-only duplicate stays in the owner through child reaping.
            // Its every-outcome Drop consumes it with evade_limbo, never I/O.
            control.check(deadline)?;
            child_live(control, child, deadline)?;
            if !is_client {
                return Err(Code::WrongDirection);
            }
            let actual_peer = actual_peer.map_err(|_| Code::Native)?;
            if actual_peer == 0 || actual_peer != child.id() {
                return Err(Code::ForeignPeer);
            }
            control.flag(MATCHED);
            let mut frame = [0_u8; 16];
            frame[0] = 15;
            frame[1..].copy_from_slice(WAKE_MESSAGE);
            control.step(Phase::Frame, deadline)?;
            control.frame_bytes.store(16, Ordering::Release);
            gated(control, deadline, original.write_all(&frame)).await?;
            child_live(control, child, deadline)?;
            control.step(Phase::Ack, deadline)?;
            let mut acknowledgement = [0_u8; 14];
            gated(control, deadline, original.read_exact(&mut acknowledgement)).await?;
            child_live(control, child, deadline)?;
            if acknowledgement != ACK_MESSAGE {
                return Err(Code::BadAck);
            }
            control.flag(ACK);
            control.step(Phase::Receipt, deadline)?;
            control.frame_bytes.store(31, Ordering::Release);
            gated(control, deadline, original.write_all(&[0xff])).await?;
            child_live(control, child, deadline)?;
            control.check(deadline)
        });
        // Seal completed successes AND on-time refusals; it witnesses completion,
        // never peer trust. Incomplete/late work leaves the expiry intact.
        self.control.check(deadline)?;
        self.control.seal_probe(Instant::now())?;
        result
    }

    fn reap(&mut self) -> Result<(), Code> {
        let mut actual_child = false;
        for child in self
            .children
            .iter_mut()
            .chain(std::iter::once(&mut self.active_cli))
            .flatten()
        {
            actual_child = true;
            // Normal checks include native cleanup in the original clock. After
            // expiry, only emergency cleanup of these exact handles is admitted.
            let _ = self.control.check(self.control.deadline);
            cleanup_gate(&self.control)?;
            let exited = child.try_wait();
            cleanup_gate(&self.control)?;
            let exited = exited.map_err(|_| Code::Cleanup)?.is_some();
            if !exited {
                cleanup_gate(&self.control)?;
                let _ = child.kill();
                cleanup_gate(&self.control)?;
                let waited = child.wait();
                cleanup_gate(&self.control)?;
                waited.map_err(|_| Code::Cleanup)?;
            }
        }
        self.reaped = true;
        if actual_child {
            self.control.flag(REAPED);
        }
        Ok(())
    }

    fn release(&mut self) -> Result<(), Code> {
        let root = self.state.as_ref().map(|state| state.path().to_path_buf());
        // Reaping, not cancellation/Drop, authorizes removal of private state.
        cleanup_gate(&self.control)?;
        drop(self.cli_stdout.take());
        cleanup_gate(&self.control)?;
        drop(self.metadata.take());
        cleanup_gate(&self.control)?;
        drop(self.query_handle.take());
        cleanup_gate(&self.control)?;
        drop(self.client.take());
        cleanup_gate(&self.control)?;
        drop(self.runtime.take());
        cleanup_gate(&self.control)?;
        drop(self.guard.take());
        cleanup_gate(&self.control)?;
        drop(self.state.take());
        // Cleanup of an already reaped exact owner may follow a refused probe.
        // It still has the helper/caller ORIGINAL clock, never a new duration.
        cleanup_gate(&self.control)?;
        if let Some(root) = root {
            let remains = root.try_exists();
            cleanup_gate(&self.control)?;
            if remains.map_err(|_| Code::Cleanup)? {
                return Err(Code::Cleanup);
            }
            self.control.flag(CLEANED);
        }
        Ok(())
    }

    fn quarantine(&self) -> ! {
        self.control.admitted.store(false, Ordering::Release);
        loop {
            thread::park();
        }
    }

    fn complete(&mut self, work: Result<(), Code>) -> CaseResult {
        let previous_phase = self.control.phase.load(Ordering::Acquire);
        self.control
            .phase
            .store(Phase::Cleanup as u8, Ordering::Release);
        if self.reap().is_err() {
            self.uncertain_cleanup = true;
            // No result/field destruction while exact root exit is unknown.
            self.quarantine();
        }
        let cleanup = self.release();
        let mut code = cleanup.and(work).err().unwrap_or(Code::Success);
        if code == Code::Success && self.control.check(self.control.deadline).is_err() {
            code = Code::Expired;
        }
        self.control.phase.store(previous_phase, Ordering::Release);
        self.control.snapshot(code)
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // Includes unwinding. Never let automatic field/TempDir destruction run
        // while actual owned-child exit is unknown. No replay/replacement here.
        if !self.reaped && !self.uncertain_cleanup && self.reap().is_err() {
            self.uncertain_cleanup = true;
        }
        if self.uncertain_cleanup {
            self.quarantine();
        }
        // Includes empty/partial setup. An unfinished native context stays here,
        // never on an admission/error/channel/driver path.
        if self.state.is_some()
            || self.guard.is_some()
            || self.runtime.is_some()
            || self.client.is_some()
            || self.metadata.is_some()
            || self.query_handle.is_some()
            || self.cli_stdout.is_some()
        {
            if self.release().is_err() {
                self.quarantine();
            }
        }
    }
}

fn cleanup_gate(control: &Control) -> Result<(), Code> {
    // Closing admission does not authorize new work; only the same owner's
    // teardown may proceed inside the one original outer horizon.
    let _ = control.check(control.deadline);
    if Instant::now() >= control.deadline {
        Err(Code::Expired)
    } else {
        Ok(())
    }
}

struct MetadataPipe(Option<PipeStream<pipe_mode::Bytes, pipe_mode::Bytes>>);

impl Drop for MetadataPipe {
    fn drop(&mut self) {
        if let Some(wrapper) = self.0.take() {
            wrapper.evade_limbo();
        }
    }
}

fn native<T, E>(
    control: &Control,
    deadline: Instant,
    error: Code,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, Code> {
    control.check(deadline)?;
    let result = operation();
    control.check(deadline)?;
    result.map_err(|_| error)
}

fn child_live(control: &Control, child: &mut Child, deadline: Instant) -> Result<(), Code> {
    control.check(deadline)?;
    let exited = native(control, deadline, Code::Native, || child.try_wait())?.is_some();
    control.check(deadline)?;
    if exited {
        Err(Code::ChildExited)
    } else {
        Ok(())
    }
}

fn pause(control: &Control, deadline: Instant, duration: Duration) -> Result<(), Code> {
    control.check(deadline)?;
    thread::sleep(duration.min(deadline.saturating_duration_since(Instant::now())));
    control.check(deadline)
}

async fn gated<T>(
    control: &Control,
    deadline: Instant,
    future: impl Future<Output = io::Result<T>>,
) -> Result<T, Code> {
    let mut future = pin!(future);
    let guarded = poll_fn(|context| {
        if let Err(error) = control.check(deadline) {
            return Poll::Ready(Err(error));
        }
        let result = future.as_mut().poll(context);
        if let Err(error) = control.check(deadline) {
            return Poll::Ready(Err(error));
        }
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => Poll::Ready(result.map_err(|_| Code::Native)),
        }
    });
    let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), guarded)
        .await
        .map_err(|_| {
            control.admitted.store(false, Ordering::Release);
            Code::Expired
        })?;
    control.check(deadline)?;
    result
}

#[derive(Clone, Copy)]
enum CaseKind {
    Wake,
    Cancel,
    Peer(PeerMode),
}

fn admit(hook: Option<ReturnGate>) -> Result<CaseAdmission, CaseResult> {
    // The only origin/outer horizon is born before even empty OS admission.
    let entered = Instant::now();
    let control = Control::new(entered, entered + Duration::from_secs(30));
    let (command, commands) = mpsc::sync_channel(1);
    let (sender, result) = mpsc::sync_channel(1);
    let worker_control = Arc::clone(&control);
    // Captures contain no fixture state, native handle, Child or owner callback.
    let admitted = thread::Builder::new()
        .name("windows-cli-case".to_owned())
        .spawn(move || {
            let mut owner = Owner::empty(worker_control);
            let mut hook = hook;
            let work = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                owner.control.check(owner.control.deadline)?;
                let received = commands.recv_timeout(
                    owner
                        .control
                        .deadline
                        .saturating_duration_since(Instant::now()),
                );
                owner.control.check(owner.control.deadline)?;
                let kind = received.map_err(|_| Code::Native)?;
                if matches!(kind, CaseKind::Peer(_)) {
                    owner.control.publish_case(owner.control.deadline)?;
                }
                owner.initialize_state()?;
                match kind {
                    CaseKind::Wake => {
                        owner.daemon(0, Some(Duration::from_secs(5)))?;
                        wake_work(&mut owner)
                    }
                    CaseKind::Cancel => {
                        owner.daemon(0, Some(Duration::from_secs(8)))?;
                        cancel_work(&mut owner)
                    }
                    CaseKind::Peer(mode) => {
                        setup_peer(&mut owner, mode)?;
                        let index = if matches!(mode, PeerMode::Foreign) {
                            owner.daemon(1, None)?;
                            owner.expected_role(1)?;
                            1
                        } else {
                            0
                        };
                        owner.probe(index, hook.take())
                    }
                }
            }))
            .unwrap_or(Err(Code::Panicked));
            // Owner stays outside the caught setup/work closure on this one worker.
            let completed = owner.complete(work);
            let _ = sender.try_send(completed);
        });
    let thread = match admitted {
        Ok(thread) => thread,
        Err(_) => return Err(control.snapshot(control.refuse(Code::Native))),
    };
    Ok(CaseAdmission {
        control,
        command: Some(command),
        result,
        thread: Some(thread),
    })
}

fn drive(kind: CaseKind) -> CaseResult {
    let mut driver = match admit(None) {
        Ok(driver) => driver,
        Err(result) => return result,
    };
    if let Err(code) = driver.dispatch(kind) {
        let result = driver.control.snapshot(code);
        driver.finish_if_returned();
        return result;
    }
    let mut result = driver.receive_until(driver.control.deadline);
    let limit = driver.control.observation_deadline(driver.control.deadline);
    driver.finish_if_returned();
    if limit.is_err() || limit.is_ok_and(|deadline| Instant::now() >= deadline) {
        result.code = Code::Expired;
    }
    result
}

/// Tests secured actual-daemon delivery and manual admission within original5s.
pub fn run_wake_case() -> CaseResult {
    drive(CaseKind::Wake)
}

fn wake_work(owner: &mut Owner) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.expected_role(0)?;
    owner.probe(0, None)?;
    owner.output(
        Phase::Add,
        owner
            .command()?
            .args(["add", "wake", "--every", "1h", "--"])
            .args(super::success_process_args()),
    )?;
    let run_id = owner.submit("wake")?;
    loop {
        match owner.history("wake", &run_id)?.as_str() {
            "succeeded" => break Ok(()),
            "queued" | "running" => {}
            _ => break Err(Code::Run),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
}

/// Tests actual Engine native progress and durable cancellation within original8s.
pub fn run_cancel_case() -> CaseResult {
    drive(CaseKind::Cancel)
}

fn cancel_work(owner: &mut Owner) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.live(0, owner.control.deadline)?;
    owner.control.check(owner.control.deadline)?;
    let executable = std::env::current_exe();
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    if !executable.is_absolute() || executable.to_str().is_none() {
        return Err(Code::Native);
    }
    let progress = owner.root()?.join("cancel-progress");
    let progress_env = format!(
        "WINDOWS_CLI_CANCEL_PROGRESS={}",
        progress.to_str().ok_or(Code::Native)?
    );
    owner.output(
        Phase::Add,
        owner
            .command()?
            .args(["add", "cancel", "--every", "1h", "--cwd"])
            .arg(owner.root()?)
            .args(["--env", "WINDOWS_CLI_CANCEL_ROLE=native-cancel-v1", "--env"])
            .arg(progress_env)
            .arg("--")
            .arg(executable)
            .args([
                "--exact",
                TARGET_SELECTOR,
                "--nocapture",
                "--test-threads=1",
            ]),
    )?;
    let run_id = owner.submit("cancel")?;
    loop {
        let state = owner.history("cancel", &run_id)?;
        let counters = read_progress(&owner.control, &progress)?;
        if state == "running" && counters.len() >= 2 {
            owner.live(0, owner.control.deadline)?;
            break;
        }
        if !matches!(state.as_str(), "queued" | "running") {
            return Err(Code::Run);
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
    owner.output(Phase::Cancel, owner.command()?.args(["cancel", &run_id]))?;
    loop {
        match owner.history("cancel", &run_id)?.as_str() {
            "cancelled" => break,
            "queued" | "running" => {}
            _ => return Err(Code::Run),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
    let stopped = read_progress(&owner.control, &progress)?;
    if stopped.len() < 2 {
        return Err(Code::Progress);
    }
    pause(
        &owner.control,
        owner.control.deadline,
        Duration::from_millis(25),
    )?;
    if read_progress(&owner.control, &progress)? != stopped {
        return Err(Code::Progress);
    }
    owner.live(0, owner.control.deadline)
}

fn read_progress(control: &Control, path: &Path) -> Result<Vec<u64>, Code> {
    control.step(Phase::Progress, control.deadline)?;
    let opened = open_read_no_follow(path);
    control.check(control.deadline)?;
    let mut file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(Code::Progress),
    };
    let mut bytes = Vec::new();
    control.check(control.deadline)?;
    let read = (&mut *file).take(READ_LIMIT + 1).read_to_end(&mut bytes);
    control.check(control.deadline)?;
    read.map_err(|_| Code::Progress)?;
    if bytes.len() > READ_LIMIT as usize || bytes.len() % 17 != 0 {
        return Err(Code::Progress);
    }
    let mut counters = Vec::new();
    for record in bytes.chunks_exact(17) {
        if record[16] != b'\n' || !record[..16].iter().all(u8::is_ascii_hexdigit) {
            return Err(Code::Progress);
        }
        let counter = u64::from_str_radix(
            std::str::from_utf8(&record[..16]).map_err(|_| Code::Progress)?,
            16,
        )
        .map_err(|_| Code::Progress)?;
        if counters.last().is_some_and(|previous| counter <= *previous) {
            return Err(Code::Progress);
        }
        counters.push(counter);
    }
    control.check(control.deadline)?;
    Ok(counters)
}

#[test]
fn native_cancel_target() {
    let Some(role) = std::env::var_os("WINDOWS_CLI_CANCEL_ROLE") else {
        return;
    };
    assert_eq!(role, "native-cancel-v1", "invalid native target role");
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(30);
    let result = (|| -> Result<(), Code> {
        let control = Control::new(entered, deadline);
        control.check(deadline)?;
        let path =
            PathBuf::from(std::env::var_os("WINDOWS_CLI_CANCEL_PROGRESS").ok_or(Code::Progress)?);
        let parent = path.parent().ok_or(Code::Progress)?;
        let guarded = DirectoryGuard::existing_private(parent);
        control.check(deadline)?;
        let guard = guarded.map_err(|_| Code::Progress)?;
        control.check(deadline)?;
        let opened = create_private_new(&path);
        control.check(deadline)?;
        let mut writer = opened.map_err(|_| Code::Progress)?;
        for counter in 0_u64..1201 {
            if Instant::now() >= deadline {
                break;
            }
            control.check(deadline)?;
            let written = writer.write_all(format!("{counter:016x}\n").as_bytes());
            control.check(deadline)?;
            written.map_err(|_| Code::Progress)?;
            control.check(deadline)?;
            let flushed = writer.flush();
            control.check(deadline)?;
            flushed.map_err(|_| Code::Progress)?;
            thread::sleep(
                Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
        drop(writer);
        drop(guard);
        Ok(())
    })();
    assert!(result.is_ok(), "native target refused: {:?}", result.err());
}

#[derive(Clone, Copy)]
enum PeerMode {
    Foreign,
    Malformed,
    Idle,
    Withheld,
}

impl PeerMode {
    fn name(self) -> &'static str {
        match self {
            Self::Foreign => "foreign",
            Self::Malformed => "malformed",
            Self::Idle => "idle",
            Self::Withheld => "withheld",
        }
    }
}

#[test]
fn native_wake_peer_target() {
    let Some(role) = std::env::var_os("WINDOWS_CLI_WAKE_PEER_ROLE") else {
        return;
    };
    let mode = match role.to_str() {
        Some("foreign") => PeerMode::Foreign,
        Some("malformed") => PeerMode::Malformed,
        Some("idle") => PeerMode::Idle,
        Some("withheld") => PeerMode::Withheld,
        _ => panic!("invalid native peer role"),
    };
    let entered = Instant::now();
    let control = Control::new(entered, entered + Duration::from_secs(30));
    let result = peer_target(mode, &control);
    assert!(result.is_ok(), "native peer refused: {:?}", result.err());
}

fn peer_target(mode: PeerMode, control: &Control) -> Result<(), Code> {
    let deadline = control.deadline;
    control.check(deadline)?;
    let root = PathBuf::from(std::env::var_os("WINDOWS_CLI_WAKE_PEER_ROOT").ok_or(Code::Native)?);
    let guarded = DirectoryGuard::existing_private(&root);
    control.check(deadline)?;
    let guard = guarded.map_err(|_| Code::Native)?;
    control.check(deadline)?;
    let named = endpoint_name_guarded(&guard, "wake", None);
    control.check(deadline)?;
    let endpoint = named.map_err(|_| Code::Native)?;
    control.check(deadline)?;
    let built = Builder::new_current_thread().enable_all().build();
    control.check(deadline)?;
    let runtime = built.map_err(|_| Code::Native)?;
    runtime.block_on(async {
        control.check(deadline)?;
        // Deliberately untrusted negative input. A private directory does not
        // confer pipe privacy; only the real daemon qualifies positive readiness.
        let created = ServerOptions::new()
            .pipe_mode(PipeMode::Byte)
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&endpoint);
        control.check(deadline)?;
        let mut server = created.map_err(|_| Code::Native)?;
        control.check(deadline)?;
        let created = create_private_new(&root.join("peer-ready"));
        control.check(deadline)?;
        let mut ready = created.map_err(|_| Code::Native)?;
        control.check(deadline)?;
        let written = ready.write_all(b"1");
        control.check(deadline)?;
        written.map_err(|_| Code::Native)?;
        control.check(deadline)?;
        let flushed = ready.flush();
        control.check(deadline)?;
        flushed.map_err(|_| Code::Native)?;
        drop(ready);
        gated(control, deadline, server.connect()).await?;
        match mode {
            PeerMode::Malformed => {
                let mut frame = [0_u8; 16];
                gated(control, deadline, server.read_exact(&mut frame)).await?;
                if frame[0] != 15 || &frame[1..] != WAKE_MESSAGE {
                    return Err(Code::Run);
                }
                gated(control, deadline, server.write_all(b"locron-ack/v0\n")).await?;
            }
            PeerMode::Foreign | PeerMode::Idle | PeerMode::Withheld => {}
        }
        // Remain a real live peer until the exact owner kills/reaps this child.
        gated(control, deadline, async {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
            Ok(())
        })
        .await
    })
}

fn setup_peer(owner: &mut Owner, mode: PeerMode) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.control.step(Phase::Setup, owner.control.deadline)?;
    let executable = std::env::current_exe();
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    owner.spawn_child(
        0,
        Command::new(executable)
            .args(["--exact", PEER_SELECTOR, "--nocapture", "--test-threads=1"])
            .env("WINDOWS_CLI_WAKE_PEER_ROLE", mode.name())
            .env("WINDOWS_CLI_WAKE_PEER_ROOT", owner.root()?)
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        None,
    )?;
    let ready = owner.root()?.join("peer-ready");
    loop {
        owner.live(0, owner.control.deadline)?;
        owner.control.check(owner.control.deadline)?;
        let file = open_read_no_follow(&ready);
        owner.control.check(owner.control.deadline)?;
        match file {
            Ok(mut file) => {
                let mut byte = Vec::new();
                owner.control.check(owner.control.deadline)?;
                let read = (&mut *file).take(2).read_to_end(&mut byte);
                owner.control.check(owner.control.deadline)?;
                read.map_err(|_| Code::Native)?;
                if byte != b"1" {
                    return Err(Code::Native);
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(Code::Native),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(5),
        )?;
    }
    Ok(())
}

fn refused_peer(mode: PeerMode, expected: Code) {
    let mut driver = admit(None).expect("resource-free peer admission failed");
    let deadline = driver.control.deadline;
    assert!(
        driver.dispatch(CaseKind::Peer(mode)).is_ok(),
        "peer command refused"
    );
    let refusal = driver.receive_until(deadline);
    assert_eq!(
        refusal.code, expected,
        "first peer refusal was incorrect: {refusal}"
    );
    assert_eq!(
        refusal.flags & (CLIENT | QUERIED),
        CLIENT | QUERIED,
        "refusal did not reach the real same-connection query: {refusal}"
    );
    if matches!(mode, PeerMode::Idle) {
        assert_ne!(driver.control.probe_expiry.load(Ordering::Acquire), 0);
        assert_eq!(driver.control.probe_complete.load(Ordering::Acquire), 0);
        assert!(!driver.control.admitted.load(Ordering::Acquire));
    }
    let result = if refusal.flags & CLEANED == 0 {
        driver.wait_cleanup_until(deadline)
    } else {
        refusal
    };
    driver.finish_if_returned();
    assert_eq!(
        result.code, expected,
        "negative native peer did not refuse as required: {result}"
    );
    assert_ne!(
        result.flags & CLEANED,
        0,
        "actual owned cleanup is unconfirmed: {result}"
    );
    assert_eq!(
        result.flags & ACK,
        0,
        "untrusted peer qualified an ACK: {result}"
    );
    if matches!(mode, PeerMode::Foreign) {
        assert_eq!(
            result.frame_bytes, 0,
            "foreign peer received a dispatched frame: {result}"
        );
    }
    assert!(
        Instant::now() < deadline,
        "negative peer exceeded original helper horizon"
    );
}

#[test]
fn foreign_wake_peer_is_refused_before_frame() {
    refused_peer(PeerMode::Foreign, Code::ForeignPeer);
}

#[test]
fn malformed_wake_ack_is_refused() {
    refused_peer(PeerMode::Malformed, Code::BadAck);
}

#[test]
fn idle_wake_peer_expires_original_probe_deadline() {
    refused_peer(PeerMode::Idle, Code::Expired);
}

struct ReturnGate {
    entered: SyncSender<HeldNotice>,
    release: Receiver<()>,
}

struct HeldNotice {
    deadline: Instant,
    root: PathBuf,
}

// Even assertion unwinding releases the controlled return; ownership stays on
// the existing worker rather than resetting/replacing a stalled context.
struct ReleaseGate(Option<SyncSender<()>>);

impl Drop for ReleaseGate {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

#[test]
fn withheld_native_return_retains_case_owner_at_deadline() {
    let (entered, notice) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let release = ReleaseGate(Some(release));
    let mut driver = admit(Some(ReturnGate {
        entered,
        release: released,
    }))
    .expect("resource-free controlled admission failed");
    let deadline = driver.control.deadline;
    assert!(
        driver.dispatch(CaseKind::Peer(PeerMode::Withheld)).is_ok(),
        "peer command refused"
    );
    let notice = notice
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("actual native query did not reach the controlled return gate");
    // The caller supplies only the original outer horizon. The driver must
    // independently discover and refuse the published 200 ms incomplete probe.
    let refused = driver.receive_until(deadline);
    assert!(
        Instant::now() >= notice.deadline,
        "driver did not observe probe expiry"
    );
    assert_eq!(driver.control.probe_complete.load(Ordering::Acquire), 0);
    assert!(!driver.control.admitted.load(Ordering::Acquire));
    assert_eq!(
        refused.code,
        Code::Expired,
        "driver accepted a withheld result: {refused}"
    );
    assert_eq!(
        refused.flags & (GUARD | CLIENT | QUERIED),
        GUARD | CLIENT | QUERIED
    );
    assert_eq!(refused.flags & REAPED, 0);
    assert_eq!(refused.frame_bytes, 0);
    assert!(Instant::now() < deadline, "original helper clock expired");
    // A real incompatible namespace operation proves the retained root guard;
    // the actual query/client/Child remain inside the owner, not a DTO authority.
    let renamed = std::fs::rename(&notice.root, notice.root.with_extension("displaced"));
    assert!(
        Instant::now() < deadline,
        "root probe returned after helper expiry"
    );
    assert_eq!(
        renamed.err().and_then(|error| error.raw_os_error()),
        Some(32)
    );
    drop(release);
    let returned = driver.wait_cleanup_until(deadline);
    assert_eq!(
        returned.code,
        Code::Expired,
        "late query result became success: {returned}"
    );
    assert_ne!(returned.flags & REAPED, 0);
    assert_ne!(returned.flags & CLEANED, 0);
    assert_eq!(returned.frame_bytes, 0);
    assert!(!returned.succeeded());
    assert!(
        Instant::now() < deadline,
        "cleanup exceeded original helper horizon"
    );
    driver.finish_if_returned();
}
