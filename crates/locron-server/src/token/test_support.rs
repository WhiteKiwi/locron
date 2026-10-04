//! Root-scoped test controls. No hook exists in a non-test build.

use std::cell::RefCell;
use std::fs;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use locron_core::filesystem::GuardedFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempDir};

pub(super) const FIXTURE_ENV: &str = "LOCRON_TOKEN_QUALIFICATION_CHILD";
pub(super) const FRAME_LIMIT: usize = 1024;
const EVENT_LIMIT: u32 = 64;
const CAPTURE_LIMIT: u64 = 65_536;
const READY: Duration = Duration::from_secs(10);
pub(super) const INTERLEAVE: Duration = Duration::from_secs(4);
const PROTOCOL: Duration = Duration::from_secs(15);
const CLEANUP: Duration = Duration::from_secs(3);
pub(super) const SEED: &[u8] = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(super) const CANARY: &[u8] = b"private_canary_not_hex";
static DRIVERS: Mutex<()> = Mutex::new(());

pub(super) fn driver() -> MutexGuard<'static, ()> {
    DRIVERS.lock().expect("token qualification driver poisoned")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Operation {
    Ensure,
    Regenerate,
    Remove,
    Hold,
    #[cfg(windows)]
    ConstructorFinal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Event {
    Ready,
    Begin,
    Busy,
    Sleep,
    Acquired,
    TokenIo,
    Decision,
    ScratchCreated,
    Write,
    Sync,
    LeafClosed,
    Rename,
    BeforeCleanup,
    Cleanup,
    CreatedOpen,
    ClosedUnpublished,
    BeforePublish,
    PublishedFinal,
    Collision,
    PublishFailed,
    Expired,
    CandidateCleanup,
    Constructor,
    ConstructorDisposed,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Fault {
    Lock,
    Write,
    Sync,
    Rename,
    Cleanup,
    Publish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Delay {
    None,
    Lock,
    #[cfg(windows)]
    BeforePublish,
    #[cfg(windows)]
    AfterCreate,
    #[cfg(windows)]
    AfterPublish,
}

#[derive(Debug)]
struct Injected(Fault);
impl std::fmt::Display for Injected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("controlled token qualification fault")
    }
}
impl std::error::Error for Injected {}

fn injected(fault: Fault) -> io::Error {
    let kind = match fault {
        Fault::Write => ErrorKind::WriteZero,
        Fault::Rename | Fault::Publish => ErrorKind::PermissionDenied,
        Fault::Cleanup => ErrorKind::Interrupted,
        Fault::Lock | Fault::Sync => ErrorKind::Other,
    };
    io::Error::new(kind, Injected(fault))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Config {
    pub root: PathBuf,
    pub address: SocketAddr,
    pub nonce: String,
    pub operation: Operation,
    pub gates: Vec<Event>,
    pub fault: Option<Fault>,
    pub cleanup_fault: bool,
    pub delay: Delay,
    pub scratch_suffix: Option<String>,
    #[cfg(windows)]
    pub constructor: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Stats {
    pub entered: u32,
    pub attempts: u32,
    pub sleeps: u32,
    pub busy: u32,
    pub acquired: u32,
    pub token_io: u32,
    pub decision: u32,
    pub scratch: u32,
    pub writes: u32,
    pub syncs: u32,
    pub closed: u32,
    pub renames: u32,
    pub cleanup: u32,
    pub cleanup_raw: Option<i32>,
    pub cleanup_fault: Option<Fault>,
    pub publish: u32,
    pub candidate_attempts: u32,
    pub candidate_created: u32,
    pub candidate_closed: u32,
    pub candidate_cleanup: u32,
    pub collision: u32,
    pub published: u32,
    pub expired: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Frame {
    pub event: Event,
    pub sequence: u32,
    pub pid: u32,
    pub native_pid: Option<u32>,
    pub nonce: String,
    pub stats: Stats,
    pub ok: bool,
    pub kind: String,
    pub raw: Option<i32>,
    pub fault: Option<Fault>,
    pub digest: String,
    pub length: usize,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Instruction {
    Go,
    Release,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(io::Error::new(
            ErrorKind::TimedOut,
            "token fixture clock elapsed",
        ))
    } else {
        Ok(remaining)
    }
}

fn send<T: Serialize>(stream: &mut TcpStream, value: &T, deadline: Instant) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() >= FRAME_LIMIT {
        return Err(io::Error::other("token fixture frame exceeds bound"));
    }
    bytes.push(b'\n');
    let mut offset = 0;
    while offset < bytes.len() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    ErrorKind::WriteZero,
                    "token fixture peer closed",
                ));
            }
            Ok(count) => offset += count,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error),
        }
    }
    remaining(deadline)?;
    Ok(())
}

fn receive<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Instant,
) -> io::Result<T> {
    let mut bytes = Vec::new();
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let mut byte = [0];
        match stream.read(&mut byte)? {
            0 => {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "token fixture peer closed",
                ));
            }
            _ if byte[0] == b'\n' => break,
            _ => bytes.push(byte[0]),
        }
        if bytes.len() >= FRAME_LIMIT {
            return Err(io::Error::other("token fixture frame exceeds bound"));
        }
    }
    remaining(deadline)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

struct Control {
    config: Config,
    stream: TcpStream,
    protocol: Instant,
    production: Option<Instant>,
    sequence: u32,
    stats: Stats,
}

thread_local! {
    static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) };
}

struct Scope(Option<Control>);
impl Drop for Scope {
    fn drop(&mut self) {
        CONTROL.with(|cell| *cell.borrow_mut() = self.0.take());
    }
}

