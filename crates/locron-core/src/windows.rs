//! Narrow, fixed Windows filesystem adapters shared by the workspace.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;

const ADAPTER_TIMEOUT: Duration = Duration::from_secs(30);
const OUTPUT_LIMIT: u64 = 128 * 1024;
static USER_SID: OnceLock<String> = OnceLock::new();
// One generic child plus the separate single fixed filesystem child, including idle retention.
static ADAPTER_WORKERS: WorkerPermits = WorkerPermits::new(1);
static SID_INITIALIZER: WorkerPermits = WorkerPermits::new(1);

mod filesystem_worker;
mod owned_child;
mod stock;
pub use stock::{StockAdapterGuard, StockModuleSet};
/// Compiled binary-only bootstrap. Paths are guarded environment data, never source.
pub const STOCK_JSON_BOOTSTRAP: &str = include_str!("windows/stock_json.ps1");
#[cfg(test)]
mod generic_trace;

/// Only short counter updates run while this mutex is held; never process or I/O work.
struct WorkerPermits {
    active: Mutex<usize>,
    changed: Condvar,
    capacity: usize,
}

impl WorkerPermits {
    const fn new(capacity: usize) -> Self {
        Self {
            active: Mutex::new(0),
            changed: Condvar::new(),
            capacity,
        }
    }

    fn lock_until(&self, deadline: Instant) -> io::Result<MutexGuard<'_, usize>> {
        loop {
            match self.active.try_lock() {
                Ok(active) => return Ok(active),
                Err(TryLockError::Poisoned(_)) => {
                    return Err(io::Error::other("Windows adapter permit state failed"));
                }
                Err(TryLockError::WouldBlock) => {
                    let remaining = remaining(deadline)?;
                    std::thread::sleep(remaining.min(Duration::from_millis(1)));
                }
            }
        }
    }

    fn acquire(&self, deadline: Instant) -> io::Result<WorkerPermit<'_>> {
        let mut active = self.lock_until(deadline)?;
        loop {
            let wait = remaining(deadline)?;
            if *active < self.capacity {
                *active += 1;
                return Ok(WorkerPermit { pool: self });
            }
            let (next, _) = self
                .changed
                .wait_timeout(active, wait)
                .map_err(|_| io::Error::other("Windows adapter permit state failed"))?;
            active = next;
        }
    }
}

struct WorkerPermit<'a> {
    pool: &'a WorkerPermits,
}

impl Drop for WorkerPermit<'_> {
    fn drop(&mut self) {
        let mut active = self
            .pool
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *active -= 1;
        self.pool.changed.notify_all();
    }
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "stock Windows adapter deadline elapsed",
        ))
    } else {
        Ok(duration)
    }
}

/// Returns the absolute stock Windows PowerShell 5.1 executable.
pub fn stock_powershell() -> io::Result<PathBuf> {
    // SystemRoot is supplied by Windows, but do not accept a relative override.
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| io::Error::other("Windows SystemRoot is unavailable"))?;
    Ok(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
}

/// Runs a fixed, caller-reviewed adapter with JSON input and bounded JSON output.
///
/// Source must be a static string. Paths and options belong in `input`, never in source.
pub fn run_script_json(script: &'static str, input: &Value) -> io::Result<Value> {
    run_script_with_timeout(script, input, ADAPTER_TIMEOUT)
}

/// Runs an adapter within the caller's remaining operation budget, capped at thirty seconds.
/// Owned child termination confirmation may add the separate three-second cleanup bound.
pub fn run_script_json_bounded(
    script: &'static str,
    input: &Value,
    timeout: Duration,
) -> io::Result<Value> {
    run_script_with_timeout(script, input, timeout.min(ADAPTER_TIMEOUT))
}

fn run_script_with_timeout(
    script: &'static str,
    input: &Value,
    timeout: Duration,
) -> io::Result<Value> {
    run_script_with_deadline(script, input, Instant::now() + timeout)
}

