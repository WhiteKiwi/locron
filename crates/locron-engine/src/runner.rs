//! Process, shell, and HTTP target runners.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use locron_core::CoreError;
use locron_core::ports::ExecutorPort;
use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use reqwest::header::{
    AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, HeaderName, HeaderValue,
    PROXY_AUTHORIZATION, TRANSFER_ENCODING,
};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::output::{Channel, OutputStats, OutputWriter};

/// Direct or explicit-shell process configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessSpec {
    /// Executable for direct mode, or absolute shell executable for shell mode.
    pub executable: String,
    /// Arguments for direct mode. Shell mode normally uses `-c, command`.
    pub args: Vec<String>,
    /// Absolute execution directory.
    pub cwd: PathBuf,
    /// Fully layered effective environment.
    pub env: BTreeMap<String, String>,
}

/// HTTP request configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpSpec {
    /// Accepted HTTP method.
    pub method: String,
    /// Absolute HTTP(S) URL.
    pub url: Url,
    /// Header names and resolved values.
    pub headers: BTreeMap<String, String>,
    /// Optional body bytes.
    pub body: Option<Vec<u8>>,
    /// Additional successful statuses.
    pub success_statuses: Vec<u16>,
    /// Whether redirects are followed.
    pub follow_redirects: bool,
}

/// Target snapshot selected for one attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TargetSpec {
    /// Direct process or shell represented as explicit argv.
    Process(ProcessSpec),
    /// HTTP request.
    Http(HttpSpec),
}

/// Durable facts made available to a runner.
#[derive(Clone, Debug)]
pub struct AttemptContext {
    /// Run identity for paths and reserved metadata.
    pub run_id: String,
    /// One-based attempt number.
    pub attempt: u32,
    /// Partial output path.
    pub partial_output: PathBuf,
    /// Final output path.
    pub final_output: PathBuf,
    /// Remaining capture allowance for the run.
    pub output_limit: u64,
    /// Attempt timeout; `None` means no timeout.
    pub timeout: Option<Duration>,
    /// External cancellation request.
    pub cancellation: CancellationToken,
}

/// Owned request passed through the runtime-neutral core executor port.
#[derive(Clone, Debug)]
pub struct ExecutionRequest {
    /// Fully materialized target to execute.
    pub target: TargetSpec,
    /// Durable attempt identity, output paths, limits, and cancellation.
    pub context: AttemptContext,
}

/// Stable target result classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    /// Process exited zero or HTTP status matched success policy.
    Succeeded,
    /// Known target failure that is retryable by default.
    FailedRetryable,
    /// Known target/configuration failure that is not retryable by default.
    Failed,
    /// Attempt exceeded its timeout.
    TimedOut,
    /// Explicit cancellation or overlap replacement.
    Cancelled,
    /// TERM/KILL completed without confirming that the owned child exited.
    TerminationUnconfirmed,
}

/// Complete result of one target attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionOutcome {
    /// Stable classification.
    pub kind: OutcomeKind,
    /// Process exit code when available.
    pub exit_code: Option<i32>,
    /// HTTP response status when available.
    pub http_status: Option<u16>,
    /// Final HTTP response content type when available.
    pub http_content_type: Option<String>,
    /// Operator-facing non-secret reason.
    pub reason: String,
    /// Monotonic elapsed time.
    pub duration_micros: u64,
    /// Output capture counters.
    #[serde(skip)]
    pub output: OutputStats,
}

/// Runner construction defaults.
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// TERM-to-KILL grace period.
    pub termination_grace: Duration,
    /// Maximum redirect hops.
    pub max_redirects: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            termination_grace: Duration::from_secs(5),
            max_redirects: 10,
        }
    }
}

/// Whether a runner failure happened before any external execution or after it
/// may have begun.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunnerFailureKind {
    /// Output storage could not be prepared, so no target was started.
    OutputPreparation,
    /// Output or execution infrastructure failed after side effects may exist.
    ExecutionMayHaveStarted,
}

/// Runner failure before a target outcome can be produced.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Output artifact could not be prepared before external execution.
    #[error("output preparation error: {0}")]
    OutputPreparation(io::Error),
    /// Output or child infrastructure failed after execution may have begun.
    #[error("output error after execution may have begun: {0}")]
    ExecutionInfrastructure(io::Error),
    /// Target configuration is invalid at execution time.
    #[error("configuration error: {0}")]
    Configuration(String),
}

impl RunnerError {
    /// Returns the durable failure classification required by the daemon.
    #[must_use]
    pub const fn failure_kind(&self) -> RunnerFailureKind {
        match self {
            Self::OutputPreparation(_) | Self::Configuration(_) => {
                RunnerFailureKind::OutputPreparation
            }
            Self::ExecutionInfrastructure(_) => RunnerFailureKind::ExecutionMayHaveStarted,
        }
    }
}

/// Executes normalized target snapshots.
#[derive(Clone)]
pub struct Runner {
    config: RunnerConfig,
    http: reqwest::Client,
}