fn root_matches(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        let a = a.to_string_lossy();
        let b = b.to_string_lossy();
        a.trim_start_matches(r"\\?\") == b.trim_start_matches(r"\\?\")
    }
    #[cfg(not(windows))]
    false
}

fn controlled<T>(root: &Path, f: impl FnOnce(&mut Control) -> T) -> Option<T> {
    CONTROL.with(|cell| {
        let mut value = cell.borrow_mut();
        value
            .as_mut()
            .filter(|value| root_matches(root, &value.config.root))
            .map(f)
    })
}

impl Control {
    fn frame(&self, event: Event) -> Frame {
        Frame {
            event,
            sequence: self.sequence,
            pid: std::process::id(),
            native_pid: None,
            nonce: self.config.nonce.clone(),
            stats: self.stats.clone(),
            ok: false,
            kind: String::new(),
            raw: None,
            fault: None,
            digest: String::new(),
            length: 0,
            elapsed_ms: self
                .production
                .map_or(0, |origin| origin.elapsed().as_millis() as u64),
        }
    }

    fn emit(&mut self, event: Event) {
        self.sequence += 1;
        assert!(
            self.sequence <= EVENT_LIMIT,
            "token fixture event count exceeds bound"
        );
        let frame = self.frame(event);
        send(&mut self.stream, &frame, self.protocol).expect("token fixture bounded event send");
        if self.config.gates.contains(&event) {
            let instruction: Instruction =
                receive(&mut self.stream, self.protocol).expect("token fixture bounded release");
            assert!(
                matches!(instruction, Instruction::Release),
                "token fixture expected release"
            );
        }
    }

    fn take_fault(&mut self, fault: Fault) -> bool {
        if self.config.fault == Some(fault) {
            self.config.fault = None;
            true
        } else {
            false
        }
    }
}

pub(super) fn begin(root: &Path, deadline: Instant) {
    controlled(root, |control| {
        control.production = Some(
            deadline
                .checked_sub(super::TOKEN_LOCK_TIMEOUT)
                .expect("fixed token fixture production origin"),
        );
        control.stats.entered += 1;
        control.emit(Event::Begin);
    });
}

pub(super) fn observe(root: &Path, event: Event) {
    controlled(root, |control| {
        match event {
            Event::Busy => {
                control.stats.busy += 1;
                if control.stats.busy != 1 {
                    return;
                }
            }
            Event::Sleep => {
                control.stats.sleeps += 1;
                return;
            }
            Event::Acquired => control.stats.acquired += 1,
            Event::TokenIo => control.stats.token_io += 1,
            Event::Decision => control.stats.decision += 1,
            Event::ScratchCreated => control.stats.scratch += 1,
            Event::Write => control.stats.writes += 1,
            Event::Sync => control.stats.syncs += 1,
            Event::LeafClosed => control.stats.closed += 1,
            Event::Rename => control.stats.renames += 1,
            Event::Cleanup => control.stats.cleanup += 1,
            Event::CandidateCleanup => control.stats.candidate_cleanup += 1,
            Event::CreatedOpen => control.stats.candidate_created += 1,
            Event::ClosedUnpublished => control.stats.candidate_closed += 1,
            Event::Collision => control.stats.collision += 1,
            Event::PublishedFinal => control.stats.published += 1,
            Event::Expired => control.stats.expired += 1,
            _ => {}
        }
        control.emit(event);
    });
}

pub(super) fn try_lock(
    root: &Path,
    file: &GuardedFile,
    deadline: Instant,
) -> Result<(), fs::TryLockError> {
    let fault = controlled(root, |control| {
        control.stats.attempts += 1;
        control.take_fault(Fault::Lock)
    })
    .unwrap_or(false);
    if fault {
        return Err(fs::TryLockError::Error(injected(Fault::Lock)));
    }
    let result = file.try_lock();
    if result.is_ok()
        && controlled(root, |control| control.config.delay == Delay::Lock).unwrap_or(false)
    {
        wait_late(deadline);
    }
    result
}

fn wait_late(deadline: Instant) {
    let target = deadline + Duration::from_millis(25);
    while Instant::now() < target {
        std::thread::sleep(
            target
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(10)),
        );
    }
}

pub(super) fn scratch_suffix(root: &Path) -> Option<String> {
    controlled(root, |control| control.config.scratch_suffix.clone()).flatten()
}

pub(super) fn write_sync(root: &Path, file: &mut GuardedFile, bytes: &[u8]) -> io::Result<()> {
    observe(root, Event::Write);
    if controlled(root, |control| control.take_fault(Fault::Write)).unwrap_or(false) {
        file.write_all(&bytes[..16])?;
        return Err(injected(Fault::Write));
    }
    file.write_all(bytes)?;
    observe(root, Event::Sync);
    if controlled(root, |control| control.take_fault(Fault::Sync)).unwrap_or(false) {
        return Err(injected(Fault::Sync));
    }
    file.sync_all()
}

pub(super) fn rename(root: &Path, from: &Path, to: &Path) -> io::Result<()> {
    observe(root, Event::Rename);
    if controlled(root, |control| control.take_fault(Fault::Rename)).unwrap_or(false) {
        return Err(injected(Fault::Rename));
    }
    locron_core::filesystem::rename_private(from, to)
}

