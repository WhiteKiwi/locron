//! Actual read-only launch qualification, compiled only with the Windows tests.
//!
//! Constructors consume native ownership. Wire frames, Bootstrap and saved paths
//! alone cannot reconstruct a live token. No operation engine or effects exist here.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use locron_core::filesystem::{FileIdentity, same_file};
use serde::Deserialize;
use tokio::runtime::Runtime;
use uuid::Uuid;

use super::windows_bootstrap::{Bootstrap, verify_at_until};
use super::windows_launch_codec::{Bindings, Codec, Frame, HelperIdentity, Phase, Transcript};
use super::windows_launch_transport::{CloseTask, PipeNames, ReceiveEndpoint, SendListener};
use super::windows_ownership::{
    Removal, Standalone, VerifiedFile, immutable_private_until, verify_removal_until, verify_until,
};
use super::windows_protocol::{Kind, maintenance_path};
use super::windows_receipt::same_path;

const PHASE: Duration = Duration::from_secs(30);
const CLEANUP: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(10);
const HELPER_LIMIT: usize = 64 * 1024 * 1024;
const ENTRY: &str = "self_update::windows_launch_gate::tests::copied_helper_entry";
const SESSION: &str = "LOCRON_LAUNCH_FIXTURE_SESSION";
const PARENT_PIPE: &str = "LOCRON_LAUNCH_FIXTURE_PARENT_PIPE";
const CHILD_PIPE: &str = "LOCRON_LAUNCH_FIXTURE_CHILD_PIPE";
const REQUEST: &str = "LOCRON_LAUNCH_FIXTURE_REQUEST";
const ROOT: &str = "LOCRON_LAUNCH_FIXTURE_ROOT";
const EXIT: &str = "LOCRON_LAUNCH_FIXTURE_EXIT";

// One live or quarantined phase owner per process; timeout cannot reopen admission.
static OWNER: AtomicBool = AtomicBool::new(false);

struct Admission;

impl Admission {
    fn acquire(deadline: Instant) -> Result<Self> {
        remaining(deadline)?;
        OWNER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow::anyhow!("previous launch owner is still live or quarantined"))?;
        let permit = Self;
        remaining(deadline)?;
        Ok(permit)
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        OWNER.store(false, Ordering::Release);
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    ensure!(!left.is_zero(), "original launch deadline expired");
    Ok(left)
}

fn advertised_budget_ms(left: Duration) -> Result<u64> {
    // Floor transport precision; a sub-millisecond remainder cannot be rounded up.
    let millis = left.as_millis().min(PHASE.as_millis());
    ensure!(
        millis != 0,
        "original launch budget is below transport precision"
    );
    Ok(millis as u64)
}

fn child_horizon(entry: Instant, original: Instant, advertised_ms: u64) -> Result<Instant> {
    ensure!(
        (1..=30_000).contains(&advertised_ms),
        "invalid advertised budget"
    );
    // This is the pre-I/O entry instant, never the later receipt/readback instant.
    let advertised = entry
        .checked_add(Duration::from_millis(advertised_ms))
        .context("advertised child horizon overflow")?;
    Ok(original.min(advertised))
}

fn within<T>(deadline: Instant, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    remaining(deadline)?;
    let result = operation();
    remaining(deadline)?;
    result
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str().context("guarded launch path is not Unicode")
}

/// Private creator receipt; no deserialization or clone constructor exists.
/// Fixtures capture this from their actual CreateNew handle, before closing it.
struct CreatedHelper {
    path: PathBuf,
    identity: FileIdentity,
}

enum Source {
    Standalone(Standalone),
    Removal(Removal),
}

impl Source {
    fn binary(&self) -> &VerifiedFile {
        match self {
            Self::Standalone(source) => &source.files["locron.exe"],
            Self::Removal(source) => &source.files["locron.exe"],
        }
    }
}

struct LaunchInput {
    request: PathBuf,
    root: PathBuf,
    created: CreatedHelper,
    exit_before_ready: bool,
    // Controlled fixture boundary, not a claim that a kernel call was forced to hang.
    pause_after_spawn: Option<SpawnPause>,
}

struct SpawnPause {
    reached: mpsc::SyncSender<u32>,
    release: mpsc::Receiver<()>,
}

struct Lease {
    bootstrap: Bootstrap,
    source: Source,
}

