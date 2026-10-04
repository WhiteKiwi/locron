//! Isolated real-kernel proof of stock adapters after their owning parent exits.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use super::loader_tests::FixtureChild;
use super::{ADAPTER_TIMEOUT, ADAPTER_WORKERS, prepare_adapter, run_adapter_worker};

const HOST: &str = include_str!("loader_crash_host.ps1");
const OBSERVER: &str = include_str!("loader_crash_observer.ps1");
const PROOF_BODY: &str = "# locron-heartbeat-proof-body";
const NORMAL_BODY: &str = "# locron-heartbeat-normal-body";
const APPEND_COMMON: &str = "# locron-heartbeat-append-common";
const APPEND_PROOF_BODY: &str = "# locron-heartbeat-append-proof-body";
const APPEND_CHILD_BEGIN: &str = "# locron-heartbeat-append-child-begin";
const APPEND_CHILD_END: &str = "# locron-heartbeat-append-child-end";
const APPEND_RECORD_BYTES: usize = 21;
const APPEND_MAX_RECORDS: u64 = 2_048;
const APPEND_MAX_BYTES: usize = 43_008;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HostProgram {
    Normal,
    SnapshotProof,
    AppendProof,
}

fn select_host_source(source: &str, program: HostProgram) -> String {
    let markers = [
        PROOF_BODY,
        NORMAL_BODY,
        APPEND_COMMON,
        APPEND_PROOF_BODY,
        APPEND_CHILD_BEGIN,
        APPEND_CHILD_END,
    ];
    for marker in markers {
        assert_eq!(source.matches(marker).count(), 1);
    }
    for line in source
        .lines()
        .filter(|line| line.starts_with("# locron-heartbeat-"))
    {
        assert!(
            markers.contains(&line),
            "unknown heartbeat program delimiter"
        );
    }
    let (shared, bodies) = source.split_once(PROOF_BODY).unwrap();
    let (proof_body, bodies) = bodies.split_once(NORMAL_BODY).unwrap();
    let (normal_body, bodies) = bodies.split_once(APPEND_COMMON).unwrap();
    let (append_common, append_proof) = bodies.split_once(APPEND_PROOF_BODY).unwrap();
    assert!(append_proof.find(APPEND_CHILD_BEGIN) < append_proof.find(APPEND_CHILD_END));
    match program {
        HostProgram::Normal => format!("{append_common}{normal_body}"),
        HostProgram::SnapshotProof => format!("{shared}{proof_body}"),
        HostProgram::AppendProof => format!("{append_common}{append_proof}"),
    }
}

fn host_source(program: HostProgram) -> &'static str {
    static NORMAL: OnceLock<String> = OnceLock::new();
    static SNAPSHOT_PROOF: OnceLock<String> = OnceLock::new();
    static APPEND_PROOF: OnceLock<String> = OnceLock::new();
    let selected = match program {
        HostProgram::Normal => &NORMAL,
        HostProgram::SnapshotProof => &SNAPSHOT_PROOF,
        HostProgram::AppendProof => &APPEND_PROOF,
    };
    selected
        .get_or_init(|| select_host_source(HOST, program))
        .as_str()
}

#[derive(Clone, Copy, Debug)]
enum OwnershipFact {
    HandlesBound,
    HostKillRequested,
    HostReaped,
    TargetsExited,
    ObserverReleased,
    ObserverExited,
    GuardRefusedBefore,
    GuardRefusedAfter,
    GuardReleased,
}

#[derive(Default)]
struct OwnershipFacts([bool; 9]);

impl OwnershipFacts {
    fn confirm(&mut self, fact: OwnershipFact) {
        self.0[fact as usize] = true;
    }
}

impl std::fmt::Debug for OwnershipFacts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = formatter.debug_map();
        for (fact, complete) in [
            OwnershipFact::HandlesBound,
            OwnershipFact::HostKillRequested,
            OwnershipFact::HostReaped,
            OwnershipFact::TargetsExited,
            OwnershipFact::ObserverReleased,
            OwnershipFact::ObserverExited,
            OwnershipFact::GuardRefusedBefore,
            OwnershipFact::GuardRefusedAfter,
            OwnershipFact::GuardReleased,
        ]
        .into_iter()
        .zip(self.0)
        {
            map.entry(&fact, &complete);
        }
        map.finish()
    }
}

#[derive(Debug, Default)]
struct CounterObservation {
    name: &'static str,
    bytes: Vec<u8>,
    status: &'static str,
    error_kind: Option<io::ErrorKind>,
    raw_os_error: Option<i32>,
    elapsed_ms: u128,
    attempts: u64,
    in_flight: bool,
}

#[derive(Debug, Default)]
struct AppendObservation {
    name: &'static str,
    status: &'static str,
    count: Option<u64>,
    tail_len: Option<usize>,
    bytes: usize,
    error_kind: Option<io::ErrorKind>,
    raw_os_error: Option<i32>,
    elapsed_ms: u128,
    attempts: u64,
    in_flight: bool,
}

struct Observations {
    started: Instant,
    deadline: Instant,
    phases: VecDeque<(&'static str, u128, u128)>,
    facts: OwnershipFacts,
    counters: [CounterObservation; 2],
    appends: [AppendObservation; 2],
}

impl Observations {
    fn new(started: Instant, deadline: Instant) -> Self {
        Self {
            started,
            deadline,
            phases: VecDeque::new(),
            facts: OwnershipFacts::default(),
            counters: std::array::from_fn(|_| CounterObservation::default()),
            appends: std::array::from_fn(|_| AppendObservation {
                status: "unknown",
                ..AppendObservation::default()
            }),
        }
    }

    fn remaining(&self) -> Result<Duration, String> {
        super::remaining(self.deadline).map_err(|error| {
            // Only saved, bounded observations: never reopen files or poll children on expiry.
            format!(
                "{error:?}: phases={:?} facts={:?} counters={:?} appends={:?}",
                self.phases, self.facts, self.counters, self.appends
            )
        })
    }

