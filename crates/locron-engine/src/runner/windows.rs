//! Safe Windows process ownership before the first target instruction.

use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use process_wrap::tokio::{ChildWrapper, CommandWrap, CommandWrapper, JobObject, KillOnDrop};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use win32job::{ExtendedLimitInfo, Job};

use super::*;

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
            .ok_or_else(|| io::Error::other("suspended child has no process handle"))?;
        if let Err(error) = self.job.assign_process(handle as isize) {
            let _ = child.start_kill();
            return Err(io::Error::other(error));
        }
        Ok(())
    }
}

fn tree_empty(job: &Job, errors: &mut Vec<String>) -> bool {
    match job.query_process_id_list() {
        Ok(processes) => processes.is_empty(),
        Err(error) => {
            if errors.len() < 8 {
                errors.push(format!("owned job process query: {error}"));
            }
            false
        }
    }
}

fn force_stop(child: &mut dyn ChildWrapper, errors: &mut Vec<String>) {
    if let Err(error) = child.start_kill()
        && errors.len() < 8
    {
        errors.push(format!("owned job termination: {error}"));
    }
}

async fn cleanup(child: &mut dyn ChildWrapper, job: &Job, grace: Duration) {
    let mut errors = Vec::new();
    force_stop(child, &mut errors);
    let deadline = Instant::now() + grace;
    loop {
        let reaped = child.try_wait().is_ok_and(|status| status.is_some());
        if tree_empty(job, &mut errors) && reaped {
            return;
        }
        if Instant::now() >= deadline {
            tracing::error!(details = ?errors, "owned Windows tree cleanup remains unconfirmed");
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn read_stream<R: tokio::io::AsyncRead + Unpin>(
    mut input: R,
    channel: Channel,
    sender: mpsc::Sender<io::Result<(Channel, Vec<u8>)>>,
) {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        match input.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => {
                if sender
                    .send(Ok((channel, buffer[..count].to_vec())))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error)).await;
                break;
            }
        }
    }
}