impl Lease {
    fn acquire(input: &LaunchInput, deadline: Instant) -> Result<Self> {
        let bootstrap =
            verify_at_until(&input.request, &input.root, &input.created.path, deadline)?;
        ensure!(
            bootstrap.helper_file.identity == input.created.identity,
            "retained helper differs from its creator's complete object identity"
        );
        let directory = Path::new(&bootstrap.request.executable)
            .parent()
            .context("selected standalone executable has no parent")?;
        let source = match bootstrap.request.kind {
            Kind::Uninstall => Source::Removal(verify_removal_until(directory, deadline)?),
            Kind::Install | Kind::SelfUpdate => {
                Source::Standalone(verify_until(directory, deadline)?)
            }
            _ => anyhow::bail!("this gate has no package, fresh-install or recovery authority"),
        };
        ensure!(
            same_path(
                path_text(source.binary().file.normalized_path())?,
                &bootstrap.request.executable
            )?,
            "source is not the exact selected executable"
        );
        ensure!(
            source.binary().sha256 == bootstrap.helper_file.sha256,
            "copied helper is not the positively owned selected source bytes"
        );
        remaining(deadline)?;
        Ok(Self { bootstrap, source })
    }

    fn bindings(&self, session: Uuid) -> Result<Bindings> {
        // Recheck the positive source, not merely a request-supplied digest.
        ensure!(
            self.source.binary().sha256 == self.bootstrap.helper_file.sha256,
            "helper source binding changed"
        );
        bindings(&self.bootstrap, session)
    }
}

fn bindings(bootstrap: &Bootstrap, session: Uuid) -> Result<Bindings> {
    let identity = bootstrap.helper_file.identity;
    Bindings::new(
        bootstrap.original.operation_id,
        session,
        bootstrap.original_file.sha256.clone(),
        bootstrap.request_file.sha256.clone(),
        HelperIdentity::new(
            identity.volume_serial_number,
            identity.file_id,
            bootstrap.helper_file.sha256.clone(),
        )?,
    )
}

fn child_live(child: &mut Child, deadline: Instant) -> Result<()> {
    within(deadline, || {
        ensure!(
            child.try_wait()?.is_none(),
            "original native Child exited during qualification"
        );
        Ok(())
    })
}

async fn while_live<T>(
    child: &mut Child,
    deadline: Instant,
    cancelled: &AtomicBool,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::pin!(future);
    loop {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "actual launch cancelled"
        );
        child_live(child, deadline)?;
        tokio::select! {
            result = &mut future => {
                child_live(child, deadline)?;
                ensure!(!cancelled.load(Ordering::Acquire), "actual launch cancelled");
                return result;
            }
            () = tokio::time::sleep(remaining(deadline)?.min(POLL)) => {}
        }
    }
}

const MODULE_QUERY: &str = r#"
$locronQueryPid = $request.pid
if (($locronQueryPid -isnot [int] -and $locronQueryPid -isnot [long]) -or $locronQueryPid -le 0 -or $locronQueryPid -gt [int]::MaxValue) { throw 'invalid actual child PID' }
$p = [Diagnostics.Process]::GetProcessById([int]$locronQueryPid)
try {
  $m = $p.get_MainModule()
  if ($null -eq $m) { throw 'No initialized main module' }
  $path = $m.get_FileName()
  if ([string]::IsNullOrEmpty($path)) { throw 'No initialized main module filename' }
  &$locronToJson -Compress @{ pid = [uint32]$p.Id; path = $path }
} finally { $p.Dispose() }
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleName {
    pid: u32,
    path: String,
}

fn image_readback(child: &mut Child, lease: &Lease, deadline: Instant) -> Result<VerifiedFile> {
    child_live(child, deadline)?;
    let actual_pid = child.id();
    let value = locron_core::windows::run_script_json_bounded(
        MODULE_QUERY,
        &serde_json::json!({"pid": actual_pid}),
        remaining(deadline)?,
    )?;
    child_live(child, deadline)?;
    let name: ModuleName = serde_json::from_value(value)?;
    ensure!(
        name.pid == actual_pid,
        "module query returned a different PID"
    );
    maintenance_path(&name.path)?;
    let (file, _) = immutable_private_until(Path::new(&name.path), HELPER_LIMIT, deadline)?;
    child_live(child, deadline)?;
    ensure!(
        same_path(
            path_text(file.file.normalized_path())?,
            path_text(lease.bootstrap.helper_file.file.normalized_path())?
        )? && within(deadline, || Ok(same_file(
            &file.file,
            &lease.bootstrap.helper_file.file
        )?))?
            && file.identity == lease.bootstrap.helper_file.identity
            && file.sha256 == lease.bootstrap.helper_file.sha256,
        "mapped module name does not agree with the continuously guarded exact helper"
    );
    child_live(child, deadline)?;
    Ok(file)
}