fn cleanup(root: &Path, path: &Path, candidate: bool) -> io::Result<()> {
    observe(root, Event::BeforeCleanup);
    let fault = controlled(root, |control| {
        let fault = control.config.cleanup_fault;
        control.config.cleanup_fault = false;
        fault
    })
    .unwrap_or(false);
    let result = if fault {
        Err(injected(Fault::Cleanup))
    } else {
        locron_core::filesystem::remove_private_file(path)
    };
    controlled(root, |control| {
        control.stats.cleanup_raw = result.as_ref().err().and_then(io::Error::raw_os_error);
        control.stats.cleanup_fault = result
            .as_ref()
            .err()
            .and_then(|error| error.get_ref())
            .and_then(|error| error.downcast_ref::<Injected>())
            .map(|error| error.0);
    });
    observe(
        root,
        if candidate {
            Event::CandidateCleanup
        } else {
            Event::Cleanup
        },
    );
    result
}

pub(super) fn scratch_cleanup(root: &Path, path: &Path) -> io::Result<()> {
    cleanup(root, path, false)
}

#[cfg(windows)]
pub(super) fn candidate_cleanup(root: &Path, path: &Path, deadline: Instant) -> io::Result<()> {
    // The gate is before native admission, so a delayed test gate cannot renew the clock.
    observe(root, Event::BeforeCleanup);
    super::lock_remaining(deadline)?;
    let fault = controlled(root, |control| {
        let fault = control.config.cleanup_fault;
        control.config.cleanup_fault = false;
        fault
    })
    .unwrap_or(false);
    let result = if fault {
        Err(injected(Fault::Cleanup))
    } else {
        locron_core::filesystem::remove_private_file(path)
    };
    controlled(root, |control| {
        control.stats.cleanup_raw = result.as_ref().err().and_then(io::Error::raw_os_error);
        control.stats.cleanup_fault = result
            .as_ref()
            .err()
            .and_then(|error| error.get_ref())
            .and_then(|error| error.downcast_ref::<Injected>())
            .map(|error| error.0);
    });
    observe(root, Event::CandidateCleanup);
    result
}

#[cfg(windows)]
pub(super) fn candidate_id(root: &Path) -> Option<uuid::Uuid> {
    controlled(root, |control| {
        uuid::Uuid::parse_str(&control.config.nonce).expect("fixture nonce UUID")
    })
}

#[cfg(windows)]
pub(super) fn before_publish(root: &Path, deadline: Instant) {
    observe(root, Event::BeforePublish);
    if controlled(root, |control| control.config.delay == Delay::BeforePublish).unwrap_or(false) {
        wait_late(deadline);
    }
}

#[cfg(windows)]
pub(super) fn publish(
    root: &Path,
    candidate: tempfile::TempPath,
    final_path: &Path,
    deadline: Instant,
) -> Result<(), tempfile::PathPersistError> {
    controlled(root, |control| control.stats.publish += 1);
    if controlled(root, |control| control.take_fault(Fault::Publish)).unwrap_or(false) {
        return Err(tempfile::PathPersistError {
            error: injected(Fault::Publish),
            path: candidate,
        });
    }
    let result = candidate.persist_noclobber(final_path);
    if result.is_ok()
        && controlled(root, |control| control.config.delay == Delay::AfterPublish).unwrap_or(false)
    {
        wait_late(deadline);
    }
    result
}

#[cfg(windows)]
pub(super) fn create_candidate(
    root: &Path,
    path: &Path,
    deadline: Instant,
) -> io::Result<GuardedFile> {
    controlled(root, |control| control.stats.candidate_attempts += 1);
    let file = if controlled(root, |control| control.config.constructor).unwrap_or(false) {
        constructor(root, path, deadline)?;
        locron_core::filesystem::open_private(path, fs::OpenOptions::new().read(true).write(true))
    } else {
        locron_core::filesystem::create_private_new(path)
    }?;
    if controlled(root, |control| control.config.delay == Delay::AfterCreate).unwrap_or(false) {
        wait_late(deadline);
    }
    Ok(file)
}

#[cfg(windows)]
pub(super) fn constructor(root: &Path, path: &Path, deadline: Instant) -> io::Result<()> {
    let (address, nonce) = controlled(root, |control| {
        (control.config.address, control.config.nonce.clone())
    })
    .expect("native constructor requires a scoped fixture");
    let result = locron_core::windows::run_script_json_until(
        include_str!("constructor_fixture.ps1"),
        &serde_json::json!({"path": path, "address": address.ip().to_string(), "port": address.port(),
            "nonce": nonce, "remaining_ms": remaining(deadline)?.as_millis() as u64}),
        deadline,
    )?;
    if result.get("disposed").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(io::Error::other("native constructor disposal unconfirmed"));
    }
    let pid = result
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid != 0)
        .ok_or_else(|| io::Error::other("native constructor identity unobserved"))?;
    controlled(root, |control| {
        control.sequence += 1;
        assert!(
            control.sequence <= EVENT_LIMIT,
            "fixture event count exceeds bound"
        );
        let mut frame = control.frame(Event::ConstructorDisposed);
        frame.native_pid = Some(pid);
        send(&mut control.stream, &frame, control.protocol)
            .expect("bounded constructor completion");
    });
    Ok(())
}