pub(super) async fn run_process(
    config: &RunnerConfig,
    spec: &ProcessSpec,
    context: &AttemptContext,
    mut writer: OutputWriter,
    start: Instant,
) -> Result<ExecutionOutcome, RunnerError> {
    if !spec.cwd.is_absolute() || !spec.cwd.is_dir() {
        return finalize_configuration_failure(
            writer,
            context,
            start,
            "working directory is missing",
        )
        .await;
    }
    let Some(executable) =
        locron_core::execution::resolve_executable(&spec.executable, &spec.cwd, &spec.env)
    else {
        return finalize_configuration_failure(
            writer,
            context,
            start,
            &format!("executable not found: {}", spec.executable),
        )
        .await;
    };
    if let Err(reason) = locron_core::execution::validate_direct_executable(&executable) {
        return finalize_configuration_failure(writer, context, start, &reason).await;
    }
    let mut limits = ExtendedLimitInfo::default();
    limits.limit_kill_on_job_close();
    let job = match Job::create_with_limit_info(&limits) {
        Ok(job) => Arc::new(job),
        Err(error) => {
            return finalize_configuration_failure(
                writer,
                context,
                start,
                &format!("cannot create owned process job: {error}"),
            )
            .await;
        }
    };
    let created = Arc::new(AtomicBool::new(false));
    let mut command = Command::new(&executable);
    let cmd_shell = executable
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("cmd"))
        && spec.args.len() == 4
        && spec.args[..3] == ["/D", "/S", "/C"];
    if cmd_shell {
        // cmd's command tail uses cmd parsing, not the C runtime argv quoting rules.
        use std::os::windows::process::CommandExt;
        command.args(&spec.args[..3]);
        command
            .as_std_mut()
            .raw_arg(format!("\"{}\"", spec.args[3]));
    } else {
        command.args(&spec.args);
    }
    command
        .current_dir(&spec.cwd)
        .env_clear()
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(command);
    wrapped.wrap(KillOnDrop).wrap(JobObject).wrap(Enroll {
        job: Arc::clone(&job),
        created: Arc::clone(&created),
    });
    let mut child = match wrapped.spawn() {
        Ok(child) => child,
        Err(error) if !created.load(Ordering::Acquire) => {
            return finalize_configuration_failure(writer, context, start, &error.to_string())
                .await;
        }
        Err(error) => {
            let stats = writer
                .finalize(&context.final_output)
                .await
                .map_err(RunnerError::ExecutionInfrastructure)?;
            return Ok(simple_outcome(
                OutcomeKind::TerminationUnconfirmed,
                &format!("suspended child enrollment/resume cleanup cannot be confirmed: {error}"),
                start,
                stats,
            ));
        }
    };
    crate::test_crash_boundary("after-spawn").await;
    let stdout = child.stdout().take().expect("piped stdout");
    let stderr = child.stderr().take().expect("piped stderr");
    let (sender, mut receiver) = mpsc::channel(32);
    let stdout_task = tokio::spawn(read_stream(stdout, Channel::Stdout, sender.clone()));
    let stderr_task = tokio::spawn(read_stream(stderr, Channel::Stderr, sender));
    let grace = config.termination_grace.max(Duration::from_millis(1));
    let mut poll = tokio::time::interval(Duration::from_millis(10));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut flush = tokio::time::interval(Duration::from_millis(200));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut result = None;
    let mut reason = None;
    let mut termination = None;
    let mut stop_at = None;
    let mut forced = false;
    let mut confirmation_failed = false;
    let mut errors = Vec::new();
    let mut infrastructure_error = None;
    loop {
        tokio::select! {
            _ = poll.tick() => {
                if result.is_none() {
                    match child.try_wait() {
                        Ok(status) => {
                            result = status;
                            if result.is_some() && stop_at.is_none() { stop_at = Some(Instant::now() + grace); }
                        }
                        Err(error) => { infrastructure_error = Some(error); break; }
                    }
                }
                let empty = tree_empty(&job, &mut errors);
                if empty && result.is_some() { break; }
                if termination.is_none() && context.timeout.is_some_and(|timeout| start.elapsed() >= timeout) {
                    termination = Some(OutcomeKind::TimedOut);
                    stop_at = Some(Instant::now() + grace);
                }
                if stop_at.is_some_and(|deadline| Instant::now() >= deadline) {
                    if forced { confirmation_failed = true; break; }
                    if termination.is_none() {
                        termination = Some(OutcomeKind::Failed);
                        reason = Some("root exited while descendants outlived the bounded natural drain".to_owned());
                    }
                    force_stop(child.as_mut(), &mut errors);
                    forced = true;
                    stop_at = Some(Instant::now() + grace);
                }
            }
            () = context.cancellation.cancelled(), if termination.is_none() => {
                termination = Some(OutcomeKind::Cancelled);
                stop_at = Some(Instant::now() + grace);
            }
            Some(event) = receiver.recv() => {
                match event {
                    Ok((channel, bytes)) => if let Err(error) = writer.write(channel, start.elapsed(), &bytes).await { infrastructure_error = Some(error); break; },
                    Err(error) => { infrastructure_error = Some(error); break; },
                }
            }
            _ = flush.tick() => if let Err(error) = writer.flush().await { infrastructure_error = Some(error); break; },
        }
    }
    if let Some(error) = infrastructure_error {
        stdout_task.abort();
        stderr_task.abort();
        drop(receiver);
        cleanup(child.as_mut(), &job, grace).await;
        return Err(RunnerError::ExecutionInfrastructure(error));
    }
    if confirmation_failed {
        stdout_task.abort();
        stderr_task.abort();
        drop(receiver);
    } else {
        let drain = async {
            while let Some(event) = receiver.recv().await {
                let (channel, bytes) = event?;
                writer.write(channel, start.elapsed(), &bytes).await?;
            }
            Ok::<(), io::Error>(())
        };
        match tokio::time::timeout(grace, drain).await {
            Ok(Ok(())) => {
                let _ = stdout_task.await;
                let _ = stderr_task.await;
            }
            Ok(Err(error)) => {
                stdout_task.abort();
                stderr_task.abort();
                drop(receiver);
                cleanup(child.as_mut(), &job, grace).await;
                return Err(RunnerError::ExecutionInfrastructure(error));
            }
            Err(_) => {
                stdout_task.abort();
                stderr_task.abort();
                drop(receiver);
                termination = Some(OutcomeKind::Failed);
                reason =
                    Some("owned process tree exited but output drain exceeded its deadline".into());
            }
        }
    }
    let stats = match writer.finalize(&context.final_output).await {
        Ok(stats) => stats,
        Err(error) => {
            cleanup(child.as_mut(), &job, grace).await;
            return Err(RunnerError::ExecutionInfrastructure(error));
        }
    };
    let kind = if confirmation_failed {
        OutcomeKind::TerminationUnconfirmed
    } else {
        termination.unwrap_or_else(|| {
            if result.as_ref().expect("root exited").success() {
                OutcomeKind::Succeeded
            } else {
                OutcomeKind::FailedRetryable
            }
        })
    };
    let reason = if confirmation_failed {
        format!(
            "owned process tree termination remains unconfirmed after its deadline: {}",
            errors.join(", ")
        )
    } else {
        reason.unwrap_or_else(|| match kind {
            OutcomeKind::Succeeded => "process tree exited successfully".into(),
            OutcomeKind::FailedRetryable => format!(
                "process exited with status {}",
                result.as_ref().expect("root exited")
            ),
            OutcomeKind::TimedOut => "attempt timed out; owned process tree exit confirmed".into(),
            OutcomeKind::Cancelled => {
                "attempt was cancelled; owned process tree exit confirmed".into()
            }
            OutcomeKind::TerminationUnconfirmed => format!(
                "owned process tree termination remains unconfirmed: {}",
                errors.join(", ")
            ),
            OutcomeKind::Failed => "process failed".into(),
        })
    };
    Ok(ExecutionOutcome {
        kind,
        exit_code: result.as_ref().and_then(std::process::ExitStatus::code),
        http_status: None,
        http_content_type: None,
        reason,
        duration_micros: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        output: stats,
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    const MODE: &str = "LOCRON_NATIVE_FIXTURE";
    const ROOT: &str = "LOCRON_NATIVE_FIXTURE_ROOT";
    const FIXTURE: &str = "runner::windows::tests::native_fixture";

    struct FixtureRoot {
        _temporary: tempfile::TempDir,
        path: PathBuf,
    }

    impl FixtureRoot {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("private 工具 root");
            let _guard = locron_core::filesystem::DirectoryGuard::private(&path).unwrap();
            Self {
                _temporary: temporary,
                path,
            }
        }

        fn context(&self) -> AttemptContext {
            AttemptContext {
                run_id: "native-run".into(),
                attempt: 1,
                partial_output: self.path.join("1.partial"),
                final_output: self.path.join("1.log"),
                output_limit: 64 * 1024,
                timeout: Some(Duration::from_secs(5)),
                cancellation: tokio_util::sync::CancellationToken::new(),
            }
        }
    }

    pub(crate) fn fixture_spec(root: &Path, mode: &str) -> ProcessSpec {
        let mut env = locron_core::execution::minimal_environment();
        env.insert(MODE.into(), mode.into());
        env.insert(ROOT.into(), root.display().to_string());
        ProcessSpec {
            executable: std::env::current_exe().unwrap().display().to_string(),
            args: vec![
                "--exact".into(),
                FIXTURE.into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            cwd: root.into(),
            env,
        }
    }

    fn fixture_child(root: &Path, mode: &str) -> std::process::Command {
        let spec = fixture_spec(root, mode);
        let mut command = std::process::Command::new(spec.executable);
        command.args(spec.args).env(MODE, mode).env(ROOT, root);
        command
    }

    // The same native test executable supplies targets; no scripting runtime is required.
    #[test]
    fn native_fixture() {
        let Ok(mode) = std::env::var(MODE) else {
            return;
        };
        let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
        match mode.as_str() {
            "echo" => {
                std::io::stdout().write_all(b"hello").unwrap();
                std::io::stderr().write_all(b"bad").unwrap();
            }
            "argv" => {
                let args = std::env::args()
                    .skip_while(|arg| arg != "--")
                    .skip(1)
                    .collect::<Vec<_>>();
                std::fs::write(root.join("argv.json"), serde_json::to_vec(&args).unwrap()).unwrap();
                std::io::stdout().write_all(b"binary\0\xff").unwrap();
            }
            "noisy" => {
                for _ in 0..2048 {
                    std::io::stdout().write_all(b"0123456789abcdef").unwrap();
                    std::io::stderr().write_all(b"0123456789abcdef").unwrap();
                }
            }
            "sleep" => std::thread::sleep(Duration::from_secs(2)),
            "append" => {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("side-effect"))
                    .unwrap()
                    .write_all(b"x")
                    .unwrap();
            }
            "natural" => {
                std::thread::sleep(Duration::from_millis(120));
                std::fs::write(root.join("done"), b"done").unwrap();
            }
            "tree" => {
                let _ = fixture_child(&root, "branch").spawn().unwrap().wait();
            }
            "branch" => {
                let _ = fixture_child(&root, "leaf").spawn().unwrap().wait();
            }
            "root-exits" => {
                let _ = fixture_child(&root, "leaf").spawn().unwrap();
            }
            "root-short" => {
                let _ = fixture_child(&root, "leaf-short").spawn().unwrap();
            }
            "leaf" | "leaf-short" => {
                std::fs::write(root.join("leaf-ready"), std::process::id().to_string()).unwrap();
                let count = if mode == "leaf-short" { 12 } else { 300 };
                for tick in 0_u32..count {
                    std::fs::write(root.join("heartbeat"), tick.to_le_bytes()).unwrap();
                    std::thread::sleep(Duration::from_millis(10));
                }
                std::fs::write(root.join("late-completion"), b"survived").unwrap();
            }
            "owner" => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let context = AttemptContext {
                        run_id: "crash".into(),
                        attempt: 1,
                        partial_output: root.join("1.partial"),
                        final_output: root.join("1.log"),
                        output_limit: 1024,
                        timeout: None,
                        cancellation: tokio_util::sync::CancellationToken::new(),
                    };
                    Runner::new(RunnerConfig::default())
                        .unwrap()
                        .execute(&TargetSpec::Process(fixture_spec(&root, "tree")), &context)
                        .await
                        .unwrap();
                });
            }
            _ => panic!("unknown native fixture mode"),
        }
        std::io::stdout().flush().unwrap();
        std::io::stderr().flush().unwrap();
        // Exit before libtest adds a completion trailer to the target output.
        std::process::exit(0);
    }

    fn runner(grace: Duration) -> Runner {
        Runner::new(RunnerConfig {
            termination_grace: grace,
            ..RunnerConfig::default()
        })
        .unwrap()
    }

    async fn wait_for_leaf(root: &Path) {
        let ready = root.join("leaf-ready");
        for _ in 0..500 {
            if ready.is_file() && root.join("heartbeat").is_file() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("native descendant did not start");
    }

    async fn assert_leaf_stopped(root: &Path) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let first = std::fs::read(root.join("heartbeat")).unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            first,
            std::fs::read(root.join("heartbeat")).unwrap(),
            "descendant kept executing"
        );
        assert!(
            !root.join("late-completion").exists(),
            "descendant survived to natural completion"
        );
    }

    #[tokio::test]
    async fn preserves_native_argv_unicode_quotes_metacharacters_and_raw_bytes() {
        let root = FixtureRoot::new();
        let arguments = ["", "a b", "工具", "quote\"inside", r"C:\tail\", "%!&|^<>"];
        let mut spec = fixture_spec(&root.path, "argv");
        spec.args.push("--".into());
        spec.args.extend(arguments.map(str::to_owned));
        let outcome = runner(Duration::from_millis(500))
            .execute(&TargetSpec::Process(spec), &root.context())
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded, "{}", outcome.reason);
        let actual: Vec<String> =
            serde_json::from_slice(&std::fs::read(root.path.join("argv.json")).unwrap()).unwrap();
        assert_eq!(actual, arguments);
        let stdout = crate::output::read_frames(root.path.join("1.log"))
            .unwrap()
            .into_iter()
            .filter(|frame| frame.channel == Channel::Stdout)
            .flat_map(|frame| frame.payload)
            .collect::<Vec<_>>();
        assert!(stdout.ends_with(b"binary\0\xff"));
    }

    #[tokio::test]
    async fn cancellation_stops_immediately_spawned_grandchildren() {
        let root = FixtureRoot::new();
        let context = root.context();
        let cancellation = context.cancellation.clone();
        let target = TargetSpec::Process(fixture_spec(&root.path, "tree"));
        let execution = tokio::spawn(async move {
            runner(Duration::from_millis(250))
                .execute(&target, &context)
                .await
        });
        wait_for_leaf(&root.path).await;
        cancellation.cancel();
        let outcome = tokio::time::timeout(Duration::from_secs(5), execution)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Cancelled, "{}", outcome.reason);
        assert_leaf_stopped(&root.path).await;
    }

    #[tokio::test]
    async fn timeout_stops_the_entire_owned_tree() {
        let root = FixtureRoot::new();
        let mut context = root.context();
        context.timeout = Some(Duration::from_secs(1));
        let target = TargetSpec::Process(fixture_spec(&root.path, "tree"));
        let outcome = runner(Duration::from_millis(250))
            .execute(&target, &context)
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::TimedOut, "{}", outcome.reason);
        assert_leaf_stopped(&root.path).await;
    }

    #[tokio::test]
    async fn root_exit_waits_for_natural_descendant_exit() {
        let root = FixtureRoot::new();
        let target = TargetSpec::Process(fixture_spec(&root.path, "root-short"));
        let outcome = runner(Duration::from_secs(1))
            .execute(&target, &root.context())
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded, "{}", outcome.reason);
        assert!(root.path.join("late-completion").exists());
        assert!(outcome.duration_micros >= 120_000);
    }

    #[tokio::test]
    async fn root_zero_exit_does_not_report_success_for_live_descendants() {
        let root = FixtureRoot::new();
        let target = TargetSpec::Process(fixture_spec(&root.path, "root-exits"));
        let outcome = runner(Duration::from_millis(250))
            .execute(&target, &root.context())
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Failed, "{}", outcome.reason);
        assert!(outcome.reason.contains("descendants outlived"));
        assert_leaf_stopped(&root.path).await;
    }

    #[tokio::test]
    async fn finalization_failure_does_not_leave_an_owned_descendant() {
        let root = FixtureRoot::new();
        let context = root.context();
        std::fs::create_dir(&context.final_output).unwrap();
        let target = TargetSpec::Process(fixture_spec(&root.path, "root-exits"));
        let error = runner(Duration::from_millis(250))
            .execute(&target, &context)
            .await
            .unwrap_err();
        assert_eq!(
            error.failure_kind(),
            RunnerFailureKind::ExecutionMayHaveStarted
        );
        assert_leaf_stopped(&root.path).await;
    }

    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[tokio::test]
    async fn crashing_the_native_owner_triggers_kill_on_job_close() {
        let root = FixtureRoot::new();
        let mut owner = OwnedChild(
            fixture_child(&root.path, "owner")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for_leaf(&root.path).await;
        owner.0.kill().unwrap();
        owner.0.wait().unwrap();
        assert_leaf_stopped(&root.path).await;
    }

    #[tokio::test]
    async fn cmd_shell_preserves_quoted_script_paths_arguments_and_metacharacters() {
        let root = FixtureRoot::new();
        let script = root.path.join("工具 & script.cmd");
        std::fs::write(&script, b"@echo off\r\n@echo(%1\r\n@echo(%2\r\n").unwrap();
        let shell = locron_core::execution::default_shell().unwrap();
        let command = format!(
            "\"{}\" \"first value\" \"a&b\" & echo tail",
            script.display()
        );
        let spec = ProcessSpec {
            executable: shell.display().to_string(),
            args: locron_core::execution::shell_arguments(&shell, &command).unwrap(),
            cwd: root.path.clone(),
            env: locron_core::execution::minimal_environment(),
        };
        let outcome = runner(Duration::from_secs(1))
            .execute(&TargetSpec::Process(spec), &root.context())
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded, "{}", outcome.reason);
        let stdout = crate::output::read_frames(root.path.join("1.log"))
            .unwrap()
            .into_iter()
            .filter(|frame| frame.channel == Channel::Stdout)
            .flat_map(|frame| frame.payload)
            .collect::<Vec<_>>();
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("\"first value\""), "{stdout}");
        assert!(stdout.contains("\"a&b\""), "{stdout}");
        assert!(stdout.contains("tail"), "{stdout}");
    }
}