#[derive(Default)]
struct Resources {
    lease: Option<Lease>,
    child: Option<Child>,
    image: Option<VerifiedFile>,
    close: Option<CloseTask>,
}

async fn parent_exchange(
    resources: &mut Resources,
    mut input: LaunchInput,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<u32> {
    let sid = locron_core::windows::current_user_sid_until(deadline)?;
    resources.lease = Some(Lease::acquire(&input, deadline)?);
    let lease = resources
        .lease
        .as_ref()
        .context("missing retained launch lease")?;
    let names = PipeNames::new(&sid, Uuid::now_v7())?;
    let listener = SendListener::parent(&names, &sid, deadline)?;
    let (session, parent_pipe, child_pipe) = names.selectors();
    let mut command = Command::new(lease.bootstrap.helper_file.file.normalized_path());
    // Microsoft CREATE_NO_WINDOW for this exact copied fixture child; no shell.
    command.creation_flags(0x0800_0000);
    command
        .args(["--exact", ENTRY, "--nocapture", "--test-threads=1"])
        .env(SESSION, session.to_string())
        .env(PARENT_PIPE, parent_pipe)
        .env(CHILD_PIPE, child_pipe)
        .env(REQUEST, &input.request)
        .env(ROOT, &input.root)
        .env(EXIT, if input.exit_before_ready { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    remaining(deadline)?;
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "launch cancelled before native spawn"
    );
    // Keep the actual Child BEFORE any fallible post-spawn check or readback.
    resources.child = Some(
        command
            .spawn()
            .context("guarded native helper spawn failed")?,
    );
    let child = resources.child.as_mut().context("missing original Child")?;
    if let Some(pause) = input.pause_after_spawn.take() {
        pause
            .reached
            .send(child.id())
            .context("fixture observer closed")?;
        pause.release.recv().context("fixture pause closed")?;
    }
    child_live(child, deadline)?;
    let mut sender = while_live(child, deadline, cancelled, listener.accept(deadline)).await?;
    child_live(child, deadline)?;
    ensure!(
        sender.peer_pid(deadline)? == child.id(),
        "outgoing peer is not original Child"
    );
    child_live(child, deadline)?;
    let mut codec = Codec::default();
    // Advertise the actual remaining budget after native spawn/accept, immediately
    // before delivery; pre-spawn metadata must not reset the child's clock.
    let challenge = Frame::challenge(
        lease.bindings(session)?,
        std::process::id(),
        advertised_budget_ms(remaining(deadline)?)?,
    )?;
    let mut transcript = Transcript::new(lease.bindings(session)?, std::process::id(), child.id())?;
    transcript.observe(&challenge)?;
    while_live(
        child,
        deadline,
        cancelled,
        sender.send(&mut codec, &challenge, deadline),
    )
    .await?;
    let mut receiver = while_live(
        child,
        deadline,
        cancelled,
        ReceiveEndpoint::parent(&names, deadline),
    )
    .await?;
    child_live(child, deadline)?;
    ensure!(
        receiver.peer_pid(deadline)? == child.id(),
        "incoming server is not original Child"
    );
    child_live(child, deadline)?;
    let ready = while_live(
        child,
        deadline,
        cancelled,
        receiver.receive(&mut codec, deadline),
    )
    .await?;
    transcript.observe(&ready)?;
    resources.image = Some(image_readback(child, lease, deadline)?);
    let nonce = ready.child_nonce().context("Ready lacks child nonce")?;
    let permit = ready.next(Phase::Permit, child.id(), nonce)?;
    transcript.observe(&permit)?;
    while_live(
        child,
        deadline,
        cancelled,
        sender.send(&mut codec, &permit, deadline),
    )
    .await?;
    // Store ownership before waiting; timeout must not drop/detach the raw flush.
    resources.close = Some(sender.close(deadline)?);
    while_live(
        child,
        deadline,
        cancelled,
        resources
            .close
            .as_mut()
            .context("missing close task")?
            .wait_until(deadline),
    )
    .await?;
    let qualified = while_live(
        child,
        deadline,
        cancelled,
        receiver.receive(&mut codec, deadline),
    )
    .await?;
    transcript.observe(&qualified)?;
    while_live(child, deadline, cancelled, receiver.terminal_eof(deadline)).await?;
    child_live(child, deadline)?;
    ensure!(
        transcript.finished() && !cancelled.load(Ordering::Acquire),
        "incomplete or cancelled actual launch exchange"
    );
    remaining(deadline)?;
    Ok(child.id())
}

/// Sole caller connection to the owner; no Clone, serde or PID constructor.
struct Connection {
    cancelled: Arc<AtomicBool>,
    finished: mpsc::Receiver<()>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

struct Pending {
    connection: Connection,
    ready: mpsc::Receiver<Result<u32>>,
    deadline: Instant,
}

struct Qualified {
    connection: Connection,
    child_pid: u32,
}

impl Pending {
    fn start(input: LaunchInput) -> Result<Self> {
        Self::start_until(input, Instant::now() + PHASE)
    }

    fn start_until(input: LaunchInput, deadline: Instant) -> Result<Self> {
        let admission = Admission::acquire(deadline)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let owner_cancelled = Arc::clone(&cancelled);
        let (notice, ready) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("locron-launch-fixture-owner".into())
            .spawn(move || {
                let mut resources = Resources::default();
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = notice.send(Err(error.into()));
                        drop(admission);
                        let _ = done.send(());
                        return;
                    }
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.block_on(parent_exchange(
                        &mut resources,
                        input,
                        deadline,
                        &owner_cancelled,
                    ))
                }))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!("launch owner panicked; ownership retained"))
                });
                let accepted = result.is_ok()
                    && Instant::now() < deadline
                    && !owner_cancelled.load(Ordering::Acquire);
                let result = if accepted {
                    result
                } else {
                    result.and_then(|_| Err(anyhow::anyhow!("late launch result refused")))
                };
                if notice.send(result).is_err() {
                    owner_cancelled.store(true, Ordering::Release);
                }
                if accepted {
                    while !owner_cancelled.load(Ordering::Acquire) {
                        match resources.child.as_mut().map(Child::try_wait) {
                            Some(Ok(None)) => std::thread::sleep(POLL),
                            _ => break,
                        }
                    }
                }
                cleanup(resources, runtime);
                drop(admission);
                let _ = done.send(());
            })?;
        Ok(Self {
            connection: Connection {
                cancelled,
                finished,
            },
            ready,
            deadline,
        })
    }

    fn qualify(self) -> Result<Qualified> {
        let result = self
            .ready
            .recv_timeout(remaining(self.deadline)?)
            .context("launch owner remains retained after original deadline")??;
        remaining(self.deadline)?;
        Ok(Qualified {
            connection: self.connection,
            child_pid: result,
        })
    }
}

