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
