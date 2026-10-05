//! Linux-only empirical RSS controls over directly owned real parser processes.

use std::fs::{File, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crc32fast::Hasher;
use locron_core::filesystem::{self, DirectoryGuard};

use super::{Channel, Frame, MAGIC, OutputStats, read_frames, repair_partial, scan_valid_frames};

const MODE_ENV: &str = "LOCRON_ENGINE_MEMORY_CASE";
const INPUT_ENV: &str = "LOCRON_ENGINE_MEMORY_INPUT";
const ADDRESS_ENV: &str = "LOCRON_ENGINE_MEMORY_ADDRESS";
const PAYLOAD: usize = 1_048_576;
const FRAME_CAP: usize = 1024;
const PROC_CAP: u64 = 32 * 1024;
const CAPTURE_CAP: u64 = 65_536;

#[derive(Clone, Copy)]
struct Clock(Instant);

impl Clock {
    fn before(seconds: u64) -> Self {
        Self(Instant::now() + Duration::from_secs(seconds))
    }

    fn inside(self, seconds: u64) -> Self {
        Self(self.0.min(Instant::now() + Duration::from_secs(seconds)))
    }

    fn check(self, phase: &str) {
        assert!(
            Instant::now() < self.0,
            "engine-memory phase={phase} deadline"
        );
    }

    fn require(self, phase: &str, condition: bool) {
        self.check(phase);
        assert!(condition, "engine-memory phase={phase}");
    }

    fn io<T>(self, phase: &str, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        self.check(phase);
        let result = operation();
        self.check(phase);
        result
    }

    fn need<T>(self, phase: &str, result: io::Result<T>) -> T {
        self.check(phase);
        result.unwrap_or_else(|error| {
            panic!(
                "engine-memory phase={phase} kind={:?} raw={:?}",
                error.kind(),
                error.raw_os_error()
            )
        })
    }

    fn remaining(self) -> Duration {
        self.check("remaining");
        self.0.saturating_duration_since(Instant::now())
    }
}

#[derive(Clone, Copy)]
enum Case {
    Repair4,
    Repair128,
    Collect128,
}

impl Case {
    fn key(self) -> &'static str {
        match self {
            Self::Repair4 => "M-REPAIR-004",
            Self::Repair128 => "M-REPAIR-128",
            Self::Collect128 => "M-COLLECT-128",
        }
    }

    fn frames(self) -> u64 {
        match self {
            Self::Repair4 => 4,
            Self::Repair128 | Self::Collect128 => 128,
        }
    }

    fn leaf(self) -> &'static str {
        match self {
            Self::Repair4 => "repair004.partial",
            Self::Repair128 => "repair128.partial",
            Self::Collect128 => "collect128.partial",
        }
    }

    fn collecting(self) -> bool {
        matches!(self, Self::Collect128)
    }

    fn prefix(self) -> u64 {
        8 + self.frames() * (25 + PAYLOAD as u64)
    }
}

struct Capture {
    file: Option<File>,
    path: Option<tempfile::TempPath>,
    identity: (u64, u64),
    validated: bool,
}

impl Capture {
    fn new(root: &Path, clock: Clock) -> Self {
        let named = clock.need(
            "capture-create",
            clock.io("capture-create", || tempfile::NamedTempFile::new_in(root)),
        );
        let (file, mut path) = named.into_parts();
        path.disable_cleanup(true);
        let metadata = clock.need(
            "capture-identity",
            clock.io("capture-identity", || file.metadata()),
        );
        Self {
            file: Some(file),
            path: Some(path),
            identity: (metadata.dev(), metadata.ino()),
            validated: false,
        }
    }

    fn writer(&self, clock: Clock) -> File {
        clock.need(
            "capture-clone",
            clock.io("capture-clone", || {
                self.file.as_ref().expect("owned capture").try_clone()
            }),
        )
    }