impl Qualified {
    fn release(self) -> Result<()> {
        self.connection.cancelled.store(true, Ordering::Release);
        self.connection
            .finished
            .recv_timeout(CLEANUP)
            .context("launch cleanup remains quarantined")?;
        Ok(())
    }
}

fn cleanup(mut resources: Resources, runtime: Runtime) {
    let deadline = Instant::now() + CLEANUP;
    let mut exited = resources.child.is_none();
    if let Some(child) = resources.child.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => exited = true,
            Ok(None) => {
                let _ = child.kill();
            }
            Err(_) => {}
        }
        while !exited && Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                exited = true;
                break;
            }
            std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
    if let Some(close) = resources.close.as_mut() {
        if close.is_pending() {
            let _ = runtime.block_on(close.wait_until(deadline));
        }
    }
    if !exited || resources.close.as_ref().is_some_and(CloseTask::is_pending) {
        // No caller join or detached raw task. Retain ALL ownership and admission.
        // Quarantine is intentionally not a fresh qualification/cleanup attempt.
        loop {
            std::thread::park();
        }
    }
    // IOCP completion records can outlive endpoint Drop. Keep Child/lease/guards
    // live across runtime disposal too; uncertainty stays in this finite owner.
    drop(runtime);
    drop(resources);
}

struct ChildSelector {
    names: PipeNames,
    request: PathBuf,
    root: PathBuf,
}

/// Actual receive endpoint and clock are consumed together with Bootstrap.
struct ChildExchange {
    names: PipeNames,
    receive: ReceiveEndpoint,
    challenge: Frame,
    codec: Codec,
    deadline: Instant,
    actual_parent_pid: u32,
}