fn run_script_with_deadline(
    script: &'static str,
    input: &Value,
    deadline: Instant,
) -> io::Result<Value> {
    let request = prepare_adapter(script, input)?;
    // Queue on the caller, before creating a thread/process. Queue time consumes the same budget.
    #[cfg(not(test))]
    let permit = ADAPTER_WORKERS.acquire(deadline)?;
    #[cfg(test)]
    let permit = ADAPTER_WORKERS
        .acquire(deadline)
        .inspect_err(|_| request.trace.report("permit-failed"))?;
    #[cfg(test)]
    request.trace.record("permit-acquired", 0);
    run_adapter_worker(request, deadline, permit)
}

struct AdapterRequest {
    encoded: String,
    input: Vec<u8>,
    #[cfg(test)]
    trace: std::sync::Arc<generic_trace::Trace>,
}

fn prepare_adapter(script: &'static str, input: &Value) -> io::Result<AdapterRequest> {
    #[cfg(test)]
    let trace = std::sync::Arc::new(generic_trace::Trace::new());
    let request = serde_json::to_vec(input).map_err(io::Error::other)?;
    if request.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows adapter input is too large",
        ));
    }
    #[cfg(not(test))]
    let source = format!(
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; [Console]::InputEncoding = [Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); try {{ {STOCK_JSON_BOOTSTRAP}\n$request = [Console]::In.ReadToEnd() | & $locronFromJson; {script}\n; }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    );
    #[cfg(test)]
    let source = generic_trace::source(script);
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        source
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    Ok(AdapterRequest {
        encoded,
        input: request,
        #[cfg(test)]
        trace,
    })
}

fn run_adapter_worker(
    request: AdapterRequest,
    deadline: Instant,
    permit: WorkerPermit<'static>,
) -> io::Result<Value> {
    #[cfg(test)]
    let trace = std::sync::Arc::clone(&request.trace);
    // A dedicated thread permits callers already inside Tokio; no async type crosses this API.
    let (reply, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("locron-windows-adapter".to_owned())
        .spawn(move || {
            // Keep the permit until all owned child/pipe cleanup completes, including errors.
            let _permit = permit;
            #[cfg(test)]
            request.trace.record("worker-entered", 0);
            let guard = match StockAdapterGuard::acquire_until(
                StockModuleSet::UtilityAndManagement,
                deadline,
            ) {
                Ok(guard) => guard,
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return;
                }
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return;
                }
            };
            let outcome = runtime.block_on(run_adapter(request, guard, deadline));
            let _ = reply.send(outcome.result);
            if let Some(ownership) = outcome.quarantine {
                // Retain runtime, all ownership and the slot; no next process/input/replay.
                let _ownership = ownership;
                loop {
                    std::thread::park();
                }
            }
        })?;
    let caller_deadline = deadline + Duration::from_secs(3);
    let mut result = receiver
        .recv_timeout(caller_deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => io::Error::new(
                io::ErrorKind::TimedOut,
                "Windows adapter ownership remains pending after deadline",
            ),
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                io::Error::other("Windows adapter worker failed")
            }
        })
        .and_then(std::convert::identity);
    if result.is_ok() && remaining(deadline).is_err() {
        result = Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "late Windows adapter result refused",
        ));
    }
    if worker.is_finished() {
        let _ = worker.join();
    }
    #[cfg(test)]
    if result.is_err() {
        trace.report("caller-failed");
    }
    result
}

struct AdapterOutcome {
    result: io::Result<Value>,
    quarantine: Option<AdapterQuarantine>,
}

enum AdapterQuarantine {
    Spawn {
        _failure: owned_child::SpawnFailure,
        _guard: StockAdapterGuard,
    },
    Running {
        _child: owned_child::OwnedChild,
        _guard: StockAdapterGuard,
        _pipes: Option<AdapterPipes>,
    },
}

struct AdapterPipes {
    writer: tokio::task::JoinHandle<io::Result<()>>,
    output: tokio::task::JoinHandle<io::Result<Vec<u8>>>,
    errors: tokio::task::JoinHandle<io::Result<Vec<u8>>>,
}