impl Runner {
    /// Builds a runner with TLS verification and automatic redirects disabled.
    pub fn new(config: RunnerConfig) -> Result<Self, RunnerError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| RunnerError::Configuration(error.to_string()))?;
        Ok(Self { config, http })
    }

    /// Executes one target and finalizes its output artifact.
    pub async fn execute(
        &self,
        target: &TargetSpec,
        context: &AttemptContext,
    ) -> Result<ExecutionOutcome, RunnerError> {
        let writer = OutputWriter::create(&context.partial_output, context.output_limit)
            .await
            .map_err(RunnerError::OutputPreparation)?;
        let start = Instant::now();
        let mut outcome = match target {
            TargetSpec::Process(spec) => self.run_process(spec, context, writer, start).await?,
            TargetSpec::Http(spec) => self.run_http(spec, context, writer, start).await?,
        };
        // Both branches finalize the writer because they need to serialize while running.
        outcome.duration_micros = start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        Ok(outcome)
    }

    async fn run_process(
        &self,
        spec: &ProcessSpec,
        context: &AttemptContext,
        writer: OutputWriter,
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
            resolve_executable(&spec.executable, &spec.cwd, spec.env.get("PATH"))
        else {
            return finalize_configuration_failure(
                writer,
                context,
                start,
                &format!("executable not found: {}", spec.executable),
            )
            .await;
        };
        let mut command = Command::new(executable);
        command
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return finalize_configuration_failure(writer, context, start, &error.to_string())
                    .await;
            }
        };
        crate::test_crash_boundary("after-spawn").await;
        let pid = child.id().map(|id| Pid::from_raw(id as i32));
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (sender, mut receiver) = mpsc::channel::<(Channel, Vec<u8>)>(32);
        let stdout_task = tokio::spawn(read_stream(stdout, Channel::Stdout, sender.clone()));
        let stderr_task = tokio::spawn(read_stream(stderr, Channel::Stderr, sender));
        let mut writer = writer;
        let mut wait = Box::pin(child.wait());
        let mut timeout = context
            .timeout
            .map(|duration| Box::pin(tokio::time::sleep(duration)));
        let mut flush_interval = tokio::time::interval(Duration::from_millis(200));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut result = None;
        let mut termination = None;
        let mut termination_stage = 0_u8;
        let mut termination_deadline = None;
        let mut termination_errors = Vec::new();
        let mut confirmation_failure = false;
        let mut termination_confirmed = false;

        while !(confirmation_failure
            || termination.is_none() && result.is_some()
            || termination.is_some() && termination_confirmed)
        {
            tokio::select! {
                biased;
                status = &mut wait, if result.is_none() => {
                    match status {
                        Ok(status) => result = Some(status),
                        Err(error) => {
                            drop(wait);
                            stdout_task.abort();
                            stderr_task.abort();
                            drop(receiver);
                            terminate_after_output_failure(
                                &mut child,
                                pid,
                                false,
                                self.config.termination_grace,
                            )
                            .await;
                            return Err(RunnerError::ExecutionInfrastructure(error));
                        }
                    }
                    if termination.is_some() {
                        termination_confirmed = observe_group_absence(pid, &mut termination_errors);
                    }
                }
                Some((channel, bytes)) = receiver.recv() => {
                    if let Err(error) = writer.write(channel, start.elapsed(), &bytes).await {
                        drop(wait);
                        stdout_task.abort();
                        stderr_task.abort();
                        drop(receiver);
                        terminate_after_output_failure(
                            &mut child,
                            pid,
                            result.is_some(),
                            self.config.termination_grace,
                        )
                        .await;
                        return Err(RunnerError::ExecutionInfrastructure(error));
                    }
                }
                _ = flush_interval.tick() => {
                    // Live follow clients (CLI `logs --follow`, the SSE stream)
                    // read the partial file on disk, so buffered frames must
                    // reach it while the process still runs (recorded at
                    // implementation step 7). A flush error is an output
                    // infrastructure failure like a write error.
                    if let Err(error) = writer.flush().await {
                        drop(wait);
                        stdout_task.abort();
                        stderr_task.abort();
                        drop(receiver);
                        terminate_after_output_failure(
                            &mut child,
                            pid,
                            result.is_some(),
                            self.config.termination_grace,
                        )
                        .await;
                        return Err(RunnerError::ExecutionInfrastructure(error));
                    }
                }
                () = context.cancellation.cancelled(), if termination.is_none() => {
                    termination = Some(OutcomeKind::Cancelled);
                    record_signal_result(&mut termination_errors, Signal::SIGTERM, signal_group(pid, Signal::SIGTERM));
                    termination_stage = 1;
                    termination_deadline = Some(Box::pin(tokio::time::sleep(self.config.termination_grace)));
                }
                () = async { if let Some(timer) = &mut timeout { timer.await } }, if timeout.is_some() && termination.is_none() => {
                    termination = Some(OutcomeKind::TimedOut);
                    timeout = None;
                    record_signal_result(&mut termination_errors, Signal::SIGTERM, signal_group(pid, Signal::SIGTERM));
                    termination_stage = 1;
                    termination_deadline = Some(Box::pin(tokio::time::sleep(self.config.termination_grace)));
                }
                () = async { if let Some(deadline) = &mut termination_deadline { deadline.await } }, if termination_deadline.is_some() => {
                    let group_absent = observe_group_absence(pid, &mut termination_errors);
                    if group_absent && result.is_some() {
                        termination_confirmed = true;
                        termination_deadline = None;
                    } else if termination_stage == 1 && !group_absent {
                        record_signal_result(&mut termination_errors, Signal::SIGKILL, signal_group(pid, Signal::SIGKILL));
                        termination_stage = 2;
                        termination_deadline = Some(Box::pin(tokio::time::sleep(self.config.termination_grace)));
                    } else if termination_stage == 1 {
                        termination_stage = 2;
                        termination_deadline = Some(Box::pin(tokio::time::sleep(self.config.termination_grace)));
                    } else if !group_absent || result.is_none() {
                        confirmation_failure = true;
                        termination_deadline = None;
                    }
                }
            }
        }
        if confirmation_failure {
            stdout_task.abort();
            stderr_task.abort();
            drop(receiver);
        } else {
            // Both stream tasks finish when the process closes its pipe ends. Keep
            // draining so a full bounded channel cannot deadlock their completion.
            while let Some((channel, bytes)) = receiver.recv().await {
                if let Err(error) = writer.write(channel, start.elapsed(), &bytes).await {
                    drop(wait);
                    stdout_task.abort();
                    stderr_task.abort();
                    drop(receiver);
                    terminate_after_output_failure(
                        &mut child,
                        pid,
                        result.is_some(),
                        self.config.termination_grace,
                    )
                    .await;
                    return Err(RunnerError::ExecutionInfrastructure(error));
                }
            }
            let _ = stdout_task.await;
            let _ = stderr_task.await;
        }
        drop(wait);
        let stats = match writer.finalize(&context.final_output).await {
            Ok(stats) => stats,
            Err(error) => {
                terminate_after_output_failure(
                    &mut child,
                    pid,
                    result.is_some(),
                    self.config.termination_grace,
                )
                .await;
                return Err(RunnerError::ExecutionInfrastructure(error));
            }
        };
        let kind = if confirmation_failure {
            OutcomeKind::TerminationUnconfirmed
        } else {
            let status = result.as_ref().expect("wait result");
            termination.unwrap_or_else(|| {
                if status.success() {
                    OutcomeKind::Succeeded
                } else {
                    OutcomeKind::FailedRetryable
                }
            })
        };
        let reason = if confirmation_failure {
            termination_confirmation_reason(&termination_errors)
        } else {
            match kind {
                OutcomeKind::Succeeded => "process exited successfully".into(),
                OutcomeKind::FailedRetryable => format!(
                    "process exited with status {}",
                    result.as_ref().expect("wait result")
                ),
                OutcomeKind::TimedOut => "attempt timed out".into(),
                OutcomeKind::Cancelled => "attempt was cancelled".into(),
                OutcomeKind::Failed => "process failed".into(),
                OutcomeKind::TerminationUnconfirmed => unreachable!("reason handled above"),
            }
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

    async fn run_http(
        &self,
        spec: &HttpSpec,
        context: &AttemptContext,
        mut writer: OutputWriter,
        start: Instant,
    ) -> Result<ExecutionOutcome, RunnerError> {
        if !matches!(spec.url.scheme(), "http" | "https") {
            return finalize_configuration_failure(
                writer,
                context,
                start,
                "URL must be HTTP or HTTPS",
            )
            .await;
        }
        let Ok(mut method) = Method::from_bytes(spec.method.as_bytes()) else {
            return finalize_configuration_failure(writer, context, start, "invalid HTTP method")
                .await;
        };
        if !matches!(
            method,
            Method::GET
                | Method::POST
                | Method::PUT
                | Method::PATCH
                | Method::DELETE
                | Method::HEAD
        ) {
            return finalize_configuration_failure(
                writer,
                context,
                start,
                "unsupported HTTP method",
            )
            .await;
        }
        let mut headers = match build_headers(&spec.headers) {
            Ok(headers) => headers,
            Err(RunnerError::Configuration(reason)) => {
                return finalize_configuration_failure(writer, context, start, &reason).await;
            }
            Err(error) => return Err(error),
        };
        let mut url = spec.url.clone();
        let mut body = spec.body.clone();
        let request = async {
            for hop in 0..=self.config.max_redirects {
                let mut builder = self
                    .http
                    .request(method.clone(), url.clone())
                    .headers(headers.clone());
                if let Some(bytes) = &body {
                    builder = builder.body(bytes.clone());
                }
                let response = builder.send().await?;
                if !spec.follow_redirects || !response.status().is_redirection() {
                    return Ok::<_, reqwest::Error>(response);
                }
                if hop == self.config.max_redirects {
                    return Ok(response);
                }
                let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                    return Ok(response);
                };
                let Ok(location) = location.to_str() else {
                    return Ok(response);
                };
                let Ok(next) = url.join(location) else {
                    return Ok(response);
                };
                if !same_origin(&url, &next) {
                    headers.remove(AUTHORIZATION);
                    headers.remove(PROXY_AUTHORIZATION);
                    headers.remove(COOKIE);
                }
                let rewrite_to_get = (response.status() == StatusCode::SEE_OTHER
                    && method != Method::HEAD)
                    || ((response.status() == StatusCode::MOVED_PERMANENTLY
                        || response.status() == StatusCode::FOUND)
                        && method == Method::POST);
                if rewrite_to_get {
                    method = Method::GET;
                    body = None;
                    headers.remove(CONTENT_LENGTH);
                    headers.remove(CONTENT_TYPE);
                    headers.remove(TRANSFER_ENCODING);
                }
                url = next;
            }
            unreachable!("bounded redirect loop returns")
        };
        let response = tokio::select! {
            () = context.cancellation.cancelled() => {
                let stats = writer.finalize(&context.final_output).await
                    .map_err(RunnerError::ExecutionInfrastructure)?;
                return Ok(simple_outcome(OutcomeKind::Cancelled, "attempt was cancelled", start, stats));
            }
            result = async {
                if let Some(timeout) = context.timeout {
                    tokio::time::timeout(timeout, request).await.map_err(|_| HttpRunError::Timeout)?
                } else {
                    request.await
                }.map_err(HttpRunError::Request)
            } => result,
        };
        let response = match response {
            Ok(response) => response,
            Err(HttpRunError::Timeout) => {
                let stats = writer
                    .finalize(&context.final_output)
                    .await
                    .map_err(RunnerError::ExecutionInfrastructure)?;
                return Ok(simple_outcome(
                    OutcomeKind::TimedOut,
                    "attempt timed out",
                    start,
                    stats,
                ));
            }
            Err(HttpRunError::Request(error)) => {
                let stats = writer
                    .finalize(&context.final_output)
                    .await
                    .map_err(RunnerError::ExecutionInfrastructure)?;
                return Ok(simple_outcome(
                    OutcomeKind::FailedRetryable,
                    &format!("HTTP transport error: {error}"),
                    start,
                    stats,
                ));
            }
        };
        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut stream = response.bytes_stream();
        let mut flush_interval = tokio::time::interval(Duration::from_millis(200));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let next = async {
                if let Some(timeout) = context.timeout {
                    let remaining = timeout.saturating_sub(start.elapsed());
                    tokio::time::timeout(remaining, stream.next())
                        .await
                        .map_err(|_| HttpRunError::Timeout)
                } else {
                    Ok(stream.next().await)
                }
            };
            tokio::select! {
                () = context.cancellation.cancelled() => {
                    let stats = writer.finalize(&context.final_output).await
                        .map_err(RunnerError::ExecutionInfrastructure)?;
                    return Ok(http_outcome(OutcomeKind::Cancelled, "attempt was cancelled", start, stats, status, content_type));
                }
                _ = flush_interval.tick() => writer
                    .flush()
                    .await
                    .map_err(RunnerError::ExecutionInfrastructure)?,
                item = next => match item {
                    Err(HttpRunError::Timeout) => {
                        let stats = writer.finalize(&context.final_output).await
                            .map_err(RunnerError::ExecutionInfrastructure)?;
                        return Ok(http_outcome(OutcomeKind::TimedOut, "attempt timed out", start, stats, status, content_type));
                    }
                    Err(HttpRunError::Request(error)) => {
                        let stats = writer.finalize(&context.final_output).await
                            .map_err(RunnerError::ExecutionInfrastructure)?;
                        return Ok(http_outcome(OutcomeKind::FailedRetryable, &format!("HTTP body error: {error}"), start, stats, status, content_type));
                    }
                    Ok(Some(Ok(bytes))) => writer.write(Channel::Body, start.elapsed(), &bytes).await
                        .map_err(RunnerError::ExecutionInfrastructure)?,
                    Ok(Some(Err(error))) => {
                        let stats = writer.finalize(&context.final_output).await
                            .map_err(RunnerError::ExecutionInfrastructure)?;
                        return Ok(http_outcome(OutcomeKind::FailedRetryable, &format!("HTTP body error: {error}"), start, stats, status, content_type));
                    }
                    Ok(None) => break,
                }
            }
        }
        let stats = writer
            .finalize(&context.final_output)
            .await
            .map_err(RunnerError::ExecutionInfrastructure)?;
        let success = status.is_success() || spec.success_statuses.contains(&status.as_u16());
        let kind = if success {
            OutcomeKind::Succeeded
        } else if status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
        {
            OutcomeKind::FailedRetryable
        } else {
            OutcomeKind::Failed
        };
        Ok(http_outcome(
            kind,
            &format!("HTTP response {status}"),
            start,
            stats,
            status,
            content_type,
        ))
    }
}