async fn begin_child(
    selector: ChildSelector,
    child_entry: Instant,
    original: Instant,
) -> Result<(ChildExchange, PathBuf, PathBuf)> {
    let mut receive = ReceiveEndpoint::child(&selector.names, original).await?;
    let actual_parent_pid = receive.peer_pid(original)?;
    let mut codec = Codec::default();
    let challenge = receive.receive(&mut codec, original).await?;
    ensure!(
        actual_parent_pid == challenge.parent_pid(),
        "Challenge is not from the actual parent endpoint"
    );
    let deadline = child_horizon(
        child_entry,
        original,
        challenge
            .remaining_ms()
            .context("Challenge lacks remaining budget")?,
    )?;
    let sid = locron_core::windows::current_user_sid_until(deadline)?;
    selector.names.bind_sid(&sid)?;
    remaining(deadline)?;
    Ok((
        ChildExchange {
            names: selector.names,
            receive,
            challenge,
            codec,
            deadline,
            actual_parent_pid,
        },
        selector.request,
        selector.root,
    ))
}

struct ChildState {
    bootstrap: Bootstrap,
    exchange: ChildExchange,
    close: Option<CloseTask>,
}

struct QualifiedBootstrap {
    state: ChildState,
}

struct ChildRefusal {
    state: ChildState,
    error: anyhow::Error,
}

/// Owner state is initialized before a future is polled, outside its unwind boundary.
struct ChildQualification {
    state: ChildState,
    complete: bool,
}

fn qualify_child(bootstrap: Bootstrap, exchange: ChildExchange) -> ChildQualification {
    ChildQualification {
        state: ChildState {
            bootstrap,
            exchange,
            close: None,
        },
        complete: false,
    }
}

impl ChildQualification {
    async fn finish(&mut self) -> Result<()> {
        child_exchange(&mut self.state).await?;
        remaining(self.state.exchange.deadline)?;
        self.complete = true;
        Ok(())
    }

    fn into_qualified(self) -> std::result::Result<QualifiedBootstrap, Box<ChildRefusal>> {
        if !self.complete || Instant::now() >= self.state.exchange.deadline {
            return Err(Box::new(ChildRefusal {
                state: self.state,
                error: anyhow::anyhow!("incomplete or late child result cannot mint a token"),
            }));
        }
        Ok(QualifiedBootstrap { state: self.state })
    }
}