    fn check(&self) -> Duration {
        self.remaining().unwrap_or_else(|error| panic!("{error}"))
    }

    fn phase(&mut self, name: &'static str) {
        if self.phases.len() == 16 {
            self.phases.pop_front();
        }
        self.phases.push_back((
            name,
            self.started.elapsed().as_millis(),
            self.deadline
                .saturating_duration_since(Instant::now())
                .as_millis(),
        ));
        self.check();
    }

    fn observe(
        &mut self,
        slot: usize,
        name: &'static str,
        read: impl FnOnce(&Self) -> io::Result<Vec<u8>>,
    ) -> Result<Option<u64>, String> {
        self.remaining()?;
        let counter = &mut self.counters[slot];
        if counter.name != name {
            *counter = CounterObservation {
                name,
                ..CounterObservation::default()
            };
        }
        counter.attempts = counter.attempts.saturating_add(1);
        counter.in_flight = true;
        let result = read(self);
        let counter = &mut self.counters[slot];
        counter.in_flight = false;
        counter.elapsed_ms = self.started.elapsed().as_millis();
        let parsed = match result {
            Ok(mut bytes) => {
                assert!(bytes.len() <= 65, "counter read exceeded its fixed bound");
                let parsed = (bytes.len() <= 64)
                    .then(|| std::str::from_utf8(&bytes).ok()?.parse::<u64>().ok())
                    .flatten();
                counter.status = if bytes.len() > 64 {
                    "oversized"
                } else if parsed.is_some() {
                    "numeric"
                } else {
                    "malformed"
                };
                bytes.truncate(64);
                counter.bytes = bytes;
                counter.error_kind = None;
                counter.raw_os_error = None;
                parsed
            }
            Err(error) => {
                counter.bytes.clear();
                counter.status = "read-error";
                counter.error_kind = Some(error.kind());
                counter.raw_os_error = error.raw_os_error();
                None
            }
        };
        self.remaining()?;
        if let Some(kind) = self.counters[slot].error_kind {
            assert_eq!(kind, io::ErrorKind::NotFound);
        }
        Ok(parsed)
    }