impl ExecutorPort for Runner {
    type Request = ExecutionRequest;
    type Output = ExecutionOutcome;

    async fn execute(&self, request: Self::Request) -> locron_core::Result<Self::Output> {
        Runner::execute(self, &request.target, &request.context)
            .await
            .map_err(|error| CoreError::Execution(error.to_string()))
    }
}

#[derive(Debug)]
enum HttpRunError {
    Timeout,
    Request(reqwest::Error),
}

async fn read_stream<R: tokio::io::AsyncRead + Unpin>(
    mut input: R,
    channel: Channel,
    sender: mpsc::Sender<(Channel, Vec<u8>)>,
) {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        match input.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                if sender
                    .send((channel, buffer[..count].to_vec()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

async fn terminate_after_output_failure(
    child: &mut Child,
    pid: Option<Pid>,
    leader_reaped: bool,
    grace: Duration,
) {
    let grace = grace.max(Duration::from_millis(1));
    let mut errors = Vec::new();
    if leader_reaped && observe_group_absence(pid, &mut errors) {
        return;
    }
    record_signal_result(
        &mut errors,
        Signal::SIGTERM,
        signal_group(pid, Signal::SIGTERM),
    );
    let (mut leader_reaped, terminated) =
        wait_for_group_exit(child, pid, leader_reaped, grace, &mut errors).await;
    if !terminated {
        record_signal_result(
            &mut errors,
            Signal::SIGKILL,
            signal_group(pid, Signal::SIGKILL),
        );
        (leader_reaped, _) =
            wait_for_group_exit(child, pid, leader_reaped, grace, &mut errors).await;
    }
    if !leader_reaped || !observe_group_absence(pid, &mut errors) {
        tracing::error!(
            details = %termination_confirmation_reason(&errors),
            "failed to confirm process cleanup after output storage failure"
        );
    }
}

async fn wait_for_group_exit(
    child: &mut Child,
    pid: Option<Pid>,
    mut leader_reaped: bool,
    grace: Duration,
    errors: &mut Vec<String>,
) -> (bool, bool) {
    let deadline = Instant::now() + grace;
    loop {
        if leader_reaped && observe_group_absence(pid, errors) {
            return (true, true);
        }
        let now = Instant::now();
        if now >= deadline {
            return (leader_reaped, false);
        }
        let pause = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(10));
        if leader_reaped {
            tokio::time::sleep(pause).await;
        } else {
            tokio::select! {
                result = child.wait() => {
                    leader_reaped = true;
                    if let Err(error) = result {
                        errors.push(format!("leader reap: {error}"));
                    }
                }
                () = tokio::time::sleep(pause) => {}
            }
        }
    }
}

fn signal_group(pid: Option<Pid>, signal: Signal) -> Result<(), Errno> {
    let Some(pid) = pid else {
        return Err(Errno::ESRCH);
    };
    killpg(pid, signal)
}

fn observe_group_absence(pid: Option<Pid>, errors: &mut Vec<String>) -> bool {
    let Some(pid) = pid else {
        return false;
    };
    match kill(Pid::from_raw(pid.as_raw().saturating_neg()), None) {
        Ok(()) | Err(Errno::EPERM) => false,
        Err(Errno::ESRCH) => true,
        Err(error) => {
            errors.push(format!("group liveness: {error}"));
            false
        }
    }
}

fn record_signal_result(errors: &mut Vec<String>, signal: Signal, result: Result<(), Errno>) {
    if let Err(error) = result
        && error != Errno::ESRCH
    {
        errors.push(format!("{signal:?}: {error}"));
    }
}

fn termination_confirmation_reason(errors: &[String]) -> String {
    if errors.is_empty() {
        "termination confirmation failed after TERM and KILL deadlines".into()
    } else {
        format!(
            "termination confirmation failed after TERM and KILL deadlines; signal errors: {}",
            errors.join(", ")
        )
    }
}

async fn finalize_configuration_failure(
    writer: OutputWriter,
    context: &AttemptContext,
    start: Instant,
    reason: &str,
) -> Result<ExecutionOutcome, RunnerError> {
    let stats = writer
        .finalize(&context.final_output)
        .await
        .map_err(RunnerError::OutputPreparation)?;
    Ok(simple_outcome(OutcomeKind::Failed, reason, start, stats))
}

fn simple_outcome(
    kind: OutcomeKind,
    reason: &str,
    start: Instant,
    output: OutputStats,
) -> ExecutionOutcome {
    ExecutionOutcome {
        kind,
        exit_code: None,
        http_status: None,
        http_content_type: None,
        reason: reason.into(),
        duration_micros: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        output,
    }
}

fn http_outcome(
    kind: OutcomeKind,
    reason: &str,
    start: Instant,
    output: OutputStats,
    status: StatusCode,
    content_type: Option<String>,
) -> ExecutionOutcome {
    let mut outcome = simple_outcome(kind, reason, start, output);
    outcome.http_status = Some(status.as_u16());
    outcome.http_content_type = content_type;
    outcome
}

fn build_headers(values: &BTreeMap<String, String>) -> Result<HeaderMap, RunnerError> {
    let mut headers = HeaderMap::new();
    for (name, value) in values {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| RunnerError::Configuration(format!("invalid header name: {name}")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| RunnerError::Configuration("invalid header value".into()))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

/// Resolves a path-bearing executable or searches an explicit PATH.
#[must_use]
pub fn resolve_executable(executable: &str, cwd: &Path, path: Option<&String>) -> Option<PathBuf> {
    let executable_path = Path::new(executable);
    if executable_path.components().count() > 1 {
        let candidate = if executable_path.is_absolute() {
            executable_path.to_path_buf()
        } else {
            cwd.join(executable_path)
        };
        return candidate.is_file().then_some(candidate);
    }
    for directory in std::env::split_paths(path.map_or("", String::as_str)) {
        let candidate = directory.join(executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 1024];
            let count = stream.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") else {
                continue;
            };
            let header_end = header_end + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            if request.len() >= header_end + content_length {
                break;
            }
        }
        request
    }

    async fn follow_redirect(status: u16, method: &str) -> (ExecutionOutcome, String) {
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_address = destination.local_addr().unwrap();
        let (request_sender, request_receiver) = tokio::sync::oneshot::channel();
        let destination_task = tokio::spawn(async move {
            let (mut stream, _) = destination.accept().await.unwrap();
            request_sender
                .send(read_request(&mut stream).await)
                .unwrap();
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Type: application/final+json; charset=utf-8\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_address = source.local_addr().unwrap();
        let source_task = tokio::spawn(async move {
            let (mut stream, _) = source.accept().await.unwrap();
            let _ = read_request(&mut stream).await;
            let reason = match status {
                301 => "Moved Permanently",
                302 => "Found",
                303 => "See Other",
                307 => "Temporary Redirect",
                308 => "Permanent Redirect",
                _ => panic!("unsupported fixture status"),
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nLocation: http://{destination_address}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let target = TargetSpec::Http(HttpSpec {
            method: method.into(),
            url: format!("http://{source_address}/start").parse().unwrap(),
            headers: BTreeMap::from([("Content-Type".into(), "text/plain".into())]),
            body: Some(b"payload".to_vec()),
            success_statuses: vec![],
            follow_redirects: true,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context(&temp))
            .await
            .unwrap();
        source_task.await.unwrap();
        destination_task.await.unwrap();
        let request = String::from_utf8(request_receiver.await.unwrap()).unwrap();
        (outcome, request)
    }

    fn context(temp: &tempfile::TempDir) -> AttemptContext {
        AttemptContext {
            run_id: "run".into(),
            attempt: 1,
            partial_output: temp.path().join("1.partial"),
            final_output: temp.path().join("1.log"),
            output_limit: 1024,
            timeout: Some(Duration::from_secs(2)),
            cancellation: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn core_executor_port_maps_runner_failures_to_execution_errors() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("blocked-parent"), b"not a directory").unwrap();
        let mut attempt = context(&temp);
        attempt.partial_output = temp.path().join("blocked-parent/attempt.partial");
        let request = ExecutionRequest {
            target: TargetSpec::Process(ProcessSpec {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
                cwd: temp.path().into(),
                env: BTreeMap::new(),
            }),
            context: attempt,
        };

        let error = ExecutorPort::execute(&Runner::new(RunnerConfig::default()).unwrap(), request)
            .await
            .unwrap_err();
        assert!(
            matches!(error, CoreError::Execution(message) if message.starts_with("output preparation error:"))
        );
    }

    #[tokio::test]
    async fn post_spawn_output_failure_terminates_and_reaps_the_process_group() {
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("grandchild.pid");
        let attempt = context(&temp);
        std::fs::create_dir(&attempt.final_output).unwrap();
        let spec = ProcessSpec {
            executable: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!(
                    "(trap '' TERM; while :; do sleep 1; done) </dev/null >/dev/null 2>&1 & echo $! > '{}'",
                    pid_file.display()
                ),
            ],
            cwd: temp.path().into(),
            env: BTreeMap::new(),
        };

        let error = Runner::new(RunnerConfig {
            termination_grace: Duration::from_millis(40),
            ..RunnerConfig::default()
        })
        .unwrap()
        .execute(&TargetSpec::Process(spec), &attempt)
        .await
        .unwrap_err();

        assert_eq!(
            error.failure_kind(),
            RunnerFailureKind::ExecutionMayHaveStarted
        );
        let grandchild = Pid::from_raw(
            std::fs::read_to_string(pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap(),
        );
        let mut gone = false;
        for _ in 0..100 {
            if matches!(kill(grandchild, None), Err(Errno::ESRCH)) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(gone, "spawned process-group member survived output failure");
    }

    #[test]
    fn injected_signal_results_distinguish_already_gone_from_unconfirmed_failure() {
        let mut errors = Vec::new();
        record_signal_result(&mut errors, Signal::SIGTERM, Err(Errno::ESRCH));
        record_signal_result(&mut errors, Signal::SIGKILL, Err(Errno::EPERM));
        assert_eq!(errors.len(), 1);
        let reason = termination_confirmation_reason(&errors);
        assert!(reason.contains("termination confirmation failed"));
        assert!(reason.contains("EPERM"));
    }

    #[tokio::test]
    async fn process_preserves_argv_and_streams_output() {
        let temp = tempfile::tempdir().unwrap();
        let spec = ProcessSpec {
            executable: "/bin/sh".into(),
            args: vec!["-c".into(), "printf 'hello'; printf 'bad' >&2".into()],
            cwd: temp.path().into(),
            env: BTreeMap::new(),
        };
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&TargetSpec::Process(spec), &context(&temp))
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded);
        let frames = crate::output::read_frames(temp.path().join("1.log")).unwrap();
        assert!(
            frames
                .iter()
                .any(|frame| frame.channel == Channel::Stdout && frame.payload == b"hello")
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.channel == Channel::Stderr && frame.payload == b"bad")
        );
    }

    #[tokio::test]
    async fn noisy_process_output_is_drained_and_truncated() {
        let temp = tempfile::tempdir().unwrap();
        let mut context = context(&temp);
        context.output_limit = 4 * 1024;
        context.timeout = Some(Duration::from_secs(5));
        let spec = ProcessSpec {
            executable: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "i=0; while [ \"$i\" -lt 2048 ]; do printf 0123456789abcdef; printf 0123456789abcdef >&2; i=$((i + 1)); done".into(),
            ],
            cwd: temp.path().into(),
            env: BTreeMap::new(),
        };
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&TargetSpec::Process(spec), &context)
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded);
        assert_eq!(outcome.output.retained_bytes, 4 * 1024);
        assert_eq!(outcome.output.discarded_bytes, 60 * 1024);
        assert!(outcome.output.truncated);
        let retained = crate::output::read_frames(temp.path().join("1.log"))
            .unwrap()
            .iter()
            .map(|frame| frame.payload.len())
            .sum::<usize>();
        assert_eq!(retained, 4 * 1024);
    }

    #[tokio::test]
    async fn missing_executable_is_a_finalized_configuration_outcome() {
        let temp = tempfile::tempdir().unwrap();
        let spec = ProcessSpec {
            executable: "definitely-not-a-locron-test-executable".into(),
            args: Vec::new(),
            cwd: temp.path().into(),
            env: BTreeMap::from([("PATH".into(), temp.path().display().to_string())]),
        };
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&TargetSpec::Process(spec), &context(&temp))
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Failed);
        assert!(outcome.reason.contains("executable not found"));
        assert!(temp.path().join("1.log").is_file());
        assert!(!temp.path().join("1.partial").exists());
    }

    #[tokio::test]
    async fn missing_working_directory_is_a_finalized_configuration_outcome() {
        let temp = tempfile::tempdir().unwrap();
        let spec = ProcessSpec {
            executable: "/usr/bin/true".into(),
            args: Vec::new(),
            cwd: temp.path().join("removed"),
            env: BTreeMap::new(),
        };
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&TargetSpec::Process(spec), &context(&temp))
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Failed);
        assert!(outcome.reason.contains("working directory"));
        assert!(temp.path().join("1.log").is_file());
        assert!(!temp.path().join("1.partial").exists());
    }

    #[tokio::test]
    async fn timeout_is_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let mut context = context(&temp);
        context.timeout = Some(Duration::from_millis(20));
        let spec = ProcessSpec {
            executable: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 2".into()],
            cwd: temp.path().into(),
            env: BTreeMap::new(),
        };
        let runner = Runner::new(RunnerConfig {
            termination_grace: Duration::from_millis(10),
            ..RunnerConfig::default()
        })
        .unwrap();
        let outcome = runner
            .execute(&TargetSpec::Process(spec), &context)
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::TimedOut);
    }

    #[tokio::test]
    async fn cancellation_kills_a_live_process_grandchild() {
        let temp = tempfile::tempdir().unwrap();
        let child_script = temp.path().join("child.sh");
        let grandchild_script = temp.path().join("grandchild.sh");
        let ready = temp.path().join("grandchild-ready");
        std::fs::write(
            &child_script,
            format!(
                "trap 'exit 0' TERM\n/bin/sh {} &\nwait\n",
                grandchild_script.display()
            ),
        )
        .unwrap();
        std::fs::write(
            &grandchild_script,
            "trap '' TERM\nprintf ready > grandchild-ready\nsleep 5\n",
        )
        .unwrap();
        let mut context = context(&temp);
        context.timeout = None;
        let cancellation = context.cancellation.clone();
        tokio::spawn(async move {
            for _ in 0..100 {
                if tokio::fs::try_exists(&ready).await.unwrap() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            cancellation.cancel();
        });
        let spec = ProcessSpec {
            executable: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!(
                    "trap 'exit 0' TERM; /bin/sh {} & wait",
                    child_script.display()
                ),
            ],
            cwd: temp.path().into(),
            env: BTreeMap::new(),
        };
        let start = Instant::now();
        let outcome = Runner::new(RunnerConfig {
            termination_grace: Duration::from_millis(40),
            ..RunnerConfig::default()
        })
        .unwrap()
        .execute(&TargetSpec::Process(spec), &context)
        .await
        .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Cancelled);
        assert!(start.elapsed() >= Duration::from_millis(40));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn materialized_http_body_survives_source_file_disappearing() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
            request
        });
        let temp = tempfile::tempdir().unwrap();
        let body_path = temp.path().join("request.body");
        std::fs::write(&body_path, b"materialized payload").unwrap();
        let body = std::fs::read(&body_path).unwrap();
        std::fs::remove_file(&body_path).unwrap();
        let target = TargetSpec::Http(HttpSpec {
            method: "POST".into(),
            url: format!("http://{address}/body").parse().unwrap(),
            headers: BTreeMap::new(),
            body: Some(body),
            success_statuses: vec![],
            follow_redirects: false,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context(&temp))
            .await
            .unwrap();
        let request = fixture.await.unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded);
        assert!(!body_path.exists());
        assert!(request.ends_with(b"materialized payload"));
    }

    #[tokio::test]
    async fn http_500_is_retryable_and_body_is_captured() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\nConnection: close\r\n\r\noops",
            )
            .await
            .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let target = TargetSpec::Http(HttpSpec {
            method: "GET".into(),
            url: format!("http://{address}/failure").parse().unwrap(),
            headers: BTreeMap::new(),
            body: None,
            success_statuses: vec![],
            follow_redirects: false,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context(&temp))
            .await
            .unwrap();
        fixture.await.unwrap();
        assert_eq!(outcome.kind, OutcomeKind::FailedRetryable);
        assert_eq!(outcome.http_status, Some(500));
        assert_eq!(outcome.http_content_type, None);
        assert_eq!(
            crate::output::read_frames(temp.path().join("1.log")).unwrap()[0].payload,
            b"oops"
        );
    }

    #[tokio::test]
    async fn timeout_while_streaming_finalizes_captured_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_request(&mut stream).await;
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n",
            )
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;
        });
        let temp = tempfile::tempdir().unwrap();
        let mut context = context(&temp);
        context.timeout = Some(Duration::from_millis(100));
        let target = TargetSpec::Http(HttpSpec {
            method: "GET".into(),
            url: format!("http://{address}/stream").parse().unwrap(),
            headers: BTreeMap::new(),
            body: None,
            success_statuses: vec![],
            follow_redirects: false,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context)
            .await
            .unwrap();
        fixture.await.unwrap();
        assert_eq!(outcome.kind, OutcomeKind::TimedOut);
        assert_eq!(outcome.http_status, Some(200));
        assert_eq!(
            outcome.http_content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );
        assert!(!temp.path().join("1.partial").exists());
        let body = crate::output::read_frames(temp.path().join("1.log"))
            .unwrap()
            .into_iter()
            .flat_map(|frame| frame.payload)
            .collect::<Vec<_>>();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn untrusted_local_tls_certificate_is_a_retryable_transport_failure() {
        let temp = tempfile::tempdir().unwrap();
        let certificate = temp.path().join("certificate.pem");
        let private_key = temp.path().join("private-key.pem");
        let generated = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                private_key.to_str().unwrap(),
                "-out",
                certificate.to_str().unwrap(),
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("openssl is required for the local TLS fixture");
        assert!(generated.success());

        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let port = address.port().to_string();
        let mut server = Command::new("openssl");
        server
            .args([
                "s_server",
                "-accept",
                &port,
                "-cert",
                certificate.to_str().unwrap(),
                "-key",
                private_key.to_str().unwrap(),
                "-www",
                "-quiet",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut server = server.spawn().unwrap();
        let mut listening = false;
        for _ in 0..100 {
            match tokio::net::TcpStream::connect(address).await {
                Ok(stream) => {
                    drop(stream);
                    listening = true;
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        assert!(listening, "local TLS fixture did not start");

        let target = TargetSpec::Http(HttpSpec {
            method: "GET".into(),
            url: format!("https://localhost:{}/", address.port())
                .parse()
                .unwrap(),
            headers: BTreeMap::new(),
            body: None,
            success_statuses: vec![],
            follow_redirects: false,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context(&temp))
            .await
            .unwrap();
        server.start_kill().unwrap();
        server.wait().await.unwrap();
        assert_eq!(outcome.kind, OutcomeKind::FailedRetryable);
        assert_eq!(outcome.http_status, None);
        assert!(outcome.reason.starts_with("HTTP transport error:"));
    }

    #[tokio::test]
    async fn cross_origin_redirect_removes_authorization() {
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_address = destination.local_addr().unwrap();
        let (request_sender, request_receiver) = tokio::sync::oneshot::channel();
        let destination_task = tokio::spawn(async move {
            let (mut stream, _) = destination.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let count = stream.read(&mut request).await.unwrap();
            request.truncate(count);
            request_sender.send(request).unwrap();
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_address = source.local_addr().unwrap();
        let source_task = tokio::spawn(async move {
            let (mut stream, _) = source.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let target = TargetSpec::Http(HttpSpec {
            method: "GET".into(),
            url: format!("http://{source_address}/start").parse().unwrap(),
            headers: BTreeMap::from([("Authorization".into(), "Bearer secret".into())]),
            body: None,
            success_statuses: vec![],
            follow_redirects: true,
        });
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&target, &context(&temp))
            .await
            .unwrap();
        source_task.await.unwrap();
        destination_task.await.unwrap();
        let request = String::from_utf8(request_receiver.await.unwrap()).unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Succeeded);
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn conventional_redirects_rewrite_method_and_drop_body() {
        for (status, method) in [(301, "POST"), (302, "POST"), (303, "PUT")] {
            let (outcome, request) = follow_redirect(status, method).await;
            assert_eq!(outcome.kind, OutcomeKind::Succeeded);
            assert_eq!(
                outcome.http_content_type.as_deref(),
                Some("application/final+json; charset=utf-8")
            );
            assert!(request.starts_with("GET /target HTTP/1.1\r\n"), "{request}");
            assert!(!request.ends_with("payload"), "{request}");
            assert!(!request.to_ascii_lowercase().contains("content-type:"));
        }
    }

    #[tokio::test]
    async fn strict_redirects_preserve_method_and_body() {
        for (status, method) in [(307, "POST"), (308, "PATCH")] {
            let (outcome, request) = follow_redirect(status, method).await;
            assert_eq!(outcome.kind, OutcomeKind::Succeeded);
            assert_eq!(
                outcome.http_content_type.as_deref(),
                Some("application/final+json; charset=utf-8")
            );
            assert!(
                request.starts_with(&format!("{method} /target HTTP/1.1\r\n")),
                "{request}"
            );
            assert!(request.ends_with("payload"), "{request}");
            assert!(request.to_ascii_lowercase().contains("content-type:"));
        }
    }

    #[tokio::test]
    async fn see_other_preserves_head() {
        let (outcome, request) = follow_redirect(303, "HEAD").await;
        assert_eq!(outcome.kind, OutcomeKind::Succeeded);
        assert_eq!(
            outcome.http_content_type.as_deref(),
            Some("application/final+json; charset=utf-8")
        );
        assert!(
            request.starts_with("HEAD /target HTTP/1.1\r\n"),
            "{request}"
        );
    }

    #[test]
    fn strips_sensitive_headers_on_cross_origin() {
        assert!(!same_origin(
            &Url::parse("https://a.test/x").unwrap(),
            &Url::parse("https://b.test/x").unwrap()
        ));
        assert!(same_origin(
            &Url::parse("https://a.test/x").unwrap(),
            &Url::parse("https://a.test/y").unwrap()
        ));
    }
}
