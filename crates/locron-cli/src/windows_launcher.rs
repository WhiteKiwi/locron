//! Finite, read-only GUI entry facts without state discovery or activation authority.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsHandle;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use serde::Serialize;

const STARTUP_LIMIT: Duration = Duration::from_secs(30);
const PROBE_LIMIT: usize = 4 * 1024;
const FAILURE_EXIT: i32 = 70;
const INVALID_ARGUMENTS_EXIT: i32 = 2;
const FILE_SHARE_READ_WRITE: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryCommand {
    Version,
    IdentityProbe,
    // Structural role arguments remain untrusted until the service-owned wire consumer exists.
    RolePending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Failure {
    Expired,
    InvalidArguments,
    UnknownConsoleError,
    UnsupportedTarget,
    ActivationUnavailable,
    Output,
}

impl Failure {
    const fn exit_code(self) -> i32 {
        match self {
            Self::InvalidArguments => INVALID_ARGUMENTS_EXIT,
            _ => FAILURE_EXIT,
        }
    }
}

#[derive(Clone)]
struct Budget {
    deadline: Instant,
    admitted: Arc<AtomicBool>,
}

impl Budget {
    fn check(&self) -> Result<(), Failure> {
        if self.admitted.load(Ordering::Acquire) && Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(Failure::Expired)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConsoleFact {
    opened: bool,
    error: Option<i32>,
}

impl ConsoleFact {
    fn from_open(result: io::Result<()>) -> Result<Self, Failure> {
        match result {
            Ok(()) => Ok(Self {
                opened: true,
                error: None,
            }),
            Err(error) => match error.raw_os_error().filter(|code| *code != 0) {
                Some(code) => Ok(Self {
                    opened: false,
                    error: Some(code),
                }),
                None => Err(Failure::UnknownConsoleError),
            },
        }
    }
}

#[derive(Serialize)]
struct IdentityProbe {
    schema: &'static str,
    version: &'static str,
    target: &'static str,
    launcher_abi: &'static str,
    initial_conout_opened: bool,
    initial_conout_error: Option<i32>,
}

fn release_target() -> Result<&'static str, Failure> {
    match (std::env::consts::ARCH, cfg!(target_env = "msvc")) {
        ("x86_64", true) => Ok("x86_64-pc-windows-msvc"),
        ("aarch64", true) => Ok("aarch64-pc-windows-msvc"),
        _ => Err(Failure::UnsupportedTarget),
    }
}

fn parse(arguments: &[OsString]) -> Result<EntryCommand, Failure> {
    if arguments.len() == 1 {
        return match arguments[0].to_str() {
            Some("--version") => Ok(EntryCommand::Version),
            Some("--identity-probe") => Ok(EntryCommand::IdentityProbe),
            _ => Err(Failure::InvalidArguments),
        };
    }
    if !matches!(arguments.len(), 4 | 6)
        || arguments[0] != "--state-dir"
        || arguments[1].is_empty()
        || arguments[2] != "--role"
        || !matches!(arguments[3].to_str(), Some("daemon" | "dashboard"))
        || (arguments.len() == 6
            && (arguments[4] != "--run-capability" || arguments[5].to_str().is_none()))
    {
        return Err(Failure::InvalidArguments);
    }
    // No capability validation, endpoint discovery, child dispatch or witness is invented here.
    // The reviewed standalone activation wire will own the next stage's exact models/validators.
    Ok(EntryCommand::RolePending)
}

fn initial_console(budget: &Budget) -> Result<ConsoleFact, Failure> {
    budget.check()?;
    let result = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE)
        .open("CONOUT$")
        .map(drop);
    budget.check()?;
    ConsoleFact::from_open(result)
}

fn probe_output(command: EntryCommand, console: ConsoleFact) -> Result<Vec<u8>, Failure> {
    let target = release_target()?;
    let output = match command {
        EntryCommand::Version => {
            format!("locron-service-launcher {}\n", env!("CARGO_PKG_VERSION")).into_bytes()
        }
        EntryCommand::IdentityProbe => serde_json::to_vec(&IdentityProbe {
            schema: "locron.windows-launcher-probe/v1",
            version: env!("CARGO_PKG_VERSION"),
            target,
            launcher_abi: "native-gui-v1",
            initial_conout_opened: console.opened,
            initial_conout_error: console.error,
        })
        .map_err(|_| Failure::Output)?,
        EntryCommand::RolePending => return Err(Failure::ActivationUnavailable),
    };
    if output.len() > PROBE_LIMIT {
        return Err(Failure::Output);
    }
    Ok(output)
}

fn write_probe(budget: &Budget, sink: &mut impl Write, output: &[u8]) -> Result<(), Failure> {
    budget.check()?;
    if output.len() > PROBE_LIMIT {
        return Err(Failure::Output);
    }
    let mut remaining = output;
    while !remaining.is_empty() {
        budget.check()?;
        let written = sink.write(remaining);
        budget.check()?;
        let written = written.map_err(|_| Failure::Output)?;
        if written == 0 || written > remaining.len() {
            return Err(Failure::Output);
        }
        remaining = &remaining[written..];
    }
    budget.check()?;
    let flushed = sink.flush();
    budget.check()?;
    flushed.map_err(|_| Failure::Output)
}

fn duplicate_output(budget: &Budget, output: &impl AsHandle) -> Result<File, Failure> {
    budget.check()?;
    let duplicate = output.as_handle().try_clone_to_owned();
    budget.check()?;
    // A cloned NULL handle still must pass real I/O; ownership alone is not output proof.
    duplicate.map(File::from).map_err(|_| Failure::Output)
}

fn entry_work(budget: &Budget, arguments: &[OsString]) -> Result<i32, Failure> {
    // This is the worker's first native operation, before parsing or any other entry work.
    let console = initial_console(budget)?;
    let command = parse(arguments)?;
    budget.check()?;
    let output = probe_output(command, console)?;
    budget.check()?;
    let stdout = io::stdout();
    // Use the inherited handle without the global line buffer, which Rust exit cleanup
    // could otherwise flush after expiry.
    let mut output_handle = duplicate_output(budget, &stdout)?;
    let result = write_probe(budget, &mut output_handle, &output);
    drop(output_handle);
    budget.check()?;
    result?;
    Ok(0)
}

fn drive(
    entered: Instant,
    limit: Duration,
    work: impl FnOnce(&Budget) -> Result<i32, Failure> + Send + 'static,
) -> i32 {
    let Some(deadline) = entered.checked_add(limit) else {
        return FAILURE_EXIT;
    };
    let budget = Budget {
        deadline,
        admitted: Arc::new(AtomicBool::new(true)),
    };
    if budget.check().is_err() {
        return FAILURE_EXIT;
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker_budget = budget.clone();
    let Ok(worker) = std::thread::Builder::new()
        .name("locron-gui-entry".into())
        .spawn(move || {
            let result = worker_budget.check().and_then(|()| work(&worker_budget));
            // All owned console/stdout guards have left work before this finite response.
            let _ = sender.send(result);
        })
    else {
        return FAILURE_EXIT;
    };
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    if budget.check().is_err() {
        budget.admitted.store(false, Ordering::Release);
        // Dropping the thread handle never joins unfinished native work. The worker retains
        // everything it is using until completion or the thin entry's immediate process exit.
        return FAILURE_EXIT;
    }
    let Ok(result) = response else {
        budget.admitted.store(false, Ordering::Release);
        return FAILURE_EXIT;
    };
    while !worker.is_finished() {
        if budget.check().is_err() {
            budget.admitted.store(false, Ordering::Release);
            return FAILURE_EXIT;
        }
        std::thread::sleep(
            Duration::from_millis(1).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    // Keep native finalization outside the caller: even after a response, never join a thread
    // across the deadline. Finished workers no longer own entry I/O; their handle may drop.
    if budget.check().is_err() {
        budget.admitted.store(false, Ordering::Release);
        return FAILURE_EXIT;
    }
    result.unwrap_or_else(Failure::exit_code)
}

pub(crate) fn run(entered: Instant, arguments: Vec<OsString>) -> i32 {
    drive(entered, STARTUP_LIMIT, move |budget| {
        entry_work(budget, &arguments)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use super::{
        Arc, AtomicBool, Budget, ConsoleFact, Duration, EntryCommand, FAILURE_EXIT, Failure, File,
        Instant, Ordering, OsString, PROBE_LIMIT, Write, drive, duplicate_output, io, mpsc, parse,
        write_probe,
    };

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn structural_arguments_are_exact_and_secrets_never_enter_errors() {
        assert_eq!(parse(&arguments(&["--version"])), Ok(EntryCommand::Version));
        assert_eq!(
            parse(&arguments(&["--identity-probe"])),
            Ok(EntryCommand::IdentityProbe)
        );
        for role in ["daemon", "dashboard"] {
            assert_eq!(
                parse(&arguments(&[
                    "--state-dir",
                    "C:\\private 状態",
                    "--role",
                    role
                ])),
                Ok(EntryCommand::RolePending)
            );
            assert_eq!(
                parse(&arguments(&[
                    "--state-dir",
                    "C:\\private 状態",
                    "--role",
                    role,
                    "--run-capability",
                    "v1:SECRET-CAPABILITY",
                ])),
                Ok(EntryCommand::RolePending)
            );
        }
        for values in [
            vec![],
            vec!["--version", "SECRET-CAPABILITY"],
            vec!["--identity-probe", "--identity-probe"],
            vec!["--state-dir", "", "--role", "daemon"],
            vec!["--role", "daemon", "--state-dir", "C:\\private"],
            vec!["--state-dir", "C:\\private", "--role", "SECRET-CAPABILITY"],
            vec![
                "--state-dir",
                "C:\\private",
                "--role",
                "daemon",
                "--state-dir",
                "SECRET-CAPABILITY",
            ],
        ] {
            let failure = parse(&arguments(&values)).unwrap_err();
            assert_eq!(failure, Failure::InvalidArguments);
            assert!(!format!("{failure:?}").contains("SECRET-CAPABILITY"));
        }
    }

    #[test]
    fn invalid_unicode_options_roles_and_capabilities_refuse_without_rendering() {
        use std::os::windows::ffi::OsStringExt;

        let invalid = OsString::from_wide(&[0xd800]);
        for index in [0, 2, 3, 4, 5] {
            let mut values = arguments(&[
                "--state-dir",
                "C:\\private",
                "--role",
                "daemon",
                "--run-capability",
                "v1:SECRET-CAPABILITY",
            ]);
            values[index] = invalid.clone();
            assert_eq!(parse(&values), Err(Failure::InvalidArguments));
        }
        let mut path = arguments(&["--state-dir", "state", "--role", "daemon"]);
        path[1] = invalid;
        assert_eq!(parse(&path), Ok(EntryCommand::RolePending));
    }

    #[test]
    fn console_facts_preserve_raw_errors_without_attachment_inference() {
        assert_eq!(
            ConsoleFact::from_open(Ok(())),
            Ok(ConsoleFact {
                opened: true,
                error: None,
            })
        );
        for code in [2, 3, 5, 6, 32, -1] {
            assert_eq!(
                ConsoleFact::from_open(Err(io::Error::from_raw_os_error(code))),
                Ok(ConsoleFact {
                    opened: false,
                    error: Some(code),
                })
            );
        }
        assert_eq!(
            ConsoleFact::from_open(Err(io::Error::from_raw_os_error(0))),
            Err(Failure::UnknownConsoleError)
        );
        assert_eq!(
            ConsoleFact::from_open(Err(io::Error::other("SECRET-CAPABILITY"))),
            Err(Failure::UnknownConsoleError)
        );
    }

    #[test]
    fn expired_entry_admits_no_worker_or_output() {
        let called = Arc::new(AtomicBool::new(false));
        let worker_called = Arc::clone(&called);
        let code = drive(Instant::now(), Duration::ZERO, move |_| {
            worker_called.store(true, Ordering::Release);
            Ok(0)
        });
        assert_eq!(code, FAILURE_EXIT);
        assert!(!called.load(Ordering::Acquire));
    }

    #[test]
    fn late_ready_output_remains_a_refusal_after_queued_bytes() {
        struct LateSink {
            delay: Duration,
            output: Arc<Mutex<Vec<u8>>>,
            flushes: Arc<AtomicBool>,
        }
        impl Write for LateSink {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.output.lock().unwrap().extend_from_slice(bytes);
                std::thread::sleep(self.delay);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.flushes.store(true, Ordering::Release);
                Ok(())
            }
        }
        let output = Arc::new(Mutex::new(Vec::new()));
        let flushes = Arc::new(AtomicBool::new(false));
        let sink_output = Arc::clone(&output);
        let sink_flushes = Arc::clone(&flushes);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_millis(50),
            admitted: Arc::new(AtomicBool::new(true)),
        };
        let result = write_probe(
            &budget,
            &mut LateSink {
                delay: Duration::from_millis(100),
                output: sink_output,
                flushes: sink_flushes,
            },
            b"queued",
        );
        assert_eq!(result, Err(Failure::Expired));
        assert_eq!(&*output.lock().unwrap(), b"queued");
        assert!(!flushes.load(Ordering::Acquire));
    }

    #[test]
    fn a_late_partial_write_admits_no_second_write_or_flush() {
        struct PartialSink {
            writes: usize,
            flushes: usize,
        }
        impl Write for PartialSink {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                std::thread::sleep(Duration::from_millis(100));
                Ok(1)
            }

            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                Ok(())
            }
        }
        let budget = Budget {
            deadline: Instant::now() + Duration::from_millis(50),
            admitted: Arc::new(AtomicBool::new(true)),
        };
        let mut sink = PartialSink {
            writes: 0,
            flushes: 0,
        };
        assert_eq!(
            write_probe(&budget, &mut sink, b"partial"),
            Err(Failure::Expired)
        );
        assert_eq!(sink.writes, 1);
        assert_eq!(sink.flushes, 0);
    }

    #[test]
    fn blocked_native_io_stays_owned_after_driver_refuses() {
        let (mut reader, mut writer) = io::pipe().unwrap();
        let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        let (returned_sender, returned_receiver) = mpsc::sync_channel(1);
        let caller = std::thread::spawn(move || {
            // Capture inside the owned caller, not before scheduling that thread.
            let entered = Instant::now();
            let code = drive(entered, Duration::from_millis(200), move |budget| {
                entered_sender.send(()).unwrap();
                let mut byte = [0];
                let result = budget.check().and_then(|()| {
                    let read = reader.read_exact(&mut byte);
                    budget.check()?;
                    read.map_err(|_| Failure::Output)
                });
                drop(reader);
                finished_sender.send(result).unwrap();
                result.map(|()| 0)
            });
            returned_sender.send((code, entered.elapsed())).unwrap();
        });
        let actually_entered = entered_receiver.recv_timeout(Duration::from_secs(1));
        let returned = returned_receiver.recv_timeout(Duration::from_secs(1));
        let unfinished = finished_receiver.try_recv().is_err();
        // Release the actual blocked ReadFile before assertions, including failure paths.
        let _ = writer.write_all(&[1]);
        drop(writer);
        let finished = finished_receiver.recv_timeout(Duration::from_secs(1));
        let _ = caller.join();
        assert!(actually_entered.is_ok());
        let (code, elapsed) = returned.unwrap();
        assert_eq!(code, FAILURE_EXIT);
        assert!(elapsed >= Duration::from_millis(200));
        assert!(elapsed < Duration::from_millis(800));
        assert!(unfinished);
        assert_eq!(finished.unwrap(), Err(Failure::Expired));
    }

    #[test]
    fn blocked_native_output_stays_owned_without_fresh_writes_or_flushes() {
        struct ObservedFile {
            file: File,
            writes: Arc<AtomicUsize>,
            returned: Arc<AtomicUsize>,
            flushes: Arc<AtomicUsize>,
        }
        impl Write for ObservedFile {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes.fetch_add(1, Ordering::SeqCst);
                let result = self.file.write(bytes);
                self.returned.fetch_add(1, Ordering::SeqCst);
                result
            }

            fn flush(&mut self) -> io::Result<()> {
                self.flushes.fetch_add(1, Ordering::SeqCst);
                self.file.flush()
            }
        }
        let (reader, writer) = io::pipe().unwrap();
        let writes = Arc::new(AtomicUsize::new(0));
        let native_returns = Arc::new(AtomicUsize::new(0));
        let flushes = Arc::new(AtomicUsize::new(0));
        let worker_writes = Arc::clone(&writes);
        let worker_returns = Arc::clone(&native_returns);
        let worker_flushes = Arc::clone(&flushes);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        let (returned_sender, returned_receiver) = mpsc::sync_channel(1);
        let caller = std::thread::spawn(move || {
            let entered = Instant::now();
            let code = drive(entered, Duration::from_millis(200), move |budget| {
                let result: Result<(), Failure> = (|| {
                    // Use the exact safe duplication and unbuffered File path used for stdout.
                    let output = duplicate_output(budget, &writer)?;
                    drop(writer);
                    let mut observed = ObservedFile {
                        file: output,
                        writes: worker_writes,
                        returned: worker_returns,
                        flushes: worker_flushes,
                    };
                    // Native anonymous capacity is unspecified. Fill it through real writes,
                    // with <=4KiB per probe and <=1MiB total under the original driver budget.
                    let bytes = [0x5a; PROBE_LIMIT];
                    for _ in 0..(1024 * 1024 / PROBE_LIMIT) {
                        write_probe(budget, &mut observed, &bytes)?;
                    }
                    Err(Failure::Output)
                })();
                // The unbuffered File has closed before this finite completion receipt.
                finished_sender.send(result).unwrap();
                result.map(|()| 0)
            });
            returned_sender.send((code, entered.elapsed())).unwrap();
        });
        let returned = returned_receiver.recv_timeout(Duration::from_secs(1));
        let unfinished = matches!(finished_receiver.try_recv(), Err(mpsc::TryRecvError::Empty));
        let writes_at_refusal = writes.load(Ordering::SeqCst);
        let native_returns_at_refusal = native_returns.load(Ordering::SeqCst);
        let flushes_at_refusal = flushes.load(Ordering::SeqCst);
        // Release only this owned read peer, including failure paths; do not cancel/replace
        // the in-flight WriteFile or assume that queued bytes were never delivered.
        drop(reader);
        let finished = finished_receiver.recv_timeout(Duration::from_secs(1));
        let _ = caller.join();
        let (code, elapsed) = returned.unwrap();
        assert_eq!(code, FAILURE_EXIT);
        assert!(elapsed >= Duration::from_millis(200));
        assert!(elapsed < Duration::from_millis(800));
        assert!(unfinished, "native output ended before peer release");
        assert_eq!(writes_at_refusal, native_returns_at_refusal + 1);
        assert_eq!(finished.unwrap(), Err(Failure::Expired));
        assert_eq!(writes.load(Ordering::SeqCst), writes_at_refusal);
        assert_eq!(native_returns.load(Ordering::SeqCst), writes_at_refusal);
        assert_eq!(flushes.load(Ordering::SeqCst), flushes_at_refusal);
    }
}
