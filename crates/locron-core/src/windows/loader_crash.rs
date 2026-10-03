//! Isolated real-kernel proof of stock adapters after their owning parent exits.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
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

fn select_host_source(source: &str, proof: bool) -> String {
    assert_eq!(source.matches(PROOF_BODY).count(), 1);
    assert_eq!(source.matches(NORMAL_BODY).count(), 1);
    let (shared, bodies) = source.split_once(PROOF_BODY).unwrap();
    let (proof_body, normal_body) = bodies.split_once(NORMAL_BODY).unwrap();
    format!("{shared}{}", if proof { proof_body } else { normal_body })
}

fn host_source(proof: bool) -> &'static str {
    static NORMAL: OnceLock<String> = OnceLock::new();
    static PROOF: OnceLock<String> = OnceLock::new();
    let selected = if proof { &PROOF } else { &NORMAL };
    selected
        .get_or_init(|| select_host_source(HOST, proof))
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

struct Observations {
    started: Instant,
    deadline: Instant,
    phases: VecDeque<(&'static str, u128, u128)>,
    facts: OwnershipFacts,
    counters: [CounterObservation; 2],
}

impl Observations {
    fn new(started: Instant, deadline: Instant) -> Self {
        Self {
            started,
            deadline,
            phases: VecDeque::new(),
            facts: OwnershipFacts::default(),
            counters: std::array::from_fn(|_| CounterObservation::default()),
        }
    }

    fn remaining(&self) -> Result<Duration, String> {
        super::remaining(self.deadline).map_err(|error| {
            // Only saved, bounded observations: never reopen files or poll children on expiry.
            format!(
                "{error:?}: phases={:?} facts={:?} counters={:?}",
                self.phases, self.facts, self.counters
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

fn beats(directory: &Path, observations: &mut Observations) -> (u64, u64) {
    (
        observations.counter(directory, "generic-heartbeat", 0),
        observations.counter(directory, "native-heartbeat", 1),
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
    let before = beats(&directory, &mut observations);
    std::thread::sleep(Duration::from_millis(200));
    observations.phase("before-crash-counter-progress");
    let live = beats(&directory, &mut observations);
    assert!(live.0 > before.0 && live.1 > before.1);
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
    let stopped = beats(&directory, &mut observations);
    std::thread::sleep(Duration::from_millis(200));
    observations.phase("after-crash-counter-settled");
    assert_eq!(beats(&directory, &mut observations), stopped);
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
    assert_eq!(beats(&directory, &mut observations), stopped);
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
    observations.phase("rust-direct-publisher-kill");
    publisher.child.0.kill().unwrap();
    let status = publisher.exit_until(observations);
    assert!(!status.success());
    observations.phase("rust-direct-publisher-reaped");
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

pub(super) fn powershell_publication_proof() {
    let directory = directory();
    let deadline = probe_deadline();
    let request = prepare_adapter(
        host_source(true),
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
        host_source(false),
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
    let mut count = 0_u64;
    while Instant::now() < deadline {
        count += 1;
        publish_heartbeat(heartbeat_snapshot(&path, count).unwrap(), &path, count == 1).unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("native heartbeat descendant was not contained by parent Job closure");
}

#[test]
fn stock_crash_sources_fit_the_encoded_windows_command_line() {
    use base64::Engine as _;

    let command_units = |encoded: &str| {
        format!(
            "\"\\\\?\\C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}\0"
        )
        .encode_utf16()
        .count()
    };
    for proof in [false, true] {
        let request = prepare_adapter(host_source(proof), &serde_json::Value::Null).unwrap();
        assert!(command_units(&request.encoded) <= 32_767);

        let lf_host = HOST.replace("\r\n", "\n");
        let lf = select_host_source(&lf_host, proof);
        let crlf = select_host_source(&lf_host.replace('\n', "\r\n"), proof);
        assert_eq!(crlf, lf.replace('\n', "\r\n"));
        assert!(!lf.contains(PROOF_BODY) && !lf.contains(NORMAL_BODY));

        let source = super::generic_trace::source(&lf).replace("\r\n", "\n");
        for source in [&source, &source.replace('\n', "\r\n")] {
            let encoded = base64::engine::general_purpose::STANDARD.encode(
                source
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            assert!(command_units(&encoded) <= 32_767);
        }
    }
}