    fn validate_after_reap(&mut self, clock: Clock) {
        let mut reader = clock.need(
            "capture-reader",
            clock.io("capture-reader", || {
                self.file.as_ref().expect("owned capture").try_clone()
            }),
        );
        let metadata = clock.need(
            "capture-metadata",
            clock.io("capture-metadata", || reader.metadata()),
        );
        clock.require(
            "capture-same-object",
            metadata.is_file()
                && (metadata.dev(), metadata.ino()) == self.identity
                && metadata.len() <= CAPTURE_CAP,
        );
        clock.need(
            "capture-seek",
            clock.io("capture-seek", || reader.seek(SeekFrom::Start(0))),
        );
        let mut bytes = Vec::new();
        clock.need(
            "capture-read",
            clock.io("capture-read", || {
                (&mut reader).take(CAPTURE_CAP + 1).read_to_end(&mut bytes)
            }),
        );
        clock.require(
            "capture-length",
            bytes.len() as u64 == metadata.len() && bytes.len() as u64 <= CAPTURE_CAP,
        );
        self.validated = true;
    }

    fn close(&mut self, deadline: Instant) -> io::Result<()> {
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if let Some(file) = self.file.as_ref() {
            let metadata = file.metadata()?;
            if (metadata.dev(), metadata.ino()) != self.identity {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            drop(self.file.take());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if let Some(path) = self.path.take() {
            path.close()?;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        Ok(())
    }
}

struct OwnedChild {
    child: Option<Child>,
    status: Option<ExitStatus>,
    pid: u32,
    stdout: Capture,
    stderr: Capture,
}

impl OwnedChild {
    fn poll(&mut self, clock: Clock) -> Option<ExitStatus> {
        if let Some(child) = self.child.as_mut() {
            let result = clock.io("actual-try-wait", || child.try_wait());
            if let Some(status) = clock.need("actual-try-wait", result) {
                self.status = Some(status);
                self.child = None;
            }
        }
        self.status
    }

    fn require_live(&mut self, clock: Clock) {
        let status = self.poll(clock);
        clock.require(
            "actual-owned-live",
            status.is_none() && self.child.is_some(),
        );
    }

    fn hwm(&mut self, clock: Clock) -> u64 {
        self.require_live(clock);
        let path = format!("/proc/{}/status", self.pid);
        let file = clock.need("proc-open", clock.io("proc-open", || File::open(path)));
        let mut bytes = Vec::new();
        clock.need(
            "proc-read",
            clock.io("proc-read", || {
                file.take(PROC_CAP + 1).read_to_end(&mut bytes)
            }),
        );
        clock.require("proc-cap", bytes.len() as u64 <= PROC_CAP);
        let text = std::str::from_utf8(&bytes).expect("engine-memory phase=proc-utf8");
        let mut metrics = text.lines().filter(|line| line.starts_with("VmHWM:"));
        let mut words = metrics
            .next()
            .expect("engine-memory phase=proc-missing")
            .split_ascii_whitespace();
        clock.require("proc-label", words.next() == Some("VmHWM:"));
        let number = words.next().expect("engine-memory phase=proc-number");
        clock.require(
            "proc-digits",
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()),
        );
        let hwm = number
            .parse::<u64>()
            .expect("engine-memory phase=proc-overflow");
        clock.require(
            "proc-one-kib",
            words.next() == Some("kB")
                && words.next().is_none()
                && metrics.next().is_none()
                && hwm > 0,
        );
        self.require_live(clock);
        hwm
    }

    fn await_reap(&mut self, clock: Clock) {
        while self.poll(clock).is_none() {
            std::thread::yield_now();
        }
        let status = self.status.expect("actual reaped status");
        clock.require(
            "natural-success",
            status.success() && status.code() == Some(0),
        );
        self.stdout.validate_after_reap(clock);
        self.stderr.validate_after_reap(clock);
    }
}

struct Input {
    path: PathBuf,
    identity: (u64, u64),
    digest: u32,
    case: Case,
}

struct Harness {
    guard: Option<DirectoryGuard>,
    temporary: tempfile::TempDir,
    root: PathBuf,
    root_identity: (u64, u64),
    temporary_identity: (u64, u64),
    inputs: Vec<Input>,
    children: Vec<OwnedChild>,
    closed: bool,
    cleanup_deadline: Option<Instant>,
}

fn dir_id(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok((metadata.dev(), metadata.ino()))
}

impl Harness {
    fn new(clock: Clock) -> Self {
        let mut temporary = clock.need("temporary", clock.io("temporary", tempfile::tempdir));
        temporary.disable_cleanup(true);
        let guard = clock.need(
            "private-root",
            clock.io("private-root", || {
                DirectoryGuard::private(&temporary.path().join("private"))
            }),
        );
        let root = guard.normalized_path().to_path_buf();
        let root_identity = clock.need("root-id", clock.io("root-id", || dir_id(&root)));
        let temporary_identity = clock.need(
            "temporary-id",
            clock.io("temporary-id", || dir_id(temporary.path())),
        );
        Self {
            guard: Some(guard),
            temporary,
            root,
            root_identity,
            temporary_identity,
            inputs: Vec::new(),
            children: Vec::new(),
            closed: false,
            cleanup_deadline: None,
        }
    }

    fn prepare(&mut self, case: Case, clock: Clock) -> usize {
        let path = self.root.join(case.leaf());
        let mut file = clock.need(
            "input-create",
            clock.io("input-create", || filesystem::create_private_new(&path)),
        );
        let payload = vec![0xa5; PAYLOAD];
        let mut digest = Hasher::new();
        clock.need(
            "input-magic",
            clock.io("input-magic", || file.write_all(MAGIC)),
        );
        for sequence in 0..case.frames() {
            let header = golden_header(sequence, &payload);
            clock.need(
                "input-header",
                clock.io("input-header", || file.write_all(&header)),
            );
            clock.need(
                "input-payload",
                clock.io("input-payload", || file.write_all(&payload)),
            );
            digest.update(&payload);
            clock.check("input-digest");
        }
        clock.need(
            "input-tail",
            clock.io("input-tail", || file.write_all(&[1, 1, 0])),
        );
        clock.need("input-sync", clock.io("input-sync", || file.sync_all()));
        let metadata = clock.need("input-id", clock.io("input-id", || file.metadata()));
        clock.require("input-size", metadata.len() == case.prefix() + 3);
        self.inputs.push(Input {
            path,
            identity: (metadata.dev(), metadata.ino()),
            digest: digest.finalize(),
            case,
        });
        drop(file);
        clock.check("input-closed");
        self.inputs.len() - 1
    }

    fn spawn(&mut self, input: usize, listener: &TcpListener, clock: Clock) -> usize {
        let stdout = Capture::new(&self.root, clock);
        let stderr = Capture::new(&self.root, clock);
        let executable = clock.need(
            "actual-executable",
            clock.io("actual-executable", std::env::current_exe),
        );
        let address = clock.need(
            "actual-address",
            clock.io("actual-address", || listener.local_addr()),
        );
        let writer_out = stdout.writer(clock);
        let writer_err = stderr.writer(clock);
        clock.check("before-spawn");
        let result = Command::new(executable)
            .args(["--exact", "output::memory::memory_child", "--nocapture"])
            .env(MODE_ENV, self.inputs[input].case.key())
            .env(INPUT_ENV, &self.inputs[input].path)
            .env(ADDRESS_ENV, address.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(writer_out))
            .stderr(Stdio::from(writer_err))
            .spawn();
        let child = match result {
            Ok(child) => child,
            Err(error) => clock.need("spawn", Err(error)),
        };
        let pid = child.id();
        self.children.push(OwnedChild {
            child: Some(child),
            status: None,
            pid,
            stdout,
            stderr,
        });
        clock.check("owned-after-spawn");
        self.children.len() - 1
    }

    fn validate_input(&self, index: usize, clock: Clock) {
        let input = &self.inputs[index];
        let mut reader = clock.need(
            "oracle-open",
            clock.io("oracle-open", || {
                filesystem::open_private(&input.path, OpenOptions::new().read(true))
            }),
        );
        let metadata = clock.need("oracle-id", clock.io("oracle-id", || reader.metadata()));
        clock.require(
            "oracle-identity-size",
            (metadata.dev(), metadata.ino()) == input.identity
                && metadata.len()
                    == input.case.prefix() + if input.case.collecting() { 3 } else { 0 },
        );
        let mut magic = [0; 8];
        clock.need(
            "oracle-magic",
            clock.io("oracle-magic", || reader.read_exact(&mut magic)),
        );
        clock.require("oracle-magic-bytes", &magic == MAGIC);
        let payload = vec![0xa5; PAYLOAD];
        let mut buffer = [0; 8192];
        for sequence in 0..input.case.frames() {
            let mut header = [0; 25];
            clock.need(
                "oracle-header",
                clock.io("oracle-header", || reader.read_exact(&mut header)),
            );
            clock.require(
                "oracle-exact-header",
                header == golden_header(sequence, &payload),
            );
            for _ in 0..PAYLOAD / buffer.len() {
                clock.need(
                    "oracle-payload",
                    clock.io("oracle-payload", || reader.read_exact(&mut buffer)),
                );
                clock.require("oracle-a5", buffer.iter().all(|&byte| byte == 0xa5));
            }
        }
        if input.case.collecting() {
            let mut tail = [0; 3];
            clock.need(
                "oracle-tail",
                clock.io("oracle-tail", || reader.read_exact(&mut tail)),
            );
            clock.require("oracle-tail-bytes", tail == [1, 1, 0]);
        }
        let mut end = [0];
        let eof = clock.need(
            "oracle-eof",
            clock.io("oracle-eof", || reader.read(&mut end)),
        );
        clock.require("oracle-exact-eof", eof == 0);
        drop(reader);
        clock.check("oracle-closed");
    }

    fn cleanup(&mut self) -> io::Result<()> {
        let deadline = *self
            .cleanup_deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(3));
        let mut failure = None;
        // Signal every still-owned process before waiting under the one cleanup clock.
        for owner in &mut self.children {
            if let Some(child) = owner.child.as_mut() {
                if Instant::now() >= deadline {
                    return Err(io::Error::from(io::ErrorKind::TimedOut));
                }
                if let Err(error) = child.kill() {
                    failure.get_or_insert(error);
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::from(io::ErrorKind::TimedOut));
                }
            }
        }
        loop {
            let mut live = false;
            for owner in &mut self.children {
                if let Some(child) = owner.child.as_mut() {
                    if Instant::now() >= deadline {
                        return Err(io::Error::from(io::ErrorKind::TimedOut));
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            owner.status = Some(status);
                            owner.child = None;
                        }
                        Ok(None) => live = true,
                        Err(error) => {
                            live = true;
                            failure.get_or_insert(error);
                        }
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::from(io::ErrorKind::TimedOut));
                    }
                }
            }
            if !live {
                break;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            std::thread::yield_now();
        }
        if let Some(error) = failure {
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if dir_id(&self.root)? != self.root_identity
            || dir_id(self.temporary.path())? != self.temporary_identity
        {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        for owner in &mut self.children {
            owner.stdout.close(deadline)?;
            owner.stderr.close(deadline)?;
        }
        for input in &self.inputs {
            if Instant::now() >= deadline {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            let file = filesystem::open_private(&input.path, OpenOptions::new().read(true))?;
            let metadata = file.metadata()?;
            if (metadata.dev(), metadata.ino()) != input.identity {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            drop(file);
            if Instant::now() >= deadline {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            std::fs::remove_file(&input.path)?;
            if Instant::now() >= deadline {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
        }
        drop(self.guard.take());
        std::fs::remove_dir(&self.root)?;
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        std::fs::remove_dir(self.temporary.path())?;
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        self.closed = true;
        Ok(())
    }

    fn finish(&mut self, clock: Clock) {
        clock.require(
            "all-natural-reaped",
            self.children.len() == 3
                && self.children.iter().all(|owner| {
                    owner.child.is_none()
                        && owner
                            .status
                            .is_some_and(|status| status.success() && status.code() == Some(0))
                        && owner.stdout.validated
                        && owner.stderr.validated
                }),
        );
        clock.need("cleanup", self.cleanup());
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if !self.closed {
            if let Err(error) = self.cleanup() {
                eprintln!(
                    "engine-memory cleanup-unconfirmed kind={:?} raw={:?}",
                    error.kind(),
                    error.raw_os_error()
                );
            }
        }
    }
}

fn golden_header(sequence: u64, payload: &[u8]) -> [u8; 25] {
    let mut header = [0; 25];
    header[0] = Channel::Stdout as u8;
    header[1..9].copy_from_slice(&sequence.to_le_bytes());
    header[9..17].copy_from_slice(&(sequence + 1).to_le_bytes());
    header[17..21].copy_from_slice(&(PAYLOAD as u32).to_le_bytes());
    let mut checksum = Hasher::new();
    checksum.update(&header[..21]);
    checksum.update(payload);
    header[21..25].copy_from_slice(&checksum.finalize().to_le_bytes());
    header
}

fn receive(stream: &mut TcpStream, clock: Clock) -> String {
    clock.need(
        "read-timeout",
        clock.io("read-timeout", || {
            stream.set_read_timeout(Some(clock.remaining()))
        }),
    );
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        clock.need(
            "frame-read",
            clock.io("frame-read", || stream.read_exact(&mut byte)),
        );
        clock.require(
            "frame-ascii-cap",
            byte[0].is_ascii() && bytes.len() < FRAME_CAP,
        );
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).expect("engine-memory phase=frame-utf8")
}

fn send(stream: &mut TcpStream, clock: Clock, frame: &str) {
    clock.require(
        "send-ascii-cap",
        frame.is_ascii() && frame.len() <= FRAME_CAP && frame.ends_with('\n'),
    );
    clock.need(
        "write-timeout",
        clock.io("write-timeout", || {
            stream.set_write_timeout(Some(clock.remaining()))
        }),
    );
    clock.need(
        "frame-write",
        clock.io("frame-write", || stream.write_all(frame.as_bytes())),
    );
}

fn frame_numbers(frame: &str, label: &str, count: usize, clock: Clock) -> Vec<u64> {
    let mut words = frame.trim_end_matches('\n').split(' ');
    clock.require("frame-label", words.next() == Some(label));
    let values: Vec<u64> = words
        .map(|word| {
            clock.require(
                "frame-number",
                !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_digit()),
            );
            word.parse().expect("engine-memory phase=frame-overflow")
        })
        .collect();
    clock.require("frame-number-count", values.len() == count);
    values
}

fn accept(listener: &TcpListener, owner: &mut OwnedChild, clock: Clock) -> TcpStream {
    loop {
        owner.require_live(clock);
        match clock.io("accept", || listener.accept()) {
            Ok((stream, address)) => {
                clock.require("actual-loopback", address.ip().is_loopback());
                clock.need(
                    "blocking-stream",
                    clock.io("blocking-stream", || stream.set_nonblocking(false)),
                );
                return stream;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => return clock.need("accept", Err(error)),
        }
    }
}

fn validate_frames(frames: &[Frame], case: Case, clock: Clock) -> u32 {
    clock.require("actual-collect-count", frames.len() as u64 == case.frames());
    let mut digest = Hasher::new();
    for (sequence, frame) in frames.iter().enumerate() {
        clock.require(
            "actual-collect-tuple",
            frame.channel == Channel::Stdout
                && frame.sequence == sequence as u64
                && frame.elapsed_micros == sequence as u64 + 1
                && frame.payload.len() == PAYLOAD
                && frame.payload.iter().all(|&byte| byte == 0xa5),
        );
        digest.update(&frame.payload);
        clock.check("actual-collect-digest");
    }
    digest.finalize()
}

#[test]
fn memory_child() {
    let mode = match std::env::var(MODE_ENV) {
        Ok(mode) => mode,
        Err(std::env::VarError::NotPresent) => return,
        Err(_) => panic!("engine-memory phase=mode-encoding"),
    };
    let case = match mode.as_str() {
        "M-REPAIR-004" => Case::Repair4,
        "M-REPAIR-128" => Case::Repair128,
        "M-COLLECT-128" => Case::Collect128,
        _ => panic!("engine-memory phase=mode-refused"),
    };
    let clock = Clock::before(30);
    let input =
        PathBuf::from(std::env::var_os(INPUT_ENV).expect("engine-memory phase=input-unassigned"));
    let address = std::env::var(ADDRESS_ENV)
        .unwrap_or_else(|_| panic!("engine-memory phase=address-unassigned"));
    let address: std::net::SocketAddr = address
        .parse()
        .expect("engine-memory phase=address-invalid");
    clock.require("child-loopback", address.ip().is_loopback());
    let mut stream = clock.need(
        "connect",
        clock.io("connect", || {
            TcpStream::connect_timeout(&address, clock.remaining())
        }),
    );
    let pid = std::process::id();
    send(&mut stream, clock, &format!("READY {pid}\n"));
    let go = receive(&mut stream, clock);
    let go = frame_numbers(&go, "GO", 1, clock);
    clock.require("actual-go-pid", go[0] == u64::from(pid));
    let mut collected = None;
    let (frames_count, physical, payload, digest) = if case.collecting() {
        let frames = clock.need(
            "actual-collect",
            clock.io("actual-collect", || read_frames(&input)),
        );
        let digest = validate_frames(&frames, case, clock);
        let frames_count = frames.len() as u64;
        collected = Some(frames);
        (
            frames_count,
            case.prefix(),
            frames_count * PAYLOAD as u64,
            digest,
        )
    } else {
        let stats = clock.need(
            "actual-repair",
            clock.io("actual-repair", || repair_partial(&input)),
        );
        clock.require(
            "actual-repair-counters",
            stats
                == OutputStats {
                    retained_bytes: case.frames() * PAYLOAD as u64,
                    physical_bytes: case.prefix(),
                    ..OutputStats::default()
                },
        );
        let mut oracle = clock.need(
            "repaired-oracle-open",
            clock.io("repaired-oracle-open", || {
                filesystem::open_private(&input, OpenOptions::new().read(true))
            }),
        );
        let mut checksum = Hasher::new();
        let mut frames_count = 0_u64;
        let (physical, payload) = clock.need(
            "repaired-oracle-scan",
            clock.io("repaired-oracle-scan", || {
                scan_valid_frames(&mut *oracle, |frame| {
                    clock.require(
                        "repaired-actual-tuple",
                        frame.channel == Channel::Stdout
                            && frame.sequence == frames_count
                            && frame.elapsed_micros == frames_count + 1
                            && frame.payload.len() == PAYLOAD
                            && frame.payload.iter().all(|&byte| byte == 0xa5),
                    );
                    checksum.update(&frame.payload);
                    frames_count += 1;
                    clock.check("repaired-actual-digest");
                })
            }),
        );
        clock.require(
            "repaired-oracle-counters",
            frames_count == case.frames()
                && physical == stats.physical_bytes
                && payload == stats.retained_bytes,
        );
        drop(oracle);
        clock.check("repaired-oracle-released");
        (frames_count, physical, payload, checksum.finalize())
    };
    send(
        &mut stream,
        clock,
        &format!("DONE {pid} {frames_count} {physical} {payload} {digest}\n"),
    );
    let ack = receive(&mut stream, clock);
    let ack = frame_numbers(&ack, "ACK", 1, clock);
    clock.require("actual-ack-pid", ack[0] == u64::from(pid));
    if let Some(frames) = &collected {
        clock.require(
            "collect-retained-through-ack",
            validate_frames(frames, case, clock) == digest,
        );
    }
    drop(collected);
    clock.check("child-complete");
}

struct Measurement {
    case: Case,
    pid: u32,
    before_kib: u64,
    after_kib: u64,
    delta_kib: u64,
    digest: u32,
}

fn run_case(harness: &mut Harness, case: Case, overall: Clock) -> Measurement {
    let clock = overall.inside(30);
    let input = harness.prepare(case, clock);
    let listener = clock.need(
        "listener",
        clock.io("listener", || {
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        }),
    );
    clock.need(
        "nonblocking-listener",
        clock.io("nonblocking-listener", || listener.set_nonblocking(true)),
    );
    let ready_clock = clock.inside(5);
    let child = harness.spawn(input, &listener, clock);
    let owner = &mut harness.children[child];
    let mut stream = accept(&listener, owner, ready_clock);
    let ready = receive(&mut stream, ready_clock);
    let ready = frame_numbers(&ready, "READY", 1, ready_clock);
    ready_clock.require("actual-ready-pid", ready[0] == u64::from(owner.pid));
    let before_kib = owner.hwm(clock);
    send(&mut stream, clock, &format!("GO {}\n", owner.pid));
    let done = receive(&mut stream, clock);
    let done = frame_numbers(&done, "DONE", 5, clock);
    clock.require(
        "actual-done-pid-counts",
        done[0] == u64::from(owner.pid)
            && done[1] == case.frames()
            && done[2] == case.prefix()
            && done[3] == case.frames() * PAYLOAD as u64
            && done[4] == u64::from(harness.inputs[input].digest),
    );
    let after_kib = owner.hwm(clock);
    clock.require("monotonic-peak", after_kib >= before_kib);
    let delta_kib = after_kib - before_kib;
    clock.require(
        "fixed-memory-threshold",
        if case.collecting() {
            delta_kib >= 96 * 1024
        } else {
            delta_kib <= 16 * 1024
        },
    );
    send(&mut stream, clock, &format!("ACK {}\n", owner.pid));
    owner.await_reap(clock);
    let measurement = Measurement {
        case,
        pid: owner.pid,
        before_kib,
        after_kib,
        delta_kib,
        digest: done[4] as u32,
    };
    drop(stream);
    drop(listener);
    clock.check("protocol-released");
    harness.validate_input(input, clock);
    measurement
}

#[test]
fn streaming_memory_contracts() {
    // This origin precedes all private roots, input creation and child admission.
    let overall = Clock::before(180);
    let mut harness = Harness::new(overall);
    let repair4 = run_case(&mut harness, Case::Repair4, overall);
    let repair128 = run_case(&mut harness, Case::Repair128, overall);
    overall.require(
        "repair-size-excess",
        repair128.delta_kib.saturating_sub(repair4.delta_kib) <= 16 * 1024,
    );
    let collect128 = run_case(&mut harness, Case::Collect128, overall);
    harness.finish(overall);
    for record in [repair4, repair128, collect128] {
        overall.check("measurement-complete");
        println!(
            "output-qualification MEMORY {} pid={} before_kib={} after_kib={} delta_kib={} frames={} input={} valid={} payload={} crc32={} exit_success=1 exit_code=0 reaped=1 cleanup=1",
            record.case.key(),
            record.pid,
            record.before_kib,
            record.after_kib,
            record.delta_kib,
            record.case.frames(),
            record.case.prefix() + 3,
            record.case.prefix(),
            record.case.frames() * PAYLOAD as u64,
            record.digest
        );
        println!("output-qualification PASS {}", record.case.key());
    }
}