    fn counter(&mut self, directory: &Path, name: &'static str, slot: usize) -> u64 {
        loop {
            let parsed = self
                .observe(slot, name, |observations| {
                    let file = File::open(directory.join(name))?;
                    observations.check();
                    let mut bytes = Vec::with_capacity(65);
                    file.take(65).read_to_end(&mut bytes)?;
                    Ok(bytes)
                })
                .unwrap_or_else(|error| panic!("{error}"));
            if let Some(value) = parsed {
                return value;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn text(&self, path: &Path) -> String {
        self.check();
        let file = File::open(path).unwrap();
        self.check();
        let mut text = String::new();
        file.take(32 * 1024).read_to_string(&mut text).unwrap();
        self.check();
        text
    }

    fn append_sample(
        &mut self,
        slot: usize,
        name: &'static str,
        initial: bool,
        read: impl FnOnce(&Self) -> io::Result<Vec<u8>>,
    ) -> Result<Option<AppendSample>, String> {
        self.remaining()?;
        let observation = &mut self.appends[slot];
        if observation.name != name {
            *observation = AppendObservation {
                name,
                status: "unknown",
                ..AppendObservation::default()
            };
        }
        observation.attempts = observation.attempts.saturating_add(1);
        observation.in_flight = true;
        let result = read(self);
        let observation = &mut self.appends[slot];
        observation.in_flight = false;
        observation.elapsed_ms = self.started.elapsed().as_millis();
        observation.count = None;
        observation.tail_len = None;
        observation.bytes = 0;
        observation.error_kind = None;
        observation.raw_os_error = None;
        let parsed = match result {
            Ok(bytes) => {
                observation.bytes = bytes.len();
                match parse_append(&bytes) {
                    Ok(sample) => {
                        observation.status = if sample.count == 0 {
                            "no_complete_record"
                        } else {
                            "ordered_prefix"
                        };
                        observation.count = Some(sample.count);
                        observation.tail_len = Some(sample.tail_len);
                        Ok(Some(sample))
                    }
                    Err(class) => {
                        observation.status = class;
                        Err(class)
                    }
                }
            }
            Err(error) => {
                observation.status = "read_error";
                observation.error_kind = Some(error.kind());
                observation.raw_os_error = error.raw_os_error();
                if initial && error.kind() == io::ErrorKind::NotFound {
                    observation.status = "not_created";
                    Ok(None)
                } else {
                    Err("read_error")
                }
            }
        };
        self.remaining()?;
        parsed.map_err(|class| format!("{class}: {:?}", self.appends[slot]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AppendSample {
    count: u64,
    tail_len: usize,
}

impl AppendSample {
    fn prefix_len(self) -> usize {
        usize::try_from(self.count).unwrap() * APPEND_RECORD_BYTES + self.tail_len
    }
}

fn append_record(sequence: u64) -> [u8; APPEND_RECORD_BYTES] {
    assert!((1..=APPEND_MAX_RECORDS).contains(&sequence));
    format!("{sequence:020}\n").into_bytes().try_into().unwrap()
}

fn parse_append(bytes: &[u8]) -> Result<AppendSample, &'static str> {
    if bytes.len() > APPEND_MAX_BYTES {
        return Err("oversized");
    }
    let count = bytes.len() / APPEND_RECORD_BYTES;
    for (index, record) in bytes.chunks_exact(APPEND_RECORD_BYTES).enumerate() {
        if record[20] != b'\n' || !record[..20].iter().all(u8::is_ascii_digit) {
            return Err("malformed_record");
        }
        if record != append_record(u64::try_from(index).unwrap() + 1) {
            return Err("sequence_mismatch");
        }
    }
    let tail = &bytes[count * APPEND_RECORD_BYTES..];
    if !tail.is_empty() && tail != &append_record(u64::try_from(count).unwrap() + 1)[..tail.len()] {
        return Err("invalid_tail");
    }
    Ok(AppendSample {
        count: u64::try_from(count).unwrap(),
        tail_len: tail.len(),
    })
}

fn read_append(file: &mut File, deadline: Instant) -> io::Result<Vec<u8>> {
    super::remaining(deadline)?;
    file.seek(SeekFrom::Start(0))?;
    super::remaining(deadline)?;
    let mut bytes = Vec::with_capacity(APPEND_MAX_BYTES + 1);
    let mut buffer = [0_u8; 4_096];
    while bytes.len() <= APPEND_MAX_BYTES {
        let limit = buffer.len().min(APPEND_MAX_BYTES + 1 - bytes.len());
        super::remaining(deadline)?;
        let count = file.read(&mut buffer[..limit])?;
        super::remaining(deadline)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}

#[derive(Default)]
struct AppendReaders {
    files: [Option<File>; 2],
    previous: [Option<AppendSample>; 2],
}

impl AppendReaders {
    fn sample(
        &mut self,
        directory: &Path,
        name: &'static str,
        slot: usize,
        observations: &mut Observations,
    ) -> AppendSample {
        self.try_sample(directory, name, slot, observations)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_sample(
        &mut self,
        directory: &Path,
        name: &'static str,
        slot: usize,
        observations: &mut Observations,
    ) -> Result<AppendSample, String> {
        loop {
            let initial = self.files[slot].is_none();
            let mut candidate = None;
            let sample = observations.append_sample(slot, name, initial, |observations| {
                let file = if let Some(file) = self.files[slot].as_mut() {
                    file
                } else {
                    observations.check();
                    candidate = Some(
                        OpenOptions::new()
                            .read(true)
                            .share_mode(3)
                            .open(directory.join(name))?,
                    );
                    observations.check();
                    candidate.as_mut().unwrap()
                };
                read_append(file, observations.deadline)
            })?;
            if let Some(sample) = sample {
                if self.previous[slot]
                    .is_some_and(|previous| sample.prefix_len() < previous.prefix_len())
                {
                    observations.appends[slot].status = "prefix_rewind";
                    return Err(format!("prefix_rewind: {:?}", observations.appends[slot]));
                }
                if sample.count > 0 {
                    if initial {
                        self.files[slot] = candidate;
                    }
                    self.previous[slot] = Some(sample);
                    return Ok(sample);
                }
                assert!(initial, "ready heartbeat lost every complete record");
            }
            observations.remaining()?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

struct HeartbeatAppender {
    file: File,
    next: u64,
}

impl HeartbeatAppender {
    fn create(path: &Path, deadline: Instant) -> io::Result<Self> {
        super::remaining(deadline)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(3)
            .open(path)?;
        super::remaining(deadline)?;
        Ok(Self { file, next: 1 })
    }

    fn append(&mut self, deadline: Instant) -> io::Result<()> {
        self.write_next(APPEND_RECORD_BYTES, deadline)?;
        self.next += 1;
        Ok(())
    }

    fn write_next(&mut self, length: usize, deadline: Instant) -> io::Result<()> {
        if self.next > APPEND_MAX_RECORDS || !(1..=APPEND_RECORD_BYTES).contains(&length) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "append bound"));
        }
        super::remaining(deadline)?;
        self.file.write_all(&append_record(self.next)[..length])?;
        super::remaining(deadline)?;
        self.file.flush()?;
        super::remaining(deadline)?;
        Ok(())
    }
}

fn heartbeat_snapshot(path: &Path, value: u64) -> io::Result<tempfile::TempPath> {
    let mut snapshot = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    write!(snapshot, "{value}")?;
    snapshot.flush()?;
    Ok(snapshot.into_temp_path())
}

fn publish_heartbeat(
    snapshot: tempfile::TempPath,
    path: &Path,
    first: bool,
) -> Result<(), tempfile::PathPersistError> {
    if first {
        snapshot.persist_noclobber(path)
    } else {
        snapshot.persist(path)
    }
}

struct Helper {
    child: FixtureChild,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Helper {
    fn spawn(mode: &str, directory: &Path) -> Self {
        Self::spawn_with_budget(mode, directory, None)
    }

    fn spawn_with_budget(mode: &str, directory: &Path, budget: Option<Duration>) -> Self {
        let stdout = directory.join(format!("{mode}.stdout"));
        let stderr = directory.join(format!("{mode}.stderr"));
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "windows::loader_tests::owned_loader_fixture_child",
                "--nocapture",
            ])
            .env("LOCRON_STOCK_LOADER_FIXTURE", mode)
            .env("LOCRON_LOADER_CRASH_DIRECTORY", directory)
            .current_dir(directory)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap());
        if let Some(budget) = budget {
            let milliseconds = budget.min(ADAPTER_TIMEOUT).as_millis();
            assert!(milliseconds > 0);
            command.env("LOCRON_HEARTBEAT_PROBE_BUDGET_MS", milliseconds.to_string());
        }
        let child = command.spawn().unwrap();
        Self {
            child: FixtureChild(child),
            stdout,
            stderr,
        }
    }

    fn live(&mut self, observations: &Observations) {
        observations.check();
        let status = self.child.0.try_wait().unwrap();
        observations.check();
        assert!(
            status.is_none(),
            "helper exited {status:?}: stdout={} stderr={}",
            observations.text(&self.stdout),
            observations.text(&self.stderr)
        );
    }

    fn exit_until(&mut self, observations: &Observations) -> ExitStatus {
        loop {
            observations.check();
            if let Some(status) = self.child.0.try_wait().unwrap() {
                observations.check();
                return status;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn directory() -> PathBuf {
    std::env::var_os("LOCRON_LOADER_CRASH_DIRECTORY")
        .map(PathBuf::from)
        .unwrap()
}

fn publish(directory: &Path, name: &str, bytes: &[u8]) {
    let pending = directory.join(format!("{name}.pending"));
    fs::write(&pending, bytes).unwrap();
    fs::rename(pending, directory.join(name)).unwrap();
}

fn wait_artifact(
    path: &Path,
    host: Option<&mut Helper>,
    observer: &mut Helper,
    observations: &Observations,
) {
    let mut host = host;
    loop {
        observations.check();
        let present = path.is_file();
        observations.check();
        if present {
            break;
        }
        if let Some(host) = host.as_mut() {
            host.live(observations);
        }
        observer.live(observations);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn beats(
    directory: &Path,
    readers: &mut AppendReaders,
    observations: &mut Observations,
) -> (AppendSample, AppendSample) {
    (
        readers.sample(directory, "generic-heartbeat", 0, observations),
        readers.sample(directory, "native-heartbeat", 1, observations),
    )
}

fn stock_is_held() {
    let error = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(super::stock_powershell().unwrap())
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(32));
}

pub(super) fn driver() {
    // This fresh orchestrator deliberately performs no SID, stock guard or adapter request.
    let temporary = tempfile::tempdir().unwrap();
    let directory = fs::canonicalize(temporary.path()).unwrap();
    let started = Instant::now();
    let deadline = started + Duration::from_secs(45);
    let mut observations = Observations::new(started, deadline);
    // These actual file owners never enter saved failure formatting.
    let mut readers = AppendReaders::default();
    observations.phase("host-spawn");
    let mut host = Helper::spawn("parent-exit-host", &directory);
    observations.phase("observer-spawn");
    let mut observer = Helper::spawn("parent-exit-observer", &directory);
    observations.phase("handles-bound-await");
    wait_artifact(
        &directory.join("handles-bound"),
        Some(&mut host),
        &mut observer,
        &observations,
    );
    assert_eq!(
        observations.text(&directory.join("handles-bound")),
        "three-live-handles"
    );
    observations.facts.confirm(OwnershipFact::HandlesBound);
    observations.phase("before-crash-counter-baseline");
    let before = beats(&directory, &mut readers, &mut observations);
    std::thread::sleep(Duration::from_millis(200));
    observations.phase("before-crash-counter-progress");
    let live = beats(&directory, &mut readers, &mut observations);
    assert!(live.0.count > before.0.count && live.1.count > before.1.count);
    host.live(&observations);
    observer.live(&observations);
    observations.check();
    stock_is_held();
    observations.check();
    observations
        .facts
        .confirm(OwnershipFact::GuardRefusedBefore);
    observations.phase("host-kill");
    // Terminate only this exact retained fixture parent. Its kernel Job handles close abruptly.
    host.child.0.kill().unwrap();
    observations.facts.confirm(OwnershipFact::HostKillRequested);
    observations.phase("host-reap");
    let status = host.exit_until(&observations);
    assert!(!status.success());
    observations.facts.confirm(OwnershipFact::HostReaped);
    observations.phase("target-exits-await");
    wait_artifact(
        &directory.join("exits-confirmed"),
        None,
        &mut observer,
        &observations,
    );
    assert_eq!(
        observations.text(&directory.join("exits-confirmed")),
        "three-associated-exits"
    );
    observations.facts.confirm(OwnershipFact::TargetsExited);
    observer.live(&observations);
    observations.check();
    stock_is_held();
    observations.check();
    observations.facts.confirm(OwnershipFact::GuardRefusedAfter);
    observations.phase("after-crash-counter-baseline");
    let stopped = beats(&directory, &mut readers, &mut observations);
    std::thread::sleep(Duration::from_millis(200));
    observations.phase("after-crash-counter-settled");
    assert_eq!(beats(&directory, &mut readers, &mut observations), stopped);
    observations.phase("observer-release");
    publish(&directory, "observer-release", b"release");
    observations.facts.confirm(OwnershipFact::ObserverReleased);
    observations.phase("observer-reap");
    let status = observer.exit_until(&observations);
    assert!(
        status.success(),
        "stdout={} stderr={}",
        observations.text(&observer.stdout),
        observations.text(&observer.stderr)
    );
    assert!(
        observations
            .text(&observer.stdout)
            .contains("observer-root-tree-pipes-confirmed")
    );
    observations.facts.confirm(OwnershipFact::ObserverExited);
    observations.phase("stock-guard-release");
    // Both helpers have actually exited; the observer already confirmed all three target handles.
    let released = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(super::stock_powershell().unwrap())
        .unwrap();
    observations.check();
    observations.facts.confirm(OwnershipFact::GuardReleased);
    std::thread::sleep(Duration::from_millis(200));
    observations.phase("final-counter-settled");
    assert_eq!(beats(&directory, &mut readers, &mut observations), stopped);
    drop(readers);
    drop(released);
    observations.check();
    println!("parent-exit-targets-handles-heartbeats-guards-confirmed");
    publication_proofs(&directory, &mut observations);
    observations.phase("heartbeat-publication-proofs-completed");
    println!("parent-exit-heartbeat-publication-proofs-confirmed");
}

fn observation_proofs(started: Instant, deadline: Instant) {
    let mut observations = Observations::new(started, deadline);
    for _ in 0..20 {
        observations.phase("bounded-observation-proof");
    }
    assert_eq!(observations.phases.len(), 16);
    assert_eq!(
        observations.observe(0, "malformed-probe", |_| Ok(b"7".to_vec())),
        Ok(Some(7))
    );
    assert_eq!(
        observations.observe(0, "malformed-probe", |_| Ok(Vec::new())),
        Ok(None)
    );
    assert_eq!(observations.counters[0].status, "malformed");
    assert_eq!(
        observations.observe(0, "malformed-probe", |_| Ok(vec![b'x'; 65])),
        Ok(None)
    );
    assert_eq!(observations.counters[0].bytes, vec![b'x'; 64]);
    assert_eq!(observations.counters[0].status, "oversized");
    assert_eq!(
        observations.observe(1, "missing-probe", |_| Err(io::ErrorKind::NotFound.into())),
        Ok(None)
    );
    observations.deadline = Instant::now();
    let mut read_entered = false;
    let refusal = observations
        .observe(0, "malformed-probe", |_| {
            read_entered = true;
            Ok(b"99".to_vec())
        })
        .unwrap_err();
    assert!(!read_entered);
    assert!(refusal.contains("bounded-observation-proof"));
    assert!(refusal.contains("malformed-probe") && refusal.contains("oversized"));
    assert!(refusal.contains("missing-probe") && refusal.contains("NotFound"));
    assert_eq!(observations.counters[0].bytes.len(), 64);
}

fn publication_proofs(directory: &Path, observations: &mut Observations) {
    observation_proofs(observations.started, observations.deadline);
    append_file_proofs(directory, observations);
    observations.phase("rust-publication-first");
    let path = directory.join("rust-publication-heartbeat");
    publish_heartbeat(heartbeat_snapshot(&path, 7).unwrap(), &path, true).unwrap();
    observations.check();
    let collision =
        publish_heartbeat(heartbeat_snapshot(&path, 99).unwrap(), &path, true).unwrap_err();
    observations.check();
    assert_eq!(collision.error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        7
    );
    assert_eq!(observations.counters[0].bytes, b"7");
    drop(collision);
    observations.phase("rust-replacement-share-refusal");
    let held = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    observations.check();
    let candidate = heartbeat_snapshot(&path, 8).unwrap();
    observations.check();
    let candidate_path = candidate.to_path_buf();
    observations.check();
    let refusal = publish_heartbeat(candidate, &path, false).unwrap_err();
    observations.check();
    assert_eq!(refusal.error.raw_os_error(), Some(5));
    let same_candidate = refusal.path.as_os_str() == candidate_path.as_os_str();
    assert!(
        same_candidate,
        "failed replacement changed the closed candidate"
    );
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        7
    );
    assert_eq!(observations.counters[0].bytes, b"7");
    observations.phase("rust-replacement-share-release");
    drop(held);
    observations.check();
    publish_heartbeat(refusal.path, &path, false).unwrap();
    observations.check();
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        8
    );
    assert_eq!(observations.counters[0].bytes, b"8");
    observations.phase("rust-publication-stage-baseline");
    let baseline = heartbeat_snapshot(&path, 7).unwrap();
    observations.check();
    publish_heartbeat(baseline, &path, false).unwrap();
    observations.check();
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        7
    );
    assert_eq!(observations.counters[0].bytes, b"7");
    observations.phase("rust-direct-publisher-spawn");
    let mut publisher = Helper::spawn_with_budget(
        "heartbeat-rust-staged",
        directory,
        Some(observations.check()),
    );
    observations.phase("rust-direct-publisher-stage-await");
    wait_artifact(
        &directory.join("rust-stage-ready"),
        None,
        &mut publisher,
        observations,
    );
    let publisher_pid = publisher.child.0.id();
    assert!(publisher_pid > 0);
    assert_eq!(
        observations.counter(directory, "rust-stage-ready", 1),
        u64::from(publisher_pid)
    );
    publisher.live(observations);
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        7
    );
    assert_eq!(observations.counters[0].bytes, b"7");
    observations.phase("rust-append-before-kill");
    let mut append_readers = AppendReaders::default();
    assert_rust_partial_files(directory, &mut append_readers, observations);
    rust_append_mutations_refuse(directory, observations);
    observations.phase("rust-direct-publisher-kill");
    publisher.child.0.kill().unwrap();
    let status = publisher.exit_until(observations);
    assert!(!status.success());
    observations.phase("rust-direct-publisher-reaped");
    assert_rust_partial_files(directory, &mut append_readers, observations);
    rust_append_mutations_refuse(directory, observations);
    drop(append_readers);
    rust_append_mutations_release(directory, observations);
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        7
    );
    assert_eq!(observations.counters[0].bytes, b"7");
    publish_heartbeat(heartbeat_snapshot(&path, 8).unwrap(), &path, false).unwrap();
    observations.check();
    assert_eq!(
        observations.counter(directory, "rust-publication-heartbeat", 0),
        8
    );
    assert_eq!(observations.counters[0].bytes, b"8");
    println!("rust-heartbeat-direct-publisher-killed-reaped-confirmed pid={publisher_pid}");
    observations.phase("powershell-publication-helper-spawn");
    let mut powershell = Helper::spawn_with_budget(
        "heartbeat-powershell-proof",
        directory,
        Some(observations.check()),
    );
    let status = powershell.exit_until(observations);
    assert!(
        status.success(),
        "stdout={} stderr={}",
        observations.text(&powershell.stdout),
        observations.text(&powershell.stderr)
    );
    assert!(
        observations
            .text(&powershell.stdout)
            .contains("powershell-heartbeat-publication-proofs-confirmed")
    );
    assert!(
        observations
            .text(&powershell.stdout)
            .contains("powershell-heartbeat-append-proofs-confirmed")
    );
}

fn partial_bytes() -> Vec<u8> {
    let mut bytes = append_record(1).to_vec();
    bytes.extend_from_slice(&append_record(2)[..20]);
    bytes
}

fn assert_rust_partial_files(
    directory: &Path,
    readers: &mut AppendReaders,
    observations: &mut Observations,
) {
    for (slot, name) in ["rust-append-rename", "rust-append-delete"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            readers.sample(directory, name, slot, observations),
            AppendSample {
                count: 1,
                tail_len: 20
            }
        );
    }
}

fn rust_append_mutations_refuse(directory: &Path, observations: &Observations) {
    observations.check();
    let rename = fs::rename(
        directory.join("rust-append-rename"),
        directory.join("rust-append-renamed"),
    )
    .unwrap_err();
    observations.check();
    let delete = fs::remove_file(directory.join("rust-append-delete")).unwrap_err();
    observations.check();
    // Exact same operations must later succeed after all actual holders release.
    println!(
        "rust-append-mutation-refused rename={:?}/{:?} delete={:?}/{:?}",
        rename.kind(),
        rename.raw_os_error(),
        delete.kind(),
        delete.raw_os_error()
    );
}

fn rust_append_mutations_release(directory: &Path, observations: &Observations) {
    observations.check();
    fs::rename(
        directory.join("rust-append-rename"),
        directory.join("rust-append-renamed"),
    )
    .unwrap();
    observations.check();
    let mut renamed = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(directory.join("rust-append-renamed"))
        .unwrap();
    observations.check();
    assert_eq!(
        read_append(&mut renamed, observations.deadline).unwrap(),
        partial_bytes()
    );
    drop(renamed);
    observations.check();
    fs::remove_file(directory.join("rust-append-delete")).unwrap();
    observations.check();
    let absent = fs::symlink_metadata(directory.join("rust-append-delete")).unwrap_err();
    observations.check();
    assert_eq!(absent.kind(), io::ErrorKind::NotFound);
    println!("rust-heartbeat-partial-append-kill-reader-release-confirmed");
}

fn append_file_proofs(directory: &Path, observations: &mut Observations) {
    observations.phase("append-actual-file-controls");
    let collision_path = directory.join("append-create-collision");
    let mut writer = HeartbeatAppender::create(&collision_path, observations.deadline).unwrap();
    writer.append(observations.deadline).unwrap();
    writer.write_next(20, observations.deadline).unwrap();
    let collision = match HeartbeatAppender::create(&collision_path, observations.deadline) {
        Ok(_) => panic!("append CreateNew adopted an existing file"),
        Err(error) => error,
    };
    assert_eq!(collision.kind(), io::ErrorKind::AlreadyExists);
    observations.check();
    let mut reader = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&collision_path)
        .unwrap();
    observations.check();
    assert_eq!(
        read_append(&mut reader, observations.deadline).unwrap(),
        partial_bytes()
    );
    drop(reader);
    drop(writer);
    observations.check();
    fs::remove_file(&collision_path).unwrap();
    observations.check();
    let mut continuation =
        HeartbeatAppender::create(&collision_path, observations.deadline).unwrap();
    continuation.append(observations.deadline).unwrap();
    drop(continuation);
    observations.check();
    let mut reader = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&collision_path)
        .unwrap();
    observations.check();
    assert_eq!(
        read_append(&mut reader, observations.deadline).unwrap(),
        append_record(1)
    );
    drop(reader);

    observations.check();
    let rewind_path = directory.join("append-rewind");
    let mut corruptor = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(3)
        .open(&rewind_path)
        .unwrap();
    observations.check();
    corruptor
        .write_all(&[append_record(1), append_record(2)].concat())
        .unwrap();
    observations.check();
    corruptor.flush().unwrap();
    observations.check();
    let mut readers = AppendReaders::default();
    assert_eq!(
        readers.sample(directory, "append-rewind", 0, observations),
        AppendSample {
            count: 2,
            tail_len: 0
        }
    );
    observations.check();
    // Explicit corruption control, never an operation of either real appender.
    corruptor.set_len(21).unwrap();
    observations.check();
    let rewind = readers
        .try_sample(directory, "append-rewind", 0, observations)
        .unwrap_err();
    assert!(rewind.starts_with("prefix_rewind"));
    assert_eq!(observations.appends[0].status, "prefix_rewind");
    drop(corruptor);
    drop(readers);

    let mut bad_tail = append_record(1).to_vec();
    bad_tail.extend_from_slice(b"x");
    for (name, bytes, expected) in [
        ("append-malformed", vec![b'x'; 21], "malformed_record"),
        (
            "append-sequence",
            append_record(2).to_vec(),
            "sequence_mismatch",
        ),
        ("append-tail", bad_tail, "invalid_tail"),
        (
            "append-overflow",
            vec![b'0'; APPEND_MAX_BYTES + 1],
            "oversized",
        ),
    ] {
        observations.check();
        let path = directory.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        observations.check();
        file.write_all(&bytes).unwrap();
        observations.check();
        file.flush().unwrap();
        observations.check();
        drop(file);
        observations.check();
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        observations.check();
        let error = observations
            .append_sample(0, name, false, |observations| {
                read_append(&mut file, observations.deadline)
            })
            .unwrap_err();
        assert!(error.starts_with(expected));
        assert_eq!(observations.appends[0].status, expected);
    }

    // The file is real, but the expired observation must not even enter its seek/read.
    observations.check();
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&collision_path)
        .unwrap();
    observations.check();
    let mut expired = Observations::new(observations.started, Instant::now());
    let mut entered = false;
    assert!(
        expired
            .append_sample(0, "append-expired", false, |observations| {
                entered = true;
                read_append(&mut file, observations.deadline)
            })
            .is_err()
    );
    assert!(!entered);
    assert_eq!(expired.appends[0].attempts, 0);
}

fn probe_deadline() -> Instant {
    let milliseconds: u64 = std::env::var("LOCRON_HEARTBEAT_PROBE_BUDGET_MS")
        .unwrap()
        .parse()
        .unwrap();
    assert!(milliseconds > 0 && milliseconds <= ADAPTER_TIMEOUT.as_millis() as u64);
    Instant::now() + Duration::from_millis(milliseconds)
}

pub(super) fn rust_staged_publisher() {
    let directory = directory();
    let deadline = probe_deadline();
    let path = directory.join("rust-publication-heartbeat");
    let _unpublished = heartbeat_snapshot(&path, 8).unwrap();
    let mut rename =
        HeartbeatAppender::create(&directory.join("rust-append-rename"), deadline).unwrap();
    rename.append(deadline).unwrap();
    rename.write_next(20, deadline).unwrap();
    let mut delete =
        HeartbeatAppender::create(&directory.join("rust-append-delete"), deadline).unwrap();
    delete.append(deadline).unwrap();
    delete.write_next(20, deadline).unwrap();
    super::remaining(deadline).unwrap();
    let ready = directory.join("rust-stage-ready");
    publish_heartbeat(
        heartbeat_snapshot(&ready, u64::from(std::process::id())).unwrap(),
        &ready,
        true,
    )
    .unwrap();
    loop {
        super::remaining(deadline).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct ManagedExceptionObservation {
    kind: &'static str,
    hresult: i32,
}

struct ManagedRefusalObservation {
    truncated: bool,
    nodes: Vec<ManagedExceptionObservation>,
}

fn managed_refusals(
    result: &serde_json::Value,
) -> Result<[ManagedRefusalObservation; 4], &'static str> {
    let records = result
        .get("refusals")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing managed refusal records")?;
    if records.len() != 4 {
        return Err("managed refusal record count");
    }
    let mut refusals = Vec::with_capacity(4);
    for record in records {
        let record = record.as_str().ok_or("managed refusal is not a string")?;
        if !record.is_ascii() || record.len() > 95 {
            return Err("managed refusal record bound");
        }
        let (truncated, nodes) = if let Some(nodes) = record.strip_prefix('=') {
            (false, nodes)
        } else if let Some(nodes) = record.strip_prefix('~') {
            (true, nodes)
        } else {
            return Err("managed refusal chain flag");
        };
        let mut observed = Vec::with_capacity(5);
        for node in nodes.split(',') {
            if observed.len() == 5 {
                return Err("managed refusal node count");
            }
            let (kind, hresult) = node.split_once('=').ok_or("managed refusal node shape")?;
            let kind = match kind {
                "io" => "io",
                "access" => "access",
                "other" => "other",
                _ => return Err("managed refusal category"),
            };
            let value = hresult
                .parse::<i32>()
                .map_err(|_| "managed refusal signed HResult")?;
            if value.to_string() != hresult {
                return Err("managed refusal noncanonical HResult");
            }
            observed.push(ManagedExceptionObservation {
                kind,
                hresult: value,
            });
        }
        if truncated && observed.len() != 5 {
            return Err("managed refusal truncated chain count");
        }
        refusals.push(ManagedRefusalObservation {
            truncated,
            nodes: observed,
        });
    }
    refusals
        .try_into()
        .map_err(|_| "managed refusal record count")
}

pub(super) fn powershell_publication_proof() {
    let directory = directory();
    let deadline = probe_deadline();
    let request = prepare_adapter(
        host_source(HostProgram::SnapshotProof),
        &json!({"directory":directory,"operation":"heartbeat-publication-proof",
            "budget_ms":super::remaining(deadline).unwrap().as_millis()}),
    )
    .unwrap();
    super::remaining(deadline).unwrap();
    let trace = Arc::clone(&request.trace);
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit).unwrap();
    super::remaining(deadline).unwrap();
    let root = trace.stage_value("spawn-complete").unwrap();
    assert!(root > 0 && Some(u64::from(root)) == result["root_pid"].as_u64());
    let publisher = result["publisher_pid"].as_u64().unwrap();
    assert!(publisher > 0 && publisher != u64::from(root));
    assert_eq!(result["initial_collision"], true);
    assert_eq!(result["replacement_share_refusal"], true);
    assert_eq!(result["staged"], true);
    assert_eq!(result["killed"], true);
    assert_eq!(result["reaped"], true);
    assert_eq!(result["old"], 7);
    assert_eq!(result["new"], 8);
    assert!(trace.has_stage("root-completed"));
    println!(
        "powershell-heartbeat-publication-proofs-confirmed root_pid={root} publisher_pid={publisher}"
    );
    // run_adapter_worker returned only after the preceding owned root, Job and pipes finished.
    super::remaining(deadline).unwrap();
    let request = prepare_adapter(
        host_source(HostProgram::AppendProof),
        &json!({"directory":directory,"budget_ms":super::remaining(deadline).unwrap().as_millis()}),
    )
    .unwrap();
    super::remaining(deadline).unwrap();
    let trace = Arc::clone(&request.trace);
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit).unwrap();
    super::remaining(deadline).unwrap();
    let root = trace.stage_value("spawn-complete").unwrap();
    assert!(root > 0 && Some(u64::from(root)) == result["root_pid"].as_u64());
    let publisher = result["publisher_pid"].as_u64().unwrap();
    assert!(publisher > 0 && publisher != u64::from(root));
    for fact in [
        "staged",
        "killed",
        "reaped",
        "reader_refused",
        "rename_released",
        "delete_released",
        "collision",
    ] {
        assert_eq!(result[fact], true);
    }
    assert_eq!(result["count"], 1);
    assert_eq!(result["tail_len"], 20);
    assert!(trace.has_stage("root-completed"));
    let refusals = managed_refusals(&result).unwrap_or_else(|message| panic!("{message}"));
    for ((phase, operation), refusal) in [
        ("producer_live", "rename"),
        ("producer_live", "delete"),
        ("writer_dead_readers_held", "rename"),
        ("writer_dead_readers_held", "delete"),
    ]
    .into_iter()
    .zip(refusals)
    {
        let chain = if refusal.truncated {
            "truncated"
        } else {
            "ended"
        };
        for (index, node) in refusal.nodes.iter().enumerate() {
            println!(
                "powershell-heartbeat-append-refusal phase={phase} operation={operation} node={} managed_kind={} hresult={} chain={chain}",
                index + 1,
                node.kind,
                node.hresult
            );
        }
    }
    println!(
        "powershell-heartbeat-append-proofs-confirmed root_pid={root} publisher_pid={publisher}"
    );
}

pub(super) fn host() {
    let directory = directory();
    let fixed_directory = directory.clone();
    let fixed = std::thread::spawn(move || {
        let sid = super::current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        let pid = super::filesystem_worker::observed_pid();
        assert!(pid > 0);
        publish(&fixed_directory, "fixed-pid", pid.to_string().as_bytes());
    });
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let request = prepare_adapter(
        host_source(HostProgram::Normal),
        &json!({"directory":directory,"executable":std::env::current_exe().unwrap()}),
    )
    .unwrap();
    let trace = Arc::clone(&request.trace);
    let generic = std::thread::spawn(move || {
        let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
        run_adapter_worker(request, deadline, permit)
    });
    let pid = loop {
        super::remaining(deadline).unwrap();
        if let Some(pid) = trace.stage_value("spawn-complete") {
            assert!(pid > 0);
            break pid;
        }
        assert!(
            !generic.is_finished(),
            "generic adapter exited before actual spawn"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    publish(&directory, "generic-spawn-pid", pid.to_string().as_bytes());
    fixed.join().unwrap();
    let result = generic.join().unwrap();
    panic!(
        "crash host unexpectedly survived: result={result:?}, spawn={:?}",
        trace.stage_value("spawn-complete")
    );
}

pub(super) fn observer() {
    let request = prepare_adapter(OBSERVER, &json!({"directory":directory()})).unwrap();
    let trace = Arc::clone(&request.trace);
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit).unwrap();
    assert_eq!(result["exits"], 3);
    assert!(trace.has_stage("root-completed"));
    assert!(
        trace
            .stage_value("spawn-complete")
            .is_some_and(|pid| pid > 0)
    );
    println!("observer-root-tree-pipes-confirmed");
}

pub(super) fn heartbeat() {
    let path = directory().join("native-heartbeat");
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let mut appender = HeartbeatAppender::create(&path, deadline).unwrap();
    while Instant::now() < deadline {
        appender.append(deadline).unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("native heartbeat descendant was not contained by parent Job closure");
}

#[test]
fn stock_crash_sources_fit_the_encoded_windows_command_line() {
    use base64::Engine as _;
    use std::os::windows::ffi::OsStrExt as _;

    // Mirror only command geometry from the configured path's validated components;
    // no guard, SID query, adapter or native canonicalization warms the cold gate.
    let configured = super::stock_powershell().unwrap();
    let mut components = configured.components();
    let Some(std::path::Component::Prefix(prefix)) = components.next() else {
        panic!("stock command lacks a local volume");
    };
    let (std::path::Prefix::Disk(volume) | std::path::Prefix::VerbatimDisk(volume)) = prefix.kind()
    else {
        panic!("stock command lacks a local volume");
    };
    assert_eq!(components.next(), Some(std::path::Component::RootDir));
    let mut executable = PathBuf::from(format!(r"\\?\{}:\", char::from(volume)));
    for component in components {
        let std::path::Component::Normal(value) = component else {
            panic!("stock command has an unsupported component");
        };
        assert!(
            !value
                .encode_wide()
                .any(|unit| unit == u16::from(b'/') || unit == u16::from(b'\\'))
        );
        executable.push(value);
    }
    let executable_units = executable.as_os_str().encode_wide().count();
    let command_units = |encoded: &str, nested: bool| {
        let flags = if nested {
            " -NoProfile -NonInteractive -EncodedCommand "
        } else {
            " -NoLogo -NoProfile -NonInteractive -EncodedCommand "
        };
        // Two executable quotes, all actual flags/separators and terminating NUL.
        executable_units + 2 + flags.encode_utf16().count() + encoded.encode_utf16().count() + 1
    };
    for program in [
        HostProgram::Normal,
        HostProgram::SnapshotProof,
        HostProgram::AppendProof,
    ] {
        let request = prepare_adapter(host_source(program), &serde_json::Value::Null).unwrap();
        assert!(command_units(&request.encoded, false) < 32_767);

        let lf_host = HOST.replace("\r\n", "\n");
        let lf = select_host_source(&lf_host, program);
        let crlf = select_host_source(&lf_host.replace('\n', "\r\n"), program);
        assert_eq!(crlf, lf.replace('\n', "\r\n"));
        for marker in [PROOF_BODY, NORMAL_BODY, APPEND_COMMON, APPEND_PROOF_BODY] {
            assert!(!lf.contains(marker));
        }

        let source = super::generic_trace::source(&lf).replace("\r\n", "\n");
        for source in [&source, &source.replace('\n', "\r\n")] {
            let encoded = base64::engine::general_purpose::STANDARD.encode(
                source
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let units = command_units(&encoded, false);
            assert!(units < 32_767);
            if program != HostProgram::SnapshotProof {
                assert!(32_767 - units >= 1_024);
            }
        }
    }
    // Exact literal definitions + child bodies, matching both ProcessStartInfo commands.
    let lf_host = HOST.replace("\r\n", "\n");
    for (program, definition_start, child_start) in [
        (
            HostProgram::SnapshotProof,
            "$heartbeatSource = @'\n",
            "$publisherSource = $heartbeatSource + @'\n",
        ),
        (HostProgram::AppendProof, "$a3 = @'\n", "$a8 = $a3 + @'\n"),
    ] {
        let selected = select_host_source(&lf_host, program);
        let definitions = selected
            .split_once(definition_start)
            .unwrap()
            .1
            .split_once("\n'@")
            .unwrap()
            .0;
        let body = selected
            .split_once(child_start)
            .unwrap()
            .1
            .split_once("\n'@")
            .unwrap()
            .0;
        if program == HostProgram::AppendProof {
            assert!(body.starts_with(APPEND_CHILD_BEGIN) && body.ends_with(APPEND_CHILD_END));
        }
        let child = format!("{definitions}{body}");
        for source in [&child, &child.replace('\n', "\r\n")] {
            let encoded = base64::engine::general_purpose::STANDARD.encode(
                source
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let units = command_units(&encoded, true);
            assert!(units < 32_767);
            if program == HostProgram::AppendProof {
                assert!(32_767 - units >= 1_024);
            }
        }
    }
}