pub(super) fn fixture(config: &Config) {
    let ready = Instant::now() + READY;
    let mut stream = TcpStream::connect_timeout(
        &config.address,
        remaining(ready).expect("fixture ready clock"),
    )
    .expect("fixture loopback connect");
    let control = Control {
        config: config.clone(),
        stream: stream.try_clone().expect("fixture control duplicate"),
        protocol: ready,
        production: None,
        sequence: 0,
        stats: Stats::default(),
    };
    let mut frame = control.frame(Event::Ready);
    frame.sequence = 1;
    send(&mut stream, &frame, ready).expect("fixture ready send");
    let instruction: Instruction = receive(&mut stream, ready).expect("fixture GO");
    assert!(
        matches!(instruction, Instruction::Go),
        "fixture expected GO"
    );
    let previous = CONTROL.with(|cell| {
        cell.borrow_mut().replace(Control {
            protocol: Instant::now() + PROTOCOL,
            sequence: 1,
            ..control
        })
    });
    let _scope = Scope(previous);
    let paths = locron_store::StatePaths::new(config.root.clone());
    let result: io::Result<Option<String>> = match config.operation {
        Operation::Ensure => super::ensure(&paths).map(Some),
        Operation::Regenerate => super::regenerate(&paths).map(Some),
        Operation::Remove => super::remove(&paths).map(|()| None),
        Operation::Hold => super::lock_token(&paths, true).map(|_lock| None),
        #[cfg(windows)]
        Operation::ConstructorFinal => {
            let deadline = Instant::now() + super::TOKEN_LOCK_TIMEOUT;
            begin(&config.root, deadline);
            constructor(
                &config.root,
                &config.root.join(super::TOKEN_LOCK_FILE_NAME),
                deadline,
            )
            .map(|()| None)
        }
    };
    controlled(&config.root, |control| {
        control.sequence += 1;
        assert!(
            control.sequence <= EVENT_LIMIT,
            "fixture event count exceeds bound"
        );
        let mut frame = control.frame(Event::Done);
        match result {
            Ok(token) => {
                frame.ok = true;
                if let Some(token) = token {
                    frame.length = token.len();
                    frame.digest = digest(token.as_bytes());
                }
            }
            Err(error) => {
                // Check actual Display and Debug, but print neither secret nor error on failure.
                for rendered in [error.to_string(), format!("{error:?}")] {
                    assert!(
                        !rendered
                            .as_bytes()
                            .windows(SEED.len())
                            .any(|part| part == SEED),
                        "error exposed token canary"
                    );
                    assert!(
                        !rendered
                            .as_bytes()
                            .windows(CANARY.len())
                            .any(|part| part == CANARY),
                        "error exposed corrupt canary"
                    );
                }
                frame.kind = format!("{:?}", error.kind());
                assert!(
                    frame.kind.len() <= 32 && frame.kind.is_ascii(),
                    "fixture error category exceeds bound"
                );
                frame.raw = error.raw_os_error();
                frame.fault = error
                    .get_ref()
                    .and_then(|error| error.downcast_ref::<Injected>())
                    .map(|error| error.0);
            }
        }
        send(&mut control.stream, &frame, control.protocol).expect("fixture done send");
    });
}

struct Channel {
    stream: TcpStream,
    bytes: Vec<u8>,
    next_sequence: u32,
    read_calls: Option<u64>,
}

#[derive(Clone, Copy)]
struct ReadSnapshot {
    next_sequence: u32,
    partial_bytes: usize,
    read_calls: Option<u64>,
}

impl Channel {
    fn new(stream: TcpStream) -> Self {
        stream
            .set_nonblocking(true)
            .expect("fixture nonblocking peer");
        Self {
            stream,
            bytes: Vec::new(),
            next_sequence: 1,
            read_calls: Some(0),
        }
    }

    fn poll(&mut self, deadline: Instant) -> io::Result<Option<Frame>> {
        loop {
            remaining(deadline).expect("fixture absolute read clock");
            if let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
                let frame: Frame =
                    serde_json::from_slice(&self.bytes[..end]).expect("bounded fixture frame");
                self.bytes.drain(..=end);
                assert_eq!(
                    frame.sequence, self.next_sequence,
                    "fixture sequence mismatch"
                );
                self.next_sequence += 1;
                assert!(
                    frame.sequence <= EVENT_LIMIT,
                    "fixture event count exceeds bound"
                );
                return Ok(Some(frame));
            }
            let mut byte = [0];
            // Overflow leaves observation unknown without changing protocol or work.
            self.read_calls = self.read_calls.and_then(|calls| calls.checked_add(1));
            match self.stream.read(&mut byte) {
                Ok(0) => return Ok(None),
                Ok(_) => self.bytes.push(byte[0]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) => return Err(error),
            }
            assert!(
                self.bytes.len() <= FRAME_LIMIT,
                "fixture frame exceeds bound"
            );
        }
    }

    fn read_snapshot(&self) -> ReadSnapshot {
        ReadSnapshot {
            next_sequence: self.next_sequence,
            partial_bytes: self.bytes.len(),
            read_calls: self.read_calls,
        }
    }

    fn instruction(&mut self, instruction: Instruction, deadline: Instant) {
        send(&mut self.stream, &instruction, deadline).expect("bounded fixture command");
    }
}