impl AdapterOutcome {
    fn failed(error: io::Error) -> Self {
        Self {
            result: Err(error),
            quarantine: None,
        }
    }
}

fn configure_stock_command(command: &mut tokio::process::Command, guard: &StockAdapterGuard) {
    command
        .env_remove("PSModulePath")
        .env("LOCRON_STOCK_UTILITY", guard.utility());
    if let Some(management) = guard.management() {
        command.env("LOCRON_STOCK_MANAGEMENT", management);
    } else {
        command.env_remove("LOCRON_STOCK_MANAGEMENT");
    }
}

async fn run_adapter(
    request: AdapterRequest,
    guard: StockAdapterGuard,
    deadline: Instant,
) -> AdapterOutcome {
    use tokio::io::AsyncWriteExt;

    if let Err(error) = remaining(deadline) {
        return AdapterOutcome::failed(error);
    }
    let mut command = tokio::process::Command::new(guard.executable());
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &request.encoded,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_stock_command(&mut command, &guard);
    let mut child = match owned_child::OwnedChild::spawn(command) {
        Ok(child) => child,
        Err(owned_child::SpawnFailure::NotStarted(error)) => return AdapterOutcome::failed(error),
        Err(failure @ owned_child::SpawnFailure::Unconfirmed { .. }) => {
            let owned_child::SpawnFailure::Unconfirmed { ref error, .. } = failure else {
                unreachable!()
            };
            return AdapterOutcome {
                result: Err(io::Error::new(error.kind(), error.to_string())),
                quarantine: Some(AdapterQuarantine::Spawn {
                    _failure: failure,
                    _guard: guard,
                }),
            };
        }
    };
    #[cfg(test)]
    request
        .trace
        .record("spawn-complete", child.id().unwrap_or_default());
    if let Err(error) = remaining(deadline) {
        return failed_adapter(error, child, guard, None).await;
    }
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.take_stdin(), child.take_stdout(), child.take_stderr())
    else {
        return failed_adapter(io::Error::other("missing adapter pipe"), child, guard, None).await;
    };
    #[cfg(not(test))]
    let writer =
        tokio::spawn(
            async move { adapter_poll_until(deadline, stdin.write_all(&request.input)).await },
        );
    #[cfg(test)]
    let writer_trace = std::sync::Arc::clone(&request.trace);
    #[cfg(test)]
    let writer = tokio::spawn(async move {
        adapter_poll_until(deadline, stdin.write_all(&request.input)).await?;
        writer_trace.record("input-written", 0);
        Ok::<_, io::Error>(())
    });
    let output = tokio::spawn(capture_output(stdout));
    #[cfg(not(test))]
    let errors = tokio::spawn(capture_output(stderr));
    #[cfg(test)]
    let errors = tokio::spawn(generic_trace::capture_stderr(
        stderr,
        std::sync::Arc::clone(&request.trace),
        OUTPUT_LIMIT,
    ));
    let mut pipes = AdapterPipes {
        writer,
        output,
        errors,
    };
    let operation = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
        let ((), output, errors, status) = tokio::try_join!(
            async { (&mut pipes.writer).await.map_err(io::Error::other)? },
            async { (&mut pipes.output).await.map_err(io::Error::other)? },
            async { (&mut pipes.errors).await.map_err(io::Error::other)? },
            child.confirm_exit_until(deadline),
        )?;
        Ok::<_, io::Error>((output, errors, status))
    })
    .await;
    let (output, errors, status) = match operation {
        Ok(Ok(result)) => result,
        failed => {
            let error = match failed {
                Ok(Err(error)) => error,
                Err(_) => {
                    io::Error::new(io::ErrorKind::TimedOut, "stock Windows adapter timed out")
                }
                Ok(Ok(_)) => unreachable!(),
            };
            let result = failed_adapter(error, child, guard, Some(pipes)).await;
            #[cfg(test)]
            if result.quarantine.is_none() {
                request.trace.record("cleanup-confirmed", 0);
            }
            return result;
        }
    };
    #[cfg(test)]
    request.trace.record("root-completed", 0);
    if let Err(error) = remaining(deadline) {
        return AdapterOutcome::failed(error);
    }
    if !status.success() {
        return AdapterOutcome::failed(io::Error::other(format!(
            "stock Windows adapter failed: {}",
            String::from_utf8_lossy(&errors).trim()
        )));
    }
    let result = serde_json::from_slice(&output).map_err(io::Error::other);
    if let Err(error) = remaining(deadline) {
        return AdapterOutcome::failed(error);
    }
    AdapterOutcome {
        result,
        quarantine: None,
    }
}