async fn child_exchange(state: &mut ChildState) -> Result<()> {
    let exchange = &mut state.exchange;
    let deadline = exchange.deadline;
    let session = exchange.names.selectors().0;
    let actual_bindings = bindings(&state.bootstrap, session)?;
    ensure!(
        &actual_bindings == exchange.challenge.bindings(),
        "Challenge differs from actual retained Bootstrap bytes/object"
    );
    let mut transcript = Transcript::new(
        actual_bindings,
        exchange.actual_parent_pid,
        std::process::id(),
    )?;
    transcript.observe(&exchange.challenge)?;
    let sid = locron_core::windows::current_user_sid_until(deadline)?;
    let listener = SendListener::child(&exchange.names, &sid, deadline)?;
    let mut sender = listener.accept(deadline).await?;
    ensure!(
        sender.peer_pid(deadline)? == exchange.actual_parent_pid,
        "child outgoing peer differs from actual parent endpoint"
    );
    let nonce = Uuid::now_v7();
    let ready = exchange
        .challenge
        .next(Phase::Ready, std::process::id(), nonce)?;
    transcript.observe(&ready)?;
    sender.send(&mut exchange.codec, &ready, deadline).await?;
    let permit = exchange
        .receive
        .receive(&mut exchange.codec, deadline)
        .await?;
    transcript.observe(&permit)?;
    exchange.receive.terminal_eof(deadline).await?;
    let qualified = permit.next(Phase::Qualified, std::process::id(), nonce)?;
    transcript.observe(&qualified)?;
    sender
        .send(&mut exchange.codec, &qualified, deadline)
        .await?;
    state.close = Some(sender.close(deadline)?);
    state
        .close
        .as_mut()
        .context("missing child close task")?
        .wait_until(deadline)
        .await?;
    ensure!(
        transcript.finished(),
        "child qualification transcript incomplete"
    );
    remaining(deadline)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use locron_core::filesystem::{DirectoryGuard, create_private_new_exclusive, file_identity};
    use serde_json::json;

    use super::super::sha256_hex;
    use super::super::windows_fixture::PrivateFixture;
    use super::super::windows_ownership::native_target;
    use super::super::windows_receipt::{PAYLOADS, RECEIPT};
    use super::*;

    fn owner_fixture_lock() -> std::sync::MutexGuard<'static, ()> {
        static OWNER_FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        OWNER_FIXTURES
            .lock()
            .expect("owner-sensitive fixture mutex poisoned")
    }

    fn write_new(path: &Path, bytes: &[u8]) -> FileIdentity {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        file_identity(&file).unwrap()
    }

    struct Fixture {
        root: PathBuf,
        directory: PathBuf,
        installation: PathBuf,
        created: Option<CreatedHelper>,
        request_bytes: Vec<u8>,
        receipt_bytes: Vec<u8>,
        _private: PrivateFixture,
    }

    impl Fixture {
        fn new() -> Self {
            let private = PrivateFixture::new("locron-live-launch-fixture-");
            let root = private.path().join("operations");
            let directory = root.join(Uuid::now_v7().to_string());
            let installation = private.path().join("installation");
            let operation_guard = DirectoryGuard::private(&directory).unwrap();
            let install_guard = DirectoryGuard::private(&installation).unwrap();
            let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
            assert!(
                bytes.len() <= HELPER_LIMIT,
                "real copied fixture binary exceeds existing 64 MiB bound"
            );
            let mut files = BTreeMap::new();
            for name in PAYLOADS {
                let payload = if name == "locron.exe" {
                    bytes.clone()
                } else {
                    format!("read-only fixture companion {name}").into_bytes()
                };
                write_new(&installation.join(name), &payload);
                files.insert(name, sha256_hex(&payload));
            }
            let sid = locron_core::windows::current_user_sid().unwrap();
            let target = native_target().unwrap();
            let receipt = json!({
                "schema":"locron.install/windows-v1", "channel":"standalone", "sid":sid,
                "directory":installation, "executable":installation.join("locron.exe"),
                "target":target, "version":"0.10.0",
                "archive_url":format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{target}.zip"),
                "archive_sha256":"ab".repeat(32), "binary_sha256":files["locron.exe"],
                "files":files, "user_path":null
            });
            let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
            write_new(&installation.join(RECEIPT), &receipt_bytes);
            let helper_path = directory.join("locron-helper.exe");
            let helper_identity = write_new(&helper_path, &bytes);
            let operation = directory.file_name().unwrap().to_str().unwrap();
            let request_bytes = serde_json::to_vec(&json!({
                "schema":"locron.windows-operation/v1", "operation_id":operation,
                "kind":"uninstall", "sid":sid, "executable":installation.join("locron.exe"),
                "target":target, "version":"0.10.0", "archive_sha256":"ab".repeat(32),
                "helper_sha256":sha256_hex(&bytes), "caller_pid":null,
                "no_service":false, "dashboard":false, "add_to_path":false, "state_root":null
            }))
            .unwrap();
            write_new(&directory.join("request.json"), &request_bytes);
            drop(operation_guard);
            drop(install_guard);
            Self {
                root,
                directory,
                installation,
                created: Some(CreatedHelper {
                    path: helper_path,
                    identity: helper_identity,
                }),
                request_bytes,
                receipt_bytes,
                _private: private,
            }
        }

        fn input(&mut self) -> LaunchInput {
            LaunchInput {
                root: self.root.clone(),
                request: self.directory.join("request.json"),
                created: self.created.take().unwrap(),
                exit_before_ready: false,
                pause_after_spawn: None,
            }
        }

        fn unchanged(&self) {
            assert_eq!(
                fs::read(self.directory.join("request.json")).unwrap(),
                self.request_bytes
            );
            assert_eq!(
                fs::read(self.installation.join(RECEIPT)).unwrap(),
                self.receipt_bytes
            );
            assert!(!self.directory.join("journal.bin").exists());
            assert!(!self.directory.join("status.json").exists());
            assert!(!self._private.path().join("state").exists());
        }
    }

    fn owner_released() {
        let deadline = Instant::now() + CLEANUP;
        while OWNER.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
        assert!(
            !OWNER.load(Ordering::Acquire),
            "actual launch owner still live/quarantined"
        );
    }

    fn refuses_module_inputs(inputs: &[serde_json::Value]) {
        // Each fixture's cold adapter admission and every case use its one original clock.
        let deadline = Instant::now() + PHASE;
        for input in inputs {
            let error = locron_core::windows::run_script_json_bounded(
                MODULE_QUERY,
                input,
                remaining(deadline).unwrap(),
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("invalid actual child PID"),
                "malformed input must refuse before process lookup: {input}; got {error}"
            );
        }
    }

    #[test]
    fn module_query_refuses_absent_and_null_pid() {
        refuses_module_inputs(&[json!({}), json!({"pid": null})]);
    }

    #[test]
    fn module_query_refuses_non_integral_pid_types() {
        refuses_module_inputs(&[
            json!({"pid": "123"}),
            json!({"pid": true}),
            json!({"pid": 1.5}),
        ]);
    }

    #[test]
    fn module_query_refuses_nonpositive_and_out_of_range_pid() {
        refuses_module_inputs(&[
            json!({"pid": 0}),
            json!({"pid": -1}),
            json!({"pid": i64::from(i32::MAX) + 1}),
        ]);
    }

    #[test]
    fn advertised_budget_floors_and_refuses_sub_millisecond_roundup() {
        assert!(advertised_budget_ms(Duration::ZERO).is_err());
        assert!(advertised_budget_ms(Duration::from_nanos(999_999)).is_err());
        assert_eq!(advertised_budget_ms(Duration::from_millis(1)).unwrap(), 1);
        assert_eq!(
            advertised_budget_ms(Duration::from_nanos(1_999_999)).unwrap(),
            1
        );
        assert_eq!(advertised_budget_ms(PHASE).unwrap(), 30_000);
        assert_eq!(advertised_budget_ms(PHASE + POLL).unwrap(), 30_000);
    }

    #[test]
    fn child_horizon_is_anchored_before_io_and_never_extended_by_receipt() {
        let entry = Instant::now();
        let original = entry + PHASE;
        let receipt = entry + Duration::from_secs(12);
        let horizon = child_horizon(entry, original, 20_000).unwrap();
        assert_eq!(horizon, entry + Duration::from_secs(20));
        assert!(horizon < receipt + Duration::from_secs(20));
        assert_eq!(
            child_horizon(entry, entry + POLL, 30_000).unwrap(),
            entry + POLL
        );
        assert!(child_horizon(entry, original, 0).is_err());
        assert!(child_horizon(entry, original, 30_001).is_err());
    }

    #[test]
    fn copied_helper_entry() {
        let child_entry = Instant::now();
        let original = child_entry + PHASE;
        let Some(session) = std::env::var_os(SESSION) else {
            return;
        };
        // This exact selector entry never touches ordinary main/state discovery.
        if std::env::var(EXIT).is_ok_and(|value| value == "1") {
            return;
        }
        let spelling = session
            .into_string()
            .expect("selector session is not Unicode");
        let session = Uuid::parse_str(&spelling).unwrap();
        assert_eq!(spelling, session.to_string());
        let selector = ChildSelector {
            names: PipeNames::parse(
                session,
                std::env::var(PARENT_PIPE).unwrap(),
                std::env::var(CHILD_PIPE).unwrap(),
            )
            .unwrap(),
            request: PathBuf::from(std::env::var_os(REQUEST).unwrap()),
            root: PathBuf::from(std::env::var_os(ROOT).unwrap()),
        };
        let admission = Admission::acquire(original).unwrap();
        let (done, response) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            // The caller owns no native I/O and never joins this retained owner.
            let _admission = admission;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let (exchange, request, root) = runtime
                .block_on(begin_child(selector, child_entry, original))
                .unwrap();
            let current = within(exchange.deadline, || Ok(std::env::current_exe()?)).unwrap();
            let bootstrap = verify_at_until(&request, &root, &current, exchange.deadline).unwrap();
            let mut attempt = qualify_child(bootstrap, exchange);
            let finished = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(attempt.finish())
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("child qualification owner panicked")));
            let result = match finished {
                Ok(()) => attempt.into_qualified(),
                Err(error) => Err(Box::new(ChildRefusal {
                    state: attempt.state,
                    error,
                })),
            };
            match result {
                Ok(proof) => {
                    assert!(!proof.state.close.as_ref().unwrap().is_pending());
                    assert!(!proof.state.bootstrap.original.operation_id.is_nil());
                    let _ = done.send(Ok::<(), String>(()));
                    // Remain live with overlapping guards; only parent's native Child
                    // cleanup may stop this exact copied fixture process.
                    loop {
                        std::thread::park();
                    }
                }
                Err(mut refusal) => {
                    if let Some(close) = refusal.state.close.as_mut() {
                        if close.is_pending() {
                            let _ = runtime.block_on(close.wait_until(Instant::now() + CLEANUP));
                        }
                        if close.is_pending() {
                            loop {
                                std::thread::park();
                            }
                        }
                    }
                    let error = refusal.error.to_string();
                    let _ = done.send(Err(error));
                    // Retain the Bootstrap even after a known refusal; only actual
                    // process exit may end child-side overlap. Unknown raw I/O is
                    // retained with the runtime, not detached by an error return.
                    loop {
                        std::thread::park();
                    }
                }
            }
        });
        let result = response
            .recv_timeout((original + CLEANUP).saturating_duration_since(Instant::now()))
            .expect("child qualification owner remains retained after original deadline");
        result.expect("actual copied child refused qualification");
        // The caller never drops/joins that retained owner behind the parent.
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn actual_copied_child_guard_overlap_and_terminal_eof_qualify_without_effects() {
        let _owner_fixture_guard = owner_fixture_lock();
        let mut fixture = Fixture::new();
        let helper = fixture.directory.join("locron-helper.exe");
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.installation.join("README.md"))
                .is_ok()
        );
        let proof = Pending::start(fixture.input()).unwrap().qualify().unwrap();
        assert_ne!(proof.child_pid, std::process::id());
        assert!(OpenOptions::new().write(true).open(&helper).is_err());
        assert!(fs::rename(&helper, fixture.directory.join("helper-moved.exe")).is_err());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.installation.join("locron.exe"))
                .is_err()
        );
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.installation.join("README.md"))
                .is_err()
        );
        fixture.unchanged();
        proof.release().unwrap();
        owner_released();
        assert!(OpenOptions::new().write(true).open(&helper).is_ok());
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.installation.join("README.md"))
                .is_ok()
        );
        fixture.unchanged();
    }

    #[test]
    fn equal_bytes_replaced_helper_object_refuses_before_native_spawn() {
        let _owner_fixture_guard = owner_fixture_lock();
        let mut fixture = Fixture::new();
        let path = fixture.directory.join("locron-helper.exe");
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, fixture.directory.join("original-helper.exe")).unwrap();
        let substituted = write_new(&path, &bytes);
        assert_ne!(substituted, fixture.created.as_ref().unwrap().identity);
        let result = Pending::start(fixture.input()).unwrap().qualify();
        assert!(result.is_err());
        owner_released();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fixture.unchanged();
    }

    #[test]
    fn actual_owned_child_exit_before_readiness_refuses_and_releases_guards() {
        let _owner_fixture_guard = owner_fixture_lock();
        let mut fixture = Fixture::new();
        let mut input = fixture.input();
        input.exit_before_ready = true;
        assert!(Pending::start(input).unwrap().qualify().is_err());
        owner_released();
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.directory.join("locron-helper.exe"))
                .is_ok()
        );
        fixture.unchanged();
    }

    #[test]
    fn uncertain_owner_retains_actual_child_and_guards_without_late_token_or_new_admission() {
        let _owner_fixture_guard = owner_fixture_lock();
        let mut fixture = Fixture::new();
        let (release, pause) = mpsc::sync_channel(1);
        let (reached, spawned) = mpsc::sync_channel(1);
        let mut input = fixture.input();
        input.pause_after_spawn = Some(SpawnPause {
            reached,
            release: pause,
        });
        // Prove actual spawn before exercising the original deadline. There is no
        // short-budget setup assumption, warm-up, retry or fabricated Child DTO.
        let deadline = Instant::now() + PHASE;
        let pending = Pending::start_until(input, deadline).unwrap();
        let pid = spawned.recv_timeout(remaining(deadline).unwrap()).unwrap();
        assert_ne!(pid, 0);
        assert_ne!(pid, std::process::id());
        assert!(pending.qualify().is_err());
        assert!(OWNER.load(Ordering::Acquire));
        let helper = fixture.directory.join("locron-helper.exe");
        assert!(OpenOptions::new().write(true).open(&helper).is_err());
        // This unexecuted companion proves lease retention independently of the
        // kernel's mapped-image write restriction on locron-helper.exe.
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.installation.join("README.md"))
                .is_err()
        );
        assert!(Admission::acquire(Instant::now() + PHASE).is_err());
        fixture.unchanged();
        release.send(()).unwrap();
        owner_released();
        assert!(OpenOptions::new().write(true).open(helper).is_ok());
        fixture.unchanged();
    }

    #[test]
    fn expired_phase_refuses_without_touching_the_existing_bootstrap_or_spawning() {
        let _owner_fixture_guard = owner_fixture_lock();
        let mut fixture = Fixture::new();
        let deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(Pending::start_until(fixture.input(), deadline).is_err());
        assert!(!OWNER.load(Ordering::Acquire));
        fixture.unchanged();
    }
}