#[derive(Clone, Copy)]
enum Peer {
    Active,
    Done {
        sequence: u32,
        reads: Option<u64>,
    },
    Stopped {
        status: ExitStatus,
        reads: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalReadReceipt {
    pid: u32,
    sequence: Option<u32>,
    reads: u64,
}

struct Owned {
    child: Child,
    stdout: NamedTempFile,
    stderr: NamedTempFile,
    nonce: String,
    ready: Instant,
    channel: Option<Channel>,
    native: Option<Channel>,
    native_pid: Option<u32>,
    frames: Vec<Frame>,
    status: Option<ExitStatus>,
    peer: Peer,
    operation: Operation,
    fault: Option<Fault>,
    cleanup_fault: bool,
    delay: Delay,
}

fn panic_locations(text: &str) -> Option<String> {
    let mut locations = Vec::new();
    for line in text.lines() {
        let Some((_, location)) = line.split_once("panicked at ") else {
            continue;
        };
        let location = location.replace('\\', "/");
        let location = location.strip_prefix("crates/locron-server/src/")?;
        let (module, numbers) = location.split_once(':')?;
        let module = match module {
            "token.rs" => "token.rs",
            "token/test_support.rs" => "token/test_support.rs",
            "token/qualification.rs" => "token/qualification.rs",
            _ => return None,
        };
        let mut numbers = numbers.split(':');
        let line = numbers.next()?;
        let column = numbers.next()?;
        if numbers.next()? != "" || numbers.next().is_some() {
            return None;
        }
        let positive = |value: &str| {
            (!value.is_empty()
                && value.len() <= 10
                && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u32>().ok())
            .flatten()
            .filter(|value| *value != 0)
        };
        locations.push(format!(
            "{module}:{}:{}",
            positive(line)?,
            positive(column)?
        ));
        if locations.len() > 2 {
            return None;
        }
    }
    if locations.is_empty() {
        None
    } else {
        Some(locations.join(","))
    }
}

fn capture_observation(capture: &NamedTempFile, deadline: Instant) -> Option<String> {
    remaining(deadline).ok()?;
    let reader = capture.as_file().try_clone();
    remaining(deadline).ok()?;
    let mut reader = reader.ok()?;
    remaining(deadline).ok()?;
    let position = reader.seek(SeekFrom::Start(0));
    remaining(deadline).ok()?;
    if position.ok()? != 0 {
        return None;
    }
    let mut bytes = Vec::new();
    remaining(deadline).ok()?;
    let read = reader.take(CAPTURE_LIMIT + 1).read_to_end(&mut bytes);
    remaining(deadline).ok()?;
    read.ok()?;
    if bytes.len() as u64 > CAPTURE_LIMIT {
        return None;
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    let locations = panic_locations(text).unwrap_or_else(|| "unobserved".to_owned());
    let observation = format!(
        "length={} sha256={} locations={locations}",
        bytes.len(),
        digest(&bytes)
    );
    remaining(deadline).ok()?;
    Some(observation)
}

pub(super) struct Harness {
    // Children precede the root owner. Explicit cleanup/reap also runs on failure via Drop.
    children: Vec<Owned>,
    listener: TcpListener,
    pending: Vec<Channel>,
    pub paths: locron_store::StatePaths,
    directory: Option<TempDir>,
    go: Option<Instant>,
}

impl Harness {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("disposable token root");
        let listener = TcpListener::bind("127.0.0.1:0").expect("fixture loopback listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture accept");
        Self {
            children: Vec::new(),
            pending: Vec::new(),
            listener,
            paths: locron_store::StatePaths::new(directory.path().join("private")),
            directory: Some(directory),
            go: None,
        }
    }

    pub fn config(&self, operation: Operation) -> Config {
        Config {
            root: self.paths.root.clone(),
            address: self.listener.local_addr().expect("fixture endpoint"),
            nonce: uuid::Uuid::now_v7().to_string(),
            operation,
            gates: Vec::new(),
            fault: None,
            cleanup_fault: false,
            delay: Delay::None,
            scratch_suffix: None,
            #[cfg(windows)]
            constructor: false,
        }
    }

    pub fn spawn(&mut self, config: Config) -> usize {
        assert!(
            self.children.len() < 5,
            "fixture child count exceeds selected bound"
        );
        let stdout = NamedTempFile::new().expect("fixture stdout capture");
        let stderr = NamedTempFile::new().expect("fixture stderr capture");
        let child = Command::new(std::env::current_exe().expect("fixture current binary"))
            .args([
                "--exact",
                "token::qualification::fixture_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(
                FIXTURE_ENV,
                serde_json::to_string(&config).expect("fixture config"),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                stdout
                    .as_file()
                    .try_clone()
                    .expect("fixture stdout duplicate"),
            ))
            .stderr(Stdio::from(
                stderr
                    .as_file()
                    .try_clone()
                    .expect("fixture stderr duplicate"),
            ))
            .spawn()
            .expect("spawn owned token fixture");
        // No fallible work after spawn precedes storing the actual Child owner.
        self.children.push(Owned {
            child,
            stdout,
            stderr,
            nonce: config.nonce,
            ready: Instant::now() + READY,
            channel: None,
            native: None,
            native_pid: None,
            frames: Vec::new(),
            status: None,
            peer: Peer::Active,
            operation: config.operation,
            fault: config.fault,
            cleanup_fault: config.cleanup_fault,
            delay: config.delay,
        });
        self.children.len() - 1
    }

    fn read_failure(
        &mut self,
        owner_index: Option<usize>,
        snapshot: ReadSnapshot,
        deadline: Instant,
        error: &io::Error,
    ) -> ! {
        let primary = format!(
            "fixture control read failed: kind={:?} raw={:?}",
            error.kind(),
            error.raw_os_error()
        );
        if self.children.len() > 5 {
            panic!("{primary}; diagnostic=unobserved:owner-bound");
        }
        let role = if owner_index.is_some() {
            "child"
        } else {
            "pending-unassigned"
        };
        let contexts: Vec<_> = self
            .children
            .iter()
            .enumerate()
            .map(|(index, owner)| {
                let peer = match owner.peer {
                    Peer::Active => "Active",
                    Peer::Done { .. } => "Done",
                    Peer::Stopped { .. } => "Stopped",
                };
                let last = owner.frames.last().map_or_else(
                    || "unobserved".to_owned(),
                    |frame| {
                        format!(
                            "{:?}/sequence={}/pid={}",
                            frame.event, frame.sequence, frame.pid
                        )
                    },
                );
                format!(
                    "owner={index} pid={} peer={peer} last={last} operation={:?} fault={:?} cleanup_fault={} delay={:?}",
                    owner.child.id(), owner.operation, owner.fault, owner.cleanup_fault, owner.delay
                )
            })
            .collect();
        let mut observations = vec![None; self.children.len()];
        let mut waiting: Vec<_> = self
            .children
            .iter()
            .map(|owner| !matches!(owner.peer, Peer::Stopped { .. }))
            .collect();
        // Only actual pre-kill TryWait, using the failed poll's unchanged absolute clock.
        loop {
            for (index, owner) in self.children.iter_mut().enumerate() {
                if !waiting[index] {
                    continue;
                }
                if remaining(deadline).is_err() {
                    break;
                }
                let status = owner.child.try_wait();
                let in_time = remaining(deadline).is_ok();
                match status {
                    Ok(Some(status)) => {
                        // Even a late actual reap belongs to cleanup, never to the diagnostic.
                        owner.status = Some(status);
                        waiting[index] = false;
                        if in_time {
                            let stdout = capture_observation(&owner.stdout, deadline)
                                .unwrap_or_else(|| "unobserved".to_owned());
                            let stderr = capture_observation(&owner.stderr, deadline)
                                .unwrap_or_else(|| "unobserved".to_owned());
                            observations[index] = Some(format!(
                                "prekill=exit success={} code={:?} stdout=[{stdout}] stderr=[{stderr}]",
                                status.success(),
                                status.code()
                            ));
                        }
                    }
                    Ok(None) => {}
                    Err(observation_error) => {
                        waiting[index] = false;
                        if in_time {
                            observations[index] = Some(format!(
                                "prekill=unobserved wait-kind={:?} wait-raw={:?} stdout=unobserved stderr=unobserved",
                                observation_error.kind(),
                                observation_error.raw_os_error()
                            ));
                        }
                    }
                }
                if !in_time {
                    break;
                }
            }
            if waiting.iter().all(|waiting| !waiting) {
                break;
            }
            let Ok(left) = remaining(deadline) else {
                break;
            };
            std::thread::sleep(left.min(Duration::from_millis(1)));
        }
        let owners: Vec<_> = contexts
            .into_iter()
            .zip(observations)
            .map(|(context, observation)| {
                format!(
                    "[{context} {}]",
                    observation.unwrap_or_else(|| {
                        "prekill=unobserved stdout=unobserved stderr=unobserved".to_owned()
                    })
                )
            })
            .collect();
        let report = format!(
            "{primary}; role={role} assigned={owner_index:?} next_sequence={} partial_bytes={} read_calls={:?}; {}",
            snapshot.next_sequence,
            snapshot.partial_bytes,
            snapshot.read_calls,
            owners.join("; ")
        );
        // Reject the entire report, rather than truncate evidence or replace the primary error.
        if report.is_ascii() && report.len() <= 4096 {
            panic!("{report}");
        }
        panic!("{primary}; diagnostic=unobserved:report-bound");
    }

    fn poll(&mut self) {
        let deadline = self.go.map_or_else(
            || {
                self.children
                    .iter()
                    .filter(|owner| owner.channel.is_none())
                    .map(|owner| owner.ready)
                    .min()
                    .expect("fixture readiness owner")
            },
            |origin| origin + PROTOCOL,
        );
        loop {
            remaining(deadline).expect("fixture absolute accept clock");
            match self.listener.accept() {
                Ok((stream, address)) => {
                    assert!(address.ip().is_loopback(), "fixture peer is not loopback");
                    assert!(
                        self.pending.len() < 10,
                        "fixture pending peer count exceeds bound"
                    );
                    self.pending.push(Channel::new(stream));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => panic!(
                    "fixture accept failed: kind={:?} raw={:?}",
                    error.kind(),
                    error.raw_os_error()
                ),
            }
        }
        let mut index = 0;
        while index < self.pending.len() {
            let frame = match self.pending[index].poll(deadline) {
                Ok(frame) => frame,
                Err(error) => {
                    let snapshot = self.pending[index].read_snapshot();
                    self.read_failure(None, snapshot, deadline, &error);
                }
            };
            if let Some(frame) = frame {
                let owner = self
                    .children
                    .iter_mut()
                    .find(|owner| owner.nonce == frame.nonce)
                    .expect("fixture nonce mismatch");
                let channel = self.pending.remove(index);
                if frame.event == Event::Ready {
                    assert_eq!(
                        frame.pid,
                        owner.child.id(),
                        "fixture actual Child PID mismatch"
                    );
                    assert!(owner.channel.is_none(), "duplicate fixture handshake");
                    owner.channel = Some(channel);
                } else {
                    assert_eq!(
                        frame.event,
                        Event::Constructor,
                        "unexpected native handshake"
                    );
                    assert!(
                        frame.pid != 0 && owner.native.is_none(),
                        "invalid native constructor handshake"
                    );
                    owner.native = Some(channel);
                    owner.native_pid = Some(frame.pid);
                }
                assert!(
                    owner.frames.len() < EVENT_LIMIT as usize,
                    "fixture event count exceeds bound"
                );
                owner.frames.push(frame);
            } else {
                index += 1;
            }
        }
        let mut failed = None;
        for (owner_index, owner) in self.children.iter_mut().enumerate() {
            if !matches!(owner.peer, Peer::Active) {
                continue;
            }
            if let Some(channel) = &mut owner.channel {
                loop {
                    let frame = match channel.poll(deadline) {
                        Ok(Some(frame)) => frame,
                        Ok(None) => break,
                        Err(error) => {
                            failed = Some((owner_index, channel.read_snapshot(), error));
                            break;
                        }
                    };
                    assert_eq!(
                        frame.pid,
                        owner.child.id(),
                        "fixture actual Child PID changed"
                    );
                    assert_eq!(frame.nonce, owner.nonce, "fixture nonce changed");
                    if frame.event == Event::ConstructorDisposed {
                        assert_eq!(
                            frame.native_pid, owner.native_pid,
                            "native constructor completion PID changed"
                        );
                    }
                    assert!(
                        owner.frames.len() < EVENT_LIMIT as usize,
                        "fixture event count exceeds bound"
                    );
                    let terminal = (frame.event == Event::Done).then_some(frame.sequence);
                    owner.frames.push(frame);
                    if let Some(sequence) = terminal {
                        owner.peer = Peer::Done {
                            sequence,
                            reads: channel.read_calls,
                        };
                        break;
                    }
                }
            }
            if failed.is_some() {
                break;
            }
        }
        if let Some((owner_index, snapshot, error)) = failed {
            self.read_failure(Some(owner_index), snapshot, deadline, &error);
        }
    }

    pub fn ready_all(&mut self) {
        loop {
            self.poll();
            if self.children.iter().all(|owner| owner.channel.is_some()) {
                return;
            }
            assert!(
                self.children
                    .iter()
                    .filter(|owner| owner.channel.is_none())
                    .all(|owner| Instant::now() < owner.ready),
                "fixture Ready clock elapsed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn go(&mut self, indices: &[usize]) {
        let origin = *self.go.get_or_insert_with(Instant::now);
        for index in indices {
            self.children[*index]
                .channel
                .as_mut()
                .expect("ready fixture")
                .instruction(Instruction::Go, origin + PROTOCOL);
        }
    }

    pub fn deadline(&self) -> Instant {
        self.go.expect("fixture GO origin") + PROTOCOL
    }
    pub fn interleave(&self) -> Instant {
        self.go.expect("fixture GO origin") + INTERLEAVE
    }

    pub fn wait(&mut self, index: usize, event: Event, deadline: Instant) -> Frame {
        loop {
            self.poll();
            if let Some(frame) = self.children[index]
                .frames
                .iter()
                .find(|frame| frame.event == event)
            {
                return frame.clone();
            }
            assert!(
                Instant::now() < deadline,
                "fixture expected event clock elapsed"
            );
            assert!(
                !self.children[index]
                    .frames
                    .iter()
                    .any(|frame| frame.event == Event::Done),
                "fixture completed before expected event"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn wait_any(&mut self, event: Event, deadline: Instant) -> usize {
        loop {
            self.poll();
            if let Some(index) = self
                .children
                .iter()
                .position(|owner| owner.frames.iter().any(|frame| frame.event == event))
            {
                return index;
            }
            assert!(
                Instant::now() < deadline,
                "fixture expected winner clock elapsed"
            );
            assert!(
                !self
                    .children
                    .iter()
                    .any(|owner| owner.frames.iter().any(|frame| frame.event == Event::Done)),
                "fixture completed before the controlled decision"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn release(&mut self, index: usize) {
        let deadline = self.deadline();
        self.children[index]
            .channel
            .as_mut()
            .expect("fixture release channel")
            .instruction(Instruction::Release, deadline);
    }

    #[cfg(windows)]
    pub fn release_constructor(&mut self, index: usize) {
        let deadline = self.deadline();
        self.children[index]
            .native
            .as_mut()
            .expect("actual native constructor")
            .instruction(Instruction::Release, deadline);
    }

    pub fn done(&mut self, index: usize) -> Frame {
        self.wait(index, Event::Done, self.deadline())
    }
    pub fn live(&mut self, index: usize) {
        assert!(
            self.children[index]
                .child
                .try_wait()
                .expect("actual fixture status")
                .is_none(),
            "expected live fixture owner"
        );
    }
    pub fn killed(&mut self, index: usize) {
        self.live(index);
        let deadline = Instant::now() + CLEANUP;
        assert!(
            matches!(self.children[index].peer, Peer::Active),
            "fixture terminal peer cannot be killed again"
        );
        self.children[index]
            .child
            .kill()
            .expect("kill actual retained fixture Child");
        loop {
            if let Some(status) = self.children[index]
                .child
                .try_wait()
                .expect("reap killed fixture")
            {
                assert!(!status.success(), "killed fixture reported success");
                self.children[index].status = Some(status);
                assert!(
                    Instant::now() < deadline,
                    "killed fixture reap returned late"
                );
                self.children[index].peer = Peer::Stopped {
                    status,
                    reads: self.children[index]
                        .channel
                        .as_ref()
                        .expect("retained killed fixture channel")
                        .read_calls,
                };
                return;
            }
            assert!(
                Instant::now() < deadline,
                "killed fixture reap clock elapsed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn terminal_reads(&self, index: usize) -> TerminalReadReceipt {
        let owner = &self.children[index];
        let channel = owner.channel.as_ref().expect("retained terminal channel");
        let (sequence, frozen) = match owner.peer {
            Peer::Active => panic!("fixture peer is not terminal"),
            Peer::Done { sequence, reads } => {
                let frame = owner.frames.last().expect("stored terminal Done");
                assert_eq!(frame.event, Event::Done, "terminal frame is not Done");
                assert_eq!(frame.sequence, sequence, "terminal sequence changed");
                assert_eq!(frame.pid, owner.child.id(), "terminal Child PID changed");
                assert!(frame.nonce == owner.nonce, "terminal nonce changed");
                assert_eq!(
                    channel.next_sequence,
                    sequence + 1,
                    "terminal next sequence changed"
                );
                (Some(sequence), reads)
            }
            Peer::Stopped { status, reads } => {
                assert!(!status.success(), "stopped fixture reported success");
                assert!(owner.status == Some(status), "stopped reap status changed");
                assert!(
                    !owner.frames.iter().any(|frame| frame.event == Event::Done),
                    "stopped fixture fabricated Done"
                );
                (None, reads)
            }
        };
        assert!(
            channel.bytes.is_empty(),
            "terminal frame buffer is not empty"
        );
        let reads = frozen.expect("terminal read observation is unknown");
        assert_eq!(
            channel.read_calls,
            Some(reads),
            "terminal peer admitted another read"
        );
        TerminalReadReceipt {
            pid: owner.child.id(),
            sequence,
            reads,
        }
    }

    pub fn done_reads(&self, index: usize, frame: &Frame) -> TerminalReadReceipt {
        let receipt = self.terminal_reads(index);
        assert_eq!(frame.event, Event::Done, "expected actual Done receipt");
        assert_eq!(
            receipt.sequence,
            Some(frame.sequence),
            "Done receipt sequence changed"
        );
        assert_eq!(receipt.pid, frame.pid, "Done receipt Child PID changed");
        assert!(
            frame.nonce == self.children[index].nonce,
            "Done receipt nonce changed"
        );
        receipt
    }

    pub fn stopped_reads(&self, index: usize) -> TerminalReadReceipt {
        let receipt = self.terminal_reads(index);
        assert!(
            receipt.sequence.is_none(),
            "expected actual stopped receipt"
        );
        receipt
    }

    pub fn assert_reads_retained(&self, index: usize, receipt: TerminalReadReceipt) {
        assert_eq!(
            self.terminal_reads(index),
            receipt,
            "terminal Child read receipt changed"
        );
    }

    pub fn poll_after_terminal(&mut self, index: usize, receipt: TerminalReadReceipt) {
        self.assert_reads_retained(index, receipt);
        self.poll();
        self.assert_reads_retained(index, receipt);
    }

    pub fn finish(&mut self) {
        let deadline = self.deadline();
        loop {
            for owner in &mut self.children {
                if owner.status.is_none() {
                    owner.status = owner.child.try_wait().expect("actual normal fixture reap");
                }
            }
            if self.children.iter().all(|owner| owner.status.is_some()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fixture normal exit clock elapsed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let token_path = super::token_path(&self.paths);
        let actual_token = match fs::symlink_metadata(&token_path) {
            Ok(_) => {
                let mut file = locron_core::filesystem::open_private(
                    &token_path,
                    fs::OpenOptions::new().read(true),
                )
                .expect("actual guarded capture token oracle");
                let mut bytes = Vec::new();
                (&mut *file)
                    .take(super::TOKEN_FILE_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .expect("bounded capture token oracle");
                std::str::from_utf8(&bytes)
                    .ok()
                    .filter(|token| super::valid_token(token.trim()))
                    .map(|token| token.trim().as_bytes().to_vec())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => panic!(
                "capture token oracle state unobserved: kind={:?} raw={:?}",
                error.kind(),
                error.raw_os_error()
            ),
        };
        for owner in &self.children {
            // Deliberately killed owners have no Done; all normal owners must pass their entry.
            if owner.frames.iter().any(|frame| frame.event == Event::Done) {
                assert!(
                    owner.status.expect("reaped fixture").success(),
                    "normal fixture entry failed"
                );
            }
            for capture in [&owner.stdout, &owner.stderr] {
                let mut bytes = Vec::new();
                fs::File::open(capture.path())
                    .expect("fixture capture")
                    .take(CAPTURE_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .expect("bounded fixture capture");
                assert!(
                    bytes.len() as u64 <= CAPTURE_LIMIT,
                    "fixture capture exceeds bound"
                );
                assert!(
                    !bytes.windows(SEED.len()).any(|part| part == SEED),
                    "fixture output exposed token canary"
                );
                assert!(
                    !bytes.windows(CANARY.len()).any(|part| part == CANARY),
                    "fixture output exposed corrupt canary"
                );
                if let Some(token) = &actual_token {
                    assert!(
                        !bytes.windows(token.len()).any(|part| part == token),
                        "fixture output exposed actual persisted token"
                    );
                }
            }
        }
    }
    pub fn complete(mut self) {
        assert!(
            self.children.iter().all(|owner| owner.status.is_some()),
            "fixture teardown before confirmed reap"
        );
        let deadline = self.deadline();
        for owner in self.children.drain(..) {
            remaining(deadline).expect("fixture capture teardown clock");
            owner
                .stdout
                .close()
                .expect("checked fixture stdout teardown");
            remaining(deadline).expect("fixture capture teardown clock");
            owner
                .stderr
                .close()
                .expect("checked fixture stderr teardown");
            remaining(deadline).expect("fixture capture teardown clock");
        }
        remaining(deadline).expect("fixture root teardown clock");
        self.directory
            .take()
            .expect("owned private fixture root")
            .close()
            .expect("checked private fixture root teardown");
        remaining(deadline).expect("fixture root teardown clock");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let deadline = Instant::now() + CLEANUP;
        // Kill all first, then poll all actual owners under one common cleanup clock.
        for owner in &mut self.children {
            if owner.status.is_none() {
                let _ = owner.child.kill();
            }
        }
        loop {
            for owner in &mut self.children {
                if owner.status.is_none() {
                    owner.status = owner.child.try_wait().ok().flatten();
                }
            }
            if self.children.iter().all(|owner| owner.status.is_some()) {
                return;
            }
            if Instant::now() >= deadline {
                // Keep the private root/captures when native ownership is unconfirmed.
                self.directory
                    .as_mut()
                    .expect("retained private fixture root")
                    .disable_cleanup(true);
                for owner in &mut self.children {
                    owner.stdout.disable_cleanup(true);
                    owner.stderr.disable_cleanup(true);
                }
                eprintln!("token-qualification-cleanup-unconfirmed");
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