async fn adapter_poll_until<T>(
    deadline: Instant,
    future: impl std::future::Future<Output = io::Result<T>>,
) -> io::Result<T> {
    use std::future::poll_fn;
    use std::task::Poll;
    let mut future = std::pin::pin!(future);
    poll_fn(|context| {
        if let Err(error) = remaining(deadline) {
            return Poll::Ready(Err(error));
        }
        let polled = future.as_mut().poll(context);
        if polled.is_ready()
            && let Err(error) = remaining(deadline)
        {
            return Poll::Ready(Err(error));
        }
        polled
    })
    .await
}

async fn failed_adapter(
    error: io::Error,
    mut child: owned_child::OwnedChild,
    guard: StockAdapterGuard,
    mut pipes: Option<AdapterPipes>,
) -> AdapterOutcome {
    if let Some(pipes) = pipes.as_ref() {
        pipes.writer.abort();
        pipes.output.abort();
        pipes.errors.abort();
    }
    let cleanup_deadline = Instant::now() + Duration::from_secs(3);
    let _ = child.start_kill();
    let cleanup =
        tokio::time::timeout_at(tokio::time::Instant::from_std(cleanup_deadline), async {
            child.confirm_exit_until(cleanup_deadline).await?;
            if let Some(pipes) = pipes.as_mut() {
                if !pipes.writer.is_finished() {
                    let _ = (&mut pipes.writer).await;
                }
                if !pipes.output.is_finished() {
                    let _ = (&mut pipes.output).await;
                }
                if !pipes.errors.is_finished() {
                    let _ = (&mut pipes.errors).await;
                }
            }
            remaining(cleanup_deadline)?;
            Ok::<_, io::Error>(())
        })
        .await;
    if matches!(cleanup, Ok(Ok(()))) {
        AdapterOutcome::failed(error)
    } else {
        AdapterOutcome {
            result: Err(io::Error::other(
                "could not confirm Windows adapter termination",
            )),
            quarantine: Some(AdapterQuarantine::Running {
                _child: child,
                _guard: guard,
                _pipes: pipes,
            }),
        }
    }
}

async fn capture_output(stream: impl tokio::io::AsyncRead + Unpin) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    stream
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > OUTPUT_LIMIT as usize {
        return Err(io::Error::other(
            "Windows adapter exceeded its output limit",
        ));
    }
    Ok(bytes)
}

/// Returns the actual current token's SID, independent of username environment text.
pub fn current_user_sid() -> io::Result<String> {
    cached_sid(
        &USER_SID,
        &SID_INITIALIZER,
        Instant::now() + ADAPTER_TIMEOUT,
        query_sid,
    )
}

fn cached_sid(
    cache: &OnceLock<String>,
    initializer: &WorkerPermits,
    deadline: Instant,
    query: impl FnOnce(Instant) -> io::Result<String>,
) -> io::Result<String> {
    if let Some(sid) = cache.get() {
        return Ok(sid.clone());
    }
    let _permit = initializer.acquire(deadline)?;
    if let Some(sid) = cache.get() {
        return Ok(sid.clone());
    }
    let sid = query(deadline)?;
    let _ = cache.set(sid.clone());
    Ok(sid)
}

fn query_sid(deadline: Instant) -> io::Result<String> {
    let result = filesystem_worker::request("sid", None, deadline)?;
    let sid = result
        .as_str()
        .filter(|sid| {
            sid.starts_with("S-1-")
                && sid
                    .split('-')
                    .skip(1)
                    .all(|part| part.parse::<u64>().is_ok())
        })
        .ok_or_else(|| io::Error::other("Windows identity adapter returned an invalid SID"))?
        .to_owned();
    Ok(sid)
}

pub(crate) fn create_private_directory(path: &std::path::Path) -> io::Result<()> {
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    filesystem_worker::request("create_directory", Some(adapter_path(path)?), deadline)?;
    Ok(())
}

pub(crate) fn create_private_file(path: &std::path::Path) -> io::Result<()> {
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let result = filesystem_worker::request("create_file", Some(adapter_path(path)?), deadline)?;
    if result["created"].as_bool() == Some(true) {
        return Ok(());
    }
    match result["win32_error"].as_i64() {
        Some(code @ (80 | 183)) => Err(io::Error::from_raw_os_error(code as i32)),
        _ => Err(io::Error::other("invalid private file creation result")),
    }
}

fn adapter_path(path: &std::path::Path) -> io::Result<&str> {
    path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "stock Windows adapters require a Unicode path",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_generic_adapter_emits_ordered_child_phases_without_rendering_input() {
        let request = prepare_adapter(
            "@{echo=[string]$request.echo}|& $locronToJson -Compress",
            &json!({"echo":"fixture 日本語 % #"}),
        )
        .unwrap();
        let trace = std::sync::Arc::clone(&request.trace);
        let deadline = Instant::now() + ADAPTER_TIMEOUT;
        let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
        let result = run_adapter_worker(request, deadline, permit).unwrap();
        assert_eq!(result["echo"], "fixture 日本語 % #");
        assert_eq!(
            trace.child_phases(),
            [
                "source-entry",
                "encoding-ready",
                "binding-start",
                "binding-ready",
                "input-complete",
                "json-start",
                "json-parsed",
                "caller-start",
                "caller-complete"
            ]
        );
    }

    #[test]
    fn saturated_permits_use_the_original_deadline_and_release_on_drop() {
        let pool = WorkerPermits::new(2);
        let first = pool
            .acquire(Instant::now() + Duration::from_secs(1))
            .unwrap();
        let second = pool
            .acquire(Instant::now() + Duration::from_secs(1))
            .unwrap();
        let started = Instant::now();
        let result = pool.acquire(started + Duration::from_millis(50));
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(first);
        let replacement = pool
            .acquire(Instant::now() + Duration::from_secs(1))
            .unwrap();
        drop((second, replacement));
        assert_eq!(*pool.active.lock().unwrap(), 0);
    }

    #[test]
    fn concurrent_sid_queries_share_one_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = OnceLock::new();
        let initializer = WorkerPermits::new(1);
        let queries = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let sid = cached_sid(
                        &cache,
                        &initializer,
                        Instant::now() + Duration::from_secs(2),
                        |_| {
                            queries.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(25));
                            Ok("S-1-5-18".to_owned())
                        },
                    )
                    .unwrap();
                    assert_eq!(sid, "S-1-5-18");
                });
            }
        });
        assert_eq!(queries.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_sid_initializer_wakes_a_waiter_without_resetting_its_deadline() {
        let cache = OnceLock::new();
        let initializer = WorkerPermits::new(1);
        let (started, ready) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                cached_sid(
                    &cache,
                    &initializer,
                    Instant::now() + Duration::from_secs(1),
                    |_| {
                        started.send(()).unwrap();
                        std::thread::sleep(Duration::from_millis(50));
                        Err(io::Error::other("owned query failure"))
                    },
                )
            });
            ready.recv_timeout(Duration::from_secs(1)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            let sid = cached_sid(&cache, &initializer, deadline, |forwarded| {
                assert_eq!(forwarded, deadline);
                Ok("S-1-5-18".to_owned())
            })
            .unwrap();
            assert_eq!(sid, "S-1-5-18");
            assert!(first.join().unwrap().is_err());
        });
    }

    #[test]
    fn sid_initializer_wait_expires_without_starting_another_query() {
        let cache = OnceLock::new();
        let initializer = WorkerPermits::new(1);
        let held = initializer
            .acquire(Instant::now() + Duration::from_secs(1))
            .unwrap();
        let start = Instant::now();
        let error = cached_sid(
            &cache,
            &initializer,
            start + Duration::from_millis(50),
            |_| panic!("a saturated initializer must not start another query"),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
        drop(held);
    }

    #[test]
    fn adapter_rejects_large_input_before_spawn() {
        assert!(
            run_script_json(
                "@{} | & $locronToJson -Compress",
                &json!({"large": "x".repeat(70 * 1024)})
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn adapter_io_does_not_accept_a_ready_result_after_its_original_deadline() {
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("fixture monotonic clock has an earlier instant");
        let polls = std::cell::Cell::new(0);
        let future = std::future::poll_fn(|_| {
            polls.set(polls.get() + 1);
            std::task::Poll::Ready(Ok(()))
        });
        assert_eq!(
            adapter_poll_until(expired, future)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(polls.get(), 0);
        let deadline = Instant::now() + Duration::from_millis(20);
        let future = std::future::poll_fn(|_| {
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            std::task::Poll::Ready(Ok(()))
        });
        assert_eq!(
            adapter_poll_until(deadline, future)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn adapter_startup_and_script_wait_are_bounded() {
        // Acquire separately so this test exercises owned startup/input cleanup, not queue timeout.
        let request = prepare_adapter(
            "Start-Sleep -Seconds 60; @{} | & $locronToJson -Compress",
            &json!({"input": "x".repeat(60 * 1024)}),
        )
        .unwrap();
        let permit = ADAPTER_WORKERS
            .acquire(Instant::now() + ADAPTER_TIMEOUT)
            .unwrap();
        let started = std::time::Instant::now();
        let result = run_adapter_worker(request, started + Duration::from_millis(100), permit);
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn adapter_reaps_a_script_that_has_entered_its_wait() {
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("owned-script-entered.txt");
        let request = prepare_adapter(
            "[IO.File]::WriteAllText([string]$request.marker, 'entered'); Start-Sleep -Seconds 60; @{} | & $locronToJson -Compress",
            &json!({"marker": marker}),
        ).unwrap();
        let permit = ADAPTER_WORKERS
            .acquire(Instant::now() + ADAPTER_TIMEOUT)
            .unwrap();
        let started = Instant::now();
        let error = run_adapter_worker(request, started + ADAPTER_TIMEOUT, permit).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "entered");
        assert!(started.elapsed() < ADAPTER_TIMEOUT + Duration::from_secs(3));
    }

    #[test]
    fn adapter_output_is_bounded() {
        for script in [
            "[IO.File]::WriteAllText([string]$request.marker, 'entered'); [Console]::Write(('x' * 200000)); [Threading.Thread]::Sleep(60000)",
            "[IO.File]::WriteAllText([string]$request.marker, 'entered'); [Console]::Error.Write(('x' * 200000)); [Threading.Thread]::Sleep(60000)",
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let marker = temporary.path().join("owned-output-phase.txt");
            let request = prepare_adapter(script, &json!({"marker": marker})).unwrap();
            // Queue admission is covered separately. Measure the owned child's cap and cleanup.
            let permit = ADAPTER_WORKERS
                .acquire(Instant::now() + ADAPTER_TIMEOUT)
                .unwrap();
            let started = Instant::now();
            let error = run_adapter_worker(request, started + ADAPTER_TIMEOUT, permit).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!(
                error.to_string(),
                "Windows adapter exceeded its output limit"
            );
            assert_eq!(std::fs::read_to_string(&marker).unwrap(), "entered");
            assert!(started.elapsed() < ADAPTER_TIMEOUT + Duration::from_secs(3));
            // Measure refusal/cleanup after real script entry, independently of stock bootstrap.
            assert!(
                std::fs::metadata(&marker)
                    .unwrap()
                    .modified()
                    .unwrap()
                    .elapsed()
                    .unwrap()
                    < Duration::from_secs(5)
            );
        }
    }
}
