//! `locron` command-line composition root.

mod maintenance;
mod mcp;
mod self_update;
mod service;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use url::Url;

use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use locron_core::command::JobDefinition;
use locron_core::policy::{BackoffMode, MissedRunPolicy, OverlapPolicy};
use locron_core::ports::{Clock, TimeZoneResolver};
use locron_core::schedule::{Schedule, ScheduleTimeZone};
use locron_core::target::{
    Environment, HttpHeaderSource, HttpMethod, HttpTarget, Target, is_valid_environment_name,
    is_valid_http_header_name,
};
use locron_core::{
    CoreError, ElapsedKind, JobId, OmittedRangeKind, SchedulerLifetimeId, Timestamp,
};
use locron_engine::admission::{RetryClass, decide_retry};
use locron_engine::daemon::{AdmittedAttempt, CompletionError, DaemonStore};
use locron_engine::runner::{OutcomeKind, RunnerConfig, resolve_executable};
use locron_engine::{
    AttemptContext, Daemon, DaemonConfig, HttpSpec, OutputWriter, ProcessSpec, Runner, TargetSpec,
};
use locron_store::{
    AdmitAttempt, AttemptCompletion, CancelOutcome, CreateJob, CursorUpdate, EventRecord,
    ImportBatch, ImportJob, ImportResolution, JobRecord, LockMetadata, NewScheduledRun,
    OutputRecord, ReconciliationSummary, RetryPlan, RunRecord, SettingsRecord, StatePaths, Store,
    StoreError, UpdateJob,
};
use self_update::SelfUpdateError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service::ServiceError;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

const ROOT_HELP: &str = "\
Examples:
  locron list
  locron add backup --every 1h -- /usr/bin/backup

Navigation:
  Run 'locron help <COMMAND>' for detailed command help.";
const ADD_HELP: &str = "\
Examples:
  locron add backup --every 1h -- /usr/bin/backup
  locron add heartbeat --cron '*/5 * * * *' --http GET https://example.test/health

Navigation:
  Run 'locron --help' to list all commands.";
const UPDATE_HELP: &str = "\
Examples:
  locron update backup --retries 3 --dry-run
  locron update heartbeat --cron '*/10 * * * *' --timezone UTC

Navigation:
  Run 'locron --help' to list all commands.";
const LIST_HELP: &str = "\
Examples:
  locron list
  locron list --all
  locron list --no-trunc

Navigation:
  Run 'locron --help' to list all commands.";
const SHOW_HELP: &str = "\
Examples:
  locron show backup

Navigation:
  Run 'locron --help' to list all commands.";
const ENABLE_HELP: &str = "\
Examples:
  locron enable backup

Navigation:
  Run 'locron --help' to list all commands.";
const DISABLE_HELP: &str = "\
Examples:
  locron disable backup

Navigation:
  Run 'locron --help' to list all commands.";
const REMOVE_HELP: &str = "\
Examples:
  locron remove backup

Navigation:
  Run 'locron --help' to list all commands.";
const PREVIEW_HELP: &str = "\
Examples:
  locron preview backup --count 10
  locron preview --cron '0 9 * * MON-FRI' --timezone Europe/London

Navigation:
  Run 'locron --help' to list all commands.";
const RUN_HELP: &str = "\
Examples:
  locron run backup
  locron run backup --wait
  locron run backup --dry-run

Navigation:
  Run 'locron --help' to list all commands.";
const CANCEL_HELP: &str = "\
Examples:
  locron cancel 018f47a2-4a12-7c35-b9d8-0123456789ab

Navigation:
  Run 'locron --help' to list all commands.";
const HISTORY_HELP: &str = "\
Examples:
  locron history
  locron history backup --limit 50

Navigation:
  Run 'locron --help' to list all commands.";
const LOGS_HELP: &str = "\
Examples:
  locron logs 018f47a2-4a12-7c35-b9d8-0123456789ab
  locron logs 018f47a2-4a12-7c35-b9d8-0123456789ab --follow --channel stderr

Navigation:
  Run 'locron --help' to list all commands.";
const WHY_HELP: &str = "\
Examples:
  locron why backup
  locron why --run 018f47a2-4a12-7c35-b9d8-0123456789ab

Navigation:
  Run 'locron --help' to list all commands.";
const CONFIG_HELP: &str = "\
Examples:
  locron config get
  locron config set global_concurrency 32 --dry-run

Navigation:
  Run 'locron help config <COMMAND>' for a config command or 'locron --help' for all commands.";
const CONFIG_GET_HELP: &str = "\
Examples:
  locron config get
  locron config get global_concurrency

Navigation:
  Run 'locron config --help' for config commands or 'locron --help' for all commands.";
const CONFIG_SET_HELP: &str = "\
Examples:
  locron config set global_concurrency 32
  locron config set environment.API_TOKEN value
  locron config set global_concurrency 32 --dry-run

Navigation:
  Run 'locron config --help' for config commands or 'locron --help' for all commands.";
const CONFIG_UNSET_HELP: &str = "\
Examples:
  locron config unset environment.API_TOKEN
  locron config unset environment.API_TOKEN --dry-run

Navigation:
  Run 'locron config --help' for config commands or 'locron --help' for all commands.";
const EXPORT_HELP: &str = "\
Examples:
  locron export
  locron export --jobs backup,heartbeat
  locron export --tag nightly
  locron export --include-values --acknowledge-plaintext

In an interactive terminal, bare 'locron export' shows a multi-select of every
job (all initially selected) on standard error; with a piped or redirected
standard output, in JSON mode, or with '--jobs'/'--tag', it exports without
prompting. Selectors are exact-name/exact-tag unions; a selector matching no
job is rejected before any output.

Navigation:
  Run 'locron --help' to list all commands.";
const IMPORT_HELP: &str = "\
Examples:
  locron import backup.json --dry-run
  locron import backup.json --accept-plaintext-values
  locron import https://example.test/backup.json --dry-run

Imports a locron.export/v1 document from a local path or an absolute HTTP(S)
URL. URL imports carry the same trust boundary as installing a script from
that URL; review first-time imports with --dry-run.

Navigation:
  Run 'locron --help' to list all commands.";
const PRUNE_HELP: &str = "\
Examples:
  locron prune --dry-run
  locron prune

Navigation:
  Run 'locron --help' to list all commands.";
const DOCTOR_HELP: &str = "\
Examples:
  locron doctor

Navigation:
  Run 'locron --help' to list all commands.";
const DAEMON_HELP: &str = "\
Examples:
  locron daemon run

Navigation:
  Run 'locron help daemon <COMMAND>' for a daemon command or 'locron --help' for all commands.";
const DAEMON_RUN_HELP: &str = "\
Examples:
  locron daemon run

Navigation:
  Run 'locron daemon --help' for daemon commands or 'locron --help' for all commands.";
const MCP_HELP: &str = "\
Examples:
  locron mcp

Navigation:
  Run 'locron --help' to list all commands.";
const SELF_UPDATE_HELP: &str = "\
Examples:
  locron self-update

Replaces the running binary with the latest stable release after verifying its
checksum against the release's SHA256SUMS.txt. The running process keeps the
old code until it restarts. Package-manager-managed installs are refused.

Navigation:
  Run 'locron --help' to list all commands.";
const SERVICE_HELP: &str = "\
Examples:
  locron service install
  locron service status
  locron service uninstall

Registers, unregisters, or inspects the per-user daemon service. On macOS the
service is a LaunchAgent (dev.locron.daemon) registered with launchctl; on
Linux it is a systemd user unit (locron.service). Registration never requires
administrative privileges. Package-manager-managed installs are refused; use
the package manager's own service commands there (for example
'brew services start locron').

Navigation:
  Run 'locron service <COMMAND>' for a subcommand or 'locron --help' for all commands.";

#[derive(Parser, Debug)]
#[command(
    name = "locron",
    about = "A predictable local-first job scheduler",
    after_help = ROOT_HELP,
    disable_version_flag = true,
    arg_required_else_help = true,
    // The subcommand is optional so -V/--version can short-circuit, but the
    // usage keeps clap's required-command spelling.
    override_usage = "locron [OPTIONS] <COMMAND>"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "LOCRON_STATE_DIR",
        value_name = "PATH",
        help = "Override the discovered state directory"
    )]
    state_dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "human",
        help = "Select the output format"
    )]
    format: Format,
    #[arg(
        long,
        global = true,
        action = ArgAction::SetTrue,
        conflicts_with = "format",
        help = "Alias for --format json"
    )]
    json: bool,
    #[arg(
        short = 'v',
        long,
        global = true,
        action = ArgAction::Count,
        help = "Repeatable diagnostics on stderr: decisions, then timing and storage context"
    )]
    verbose: u8,
    #[arg(
        long,
        global = true,
        help = "Developer trace diagnostics on stderr; implies maximum verbosity"
    )]
    debug: bool,
    #[arg(
        short = 'V',
        long,
        action = ArgAction::SetTrue,
        display_order = 1000,
        help = "Print version"
    )]
    version: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum Format {
    /// Human-readable output
    Human,
    /// Machine-readable locron.cli/v1 JSON envelope
    Json,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Register a scheduled job", after_help = ADD_HELP)]
    Add(AddArgs),
    #[command(about = "Change an existing job", after_help = UPDATE_HELP)]
    Update(UpdateArgs),
    #[command(about = "List jobs", visible_alias = "ls", after_help = LIST_HELP)]
    List {
        /// Include disabled jobs
        #[arg(long)]
        all: bool,
        /// Print full TARGET values instead of fitting the terminal width
        #[arg(long)]
        no_trunc: bool,
    },
    #[command(about = "Show a job's current definition", after_help = SHOW_HELP)]
    Show {
        /// Job name or canonical UUID
        name: String,
    },
    #[command(about = "Enable a job", after_help = ENABLE_HELP)]
    Enable {
        /// Job name or canonical UUID
        name: String,
    },
    #[command(about = "Disable a job", after_help = DISABLE_HELP)]
    Disable {
        /// Job name or canonical UUID
        name: String,
    },
    #[command(about = "Soft-remove a job", visible_alias = "rm", after_help = REMOVE_HELP)]
    Remove {
        /// Job name or canonical UUID
        name: String,
    },
    #[command(
        about = "Preview upcoming schedule occurrences",
        after_help = PREVIEW_HELP
    )]
    Preview(PreviewArgs),
    #[command(about = "Queue a manual run", after_help = RUN_HELP)]
    Run {
        /// Job name or canonical UUID
        name: String,
        /// Wait for the run to reach a terminal state, streaming output
        #[arg(long)]
        wait: bool,
        /// Simulate the admission decision without enqueueing
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Cancel a queued or active run", after_help = CANCEL_HELP)]
    Cancel {
        /// Run UUID to cancel
        run_id: String,
        /// Accept the risk that a quarantined target may still run; valid only for termination_unconfirmed runs
        #[arg(long)]
        acknowledge_unconfirmed: bool,
    },
    #[command(about = "List run history", after_help = HISTORY_HELP)]
    History {
        /// Job name or canonical UUID to restrict history to
        name: Option<String>,
        /// Maximum number of runs to return
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    #[command(about = "Read captured run output", after_help = LOGS_HELP)]
    Logs {
        /// Run UUID whose output to read
        run_id: String,
        /// Attempt number to read; defaults to 1
        #[arg(long)]
        attempt: Option<u16>,
        /// Stream output until the attempt finalizes
        #[arg(long)]
        follow: bool,
        /// Output channel to include
        #[arg(long, value_enum, default_value = "all")]
        channel: LogChannel,
    },
    #[command(about = "Explain durable job or run decisions", after_help = WHY_HELP)]
    Why {
        /// Job name or canonical UUID to explain
        name: Option<String>,
        /// Run UUID to explain instead of a job
        #[arg(long)]
        run: Option<String>,
    },
    #[command(about = "Inspect or change global settings", after_help = CONFIG_HELP)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    #[command(about = "Export settings and job definitions", after_help = EXPORT_HELP)]
    Export {
        /// Export exactly these job names (comma-separated; union with --tag)
        #[arg(long, value_name = "NAME[,NAME...]")]
        jobs: Option<String>,
        /// Export exactly these tags (comma-separated; union with --jobs)
        #[arg(long, value_name = "TAG[,TAG...]")]
        tag: Option<String>,
        /// Include inline environment values, headers, and bodies in plaintext
        #[arg(long)]
        include_values: bool,
        /// Confirm the export contains plaintext secrets; requires --include-values
        #[arg(long)]
        acknowledge_plaintext: bool,
        /// Include run history (rejected: unsupported by locron.export/v1)
        #[arg(long)]
        include_history: bool,
    },
    #[command(about = "Import settings and job definitions", after_help = IMPORT_HELP)]
    Import {
        /// locron.export/v1 document to import (local path or http(s) URL)
        path: PathBuf,
        /// Confirm the document's plaintext values may be imported
        #[arg(long)]
        accept_plaintext_values: bool,
        /// Plan the import without writing
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Apply configured retention limits", after_help = PRUNE_HELP)]
    Prune {
        /// Report candidates without deleting
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Report state and daemon diagnostics", after_help = DOCTOR_HELP)]
    Doctor,
    #[command(about = "Run scheduler service commands", after_help = DAEMON_HELP)]
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Serve the Model Context Protocol (MCP) over stdio
    #[command(about = "Serve the Model Context Protocol (MCP) over stdio", after_help = MCP_HELP)]
    Mcp,
    #[command(about = "Replace this binary with the latest stable release", after_help = SELF_UPDATE_HELP)]
    SelfUpdate,
    #[command(about = "Register, unregister, or inspect the daemon service", after_help = SERVICE_HELP)]
    Service {
        #[command(subcommand)]
        command: service::ServiceCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    #[command(about = "Read one or all global settings", after_help = CONFIG_GET_HELP)]
    Get {
        /// Setting key, or environment.NAME; omit to read all
        key: Option<String>,
    },
    #[command(about = "Change a global setting", after_help = CONFIG_SET_HELP)]
    Set {
        /// Setting key or environment.NAME
        key: String,
        /// New value
        value: String,
        /// Validate and report without writing
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Remove a global environment value", after_help = CONFIG_UNSET_HELP)]
    Unset {
        /// environment.NAME to remove
        key: String,
        /// Validate and report without writing
        #[arg(long)]
        dry_run: bool,
    },
}
#[derive(Subcommand, Debug)]
enum DaemonCommand {
    #[command(about = "Run the scheduler in the foreground", after_help = DAEMON_RUN_HELP)]
    Run,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum LogChannel {
    /// stdout, stderr, and HTTP response body
    All,
    /// Process stdout only
    Stdout,
    /// Process stderr only
    Stderr,
    /// HTTP response body only
    Body,
}

#[derive(Args, Debug, Clone, Default)]
struct ScheduleArgs {
    /// Five-field cron expression: minute hour day-of-month month day-of-week
    #[arg(long, value_name = "EXPR")]
    cron: Option<String>,
    /// Fixed interval, such as 30s, 1h, or 2d
    #[arg(long, value_name = "DURATION")]
    every: Option<String>,
    /// One-time ISO 8601 timestamp with an explicit offset
    #[arg(long, value_name = "RFC3339")]
    at: Option<String>,
    /// 'local' or an IANA time zone name; valid only with --cron
    #[arg(long, value_name = "IANA")]
    timezone: Option<String>,
    /// Interval origin; valid only with --every, defaults to creation time
    #[arg(long, value_name = "RFC3339")]
    anchor: Option<String>,
}

#[derive(Args, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct TargetArgs {
    /// Shell command string; one target selector of -- COMMAND, --shell, or --http
    #[arg(long, value_name = "COMMAND")]
    shell: Option<String>,
    /// HTTP request target; METHOD is one of GET, POST, PUT, PATCH, DELETE, or HEAD
    #[arg(long, num_args = 2, value_names = ["METHOD", "URL"])]
    http: Option<Vec<String>>,
    /// Working directory; requires a process or shell target
    #[arg(long, value_name = "PATH")]
    cwd: Option<PathBuf>,
    /// Set an environment variable; repeatable
    #[arg(long = "env", value_parser = parse_key_value, value_name = "NAME=VALUE")]
    env: Vec<(String, String)>,
    /// Remove an environment variable; update only, repeatable
    #[arg(long, value_name = "NAME")]
    unset_env: Vec<String>,
    /// Remove all job environment variables; update only
    #[arg(long)]
    clear_env: bool,
    /// Read environment variables from a file at execution time
    #[arg(long, value_name = "PATH")]
    env_file: Option<PathBuf>,
    /// Stop reading the configured environment file; update only
    #[arg(long)]
    no_env_file: bool,
    /// Execution PATH for the target, colon-separated
    #[arg(long, value_name = "PATH_LIST")]
    path: Option<String>,
    /// Restore the global execution PATH; update only
    #[arg(long)]
    no_path: bool,
    /// Absolute shell executable for a shell target
    #[arg(long, value_name = "PATH")]
    shell_executable: Option<PathBuf>,
    /// HTTP request body as inline text
    #[arg(long, conflicts_with_all = ["body_file", "json_body", "clear_body"], value_name = "TEXT")]
    body: Option<String>,
    /// HTTP request body read from a file at execution time
    #[arg(long, conflicts_with_all = ["body", "json_body", "clear_body"], value_name = "PATH")]
    body_file: Option<PathBuf>,
    /// HTTP request body from one JSON value; sets Content-Type: application/json
    #[arg(long, conflicts_with_all = ["body", "body_file", "clear_body"], value_name = "JSON")]
    json_body: Option<String>,
    /// Remove the configured HTTP body; update only
    #[arg(long, conflicts_with_all = ["body", "body_file", "json_body"])]
    clear_body: bool,
    /// Set an HTTP request header from an inline value; repeatable
    #[arg(long, value_parser = parse_key_value, value_name = "NAME=VALUE")]
    header: Vec<(String, String)>,
    /// Set an HTTP request header from an environment variable; repeatable
    #[arg(long, value_parser = parse_key_value, value_name = "NAME=ENV_NAME")]
    header_env: Vec<(String, String)>,
    /// Remove an HTTP request header; update only, repeatable
    #[arg(long, value_name = "NAME")]
    unset_header: Vec<String>,
    /// Remove all HTTP request headers; update only
    #[arg(long)]
    clear_headers: bool,
    /// Add a success status code or inclusive range such as 200-204; repeatable
    #[arg(long, value_name = "STATUS")]
    success_status: Vec<String>,
    /// Restore the default 2xx success statuses; update only
    #[arg(long)]
    clear_success_statuses: bool,
    /// Follow up to 10 HTTP redirects
    #[arg(long, conflicts_with = "no_follow_redirects")]
    follow_redirects: bool,
    /// Do not follow HTTP redirects; update only
    #[arg(long, conflicts_with = "follow_redirects")]
    no_follow_redirects: bool,
    /// Command and arguments for a direct process target, after '--'
    #[arg(last = true, value_name = "COMMAND")]
    command: Vec<String>,
}

#[derive(Args, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct PolicyArgs {
    /// Overlap policy while a run is active
    #[arg(long, value_enum)]
    overlap: Option<OverlapArg>,
    /// Missed-run policy after downtime or disablement
    #[arg(long, value_enum)]
    missed_run: Option<MissedArg>,
    /// Skip occurrences older than this lateness limit
    #[arg(long, value_name = "DURATION")]
    start_deadline: Option<String>,
    /// Remove the start deadline; update only
    #[arg(long, conflicts_with = "start_deadline")]
    no_start_deadline: bool,
    /// Maximum missed-run catch-up batch size, 1 through 1000; default 100
    #[arg(long, value_name = "N")]
    catch_up_limit: Option<u16>,
    /// Retries after a known failure, 0 through 10; default 0
    #[arg(long, value_name = "N")]
    retries: Option<u8>,
    /// Retry delay schedule
    #[arg(long, value_enum)]
    backoff: Option<BackoffArg>,
    /// Initial retry delay; default 10s
    #[arg(long, value_name = "DURATION")]
    retry_delay: Option<String>,
    /// Maximum exponential retry delay; default 5m
    #[arg(long, value_name = "DURATION")]
    retry_cap: Option<String>,
    /// Attempt timeout; default 60s
    #[arg(long, conflicts_with = "no_timeout", value_name = "DURATION")]
    timeout: Option<String>,
    /// Run attempts without a timeout
    #[arg(long, conflicts_with = "timeout")]
    no_timeout: bool,
    /// Make timeout failures eligible for retry
    #[arg(long, conflicts_with = "no_retry_timeout")]
    retry_timeout: bool,
    /// Do not retry timeout failures; update only
    #[arg(long, conflicts_with = "retry_timeout")]
    no_retry_timeout: bool,
    /// Grace period before SIGKILL on timeout, cancel, or replace; default 5s
    #[arg(long, value_name = "DURATION")]
    termination_grace: Option<String>,
    /// Concurrent attempts allowed for this job; default 1, or 2 with --overlap allow
    #[arg(long, value_name = "N")]
    per_job_concurrency: Option<u8>,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OverlapArg {
    /// Record the occurrence as skipped_overlap
    Skip,
    /// Terminate the active run and start the newest occurrence
    Replace,
    /// Permit concurrent runs up to per-job concurrency
    Allow,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum MissedArg {
    /// Do not run missed occurrences (default for cron and interval schedules)
    Skip,
    /// Run only the latest eligible missed occurrence (default for one-time schedules)
    Latest,
    /// Run all eligible missed occurrences oldest-first, bounded by --catch-up-limit
    All,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackoffArg {
    /// Constant delay, each retry waits --retry-delay
    Fixed,
    /// Double the delay per attempt, capped by --retry-cap
    Exponential,
}

#[derive(Args, Debug)]
struct AddArgs {
    /// Unique job name
    name: String,
    #[command(flatten)]
    schedule: ScheduleArgs,
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    policy: PolicyArgs,
    /// Free-text description
    #[arg(long)]
    description: Option<String>,
    /// Attach a tag; repeatable
    #[arg(long)]
    tag: Vec<String>,
    /// Register the job disabled
    #[arg(long)]
    disabled: bool,
    /// Show the normalized definition without registering
    #[arg(long)]
    dry_run: bool,
}
#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct UpdateArgs {
    /// Job to update, by name or canonical UUID
    name: String,
    #[command(flatten)]
    schedule: ScheduleArgs,
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    policy: PolicyArgs,
    /// New job name
    #[arg(long)]
    rename: Option<String>,
    /// New description
    #[arg(long, conflicts_with = "clear_description")]
    description: Option<String>,
    /// Remove the description
    #[arg(long, conflicts_with = "description")]
    clear_description: bool,
    /// Replace the complete tag list; repeatable
    #[arg(long)]
    tag: Vec<String>,
    /// Remove all tags
    #[arg(long, conflicts_with = "tag")]
    clear_tags: bool,
    /// Enable the job
    #[arg(long, conflicts_with = "disabled")]
    enabled: bool,
    /// Disable the job
    #[arg(long, conflicts_with = "enabled")]
    disabled: bool,
    /// Show the normalized diff without writing a revision
    #[arg(long)]
    dry_run: bool,
}
#[derive(Args, Debug)]
struct PreviewArgs {
    /// Job name or canonical UUID to preview; omit with a schedule selector
    value: Option<String>,
    #[command(flatten)]
    schedule: ScheduleArgs,
    /// Number of upcoming occurrences to show
    #[arg(long, default_value_t = 5)]
    count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ValuesMode {
    Redacted,
    Plaintext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportDocument {
    schema: String,
    values_mode: ValuesMode,
    settings: SettingsRecord,
    jobs: Vec<ExportJob>,
    #[serde(default)]
    omitted_values: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportJob {
    id: String,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
    enabled: bool,
    definition: JobDefinition,
    #[serde(default)]
    omitted_values: Vec<String>,
}

#[derive(Debug)]
enum PlannedImportJob {
    Create {
        source_id: String,
        job: CreateJob,
        resolution: ImportResolution,
    },
    Update {
        source_id: String,
        job: UpdateJob,
        resolution: ImportResolution,
    },
    NoOp {
        source_id: String,
        destination_id: String,
        job: UpdateJob,
        resolution: ImportResolution,
    },
}

#[derive(Debug)]
struct ImportPlan {
    settings: SettingsRecord,
    settings_changed: bool,
    jobs: Vec<PlannedImportJob>,
    now_us: i64,
}

#[derive(Debug)]
struct TargetOutcomeError {
    run_id: String,
    state: String,
    reason: Option<String>,
}

impl std::fmt::Display for TargetOutcomeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "target for run {} finished with {}",
            self.run_id, self.state
        )
    }
}

impl StdError for TargetOutcomeError {}

#[tokio::main]
async fn main() {
    let Cli {
        state_dir,
        format,
        json,
        verbose,
        debug,
        version,
        command,
    } = Cli::parse();
    let format = if json { Format::Json } else { format };
    if version {
        print_version(format);
        return;
    }
    let Some(command) = command else {
        missing_subcommand_error().exit();
    };
    init_tracing(verbose, debug);
    let command_name = command_name(&command);
    let streaming = format == Format::Json && command_uses_stream(&command);
    if let Err(error) = execute(state_dir, command, format).await {
        if streaming {
            render_stream_error(command_name, &error);
        } else {
            render_error(format, command_name, &error);
        }
        std::process::exit(exit_code(&error));
    }
}

fn print_version(format: Format) {
    match format {
        Format::Human => println!("locron {}", env!("CARGO_PKG_VERSION")),
        Format::Json => {
            render(
                format,
                "version",
                json!({"version": env!("CARGO_PKG_VERSION")}),
                &[],
            );
        }
    }
}

/// Reproduce clap's native missing-subcommand error for an invocation that
/// supplied arguments but no subcommand. The subcommand field is optional so
/// that `-V/--version` can short-circuit before clap requires one; re-parsing
/// the original arguments with the subcommand requirement restored runs the
/// same validator that produced the error before the flag was made custom.
fn missing_subcommand_error() -> clap::Error {
    Cli::command()
        .subcommand_required(true)
        .try_get_matches_from(std::env::args_os())
        .expect_err("parse with a required subcommand must fail")
}

fn command_uses_stream(command: &Command) -> bool {
    matches!(
        command,
        Command::Run {
            wait: true,
            dry_run: false,
            ..
        } | Command::Logs { follow: true, .. }
    )
}

async fn execute(state_dir: Option<PathBuf>, command: Command, format: Format) -> Result<()> {
    // Service commands tolerate a missing state directory (the registration
    // probe treats it as lock-free), so they run before state discovery.
    if matches!(command, Command::Service { .. }) {
        let Command::Service { command } = command else {
            unreachable!("matched Command::Service")
        };
        return service::execute(state_dir, command, format);
    }
    let paths = StatePaths::discover(state_dir.as_deref())?;
    match command {
        Command::Add(args) => add(&paths, args, format),
        Command::Update(args) => update(&paths, &args, format),
        Command::List { all, no_trunc } => {
            let jobs = open(&paths)?
                .list_jobs(all)?
                .into_iter()
                .map(redacted_job)
                .collect::<Result<Vec<_>>>()?;
            if format == Format::Human {
                // The TIOCGWINSZ size lookup is both the width source and the
                // TTY gate: it fails on a pipe or redirect, so piped output
                // always prints full values. `--no-trunc` restores full values
                // on a terminal. The width is sampled once per invocation;
                // size_checked returns (rows, cols), so the second element is
                // the column count.
                let width = if no_trunc {
                    None
                } else {
                    console::Term::stdout().size_checked().map(|(_, cols)| cols)
                };
                render_list_table(&jobs, width)?;
            } else {
                render(format, "list", json!(jobs), &[]);
            }
            Ok(())
        }
        Command::Show { name } => {
            let job = open(&paths)?.job(&name)?;
            let value = redacted_job(job)?;
            if format == Format::Human {
                render_show(&value)
            } else {
                render(format, "show", value, &[]);
                Ok(())
            }
        }
        Command::Enable { name } => toggle(&paths, &name, true, format),
        Command::Disable { name } => toggle(&paths, &name, false, format),
        Command::Remove { name } => {
            open(&paths)?.remove_job(&name, now_us())?;
            send_wake(&paths);
            if format == Format::Human {
                println!("job removed: {name}");
                Ok(())
            } else {
                render(format, "remove", json!({"name":name,"removed":true}), &[]);
                Ok(())
            }
        }
        Command::Preview(args) => preview(&paths, args, format),
        Command::Run {
            name,
            wait,
            dry_run,
        } => run_job(&paths, &name, wait, dry_run, format).await,
        Command::Cancel {
            run_id,
            acknowledge_unconfirmed,
        } => {
            Uuid::parse_str(&run_id).context("invalid run UUID")?;
            let outcome = open(&paths)?.cancel_with_acknowledgement(
                &run_id,
                now_us(),
                acknowledge_unconfirmed,
            )?;
            send_wake(&paths);
            if format == Format::Human {
                match outcome {
                    CancelOutcome::CancelledBeforeExecution => {
                        println!("cancellation requested: {run_id} (cancelled before execution)")
                    }
                    CancelOutcome::CancellationRequested => {
                        println!("cancellation requested: {run_id}")
                    }
                    CancelOutcome::AcknowledgedUnconfirmed => println!(
                        "cancellation acknowledged: {run_id} (termination unconfirmed; run terminalized as interrupted_unknown)"
                    ),
                }
            } else {
                let data = match outcome {
                    CancelOutcome::CancelledBeforeExecution => {
                        json!({"run_id":run_id,"requested":true,"cancelled":true,"before_execution":true})
                    }
                    CancelOutcome::CancellationRequested => {
                        json!({"run_id":run_id,"requested":true})
                    }
                    CancelOutcome::AcknowledgedUnconfirmed => {
                        json!({"run_id":run_id,"acknowledged_unconfirmed":true,"state":"interrupted_unknown"})
                    }
                };
                render(format, "cancel", data, &[]);
            }
            Ok(())
        }
        Command::History { name, limit } => {
            let store = open(&paths)?;
            let runs = store
                .history(name.as_deref(), limit)?
                .into_iter()
                .map(|run| redacted_observable_run(&store, run))
                .collect::<Result<Vec<_>>>()?;
            if format == Format::Human {
                let names = store
                    .list_jobs(true)?
                    .into_iter()
                    .map(|job| (job.id, job.name))
                    .collect::<BTreeMap<_, _>>();
                render_history_table(&runs, &names)
            } else {
                render(format, "history", json!(runs), &[]);
                Ok(())
            }
        }
        Command::Logs {
            run_id,
            attempt,
            follow,
            channel,
        } => logs(&paths, &run_id, attempt, follow, channel, format).await,
        Command::Why { name, run } => why(&paths, name, run, format),
        Command::Config { command } => config(&paths, command, format),
        Command::Export {
            jobs,
            tag,
            include_values,
            acknowledge_plaintext,
            include_history,
        } => export(
            &paths,
            jobs.as_deref(),
            tag.as_deref(),
            include_values,
            acknowledge_plaintext,
            include_history,
            format,
        ),
        Command::Import {
            path,
            accept_plaintext_values,
            dry_run,
        } => import(&paths, &path, accept_plaintext_values, dry_run, format).await,
        Command::Prune { dry_run } => prune(&paths, dry_run, format),
        Command::Doctor => doctor(&paths, format),
        Command::Daemon {
            command: DaemonCommand::Run,
        } => daemon(paths).await,
        Command::Mcp => mcp::run_mcp_server(paths).await,
        Command::SelfUpdate => {
            let outcome = self_update::update().await?;
            let warnings: Vec<&str> = outcome.warnings.iter().map(String::as_str).collect();
            render(
                format,
                "self-update",
                json!({
                    "current_version": outcome.current_version,
                    "new_version": outcome.new_version,
                    "updated": outcome.updated,
                }),
                &warnings,
            );
            Ok(())
        }
        Command::Service { .. } => {
            unreachable!("service commands run before state discovery")
        }
    }
}

pub(crate) fn open(paths: &StatePaths) -> Result<Store> {
    Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), now_us()).map_err(Into::into)
}
pub(crate) fn open_read_only(paths: &StatePaths) -> Result<Store> {
    if !paths.database.is_file() {
        return Err(anyhow!("state database does not exist"));
    }
    Store::open_read_only(&paths.database).map_err(Into::into)
}

fn add(paths: &StatePaths, args: AddArgs, format: Format) -> Result<()> {
    let global_concurrency = configured_global_concurrency(paths)?;
    let now = now_us();
    let (definition, _) = normalize_definition(
        None,
        &args.schedule,
        &args.target,
        &args.policy,
        global_concurrency,
        now,
    )?;
    validate_metadata(&args.name, args.description.as_deref(), &args.tag)?;
    let warnings = environment_warnings(&definition.environment);
    if args.dry_run {
        if format == Format::Human {
            println!("job added: {} (dry run; no changes made)", args.name);
            render_definition_summary_lines(&redact_definition(serde_json::to_value(
                &definition,
            )?))?;
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
        } else {
            render(
                format,
                "add",
                json!({"dry_run":true,"normalized":{"name":args.name,"enabled":!args.disabled,"definition":redact_definition(serde_json::to_value(&definition)?)},"id":"<non-durable>"}),
                &warnings,
            );
        }
        return Ok(());
    }
    let store = open(paths)?;
    let record = store.create_job(&CreateJob {
        id: JobId::new().to_string(),
        name: args.name,
        description: args.description,
        tags_json: serde_json::to_string(&args.tag)?,
        enabled: !args.disabled,
        definition_json: serde_json::to_string(&definition)?,
        now_us: now,
        cursor_us: now,
    })?;
    send_wake(paths);
    if format == Format::Human {
        println!("job added: {} ({})", record.name, record.id);
        let value = redacted_job(record)?;
        let definition: Value = serde_json::from_str(
            value["definition_json"]
                .as_str()
                .context("job record lacks definition_json")?,
        )?;
        render_definition_summary_lines(&definition)?;
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
    } else {
        render(format, "add", redacted_job(record)?, &warnings);
    }
    Ok(())
}

fn update(paths: &StatePaths, args: &UpdateArgs, format: Format) -> Result<()> {
    let store = if args.dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let current = store.job(&args.name)?;
    let current_definition: JobDefinition = serde_json::from_str(&current.definition_json)?;
    let now = now_us();
    let global_concurrency = u8::try_from(store.settings()?.global_concurrency)
        .context("configured global concurrency is out of range")?;
    let (definition, schedule_changed) = normalize_definition(
        Some(&current_definition),
        &args.schedule,
        &args.target,
        &args.policy,
        global_concurrency,
        now,
    )?;
    let name = args.rename.clone().unwrap_or_else(|| current.name.clone());
    let description = if args.clear_description {
        None
    } else {
        args.description
            .clone()
            .or_else(|| current.description.clone())
    };
    let tags = if args.clear_tags {
        Vec::new()
    } else if args.tag.is_empty() {
        serde_json::from_str::<Vec<String>>(&current.tags_json)?
    } else {
        args.tag.clone()
    };
    let enabled = if args.enabled {
        true
    } else if args.disabled {
        false
    } else {
        current.enabled
    };
    validate_metadata(&name, description.as_deref(), &tags)?;
    let tags_json = serde_json::to_string(&tags)?;
    if name == current.name
        && description == current.description
        && tags_json == current.tags_json
        && enabled == current.enabled
        && definition == current_definition
    {
        return Err(anyhow!("update does not change any field"));
    }
    let before = job_fields(
        &current.name,
        current.description.as_deref(),
        &serde_json::from_str::<Vec<String>>(&current.tags_json)?,
        current.enabled,
        &current_definition,
    )?;
    let after = job_fields(&name, description.as_deref(), &tags, enabled, &definition)?;
    let changed_fields = changed_fields(&before, &after);
    let warnings = environment_warnings(&definition.environment);
    if args.dry_run {
        if format == Format::Human {
            println!("job updated: {} (dry run; no changes made)", current.name);
            let after = redact_definition(after);
            render_definition_summary_lines(
                after
                    .get("definition")
                    .context("dry-run after lacks a definition")?,
            )?;
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
        } else {
            render(
                format,
                "update",
                json!({
                    "dry_run":true,"id":current.id,"revision":current.current_revision+1,
                    "schedule_changed":schedule_changed,
                    "changed_fields":changed_fields,
                    "before":redact_definition(before),
                    "after":redact_definition(after),
                    "cursor_us":if schedule_changed { now } else { current.cursor_us }
                }),
                &warnings,
            );
        }
        return Ok(());
    }
    let record = store.update_job(&UpdateJob {
        id: current.id,
        expected_revision: current.current_revision,
        name,
        description,
        tags_json,
        enabled,
        definition_json: serde_json::to_string(&definition)?,
        now_us: now,
        cursor_us: if schedule_changed {
            now
        } else {
            current.cursor_us
        },
    })?;
    send_wake(paths);
    if format == Format::Human {
        println!(
            "job updated: {} ({}, revision {})",
            record.name, record.id, record.current_revision
        );
        let value = redacted_job(record)?;
        let definition: Value = serde_json::from_str(
            value["definition_json"]
                .as_str()
                .context("job record lacks definition_json")?,
        )?;
        render_definition_summary_lines(&definition)?;
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
    } else {
        render(format, "update", redacted_job(record)?, &warnings);
    }
    Ok(())
}

fn normalize_definition(
    current: Option<&JobDefinition>,
    schedule: &ScheduleArgs,
    target: &TargetArgs,
    policy: &PolicyArgs,
    global_concurrency: u8,
    now_us: i64,
) -> Result<(JobDefinition, bool)> {
    if current.is_none()
        && (target.clear_env
            || !target.unset_env.is_empty()
            || target.no_env_file
            || target.no_path
            || target.clear_body
            || !target.unset_header.is_empty()
            || target.clear_headers
            || target.clear_success_statuses
            || target.no_follow_redirects
            || policy.no_start_deadline
            || policy.no_retry_timeout)
    {
        return Err(anyhow!(
            "clear, unset, and negative selector flags are update-only"
        ));
    }
    let now = Timestamp::from_epoch_micros(now_us);
    let schedule_selectors = usize::from(schedule.cron.is_some())
        + usize::from(schedule.every.is_some())
        + usize::from(schedule.at.is_some());
    if schedule_selectors > 1 {
        return Err(anyhow!(
            "exactly one of --cron, --every, or --at may be supplied"
        ));
    }
    if schedule_selectors == 0 && (schedule.timezone.is_some() || schedule.anchor.is_some()) {
        return Err(anyhow!(
            "--timezone and --anchor require a complete new schedule selector"
        ));
    }
    if schedule.cron.is_some() && schedule.anchor.is_some() {
        return Err(anyhow!("--anchor is valid only with --every"));
    }
    if schedule.every.is_some() && schedule.timezone.is_some() {
        return Err(anyhow!("--timezone is valid only with --cron"));
    }
    if schedule.at.is_some() && (schedule.timezone.is_some() || schedule.anchor.is_some()) {
        return Err(anyhow!("--at does not accept --timezone or --anchor"));
    }
    let normalized_schedule = match (&schedule.cron, &schedule.every, &schedule.at) {
        (Some(expr), None, None) => Schedule::Cron {
            expression: expr.clone(),
            timezone: match schedule.timezone.as_deref().unwrap_or("local") {
                "local" => ScheduleTimeZone::Local,
                name => ScheduleTimeZone::Iana(name.into()),
            },
        },
        (None, Some(duration), None) => Schedule::Every {
            interval: duration.parse()?,
            anchor: match &schedule.anchor {
                Some(value) => value.parse()?,
                None => now,
            },
        },
        (None, None, Some(at)) => Schedule::At { at: at.parse()? },
        (None, None, None) => current
            .map(|definition| definition.schedule.clone())
            .ok_or_else(|| anyhow!("exactly one of --cron, --every, or --at is required"))?,
        _ => unreachable!("selector count was validated"),
    };
    let schedule_changed =
        current.is_none_or(|definition| definition.schedule != normalized_schedule);

    let target_selectors = usize::from(target.shell.is_some())
        + usize::from(target.http.is_some())
        + usize::from(!target.command.is_empty());
    if target_selectors > 1 {
        return Err(anyhow!(
            "exactly one of -- COMMAND, --shell, or --http may be supplied"
        ));
    }
    let mut normalized_target = match (&target.shell, &target.http, target.command.is_empty()) {
        (Some(command), None, true) => Target::Shell {
            command: command.clone(),
            shell: normalize_path(
                target
                    .shell_executable
                    .as_deref()
                    .unwrap_or_else(|| Path::new("/bin/sh")),
            )?,
        },
        (None, Some(parts), true) => Target::Http(HttpTarget {
            method: parse_method(&parts[0])?,
            url: parts[1].clone(),
            headers: BTreeMap::new(),
            body: None,
            body_file: None,
            success_statuses: vec![],
            follow_redirects: false,
        }),
        (None, None, false) => Target::Process {
            executable: target.command[0].clone(),
            args: target.command[1..].to_vec(),
        },
        (None, None, true) => current
            .map(|definition| definition.target.clone())
            .ok_or_else(|| anyhow!("exactly one of -- COMMAND, --shell, or --http is required"))?,
        _ => unreachable!("selector count was validated"),
    };
    if let Some(shell) = &target.shell_executable {
        match &mut normalized_target {
            Target::Shell { shell: value, .. } => *value = normalize_path(shell)?,
            _ => return Err(anyhow!("--shell-executable requires a shell target")),
        }
    }

    let cwd = match &target.cwd {
        Some(path) => normalize_path(path)?,
        None => match current {
            Some(definition) => definition.cwd.clone(),
            None => std::env::current_dir()?,
        },
    };
    if target.cwd.is_some() && matches!(normalized_target, Target::Http(_)) {
        return Err(anyhow!("--cwd requires a process or shell target"));
    }
    if let Target::Process { executable, .. } = &mut normalized_target
        && executable.contains('/')
    {
        *executable = normalize_path_from(&cwd, Path::new(executable))?
            .to_string_lossy()
            .into_owned();
    }
    normalize_http_options(&mut normalized_target, target)?;
    let mut environment = current
        .map(|definition| definition.environment.clone())
        .unwrap_or_default();
    if target.clear_env {
        environment.values.clear();
    }
    for name in &target.unset_env {
        if !is_valid_environment_name(name) || name.starts_with("LOCRON_") {
            return Err(anyhow!("invalid or reserved environment name: {name}"));
        }
        environment.values.remove(name);
    }
    environment.values.extend(target.env.iter().cloned());
    if target.env_file.is_some() && target.no_env_file {
        return Err(anyhow!("--env-file and --no-env-file conflict"));
    }
    if target.no_env_file {
        environment.file = None;
    } else if let Some(path) = &target.env_file {
        environment.file = Some(normalize_path(path)?);
    }
    if target.path.is_some() && target.no_path {
        return Err(anyhow!("--path and --no-path conflict"));
    }
    if target.no_path {
        environment.path = None;
    } else if let Some(path) = &target.path {
        environment.path = Some(normalize_path_list(path)?);
    }

    let mut execution = current
        .map(|definition| definition.policy.clone())
        .unwrap_or_default();
    if let Some(overlap) = policy.overlap {
        let normalized_overlap = match overlap {
            OverlapArg::Skip => OverlapPolicy::Skip,
            OverlapArg::Replace => OverlapPolicy::Replace,
            OverlapArg::Allow => OverlapPolicy::Allow,
        };
        let overlap_changed = normalized_overlap != execution.overlap;
        execution.overlap = normalized_overlap;
        if policy.per_job_concurrency.is_none() && (current.is_none() || overlap_changed) {
            execution.per_job_concurrency = if execution.overlap == OverlapPolicy::Allow {
                2
            } else {
                1
            };
        }
    }
    if let Some(missed) = policy.missed_run {
        execution.missed_run = match missed {
            MissedArg::Skip => MissedRunPolicy::Skip,
            MissedArg::Latest => MissedRunPolicy::Latest,
            MissedArg::All => MissedRunPolicy::All,
        };
    } else if current.is_none() && matches!(normalized_schedule, Schedule::At { .. }) {
        execution.missed_run = MissedRunPolicy::Latest;
    }
    if policy.start_deadline.is_some() && policy.no_start_deadline {
        return Err(anyhow!("--start-deadline and --no-start-deadline conflict"));
    }
    if policy.no_start_deadline {
        execution.start_deadline = None;
    } else if let Some(value) = &policy.start_deadline {
        execution.start_deadline = Some(value.parse()?);
    }
    if let Some(value) = policy.catch_up_limit {
        execution.catch_up_limit = value;
    }
    if let Some(value) = policy.retries {
        execution.retries = value;
    }
    if let Some(value) = policy.backoff {
        execution.backoff = match value {
            BackoffArg::Fixed => BackoffMode::Fixed,
            BackoffArg::Exponential => BackoffMode::Exponential,
        };
    }
    if let Some(value) = &policy.retry_delay {
        execution.retry_delay = value.parse()?;
    }
    if let Some(value) = &policy.retry_cap {
        execution.retry_cap = value.parse()?;
    }
    if policy.timeout.is_some() && policy.no_timeout {
        return Err(anyhow!("--timeout and --no-timeout conflict"));
    }
    if policy.no_timeout {
        execution.timeout = None;
    } else if let Some(value) = &policy.timeout {
        execution.timeout = Some(value.parse()?);
    }
    if policy.retry_timeout && policy.no_retry_timeout {
        return Err(anyhow!("--retry-timeout and --no-retry-timeout conflict"));
    }
    if policy.retry_timeout {
        execution.retry_timeout = true;
    } else if policy.no_retry_timeout {
        execution.retry_timeout = false;
    }
    if let Some(value) = &policy.termination_grace {
        execution.termination_grace = value.parse()?;
    }
    if let Some(value) = policy.per_job_concurrency {
        execution.per_job_concurrency = value;
    }
    let definition = JobDefinition {
        schedule: normalized_schedule,
        target: normalized_target,
        cwd,
        environment,
        policy: execution,
    };
    definition.validate(global_concurrency)?;
    Ok((definition, schedule_changed))
}

fn normalize_http_options(normalized: &mut Target, args: &TargetArgs) -> Result<()> {
    let has_http_options = args.body.is_some()
        || args.body_file.is_some()
        || args.json_body.is_some()
        || args.clear_body
        || !args.header.is_empty()
        || !args.header_env.is_empty()
        || !args.unset_header.is_empty()
        || args.clear_headers
        || !args.success_status.is_empty()
        || args.clear_success_statuses
        || args.follow_redirects
        || args.no_follow_redirects;
    let Target::Http(http) = normalized else {
        if has_http_options {
            return Err(anyhow!("HTTP request options require an HTTP target"));
        }
        return Ok(());
    };
    for (inline_name, _) in &args.header {
        if args
            .header_env
            .iter()
            .any(|(environment_name, _)| environment_name.eq_ignore_ascii_case(inline_name))
        {
            return Err(anyhow!(
                "header {inline_name} cannot use both an inline and environment source"
            ));
        }
    }
    if args.clear_headers {
        http.headers.clear();
    }
    for name in &args.unset_header {
        if !is_valid_http_header_name(name) {
            return Err(anyhow!("invalid HTTP header name: {name}"));
        }
        remove_header(&mut http.headers, name);
    }
    if args.clear_body {
        http.body = None;
        http.body_file = None;
    } else if let Some(body) = &args.body {
        http.body = Some(body.as_bytes().to_vec());
        http.body_file = None;
    } else if let Some(body) = &args.json_body {
        let value: Value = serde_json::from_str(body).context("invalid --json-body value")?;
        http.body = Some(serde_json::to_vec(&value)?);
        http.body_file = None;
        if !has_header(&http.headers, "Content-Type") {
            set_header(
                &mut http.headers,
                "Content-Type",
                HttpHeaderSource::Inline("application/json".into()),
            );
        }
    } else if let Some(path) = &args.body_file {
        http.body_file = Some(normalize_path(path)?);
        http.body = None;
    }
    for (name, value) in &args.header {
        set_header(
            &mut http.headers,
            name,
            HttpHeaderSource::Inline(value.clone()),
        );
    }
    for (name, value) in &args.header_env {
        set_header(
            &mut http.headers,
            name,
            HttpHeaderSource::Environment(value.clone()),
        );
    }
    if args.clear_success_statuses {
        http.success_statuses.clear();
    }
    if !args.success_status.is_empty() {
        http.success_statuses = parse_success_statuses(&args.success_status)?;
    }
    if args.follow_redirects {
        http.follow_redirects = true;
    } else if args.no_follow_redirects {
        http.follow_redirects = false;
    }
    Ok(())
}

fn set_header(
    headers: &mut BTreeMap<String, HttpHeaderSource>,
    name: &str,
    value: HttpHeaderSource,
) {
    remove_header(headers, name);
    headers.insert(name.to_owned(), value);
}

fn remove_header(headers: &mut BTreeMap<String, HttpHeaderSource>, name: &str) {
    if let Some(key) = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&key);
    }
}

fn has_header(headers: &BTreeMap<String, HttpHeaderSource>, name: &str) -> bool {
    headers.keys().any(|key| key.eq_ignore_ascii_case(name))
}

fn parse_success_statuses(values: &[String]) -> Result<Vec<u16>> {
    let mut statuses = std::collections::BTreeSet::new();
    for value in values {
        if let Some((start, end)) = value.split_once('-') {
            let start: u16 = start.parse().context("invalid success status range")?;
            let end: u16 = end.parse().context("invalid success status range")?;
            if start > end || !(100..=599).contains(&start) || !(100..=599).contains(&end) {
                return Err(anyhow!("success status range must be within 100-599"));
            }
            statuses.extend(start..=end);
        } else {
            let status: u16 = value.parse().context("invalid success status")?;
            if !(100..=599).contains(&status) {
                return Err(anyhow!("success status must be within 100-599"));
            }
            statuses.insert(status);
        }
    }
    Ok(statuses.into_iter().collect())
}

fn toggle(paths: &StatePaths, name: &str, enabled: bool, format: Format) -> Result<()> {
    let record = open(paths)?.set_enabled(name, enabled, now_us())?;
    send_wake(paths);
    if format == Format::Human {
        println!(
            "job {}: {}",
            if enabled { "enabled" } else { "disabled" },
            record.name
        );
        Ok(())
    } else {
        render(
            format,
            if enabled { "enable" } else { "disable" },
            redacted_job(record)?,
            &[],
        );
        Ok(())
    }
}

fn preview(paths: &StatePaths, args: PreviewArgs, format: Format) -> Result<()> {
    let (schedule, summary) = if let Some(name) = args.value {
        let job = open(paths)?.job(&name)?;
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let value = redacted_job(job)?;
        let redacted: Value = serde_json::from_str(
            value["definition_json"]
                .as_str()
                .context("job record lacks definition_json")?,
        )?;
        (definition.schedule, Some(list_schedule_summary(&redacted)?))
    } else {
        let schedule = build_schedule_only(&args.schedule)?;
        let summary = schedule_summary(&schedule);
        (schedule, Some(summary))
    };
    let next = schedule.next(Timestamp::from_epoch_micros(now_us()), args.count)?;
    let occurrences = next.iter().map(ToString::to_string).collect::<Vec<_>>();
    if format == Format::Human {
        if let Some(summary) = summary {
            println!("schedule: {summary}");
        }
        for occurrence in &occurrences {
            println!("{occurrence}");
        }
    } else {
        render(format, "preview", json!({"occurrences":occurrences}), &[]);
    }
    Ok(())
}
fn build_schedule_only(args: &ScheduleArgs) -> Result<Schedule> {
    let target = TargetArgs {
        shell: Some(":".into()),
        cwd: Some(std::env::current_dir()?),
        ..TargetArgs::default()
    };
    Ok(
        normalize_definition(None, args, &target, &PolicyArgs::default(), 16, now_us())?
            .0
            .schedule,
    )
}

async fn run_job(
    paths: &StatePaths,
    name: &str,
    wait: bool,
    dry_run: bool,
    format: Format,
) -> Result<()> {
    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let job = store.job(name)?;
    if dry_run {
        let active = store
            .history(Some(name), 100)?
            .into_iter()
            .filter(|run| {
                matches!(
                    run.state.as_str(),
                    "queued" | "starting" | "running" | "retry_wait"
                )
            })
            .count();
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let decision = if active == 0 {
            "eligible"
        } else {
            match definition.policy.overlap {
                OverlapPolicy::Skip => "would_skip_overlap",
                OverlapPolicy::Replace => "would_replace",
                OverlapPolicy::Allow => "eligible_subject_to_capacity",
            }
        };
        if format == Format::Human {
            let decision_text = match decision {
                "eligible" => "run eligible",
                "would_skip_overlap" => "run would skip (overlap policy)",
                "would_replace" => "run would replace",
                _ => "run eligible subject to capacity",
            };
            println!("{decision_text}: {name}");
            println!("dry run: no run created");
        } else {
            render(
                format,
                "run",
                json!({"dry_run":true,"durable":false,"decision":decision,"capacity_reserved":false}),
                &[],
            );
        }
        return Ok(());
    }
    let run_id = Uuid::now_v7().to_string();
    let run = store.enqueue_manual(name, &run_id, now_us())?;
    send_wake(paths);
    let warnings = if daemon_lock_free(paths) {
        vec!["daemon is not running; run remains durably queued"]
    } else {
        vec![]
    };
    if !wait || format == Format::Human {
        if format == Format::Human {
            println!("run queued: {} (job {})", run.id, name);
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
        } else {
            render(
                format,
                "run",
                json!({"run_id":run.id,"state":run.state}),
                &warnings,
            );
        }
    }
    if wait {
        let run = wait_run(paths, &store, &run_id, format).await?;
        if format == Format::Json {
            render_stream_result(
                "run",
                true,
                json!({"run_id":run.id,"state":run.state,"reason":run.reason}),
                &warnings,
                None,
            );
        }
    }
    Ok(())
}

async fn wait_run(
    paths: &StatePaths,
    store: &Store,
    id: &str,
    format: Format,
) -> Result<RunRecord> {
    let mut attempt = 1_u16;
    let mut emitted = 0_usize;
    loop {
        let run = store.run(id)?;
        let finalized = emit_available_attempt_output(
            paths,
            id,
            attempt,
            &mut emitted,
            LogChannel::All,
            format,
            "run",
        )?;
        if !matches!(
            run.state.as_str(),
            "queued" | "starting" | "running" | "retry_wait"
        ) {
            if format == Format::Human {
                println!("run finished: {} ({})", run.id, run.state);
            }
            if run.state != "succeeded" {
                return Err(TargetOutcomeError {
                    run_id: run.id,
                    state: run.state,
                    reason: run.reason,
                }
                .into());
            }
            return Ok(run);
        }
        if finalized {
            attempt = attempt
                .checked_add(1)
                .ok_or_else(|| anyhow!("attempt number exceeds output path range"))?;
            emitted = 0;
        }
        tokio::time::sleep(Duration::from_millis(200)).await
    }
}

async fn logs(
    paths: &StatePaths,
    run_id: &str,
    attempt: Option<u16>,
    follow: bool,
    channel: LogChannel,
    format: Format,
) -> Result<()> {
    let attempt = attempt.unwrap_or(1);
    if follow {
        let mut emitted = 0;
        loop {
            if emit_available_attempt_output(
                paths,
                run_id,
                attempt,
                &mut emitted,
                channel,
                format,
                "logs",
            )? {
                if format == Format::Json {
                    render_stream_result(
                        "logs",
                        true,
                        json!({"run_id":run_id,"attempt":attempt,"state":"finalized"}),
                        &[],
                        None,
                    );
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    let path = paths.final_output(run_id, attempt)?;
    if path.exists() {
        for frame in locron_engine::read_frames(&path)? {
            if channel_selected(channel, frame.channel) {
                if format == Format::Human {
                    print!("{}", String::from_utf8_lossy(&frame.payload))
                } else {
                    render(
                        format,
                        "logs",
                        json!({"channel":format!("{:?}",frame.channel).to_lowercase(),"sequence":frame.sequence,"bytes":base64::engine::general_purpose::STANDARD.encode(&frame.payload),"encoding":"base64"}),
                        &[],
                    )
                }
            }
        }
        return Ok(());
    }
    Err(anyhow!("output not found"))
}

fn emit_available_attempt_output(
    paths: &StatePaths,
    run_id: &str,
    attempt: u16,
    emitted: &mut usize,
    channel: LogChannel,
    format: Format,
    command: &str,
) -> Result<bool> {
    let final_path = paths.final_output(run_id, attempt)?;
    let partial_path = paths.partial_output(run_id, attempt)?;
    let (frames, finalized) = match locron_engine::read_frames(&final_path) {
        Ok(frames) => (frames, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match locron_engine::read_frames(&partial_path) {
                Ok(frames) => (frames, false),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::UnexpectedEof
                    ) =>
                {
                    (Vec::new(), false)
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    };

    if frames.len() < *emitted {
        return Err(anyhow!(
            "attempt output regressed from {} to {} frames",
            *emitted,
            frames.len()
        ));
    }
    for frame in &frames[*emitted..] {
        if channel_selected(channel, frame.channel) {
            emit_output_frame(format, command, run_id, attempt, frame);
        }
    }
    *emitted = frames.len();
    Ok(finalized)
}

fn channel_selected(channel: LogChannel, frame_channel: locron_engine::Channel) -> bool {
    matches!(channel, LogChannel::All)
        || matches!(
            (channel, frame_channel),
            (LogChannel::Stdout, locron_engine::Channel::Stdout)
                | (LogChannel::Stderr, locron_engine::Channel::Stderr)
                | (LogChannel::Body, locron_engine::Channel::Body)
        )
}

fn emit_output_frame(
    format: Format,
    command: &str,
    run_id: &str,
    attempt: u16,
    frame: &locron_engine::Frame,
) {
    if format == Format::Human {
        print!("{}", String::from_utf8_lossy(&frame.payload));
    } else {
        println!(
            "{}",
            json!({
                "schema":"locron.stream/v1",
                "record":"frame",
                "command":command,
                "data":{
                    "run_id":run_id,
                    "attempt":attempt,
                    "channel":format!("{:?}",frame.channel).to_lowercase(),
                    "sequence":frame.sequence,
                    "elapsed_micros":frame.elapsed_micros,
                    "bytes":base64::engine::general_purpose::STANDARD.encode(&frame.payload),
                    "encoding":"base64"
                }
            })
        );
    }
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

fn why(
    paths: &StatePaths,
    name: Option<String>,
    run: Option<String>,
    format: Format,
) -> Result<()> {
    let store = open(paths)?;
    match (name, run) {
        (Some(name), None) => {
            let job = store.job(&name)?;
            let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
            let next = definition
                .schedule
                .next(Timestamp::from_epoch_micros(now_us()), 1)?
                .first()
                .map(ToString::to_string);
            let active = store
                .history(Some(&name), 100)?
                .into_iter()
                .filter(|run| {
                    matches!(
                        run.state.as_str(),
                        "queued" | "starting" | "running" | "retry_wait"
                    )
                })
                .map(redacted_run)
                .collect::<Result<Vec<_>>>()?;
            let job = redacted_job(job)?;
            let daemon_running = !daemon_lock_free(paths);
            if format == Format::Human {
                render_why_job(
                    &job,
                    &definition,
                    next.as_deref(),
                    &active,
                    daemon_running,
                    configured_global_concurrency(paths)?,
                )?;
            } else {
                render(
                    format,
                    "why",
                    json!({"job":job,"next_occurrence":next,"active_runs":active,"overlap":definition.policy.overlap,"daemon_running":daemon_running,"explanation":"facts are read from durable state; unknown execution facts are not inferred"}),
                    &[],
                );
            }
            Ok(())
        }
        (None, Some(id)) => {
            let durable_events = store.events_for_run(&id)?;
            let run = redacted_observable_run(&store, store.run(&id)?)?;
            let daemon_running = !daemon_lock_free(paths);
            if format == Format::Human {
                render_why_run(&run, &durable_events)?;
            } else {
                render(
                    format,
                    "why",
                    json!({"run":run,"events":durable_events,"daemon_running":daemon_running,"explanation":"terminal reason, immutable snapshot, ordered attempts, and audit events are durable facts"}),
                    &[],
                );
            }
            Ok(())
        }
        _ => Err(anyhow!("provide a job name or --run RUN_ID")),
    }
}

fn config(paths: &StatePaths, command: ConfigCommand, format: Format) -> Result<()> {
    match command {
        ConfigCommand::Get { key } => {
            let store = open(paths)?;
            let settings = store.settings()?;
            render_config_get(format, key.as_deref(), &settings)?;
        }
        ConfigCommand::Set {
            key,
            value,
            dry_run,
        } => {
            if let Some(name) = environment_config_name(&key)? {
                validate_environment_value(name, &value)?;
                let before = config_dry_run_settings(paths, dry_run)?;
                let action = if before.environment.contains_key(name) {
                    "replaced"
                } else {
                    "created"
                };
                if !dry_run {
                    open(paths)?.set_environment(name, Some(&value), now_us())?;
                    send_wake(paths);
                }
                render_environment_change(format, &key, action, true, dry_run);
                return Ok(());
            }
            if dry_run {
                validate_config_value(&key, &value)?;
                if format == Format::Human {
                    println!("{key}: would be configured (dry run; no changes made)");
                } else {
                    render(
                        format,
                        "config set",
                        json!({"key":key,"value":value,"dry_run":true}),
                        &[],
                    );
                }
            } else {
                let store = open(paths)?;
                let settings = store.set_setting(&key, &value, now_us())?;
                send_wake(paths);
                if format == Format::Human {
                    println!("{key}: configured");
                } else {
                    render(
                        format,
                        "config set",
                        redacted_settings_value(&settings)?,
                        &[],
                    );
                }
            }
        }
        ConfigCommand::Unset { key, dry_run } => {
            let name = environment_config_name(&key)?
                .ok_or_else(|| anyhow!("only environment.NAME settings can be unset"))?;
            let before = config_dry_run_settings(paths, dry_run)?;
            let action = if before.environment.contains_key(name) {
                "removed"
            } else {
                "unchanged"
            };
            if !dry_run {
                open(paths)?.set_environment(name, None, now_us())?;
                send_wake(paths);
            }
            render_environment_change(format, &key, action, false, dry_run);
        }
    }
    Ok(())
}

fn environment_config_name(key: &str) -> Result<Option<&str>> {
    let Some(name) = key.strip_prefix("environment.") else {
        if key == "environment" {
            return Err(anyhow!("environment requires a named environment.NAME key"));
        }
        return Ok(None);
    };
    if !is_valid_environment_name(name) || name.starts_with("LOCRON_") {
        return Err(anyhow!("invalid or reserved environment name {name}"));
    }
    Ok(Some(name))
}

fn validate_environment_value(name: &str, value: &str) -> Result<()> {
    if value.contains('\0') {
        return Err(anyhow!("environment value for {name} contains NUL"));
    }
    Ok(())
}

fn config_dry_run_settings(paths: &StatePaths, dry_run: bool) -> Result<SettingsRecord> {
    if dry_run && !paths.database.is_file() {
        return Ok(default_settings());
    }
    if dry_run {
        open_read_only(paths)?.settings().map_err(Into::into)
    } else {
        open(paths)?.settings().map_err(Into::into)
    }
}

fn default_settings() -> SettingsRecord {
    SettingsRecord {
        global_concurrency: 16,
        execution_path: "/usr/local/bin:/usr/bin:/bin".into(),
        run_retention_count: 10_000,
        run_retention_age_us: Some(7_776_000_000_000),
        output_limit_bytes: 268_435_456,
        per_run_output_limit_bytes: 10_485_760,
        environment: BTreeMap::new(),
    }
}

fn render_config_get(format: Format, key: Option<&str>, settings: &SettingsRecord) -> Result<()> {
    if let Some(key) = key
        && let Some(name) = environment_config_name(key)?
    {
        let configured = settings.environment.contains_key(name);
        if format == Format::Human {
            println!(
                "{key}: {}",
                if configured {
                    "configured (value redacted)"
                } else {
                    "not configured"
                }
            );
        } else {
            render(
                format,
                "config get",
                json!({"key":key,"configured":configured,"value_redacted":true}),
                &[],
            );
        }
        return Ok(());
    }

    let mut value = redacted_settings_value(settings)?;
    let object = value
        .as_object_mut()
        .expect("settings serialize as an object");
    if let Some(key) = key {
        let value = object
            .get(key)
            .ok_or_else(|| anyhow!("unknown configuration key"))?;
        if format == Format::Human {
            println!("{key}={value}");
        } else {
            render(format, "config get", json!({"key":key,"value":value}), &[]);
        }
    } else if format == Format::Human {
        for (name, value) in object.iter().filter(|(name, _)| *name != "environment") {
            println!("{name}={value}");
        }
        for name in settings.environment.keys() {
            println!("environment.{name}=<redacted>");
        }
    } else {
        render(format, "config get", value, &[]);
    }
    Ok(())
}

fn redacted_settings_value(settings: &SettingsRecord) -> Result<Value> {
    let mut value = serde_json::to_value(settings)?;
    let environment = settings
        .environment
        .keys()
        .map(|name| {
            (
                name.clone(),
                json!({"configured":true,"value_redacted":true}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    value
        .as_object_mut()
        .expect("settings serialize as an object")
        .insert("environment".into(), Value::Object(environment));
    Ok(value)
}

fn render_environment_change(
    format: Format,
    key: &str,
    action: &str,
    configured: bool,
    dry_run: bool,
) {
    if format == Format::Human {
        let state = if configured {
            "configured (value redacted)"
        } else {
            "unset"
        };
        if dry_run {
            println!("{key}: {state} (dry run; no changes made)");
        } else {
            println!("{key}: {state}");
        }
    } else {
        render(
            format,
            if configured {
                "config set"
            } else {
                "config unset"
            },
            json!({"key":key,"action":action,"configured":configured,"value_redacted":true,"dry_run":dry_run}),
            &[],
        );
    }
}

fn validate_config_value(key: &str, value: &str) -> Result<()> {
    match key {
        "global_concurrency" => {
            let parsed: i64 = value
                .parse()
                .context("global_concurrency must be an integer")?;
            if !(1..=64).contains(&parsed) {
                return Err(anyhow!("global_concurrency must be from 1 through 64"));
            }
        }
        "execution_path" => {}
        "run_retention_count" | "output_limit_bytes" | "per_run_output_limit_bytes" => {
            let parsed: i64 = value
                .parse()
                .with_context(|| format!("{key} must be a non-negative integer"))?;
            if parsed < 0 {
                return Err(anyhow!("{key} must be non-negative"));
            }
        }
        _ => return Err(anyhow!("unknown configuration key")),
    }
    Ok(())
}
fn export(
    paths: &StatePaths,
    jobs: Option<&str>,
    tags: Option<&str>,
    include_values: bool,
    acknowledge_plaintext: bool,
    include_history: bool,
    format: Format,
) -> Result<()> {
    if include_history {
        return Err(anyhow!(
            "--include-history is not supported by locron.export/v1"
        ));
    }
    if include_values != acknowledge_plaintext {
        return Err(anyhow!(
            "plaintext export requires both --include-values and --acknowledge-plaintext"
        ));
    }
    let values_mode = if include_values {
        ValuesMode::Plaintext
    } else {
        ValuesMode::Redacted
    };
    // Interactivity is decided once, before any output: three terminals,
    // `CI` unset, human format, and no selector.
    let selectors = ExportSelectors::parse(jobs, tags);
    let interactive = should_show_picker(
        export_tty_state(),
        std::env::var_os("CI").is_some(),
        format,
        !selectors.is_empty(),
    );
    let store = open(paths)?;
    let selected = select_export_jobs(
        store.list_jobs(true)?,
        &selectors,
        interactive,
        picker_for_export().as_ref(),
    )?;
    let jobs = selected
        .into_iter()
        .map(|job| export_job(job, values_mode))
        .collect::<Result<Vec<_>>>()?;
    let mut settings = store.settings()?;
    let omitted_values = if values_mode == ValuesMode::Redacted {
        let omitted = settings
            .environment
            .keys()
            .map(|name| format!("settings.environment.{name}"))
            .collect();
        settings.environment.clear();
        omitted
    } else {
        Vec::new()
    };
    let document = ExportDocument {
        schema: "locron.export/v1".into(),
        values_mode,
        settings,
        jobs,
        omitted_values,
    };
    match format {
        Format::Json => render(format, "export", serde_json::to_value(&document)?, &[]),
        Format::Human => println!("{}", serde_json::to_string_pretty(&document)?),
    }
    Ok(())
}
async fn import(
    paths: &StatePaths,
    path: &Path,
    accept: bool,
    dry_run: bool,
    format: Format,
) -> Result<()> {
    let document = match import_source(path)? {
        ImportSource::Path(source) => parse_import_document(&source, accept)?,
        ImportSource::Url(url) => {
            let bytes = fetch_import_url(&url)
                .await
                .with_context(|| format!("could not fetch import URL {url}"))?;
            parse_import_bytes(&bytes, accept)?
        }
    };
    let now = now_us();
    let store = if dry_run {
        paths
            .database
            .is_file()
            .then(|| open_read_only(paths))
            .transpose()?
    } else {
        Some(open(paths)?)
    };
    let plan = plan_import(store.as_ref(), document, now, dry_run)?;
    let action_lines = import_action_lines(&plan);
    let (planned_created, planned_updated, planned_no_op) = import_plan_counts(&plan);
    let settings_changed = plan.settings_changed;
    if dry_run {
        if format == Format::Human {
            println!(
                "dry run: would create {planned_created}, update {planned_updated}, unchanged {planned_no_op}; no changes made"
            );
            for line in &action_lines {
                println!("{line}");
            }
        } else {
            render(format, "import", import_plan_value(&plan, dry_run), &[]);
        }
        return Ok(());
    }

    let mut mutations = Vec::new();
    let mut no_op = 0;
    for action in plan.jobs {
        match action {
            PlannedImportJob::Create {
                job, resolution, ..
            } => mutations.push(ImportJob::Create { job, resolution }),
            PlannedImportJob::Update {
                job, resolution, ..
            } => mutations.push(ImportJob::Update { job, resolution }),
            PlannedImportJob::NoOp {
                job, resolution, ..
            } => {
                no_op += 1;
                mutations.push(ImportJob::Verify { job, resolution });
            }
        }
    }
    if no_op == mutations.len() && !settings_changed {
        if format == Format::Human {
            println!("created 0, updated 0, unchanged {no_op}");
            for line in &action_lines {
                println!("{line}");
            }
        } else {
            render(
                format,
                "import",
                json!({"created":0,"updated":0,"no_op":no_op,"settings_changed":false}),
                &[],
            );
        }
        return Ok(());
    }
    let summary = store
        .as_ref()
        .expect("non-dry import opens a store")
        .apply_import(&ImportBatch {
            settings: plan.settings,
            jobs: mutations,
            now_us: plan.now_us,
        })?;
    send_wake(paths);
    if format == Format::Human {
        println!(
            "created {}, updated {}, unchanged {no_op}",
            summary.created, summary.updated
        );
        for line in &action_lines {
            println!("{line}");
        }
    } else {
        render(
            format,
            "import",
            json!({"created":summary.created,"updated":summary.updated,"no_op":no_op,"settings_changed":settings_changed}),
            &[],
        );
    }
    Ok(())
}

/// The export selection interface: renders a multi-select on stderr and
/// returns the IDs of the chosen jobs. Standard output is reserved for the
/// export document in every mode, so the picker must never write to it.
trait JobPicker {
    fn pick(&self, jobs: &[JobRecord]) -> Result<Vec<String>>;
}

/// Real dialoguer `MultiSelect` picker with the term target on stderr, every
/// item initially selected, and Enter confirming the selection.
struct DialoguerPicker;

impl JobPicker for DialoguerPicker {
    fn pick(&self, jobs: &[JobRecord]) -> Result<Vec<String>> {
        let items: Vec<String> = jobs.iter().map(picker_item_text).collect();
        let defaults = vec![true; jobs.len()];
        let chosen = dialoguer::MultiSelect::new()
            .with_prompt("Select jobs to export")
            .items(&items)
            .defaults(&defaults)
            .interact()
            .context("export selection failed")?;
        Ok(chosen
            .into_iter()
            .map(|index| jobs[index].id.clone())
            .collect())
    }
}

/// One picker item: the job name plus a human schedule summary.
fn picker_item_text(job: &JobRecord) -> String {
    let summary = serde_json::from_str::<JobDefinition>(&job.definition_json).map_or_else(
        |_| "unknown schedule".to_owned(),
        |definition| schedule_summary(&definition.schedule),
    );
    format!("{} — {}", job.name, summary)
}

/// Deterministic stand-in for the real picker used by contract tests: the
/// `LOCRON_TEST_EXPORT_PICKER` hook's comma-separated value is the confirmed
/// selection. It renders its prompt line on stderr, like the real picker, so
/// tests can assert the interface never touches stdout. An empty value means
/// the user deselected everything (a settings-only export).
struct ScriptedPicker {
    names: String,
}

impl JobPicker for ScriptedPicker {
    fn pick(&self, jobs: &[JobRecord]) -> Result<Vec<String>> {
        let mut wanted: BTreeSet<&str> = self
            .names
            .split(',')
            .filter(|name| !name.is_empty())
            .collect();
        let mut picked = Vec::new();
        for job in jobs {
            if wanted.remove(job.name.as_str()) {
                picked.push(job.id.clone());
            }
        }
        if !wanted.is_empty() {
            return Err(anyhow!(
                "picker selection matched no job: {}",
                wanted.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        eprintln!(
            "Select jobs to export: picked {} of {} jobs",
            picked.len(),
            jobs.len()
        );
        Ok(picked)
    }
}

/// Terminal status of the three standard streams for the interactive export
/// picker decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalState {
    stdin: bool,
    stdout: bool,
    stderr: bool,
}

/// Reports whether stdin, stdout, and stderr are all terminals for the
/// purpose of the interactive export picker. The `LOCRON_TEST_EXPORT_PICKER`
/// test hook substitutes three terminals so contract tests can drive the
/// picker branch of the real binary without a PTY; the hook never bypasses
/// the `CI`, format, or selector terms of the decision.
fn export_tty_state() -> TerminalState {
    if std::env::var_os("LOCRON_TEST_EXPORT_PICKER").is_some() {
        TerminalState {
            stdin: true,
            stdout: true,
            stderr: true,
        }
    } else {
        TerminalState {
            stdin: std::io::stdin().is_terminal(),
            stdout: std::io::stdout().is_terminal(),
            stderr: std::io::stderr().is_terminal(),
        }
    }
}

/// The pure interactivity decision for `export`: the selection interface is
/// shown only when every stream is a terminal, `CI` is unset, the output
/// format is human, and no `--jobs`/`--tag` selector is present.
fn should_show_picker(
    tty: TerminalState,
    ci_set: bool,
    format: Format,
    has_selectors: bool,
) -> bool {
    tty.stdin && tty.stdout && tty.stderr && !ci_set && format == Format::Human && !has_selectors
}

/// Returns the picker implementation for this invocation: the scripted
/// test-hook picker when `LOCRON_TEST_EXPORT_PICKER` is set, otherwise the
/// real dialoguer picker.
fn picker_for_export() -> Box<dyn JobPicker> {
    if let Ok(script) = std::env::var("LOCRON_TEST_EXPORT_PICKER") {
        Box::new(ScriptedPicker { names: script })
    } else {
        Box::new(DialoguerPicker)
    }
}

/// The `--jobs`/`--tag` selection for `export`, split on commas. Values are
/// exact names and exact tags; an empty set means no explicit selection.
struct ExportSelectors {
    names: BTreeSet<String>,
    tags: BTreeSet<String>,
}

impl ExportSelectors {
    fn parse(jobs: Option<&str>, tags: Option<&str>) -> Self {
        Self {
            names: jobs
                .map(|value| value.split(',').map(str::to_owned).collect())
                .unwrap_or_default(),
            tags: tags
                .map(|value| value.split(',').map(str::to_owned).collect())
                .unwrap_or_default(),
        }
    }

    fn is_empty(&self) -> bool {
        self.names.is_empty() && self.tags.is_empty()
    }
}

/// Resolves the export subset from one `list_jobs(true)` snapshot. Without
/// selectors the whole snapshot is exported unless `interactive` shows the
/// picker, whose selection narrows it. With selectors the result is the
/// exact-name/exact-tag union deduplicated by job ID, and any selector value
/// matching no job is a validation error before any output is produced.
fn select_export_jobs(
    jobs: Vec<JobRecord>,
    selectors: &ExportSelectors,
    interactive: bool,
    picker: &dyn JobPicker,
) -> Result<Vec<JobRecord>> {
    if selectors.is_empty() {
        if !interactive {
            return Ok(jobs);
        }
        if jobs.is_empty() {
            // Nothing to select; a settings-only export remains legal.
            return Ok(jobs);
        }
        let picked = picker.pick(&jobs)?;
        return Ok(jobs
            .into_iter()
            .filter(|job| picked.iter().any(|id| id == &job.id))
            .collect());
    }
    let mut selected: Vec<JobRecord> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    // Which selector values have matched at least one job. A tag value must
    // match every job carrying it, so matching is a containment check and the
    // matched set only grows.
    let mut matched_names: BTreeSet<String> = BTreeSet::new();
    let mut matched_tags: BTreeSet<String> = BTreeSet::new();
    for job in jobs {
        let job_tags: Vec<String> = serde_json::from_str(&job.tags_json)?;
        let name_hit = selectors.names.contains(job.name.as_str());
        let mut tag_hit = false;
        for tag in job_tags {
            if selectors.tags.contains(tag.as_str()) {
                tag_hit = true;
                matched_tags.insert(tag);
            }
        }
        if name_hit {
            matched_names.insert(job.name.clone());
        }
        if (name_hit || tag_hit) && seen_ids.insert(job.id.clone()) {
            selected.push(job);
        }
    }
    let mut missing = Vec::new();
    missing.extend(
        selectors
            .names
            .iter()
            .filter(|name| !matched_names.contains(*name))
            .map(|name| format!("--jobs {name}")),
    );
    missing.extend(
        selectors
            .tags
            .iter()
            .filter(|tag| !matched_tags.contains(*tag))
            .map(|tag| format!("--tag {tag}")),
    );
    if !missing.is_empty() {
        return Err(anyhow!(
            "export selection matched no job: {}; selectors are exact-name and exact-tag matches",
            missing.join(", ")
        ));
    }
    Ok(selected)
}

/// Maximum import document size, enforced while streaming the body.
const IMPORT_MAX_BYTES: usize = 16 * 1024 * 1024;
/// Maximum redirects followed while fetching an import URL.
const IMPORT_MAX_REDIRECTS: usize = 10;
/// Total timeout for one import fetch, DNS through final byte.
const IMPORT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Where an `import` argument obtains its document: a local file or a URL.
#[derive(Debug)]
enum ImportSource {
    Path(PathBuf),
    Url(Url),
}

/// Classifies an import argument as a local path or an HTTP(S) URL. A string
/// shaped like `scheme://...` is parsed as a URL; anything else (including a
/// non-UTF-8 path) is treated as a path. Classification is an explicit scheme
/// check, never a `Path::exists` guess.
fn import_source(path: &Path) -> Result<ImportSource> {
    let Some(input) = path.to_str() else {
        return Ok(ImportSource::Path(path.to_owned()));
    };
    if !url_like(input) {
        return Ok(ImportSource::Path(path.to_owned()));
    }
    let url = Url::parse(input).context("invalid import URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ImportFetchError::UnsupportedScheme {
            scheme: url.scheme().to_owned(),
        }
        .into());
    }
    if url.username() != "" || url.password().is_some() {
        return Err(anyhow!(
            "import URL must not contain userinfo (username or password): {input}"
        ));
    }
    Ok(ImportSource::Url(url))
}

/// True when `input` is shaped like an absolute `scheme://...` URL.
fn url_like(input: &str) -> bool {
    let Some((scheme, _)) = input.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Fetches an import document over HTTP(S) with the same reqwest/rustls
/// client configuration the CLI uses elsewhere: mandatory TLS certificate
/// verification, a bounded redirect policy, a total timeout, and a 16 MiB
/// in-memory cap enforced while streaming. The returned bytes feed the same
/// validation path as a local file.
async fn fetch_import_url(url: &Url) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("locron/{} import", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(IMPORT_MAX_REDIRECTS))
        .timeout(IMPORT_FETCH_TIMEOUT)
        .build()
        .map_err(|error| ImportFetchError::Network(error.to_string()))?;
    let response = client.get(url.clone()).send().await.map_err(|error| {
        if error.is_redirect() {
            ImportFetchError::TooManyRedirects
        } else if error.is_timeout() {
            ImportFetchError::TotalTimeout
        } else {
            ImportFetchError::Network(error.to_string())
        }
    })?;
    if !response.status().is_success() {
        return Err(ImportFetchError::HttpStatus {
            status: response.status().as_u16(),
        }
        .into());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ImportFetchError::Network(error.to_string()))?;
        body.extend_from_slice(&chunk);
        if body.len() > IMPORT_MAX_BYTES {
            return Err(ImportFetchError::BodyTooLarge {
                limit: IMPORT_MAX_BYTES,
            }
            .into());
        }
    }
    Ok(body)
}

/// Failures that occur while obtaining a document from an import URL. Every
/// variant maps to exit category 5 (unexpected I/O/protocol failure); a URL
/// with userinfo and document validation failures keep their existing
/// validation categories.
#[derive(Debug)]
enum ImportFetchError {
    UnsupportedScheme { scheme: String },
    Network(String),
    HttpStatus { status: u16 },
    BodyTooLarge { limit: usize },
    TooManyRedirects,
    TotalTimeout,
}

impl std::fmt::Display for ImportFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedScheme { scheme } => write!(
                f,
                "import URL scheme \"{scheme}\" is not supported; use an absolute http:// or https:// URL"
            ),
            Self::Network(detail) => write!(
                f,
                "network or TLS failure: {detail}; check the URL and network, then retry"
            ),
            Self::HttpStatus { status } => write!(
                f,
                "import server returned HTTP {status}; check that the URL serves a locron.export/v1 document, then retry"
            ),
            Self::BodyTooLarge { limit } => write!(
                f,
                "import document exceeds the {limit}-byte limit; export a smaller document and retry"
            ),
            Self::TooManyRedirects => write!(
                f,
                "import URL redirected more than {IMPORT_MAX_REDIRECTS} times; use a direct document URL and retry"
            ),
            Self::TotalTimeout => write!(
                f,
                "import fetch timed out after 30 seconds; check the URL and network, then retry"
            ),
        }
    }
}

impl StdError for ImportFetchError {}

fn export_job(job: JobRecord, mode: ValuesMode) -> Result<ExportJob> {
    let mut definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
    let mut omitted_values = Vec::new();
    if mode == ValuesMode::Redacted {
        for name in definition.environment.values.keys() {
            omitted_values.push(format!("definition.environment.values.{name}"));
        }
        definition.environment.values.clear();
        if let Target::Http(http) = &mut definition.target {
            let inline_headers = http
                .headers
                .iter()
                .filter(|(_, source)| matches!(source, HttpHeaderSource::Inline(_)))
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            for name in inline_headers {
                http.headers.remove(&name);
                omitted_values.push(format!("definition.target.headers.{name}"));
            }
            if http.body.take().is_some() {
                omitted_values.push("definition.target.body".into());
            }
        }
        omitted_values.sort();
    }
    Ok(ExportJob {
        id: job.id,
        name: job.name,
        description: job.description,
        tags: serde_json::from_str(&job.tags_json)?,
        enabled: job.enabled,
        definition,
        omitted_values,
    })
}

fn parse_import_document(path: &Path, accept_plaintext: bool) -> Result<ExportDocument> {
    let bytes = std::fs::read(path).context("cannot read import document")?;
    parse_import_bytes(&bytes, accept_plaintext)
}

fn parse_import_bytes(bytes: &[u8], accept_plaintext: bool) -> Result<ExportDocument> {
    let mut document: ExportDocument =
        serde_json::from_slice(bytes).context("invalid export document")?;
    if document.schema != "locron.export/v1" {
        return Err(anyhow!("unsupported export schema: {}", document.schema));
    }
    validate_import_settings_cli(&document.settings)?;
    match document.values_mode {
        ValuesMode::Plaintext if !accept_plaintext => {
            return Err(anyhow!(
                "plaintext values require --accept-plaintext-values"
            ));
        }
        ValuesMode::Redacted => {
            if !document.omitted_values.is_empty()
                || document
                    .jobs
                    .iter()
                    .any(|job| !job.omitted_values.is_empty())
            {
                return Err(anyhow!(
                    "redacted export contains omitted values and cannot be imported faithfully"
                ));
            }
        }
        ValuesMode::Plaintext => {
            if !document.omitted_values.is_empty()
                || document
                    .jobs
                    .iter()
                    .any(|job| !job.omitted_values.is_empty())
            {
                return Err(anyhow!(
                    "plaintext export must not contain omitted_values entries"
                ));
            }
        }
    }

    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for job in &mut document.jobs {
        let id = Uuid::parse_str(&job.id).context("invalid imported job UUID")?;
        if id.to_string() != job.id {
            return Err(anyhow!(
                "imported job UUID must be lowercase canonical text"
            ));
        }
        validate_metadata(&job.name, job.description.as_deref(), &job.tags)?;
        if !ids.insert(job.id.clone()) {
            return Err(anyhow!("duplicate imported job ID: {}", job.id));
        }
        if !names.insert(job.name.clone()) {
            return Err(anyhow!("duplicate imported job name: {}", job.name));
        }
        normalize_import_definition(&mut job.definition)?;
        job.definition
            .validate(u8::try_from(document.settings.global_concurrency)?)?;
        if document.values_mode == ValuesMode::Redacted
            && definition_contains_inline_values(&job.definition)
        {
            return Err(anyhow!(
                "redacted export unexpectedly contains inline plaintext values"
            ));
        }
    }
    document
        .jobs
        .sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    Ok(document)
}

fn definition_contains_inline_values(definition: &JobDefinition) -> bool {
    if !definition.environment.values.is_empty() {
        return true;
    }
    match &definition.target {
        Target::Http(http) => {
            http.body.is_some()
                || http
                    .headers
                    .values()
                    .any(|source| matches!(source, HttpHeaderSource::Inline(_)))
        }
        Target::Process { .. } | Target::Shell { .. } => false,
    }
}

fn validate_import_settings_cli(settings: &SettingsRecord) -> Result<()> {
    if !(1..=64).contains(&settings.global_concurrency) {
        return Err(anyhow!(
            "import global_concurrency must be from 1 through 64"
        ));
    }
    if settings.run_retention_count < 0
        || settings.run_retention_age_us.is_some_and(|value| value < 0)
        || settings.output_limit_bytes < 0
        || settings.per_run_output_limit_bytes < 0
    {
        return Err(anyhow!(
            "import retention and output limits must be non-negative"
        ));
    }
    if settings.execution_path.contains('\0') {
        return Err(anyhow!("import execution_path contains NUL"));
    }
    for (name, value) in &settings.environment {
        environment_config_name(&format!("environment.{name}"))?;
        validate_environment_value(name, value)?;
    }
    Ok(())
}

fn normalize_import_definition(definition: &mut JobDefinition) -> Result<()> {
    definition.cwd = normalize_path(&definition.cwd)?;
    if let Some(path) = &definition.environment.file {
        definition.environment.file = Some(normalize_path(path)?);
    }
    if let Some(path) = &definition.environment.path {
        definition.environment.path = Some(normalize_path_list(path)?);
    }
    match &mut definition.target {
        Target::Process { executable, .. } if executable.contains('/') => {
            *executable = normalize_path_from(&definition.cwd, Path::new(executable))?
                .to_string_lossy()
                .into_owned();
        }
        Target::Shell { shell, .. } => *shell = normalize_path(shell)?,
        Target::Http(http) => {
            if let Some(path) = &http.body_file {
                http.body_file = Some(normalize_path(path)?);
            }
            http.success_statuses.sort_unstable();
            http.success_statuses.dedup();
        }
        Target::Process { .. } => {}
    }
    Ok(())
}

fn plan_import(
    store: Option<&Store>,
    document: ExportDocument,
    now_us: i64,
    dry_run: bool,
) -> Result<ImportPlan> {
    let identities = store
        .map(Store::job_identities)
        .transpose()?
        .unwrap_or_default();
    let live_jobs = store
        .map(|store| store.list_jobs(true))
        .transpose()?
        .unwrap_or_default();
    let current_settings = store.map(Store::settings).transpose()?;
    let settings_changed = current_settings.as_ref() != Some(&document.settings);
    let live_by_id = live_jobs
        .iter()
        .map(|job| (job.id.as_str(), job))
        .collect::<BTreeMap<_, _>>();
    let live_by_name = live_jobs
        .iter()
        .map(|job| (job.name.as_str(), job))
        .collect::<BTreeMap<_, _>>();
    let mut owned_ids = identities
        .iter()
        .map(|identity| identity.id.clone())
        .collect::<BTreeSet<_>>();
    let mut claimed_destinations = BTreeSet::new();
    let mut jobs = Vec::with_capacity(document.jobs.len());

    for source in document.jobs {
        let destination_by_id = live_by_id.get(source.id.as_str()).copied();
        let destination_by_name = live_by_name.get(source.name.as_str()).copied();
        let resolution = ImportResolution {
            source_id: source.id.clone(),
            source_name: source.name.clone(),
            expected_id_destination: destination_by_id.map(|job| job.id.clone()),
            expected_name_destination: destination_by_name.map(|job| job.id.clone()),
        };
        if let (Some(by_id), Some(by_name)) = (destination_by_id, destination_by_name)
            && by_id.id != by_name.id
        {
            return Err(StoreError::Conflict(format!(
                "source ID {} and name {} resolve to different destination jobs",
                source.id, source.name
            ))
            .into());
        }
        let destination = destination_by_id.or(destination_by_name);
        if let Some(destination) = destination {
            if !claimed_destinations.insert(destination.id.clone()) {
                return Err(StoreError::Conflict(format!(
                    "multiple imported jobs resolve to destination {}",
                    destination.id
                ))
                .into());
            }
            let destination_definition: JobDefinition =
                serde_json::from_str(&destination.definition_json)?;
            let destination_tags: Vec<String> = serde_json::from_str(&destination.tags_json)?;
            if destination.name == source.name
                && destination.description == source.description
                && destination_tags == source.tags
                && destination.enabled == source.enabled
                && destination_definition == source.definition
            {
                jobs.push(PlannedImportJob::NoOp {
                    source_id: source.id,
                    destination_id: destination.id.clone(),
                    job: UpdateJob {
                        id: destination.id.clone(),
                        expected_revision: destination.current_revision,
                        name: source.name,
                        description: source.description,
                        tags_json: serde_json::to_string(&source.tags)?,
                        enabled: source.enabled,
                        definition_json: serde_json::to_string(&source.definition)?,
                        now_us,
                        cursor_us: destination.cursor_us,
                    },
                    resolution,
                });
                continue;
            }
            let schedule_changed = destination_definition.schedule != source.definition.schedule;
            jobs.push(PlannedImportJob::Update {
                source_id: source.id,
                job: UpdateJob {
                    id: destination.id.clone(),
                    expected_revision: destination.current_revision,
                    name: source.name,
                    description: source.description,
                    tags_json: serde_json::to_string(&source.tags)?,
                    enabled: source.enabled,
                    definition_json: serde_json::to_string(&source.definition)?,
                    now_us,
                    cursor_us: if schedule_changed {
                        now_us
                    } else {
                        destination.cursor_us
                    },
                },
                resolution,
            });
        } else {
            let destination_id = if owned_ids.contains(&source.id) {
                if dry_run {
                    format!("<non-durable:{}>", source.id)
                } else {
                    Uuid::now_v7().to_string()
                }
            } else {
                source.id.clone()
            };
            owned_ids.insert(destination_id.clone());
            jobs.push(PlannedImportJob::Create {
                source_id: source.id,
                job: CreateJob {
                    id: destination_id,
                    name: source.name,
                    description: source.description,
                    tags_json: serde_json::to_string(&source.tags)?,
                    enabled: source.enabled,
                    definition_json: serde_json::to_string(&source.definition)?,
                    now_us,
                    cursor_us: now_us,
                },
                resolution,
            });
        }
    }
    Ok(ImportPlan {
        settings: document.settings,
        settings_changed,
        jobs,
        now_us,
    })
}

fn import_plan_value(plan: &ImportPlan, dry_run: bool) -> Value {
    let actions = plan
        .jobs
        .iter()
        .map(|action| match action {
            PlannedImportJob::Create { source_id, job, .. } => json!({
                "action":"create","source_id":source_id,"destination_id":job.id,"name":job.name
            }),
            PlannedImportJob::Update { source_id, job, .. } => json!({
                "action":"update","source_id":source_id,"destination_id":job.id,"name":job.name
            }),
            PlannedImportJob::NoOp {
                source_id,
                destination_id,
                ..
            } => json!({
                "action":"no_op","source_id":source_id,"destination_id":destination_id
            }),
        })
        .collect::<Vec<_>>();
    json!({
        "dry_run":dry_run,
        "settings_changed":plan.settings_changed,
        "actions":actions
    })
}

fn doctor(paths: &StatePaths, format: Format) -> Result<()> {
    let store = open(paths)?;
    let settings = store.settings()?;
    let mut resolutions = Vec::new();
    for job in store.list_jobs(true)? {
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let requested = match &definition.target {
            Target::Process { executable, .. } => executable.clone(),
            Target::Shell { shell, .. } => shell.display().to_string(),
            Target::Http(_) => continue,
        };
        let diagnostic_attempt = AdmitAttempt {
            run_id: Uuid::nil().to_string(),
            job_id: job.id.clone(),
            attempt_number: 1,
            trigger: "diagnostic".into(),
            nominal_us: None,
            snapshot_json: String::new(),
        };
        match engine_target(&definition, &diagnostic_attempt, &settings) {
            Ok(TargetSpec::Process(process)) => resolutions.push(json!({
                "job_id":job.id,
                "job_name":job.name,
                "requested_executable":requested,
                "effective_path":process.env.get("PATH"),
                "resolved_executable":process.executable,
                "status":"resolved"
            })),
            Ok(TargetSpec::Http(_)) => unreachable!("process target diagnostic returned HTTP"),
            Err(error) => resolutions.push(json!({
                "job_id":job.id,
                "job_name":job.name,
                "requested_executable":requested,
                "status":"unresolved",
                "error":error
            })),
        }
    }
    if format == Format::Human {
        render_doctor_human(paths, &settings, &resolutions, &store.integrity_check()?);
    } else {
        render(
            format,
            "doctor",
            json!({
                "state_dir":paths.root,
                "database":paths.database,
                "daemon_running":!daemon_lock_free(paths),
                "wake_socket":paths.wake_socket.exists(),
                "execution_path":settings.execution_path,
                "global_environment_names":settings.environment.keys().collect::<Vec<_>>(),
                "process_resolution":resolutions,
                "checks":store.integrity_check()?
            }),
            &[],
        );
    }
    Ok(())
}

fn prune(paths: &StatePaths, dry_run: bool, format: Format) -> Result<()> {
    if dry_run && !paths.database.is_file() {
        if format == Format::Human {
            println!("dry run: would prune 0 runs, 0 outputs (0 bytes)");
        } else {
            render(
                format,
                "prune",
                json!({"dry_run":true,"candidate_count":0,"bytes":0}),
                &[],
            );
        }
        return Ok(());
    }
    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let settings = store.settings()?;
    let mut retained = store.retained_output_bytes()?;
    let age_cutoff = now_us().saturating_sub(30_i64 * 24 * 60 * 60 * 1_000_000);
    let candidates = store
        .output_retention_candidates(100)?
        .into_iter()
        .filter(|candidate| {
            candidate.finalized_at_us < age_cutoff || retained > settings.output_limit_bytes
        })
        .collect::<Vec<_>>();
    if !dry_run {
        for candidate in &candidates {
            store.mark_output_prune_pending(candidate, now_us())?;
            let path = paths.outputs.join(&candidate.relative_path);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(anyhow!("refusing to prune symbolic-link output"));
                }
                Ok(metadata) if metadata.is_file() => std::fs::remove_file(&path)?,
                Ok(_) => return Err(anyhow!("refusing to prune non-file output")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            store.finish_output_prune(candidate, now_us())?;
            retained = retained.saturating_sub(candidate.physical_bytes);
        }
    }
    let outputs = candidates.len();
    let runs = candidates
        .iter()
        .map(|candidate| candidate.run_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let bytes: i64 = candidates
        .iter()
        .map(|candidate| candidate.physical_bytes)
        .sum();
    if format == Format::Human {
        if dry_run {
            println!("dry run: would prune {runs} runs, {outputs} outputs ({bytes} bytes)");
        } else {
            println!("pruned: {runs} runs, {outputs} outputs ({bytes} bytes)");
        }
    } else {
        render(
            format,
            "prune",
            json!({"dry_run":dry_run,"candidate_count":outputs,"bytes":bytes}),
            &[],
        );
    }
    Ok(())
}

async fn daemon(paths: StatePaths) -> Result<()> {
    let store = Arc::new(Store::open(
        paths.clone(),
        env!("CARGO_PKG_VERSION"),
        now_us(),
    )?);
    let lifetime = SchedulerLifetimeId::new().to_string();
    let global_concurrency = usize::try_from(store.settings()?.global_concurrency)?;
    let adapter = Arc::new(StoreAdapter {
        store,
        lifetime,
        paths: paths.clone(),
        clock: Arc::new(SystemClock::new()),
        timezone: Arc::new(SystemTimeZoneResolver),
        first_reconcile: AtomicBool::new(true),
        last_clock_sample: Mutex::new(None),
        compiled_schedules: Mutex::new(BTreeMap::new()),
        wake: Mutex::new(None),
        wake_task: Mutex::new(None),
        lock: Mutex::new(None),
    });
    let daemon = Daemon::new(
        Arc::clone(&adapter),
        Runner::new(RunnerConfig::default())?,
        DaemonConfig {
            global_concurrency,
            ..DaemonConfig::default()
        },
    )?;
    let wake = daemon.wake_handle();
    *adapter
        .wake
        .lock()
        .map_err(|_| anyhow!("wake mutex poisoned"))? = Some(wake);
    tracing::info!(state_dir = %paths.root.display(), "daemon started");
    daemon.run(CancellationToken::new()).await?;
    Ok(())
}

fn bind_wake_socket(
    paths: &StatePaths,
    wake: Arc<tokio::sync::Notify>,
) -> Result<tokio::task::JoinHandle<()>> {
    if let Ok(metadata) = std::fs::symlink_metadata(&paths.wake_socket) {
        use std::os::unix::fs::FileTypeExt;
        if !metadata.file_type().is_socket() {
            return Err(anyhow!("refusing to replace non-socket wake path"));
        }
        std::fs::remove_file(&paths.wake_socket)?;
    }
    let socket = match tokio::net::UnixDatagram::bind(&paths.wake_socket) {
        Ok(socket) => socket,
        Err(error) => {
            tracing::warn!(%error, "wake socket unavailable; safety reconciliation remains active");
            return Ok(tokio::spawn(async {}));
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&paths.wake_socket, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(tokio::spawn(async move {
        let mut buffer = [0_u8; 64];
        while let Ok(length) = socket.recv(&mut buffer).await {
            if length > 0 {
                wake.notify_one();
            }
        }
    }))
}

pub(crate) fn send_wake(paths: &StatePaths) {
    use std::os::unix::net::UnixDatagram;
    let result = UnixDatagram::unbound().and_then(|socket| {
        socket.connect(&paths.wake_socket)?;
        socket.send(b"locron-wake/v1").map(|_| ())
    });
    if let Err(error) = result {
        tracing::debug!(%error, "wake notification unavailable; command is already durable");
    }
}

struct StoreAdapter {
    store: Arc<Store>,
    lifetime: String,
    paths: StatePaths,
    clock: Arc<dyn Clock>,
    timezone: Arc<dyn TimeZoneResolver>,
    first_reconcile: AtomicBool,
    last_clock_sample: Mutex<Option<(i64, u64)>>,
    compiled_schedules: Mutex<BTreeMap<(String, i64), locron_core::CompiledSchedule>>,
    wake: Mutex<Option<Arc<tokio::sync::Notify>>>,
    wake_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    lock: Mutex<Option<locron_store::DaemonLock>>,
}

impl StoreAdapter {
    fn now_us(&self) -> i64 {
        self.clock.now().epoch_micros()
    }

    async fn fail_admitted_attempt(
        &self,
        attempt: &AdmitAttempt,
        configuration_reason: &str,
    ) -> Result<(), String> {
        let mut reason = format!("runtime configuration failure: {configuration_reason}");
        let output = if let Ok(number) = u16::try_from(attempt.attempt_number) {
            match (
                self.paths.partial_output(&attempt.run_id, number),
                self.paths.final_output(&attempt.run_id, number),
            ) {
                (Ok(partial), Ok(final_path)) => match OutputWriter::create(&partial, 0).await {
                    Ok(writer) => match writer.finalize(&final_path).await {
                        Ok(stats) => Some(OutputRecord {
                            run_id: attempt.run_id.clone(),
                            attempt_number: attempt.attempt_number,
                            relative_path: String::new(),
                            state: "finalized".into(),
                            retained_payload_bytes: i64::try_from(stats.retained_bytes)
                                .unwrap_or(i64::MAX),
                            physical_bytes: i64::try_from(stats.physical_bytes).unwrap_or(i64::MAX),
                            discarded_bytes: i64::try_from(stats.discarded_bytes)
                                .unwrap_or(i64::MAX),
                            truncated: stats.truncated,
                        }),
                        Err(error) => {
                            let _ = write!(reason, "; output finalization also failed: {error}");
                            None
                        }
                    },
                    Err(error) => {
                        let _ = write!(reason, "; output creation also failed: {error}");
                        None
                    }
                },
                (Err(error), _) | (_, Err(error)) => {
                    let _ = write!(reason, "; output path is invalid: {error}");
                    None
                }
            }
        } else {
            reason.push_str("; attempt number cannot be represented in an output path");
            None
        };
        self.store
            .complete_pre_execution_failure(
                &attempt.run_id,
                attempt.attempt_number,
                output.as_ref(),
                self.now_us(),
                &reason,
            )
            .map_err(|error| error.to_string())
    }
}

/// Maps a store completion error onto the typed engine completion error:
/// permanent durable conflicts stay conflicts, everything else is transient.
fn map_completion_error(error: StoreError) -> CompletionError {
    match error {
        StoreError::Conflict(_) => CompletionError::Conflict(error.to_string()),
        other => CompletionError::Transient(other.to_string()),
    }
}

#[async_trait::async_trait]
impl DaemonStore for StoreAdapter {
    async fn begin_lifetime(&self) -> Result<(), String> {
        let metadata = LockMetadata {
            pid: std::process::id(),
            lifetime_id: self.lifetime.clone(),
            started_at_us: self.now_us(),
            binary_version: env!("CARGO_PKG_VERSION").into(),
        };
        let lock = self
            .store
            .acquire_daemon_lock(&metadata)
            .map_err(|error| error.to_string())?;
        *self.lock.lock().map_err(|_| "lock mutex poisoned")? = Some(lock);
        let wake = self
            .wake
            .lock()
            .map_err(|_| "wake mutex poisoned")?
            .clone()
            .ok_or_else(|| "wake handle not configured".to_string())?;
        let task = bind_wake_socket(&self.paths, wake).map_err(|error| error.to_string())?;
        *self
            .wake_task
            .lock()
            .map_err(|_| "wake task mutex poisoned")? = Some(task);
        self.store
            .begin_lifetime(&self.lifetime, self.now_us(), env!("CARGO_PKG_VERSION"))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    async fn reconcile(&self) -> Result<usize, String> {
        let now = self.now_us();
        let monotonic = self.clock.monotonic_micros();
        let clock_discontinuity = self
            .last_clock_sample
            .lock()
            .map_err(|_| "clock sample mutex poisoned")?
            .is_some_and(|(previous_wall, previous_monotonic)| {
                let wall_delta = i128::from(now) - i128::from(previous_wall);
                let monotonic_delta = i128::from(monotonic.saturating_sub(previous_monotonic));
                (wall_delta - monotonic_delta).unsigned_abs() > 1_000_000
            });
        let local_timezone = self
            .timezone
            .local_timezone()
            .map_err(|error| error.to_string())?;
        let startup = self.first_reconcile.load(Ordering::Acquire);
        let mut total = 0;
        let jobs = self.store.list_jobs(false).map_err(|e| e.to_string())?;
        let current_revisions = jobs
            .iter()
            .map(|job| (job.id.clone(), job.current_revision))
            .collect::<BTreeSet<_>>();
        self.compiled_schedules
            .lock()
            .map_err(|_| "compiled schedule cache mutex poisoned")?
            .retain(|key, _| current_revisions.contains(key));
        for job in jobs {
            let definition: JobDefinition =
                serde_json::from_str(&job.definition_json).map_err(|e| e.to_string())?;
            if now <= job.cursor_us {
                continue;
            }
            let elapsed_kind = if startup || clock_discontinuity || job.disabled_since_us.is_some()
            {
                ElapsedKind::Missed
            } else {
                ElapsedKind::Normal
            };
            let reconciliation = {
                let cache_key = (job.id.clone(), job.current_revision);
                let mut cache = self
                    .compiled_schedules
                    .lock()
                    .map_err(|_| "compiled schedule cache mutex poisoned")?;
                cache.retain(|(job_id, revision), _| {
                    job_id != &job.id || *revision == job.current_revision
                });
                let compiled = match cache.entry(cache_key) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                        definition
                            .schedule
                            .compile()
                            .map_err(|error| error.to_string())?,
                    ),
                };
                compiled
                    .reconcile(
                        Timestamp::from_epoch_micros(job.cursor_us),
                        Timestamp::from_epoch_micros(now),
                        definition.policy.missed_run,
                        definition.policy.start_deadline,
                        definition.policy.catch_up_limit,
                        &local_timezone,
                        elapsed_kind,
                    )
                    .map_err(|error| error.to_string())?
            };
            let runs = reconciliation
                .selected
                .into_iter()
                .map(|occurrence| NewScheduledRun {
                    id: Uuid::now_v7().to_string(),
                    job_id: job.id.clone(),
                    revision: job.current_revision,
                    trigger: if occurrence.catch_up {
                        "catch_up".into()
                    } else {
                        "scheduled".into()
                    },
                    nominal_us: occurrence.nominal.epoch_micros(),
                    requested_at_us: now,
                    eligible_at_us: now,
                    snapshot_json: job.definition_json.clone(),
                })
                .collect::<Vec<_>>();
            let summaries = reconciliation
                .omitted
                .into_iter()
                .map(|summary| ReconciliationSummary {
                    kind: match summary.kind {
                        OmittedRangeKind::StartDeadline => "missed_start_deadline",
                        OmittedRangeKind::MissedRunPolicy => "missed_policy_skipped",
                        OmittedRangeKind::CatchUpLimit => "catch_up_omitted",
                    }
                    .into(),
                    count: summary.count,
                    first_nominal_us: summary.first.epoch_micros(),
                    last_nominal_us: summary.last.epoch_micros(),
                })
                .collect::<Vec<_>>();
            total += self
                .store
                .materialize_with_summaries(
                    &job.id,
                    CursorUpdate {
                        expected_revision: job.current_revision,
                        expected_cursor_us: job.cursor_us,
                        new_cursor_us: now,
                        resolve_one_time: matches!(
                            definition.schedule,
                            Schedule::At { at } if at.epoch_micros() <= now
                        ),
                    },
                    &runs,
                    &summaries,
                    now,
                )
                .map_err(|e| e.to_string())?
                .inserted;
        }
        *self
            .last_clock_sample
            .lock()
            .map_err(|_| "clock sample mutex poisoned")? = Some((now, monotonic));
        self.first_reconcile.store(false, Ordering::Release);
        Ok(total)
    }
    async fn maintain(&self) -> Result<(), String> {
        maintenance::maintain(&self.store, &self.paths, &self.lifetime, self.now_us())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    async fn admit(&self, capacity: usize) -> Result<Vec<AdmittedAttempt>, String> {
        let settings = self.store.settings().map_err(|e| e.to_string())?;
        let global_retained = self
            .store
            .retained_output_bytes()
            .map_err(|e| e.to_string())?;
        let attempts = self
            .store
            .admit(&self.lifetime, self.now_us(), capacity)
            .map_err(|e| e.to_string())?
            .attempts;
        let mut runnable = Vec::with_capacity(attempts.len());
        for attempt in attempts {
            let prepared = (|| {
                let definition: JobDefinition =
                    serde_json::from_str(&attempt.snapshot_json).map_err(|e| e.to_string())?;
                let target = engine_target(&definition, &attempt, &settings)?;
                let number = u16::try_from(attempt.attempt_number)
                    .map_err(|_| "attempt number overflow".to_string())?;
                let run_retained = self
                    .store
                    .retained_run_output_bytes(&attempt.run_id)
                    .map_err(|e| e.to_string())?;
                let run_remaining = settings
                    .per_run_output_limit_bytes
                    .saturating_sub(run_retained)
                    .max(0);
                let global_remaining = settings
                    .output_limit_bytes
                    .saturating_sub(global_retained)
                    .max(0);
                Ok::<_, String>(AdmittedAttempt {
                    run_id: attempt.run_id.clone(),
                    target,
                    context: AttemptContext {
                        run_id: attempt.run_id.clone(),
                        attempt: u32::from(number),
                        partial_output: self
                            .store
                            .paths()
                            .partial_output(&attempt.run_id, number)
                            .map_err(|e| e.to_string())?,
                        final_output: self
                            .store
                            .paths()
                            .final_output(&attempt.run_id, number)
                            .map_err(|e| e.to_string())?,
                        output_limit: u64::try_from(run_remaining.min(global_remaining))
                            .unwrap_or(0),
                        timeout: definition
                            .policy
                            .timeout
                            .map(|v| Duration::from_micros(v.get())),
                        cancellation: CancellationToken::new(),
                    },
                })
            })();
            match prepared {
                Ok(admitted) => runnable.push(admitted),
                Err(error) => self.fail_admitted_attempt(&attempt, &error).await?,
            }
        }
        Ok(runnable)
    }
    async fn mark_running(&self, attempt: &AdmittedAttempt) -> Result<bool, String> {
        let resolved_executable = match &attempt.target {
            TargetSpec::Process(process) => Some(Path::new(&process.executable)),
            TargetSpec::Http(_) => None,
        };
        self.store
            .mark_attempt_running_with_executable(
                &attempt.run_id,
                i64::from(attempt.context.attempt),
                self.now_us(),
                resolved_executable,
            )
            .map(|decision| matches!(decision, locron_store::StartDecision::Ready))
            .map_err(|error| error.to_string())
    }
    fn completion_instant_us(&self) -> i64 {
        self.now_us()
    }
    async fn complete(
        &self,
        attempt: &AdmittedAttempt,
        outcome: &locron_engine::runner::ExecutionOutcome,
        completed_at: i64,
    ) -> Result<(), CompletionError> {
        let run = self
            .store
            .run(&attempt.run_id)
            .map_err(|e| CompletionError::Transient(e.to_string()))?;
        let definition: JobDefinition = serde_json::from_str(&run.snapshot_json)
            .map_err(|e| CompletionError::Transient(e.to_string()))?;
        let retry_class = match outcome.kind {
            OutcomeKind::Succeeded => RetryClass::Succeeded,
            OutcomeKind::FailedRetryable => RetryClass::KnownFailure,
            OutcomeKind::TimedOut => RetryClass::Timeout,
            OutcomeKind::Cancelled => RetryClass::Cancelled,
            OutcomeKind::TerminationUnconfirmed => RetryClass::InterruptedUnknown,
            OutcomeKind::Failed => RetryClass::Configuration,
        };
        let retry = decide_retry(
            &definition.policy,
            attempt.context.attempt,
            completed_at,
            retry_class,
        )
        .map(|decision| RetryPlan {
            not_before_us: decision.not_before_us,
            classification: decision.classification.into(),
        });
        let state = match outcome.kind {
            OutcomeKind::Succeeded => "succeeded",
            OutcomeKind::TimedOut => "timed_out",
            OutcomeKind::Cancelled => "cancelled",
            OutcomeKind::TerminationUnconfirmed => "termination_unconfirmed",
            OutcomeKind::Failed | OutcomeKind::FailedRetryable => "failed",
        };
        self.store
            .finalize_output(
                &OutputRecord {
                    run_id: attempt.run_id.clone(),
                    attempt_number: i64::from(attempt.context.attempt),
                    relative_path: String::new(),
                    state: "finalized".into(),
                    retained_payload_bytes: i64::try_from(outcome.output.retained_bytes)
                        .unwrap_or(i64::MAX),
                    physical_bytes: i64::try_from(outcome.output.physical_bytes)
                        .unwrap_or(i64::MAX),
                    discarded_bytes: i64::try_from(outcome.output.discarded_bytes)
                        .unwrap_or(i64::MAX),
                    truncated: outcome.output.truncated,
                },
                completed_at,
            )
            .map_err(map_completion_error)?;
        self.store
            .complete_attempt(&AttemptCompletion {
                run_id: attempt.run_id.clone(),
                attempt_number: i64::from(attempt.context.attempt),
                now_us: completed_at,
                duration_us: i64::try_from(outcome.duration_micros).unwrap_or(i64::MAX),
                state: state.into(),
                exit_code: outcome.exit_code,
                http_status: outcome.http_status,
                http_content_type: outcome.http_content_type.clone(),
                reason: outcome.reason.clone(),
                retry,
            })
            .map_err(map_completion_error)
    }
    async fn complete_runner_failure(
        &self,
        attempt: &AdmittedAttempt,
        kind: locron_engine::runner::RunnerFailureKind,
        reason: &str,
        completed_at: i64,
    ) -> Result<(), CompletionError> {
        self.store
            .complete_runner_failure(
                &attempt.run_id,
                i64::from(attempt.context.attempt),
                completed_at,
                reason,
                matches!(
                    kind,
                    locron_engine::runner::RunnerFailureKind::ExecutionMayHaveStarted
                ),
            )
            .map_err(map_completion_error)
    }
    async fn next_admission_delay(&self) -> Result<Option<Duration>, String> {
        let earliest = self
            .store
            .earliest_pending_eligible_at_us()
            .map_err(|error| error.to_string())?;
        Ok(earliest.map(|earliest| {
            let remaining = earliest.saturating_sub(self.now_us()).max(0);
            Duration::from_micros(u64::try_from(remaining).unwrap_or(0))
        }))
    }
    async fn persistence_degraded(&self, reason: &str) {
        tracing::error!(%reason, "persistence degraded")
    }
    async fn cancellation_requested(&self, run_id: &str) -> Result<bool, String> {
        self.store
            .cancellation_requested(run_id)
            .map_err(|error| error.to_string())
    }
    async fn end_lifetime(&self) -> Result<(), String> {
        self.store
            .end_lifetime(&self.lifetime, self.now_us())
            .map_err(|e| e.to_string())?;
        if let Some(task) = self
            .wake_task
            .lock()
            .map_err(|_| "wake task mutex poisoned")?
            .take()
        {
            task.abort();
        }
        if self.paths.wake_socket.exists() {
            std::fs::remove_file(&self.paths.wake_socket).map_err(|error| error.to_string())?;
        }
        self.lock.lock().map_err(|_| "lock mutex poisoned")?.take();
        Ok(())
    }
}

pub(crate) fn engine_target(
    definition: &JobDefinition,
    attempt: &locron_store::AdmitAttempt,
    settings: &SettingsRecord,
) -> Result<TargetSpec, String> {
    let mut env = minimal_env();
    env.insert("PATH".into(), settings.execution_path.clone());
    for (key, value) in &settings.environment {
        env.insert(key.clone(), value.clone());
    }
    if let Some(path) = &definition.environment.path {
        env.insert("PATH".into(), path.clone());
    }
    if let Some(path) = &definition.environment.file {
        let content =
            std::fs::read_to_string(path).map_err(|error| format!("environment file: {error}"))?;
        for (line_number, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("environment file line {} is malformed", line_number + 1))?;
            if !is_valid_environment_name(key) || key.starts_with("LOCRON_") {
                return Err(format!("invalid or reserved environment name {key}"));
            }
            if value.contains('\0') {
                return Err(format!("environment file value for {key} contains NUL"));
            }
            env.insert(key.to_owned(), value.to_owned());
        }
    }
    for (key, value) in &definition.environment.values {
        env.insert(key.clone(), value.clone());
    }
    env.insert("LOCRON_JOB_ID".into(), attempt.job_id.clone());
    env.insert("LOCRON_RUN_ID".into(), attempt.run_id.clone());
    env.insert("LOCRON_ATTEMPT".into(), attempt.attempt_number.to_string());
    env.insert("LOCRON_TRIGGER".into(), attempt.trigger.clone());
    env.insert(
        "LOCRON_SCHEDULED_AT".into(),
        attempt.nominal_us.map_or_else(String::new, |value| {
            Timestamp::from_epoch_micros(value).to_string()
        }),
    );
    match &definition.target {
        Target::Process { executable, args } => {
            let executable = resolve_attempt_executable(executable, &definition.cwd, &env)?;
            Ok(TargetSpec::Process(ProcessSpec {
                executable,
                args: args.clone(),
                cwd: definition.cwd.clone(),
                env,
            }))
        }
        Target::Shell { command, shell } => {
            let executable = resolve_attempt_executable(
                shell
                    .to_str()
                    .ok_or_else(|| "shell executable path is not valid UTF-8".to_string())?,
                &definition.cwd,
                &env,
            )?;
            Ok(TargetSpec::Process(ProcessSpec {
                executable,
                args: vec!["-c".into(), command.clone()],
                cwd: definition.cwd.clone(),
                env,
            }))
        }
        Target::Http(http) => {
            let headers = http
                .headers
                .iter()
                .map(|(name, source)| {
                    let value = match source {
                        HttpHeaderSource::Inline(value) => value.clone(),
                        HttpHeaderSource::Environment(environment) => {
                            env.get(environment).cloned().ok_or_else(|| {
                                format!("header environment {environment} is missing")
                            })?
                        }
                    };
                    Ok((name.clone(), value))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            Ok(TargetSpec::Http(HttpSpec {
                method: http.method.as_str().into(),
                url: http.url.parse().map_err(|e| format!("{e}"))?,
                headers,
                body: match (&http.body, &http.body_file) {
                    (Some(body), None) => Some(body.clone()),
                    (None, Some(path)) => Some(
                        std::fs::read(path).map_err(|error| format!("HTTP body file: {error}"))?,
                    ),
                    (None, None) => None,
                    (Some(_), Some(_)) => return Err("conflicting HTTP body sources".into()),
                },
                success_statuses: http.success_statuses.clone(),
                follow_redirects: http.follow_redirects,
            }))
        }
    }
}

fn resolve_attempt_executable(
    executable: &str,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<String, String> {
    let path = env.get("PATH").map_or("", String::as_str);
    let absolute_directories = std::env::split_paths(path)
        .map(|directory| {
            if directory.is_absolute() {
                directory
            } else {
                cwd.join(directory)
            }
        })
        .collect::<Vec<_>>();
    let normalized_path = std::env::join_paths(absolute_directories)
        .map_err(|_| "effective PATH cannot be represented".to_string())?
        .into_string()
        .map_err(|_| "effective PATH is not valid UTF-8".to_string())?;
    let resolved = resolve_executable(executable, cwd, Some(&normalized_path))
        .ok_or_else(|| format!("executable not found: {executable}"))?;
    let absolute = if resolved.is_absolute() {
        resolved
    } else {
        cwd.join(resolved)
    };
    absolute
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "resolved executable path is not valid UTF-8".to_string())
}
fn minimal_env() -> BTreeMap<String, String> {
    [
        "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TMPDIR", "PATH",
    ]
    .into_iter()
    .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
    .collect()
}
fn parse_method(value: &str) -> Result<HttpMethod> {
    match value.to_ascii_uppercase().as_str() {
        "GET" => Ok(HttpMethod::Get),
        "POST" => Ok(HttpMethod::Post),
        "PUT" => Ok(HttpMethod::Put),
        "PATCH" => Ok(HttpMethod::Patch),
        "DELETE" => Ok(HttpMethod::Delete),
        "HEAD" => Ok(HttpMethod::Head),
        _ => Err(anyhow!("unsupported HTTP method")),
    }
}
fn parse_key_value(value: &str) -> Result<(String, String), String> {
    value
        .split_once('=')
        .map(|(key, value)| (key.into(), value.into()))
        .ok_or_else(|| "expected KEY=VALUE".into())
}
fn normalize_path(path: &Path) -> Result<PathBuf> {
    let expanded = if let Ok(rest) = path.strip_prefix("~") {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is unavailable"))?;
        PathBuf::from(home).join(rest)
    } else {
        path.to_path_buf()
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(expanded)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn normalize_path_from(base: &Path, path: &Path) -> Result<PathBuf> {
    if path.is_absolute() || path.starts_with("~") {
        normalize_path(path)
    } else {
        normalize_path(&base.join(path))
    }
}

fn normalize_path_list(value: &str) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let paths = std::env::split_paths(value)
        .map(|path| normalize_path(&path))
        .collect::<Result<Vec<_>>>()?;
    std::env::join_paths(paths)?
        .into_string()
        .map_err(|_| anyhow!("normalized PATH is not valid UTF-8"))
}

pub(crate) fn configured_global_concurrency(paths: &StatePaths) -> Result<u8> {
    if !paths.database.is_file() {
        return Ok(16);
    }
    let value = open_read_only(paths)?.settings()?.global_concurrency;
    u8::try_from(value).context("configured global concurrency is out of range")
}

pub(crate) fn validate_metadata(
    name: &str,
    description: Option<&str>,
    tags: &[String],
) -> Result<()> {
    if name.trim().is_empty() || name.contains('\0') {
        return Err(anyhow!("job name must be non-empty and contain no NUL"));
    }
    if tags
        .iter()
        .any(|tag| tag.trim().is_empty() || tag.contains('\0'))
    {
        return Err(anyhow!("tags must be non-empty and contain no NUL"));
    }
    if description.is_some_and(|description| description.contains('\0')) {
        return Err(anyhow!("job description must not contain NUL"));
    }
    Ok(())
}

pub(crate) fn environment_warnings(environment: &Environment) -> Vec<&'static str> {
    let Some(path) = &environment.file else {
        return Vec::new();
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.permissions().mode() & 0o077 != 0
        {
            return vec!["environment file is readable or writable by group/others"];
        }
    }
    Vec::new()
}

fn job_fields(
    name: &str,
    description: Option<&str>,
    tags: &[String],
    enabled: bool,
    definition: &JobDefinition,
) -> Result<Value> {
    Ok(json!({
        "name":name,"description":description,"tags":tags,"enabled":enabled,
        "definition":serde_json::to_value(definition)?
    }))
}

fn changed_fields(before: &Value, after: &Value) -> Vec<String> {
    fn collect(
        path: &str,
        before: Option<&Value>,
        after: Option<&Value>,
        output: &mut Vec<String>,
    ) {
        if before == after {
            return;
        }
        match (
            before.and_then(Value::as_object),
            after.and_then(Value::as_object),
        ) {
            (Some(before), Some(after)) => {
                let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
                for key in keys {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    collect(&child, before.get(key), after.get(key), output);
                }
            }
            _ => output.push(path.to_owned()),
        }
    }
    let mut output = Vec::new();
    collect("", Some(before), Some(after), &mut output);
    output
}
pub(crate) fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| {
            i64::try_from(value.as_micros()).unwrap_or(i64::MAX)
        })
}

struct SystemClock {
    started: Instant,
}

impl SystemClock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_epoch_micros(now_us())
    }

    fn monotonic_micros(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

struct SystemTimeZoneResolver;

impl TimeZoneResolver for SystemTimeZoneResolver {
    fn local_timezone(&self) -> std::result::Result<jiff::tz::TimeZone, CoreError> {
        Ok(jiff::tz::TimeZone::system())
    }
}

pub(crate) fn daemon_lock_free(paths: &StatePaths) -> bool {
    locron_store::DaemonLock::try_prove_free(&paths.daemon_lock).is_ok()
}
fn init_tracing(verbose: u8, debug: bool) {
    let level = if debug {
        "trace"
    } else if verbose > 1 {
        "debug"
    } else if verbose > 0 {
        "info"
    } else {
        "warn"
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(level))
        .with_writer(std::io::stderr)
        .try_init();
}
fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Add(_) => "add",
        Command::Update(_) => "update",
        Command::List { .. } => "list",
        Command::Show { .. } => "show",
        Command::Enable { .. } => "enable",
        Command::Disable { .. } => "disable",
        Command::Remove { .. } => "remove",
        Command::Preview(_) => "preview",
        Command::Run { .. } => "run",
        Command::Cancel { .. } => "cancel",
        Command::History { .. } => "history",
        Command::Logs { .. } => "logs",
        Command::Why { .. } => "why",
        Command::Config { .. } => "config",
        Command::Export { .. } => "export",
        Command::Import { .. } => "import",
        Command::Prune { .. } => "prune",
        Command::Doctor => "doctor",
        Command::Daemon { .. } => "daemon",
        Command::Mcp => "mcp",
        Command::SelfUpdate => "self-update",
        Command::Service { .. } => "service",
    }
}
#[derive(Serialize)]
struct Envelope<'a, T> {
    schema: &'static str,
    ok: bool,
    command: &'a str,
    data: T,
    warnings: &'a [&'a str],
}

pub(crate) fn redacted_job(job: JobRecord) -> Result<Value> {
    let mut value = serde_json::to_value(job)?;
    if let Some(definition) = value.get_mut("definition_json") {
        let source = definition.as_str().unwrap_or("{}");
        *definition = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

pub(crate) fn redacted_run(run: RunRecord) -> Result<Value> {
    let mut value = serde_json::to_value(run)?;
    if let Some(snapshot) = value.get_mut("snapshot_json") {
        let source = snapshot.as_str().unwrap_or("{}");
        *snapshot = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

pub(crate) fn redacted_observable_run(store: &Store, run: RunRecord) -> Result<Value> {
    let source = run.trigger.clone();
    let finished_at_us = run.finished_at_us;
    let outcome = terminal_run_state(&run.state).then(|| run.state.clone());
    let attempts = serde_json::to_value(store.attempts_for_run(&run.id)?)?;
    let actual_started_at_us = attempts.as_array().and_then(|attempts| {
        attempts
            .iter()
            .filter_map(|attempt| attempt["running_at_us"].as_i64())
            .min()
    });
    let duration_us = actual_started_at_us
        .zip(finished_at_us)
        .and_then(|(started, finished)| finished.checked_sub(started))
        .filter(|duration| *duration >= 0);
    let mut value = redacted_run(run)?;
    let object = value
        .as_object_mut()
        .expect("run records serialize as objects");
    object.insert("source".into(), json!(source));
    object.insert("outcome".into(), json!(outcome));
    object.insert("actual_started_at_us".into(), json!(actual_started_at_us));
    object.insert("duration_us".into(), json!(duration_us));
    object.insert("attempts".into(), attempts);
    Ok(value)
}

pub(crate) fn terminal_run_state(state: &str) -> bool {
    matches!(
        state,
        "succeeded"
            | "failed"
            | "timed_out"
            | "cancelled"
            | "skipped_overlap"
            | "skipped_concurrency"
            | "interrupted_unknown"
    )
}

pub(crate) fn redact_definition(mut definition: Value) -> Value {
    if let Some(nested) = definition.get_mut("definition") {
        *nested = redact_definition(nested.take());
        return definition;
    }
    if let Some(values) = definition
        .get_mut("environment")
        .and_then(|environment| environment.get_mut("values"))
        .and_then(Value::as_object_mut)
    {
        for value in values.values_mut() {
            *value = Value::String("<redacted>".into());
        }
    }
    if let Some(headers) = definition
        .get_mut("target")
        .and_then(|target| target.get_mut("headers"))
        .and_then(Value::as_object_mut)
    {
        for value in headers.values_mut() {
            if value.get("source").and_then(Value::as_str) == Some("inline")
                && let Some(inline) = value.get_mut("value")
            {
                *inline = Value::String("<redacted>".into());
            }
        }
    }
    if let Some(body) = definition
        .get_mut("target")
        .and_then(|target| target.get_mut("body"))
        && !body.is_null()
    {
        *body = Value::String("<redacted>".into());
    }
    definition
}

/// Truncates `text` to at most `max_width` display columns, appending the `…`
/// marker (one display column) only when the value actually shrinks; a value
/// that fits is returned unchanged. Column fitting follows character display
/// width (East Asian wide characters count as two columns), never byte or
/// character count. A width too small to hold the marker returns the text
/// unchanged rather than producing an unreadable cell.
fn truncate_display(text: &str, max_width: usize) -> String {
    if text.width() <= max_width || max_width < 1 {
        return text.to_owned();
    }
    let mut truncated = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if used + ch_width > max_width - 1 {
            break;
        }
        truncated.push(ch);
        used += ch_width;
    }
    truncated.push('…');
    truncated
}

/// Renders the docker-style aligned `list` table for human output.
fn render_list_table(jobs: &[Value], width: Option<u16>) -> Result<()> {
    print!("{}", list_table(jobs, width)?);
    Ok(())
}

/// Builds the docker-style aligned `list` table text: a header line (`NAME`,
/// `SCHEDULE`, `TARGET`, `ENABLED`) followed by one left-aligned row per job
/// in the store's name order, with each column padded to the maximum width
/// and columns separated by a single space. The header prints even when no
/// job exists. The header and every value are derived only from the redacted
/// job records (the summaries parse the redacted `definition_json` as JSON
/// values, so no configured environment value, header value, or body can
/// appear).
///
/// When `width` is `Some(terminal width)` and the natural table width
/// (`name_width + 1 + schedule_width + 1 + target_width + 1 + 7` for the
/// unpadded `ENABLED` column) exceeds it, only the `TARGET` column — the
/// table's final data column — shrinks to the remaining width, marking cut
/// values with a trailing `…`. `NAME`, `SCHEDULE`, the header, and `ENABLED`
/// never truncate; a remaining width below one display column prints the
/// table untruncated (rows wrap as before). A `None` width prints the
/// full-value table.
fn list_table(jobs: &[Value], width: Option<u16>) -> Result<String> {
    let mut rows: Vec<(String, String, String, &str)> = Vec::with_capacity(jobs.len());
    for job in jobs {
        let name = job["name"].as_str().context("job record lacks name")?;
        let enabled = if job["enabled"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        let definition: Value = serde_json::from_str(
            job["definition_json"]
                .as_str()
                .context("job record lacks definition_json")?,
        )
        .context("invalid definition_json in job record")?;
        rows.push((
            name.to_owned(),
            list_schedule_summary(&definition)?,
            list_target_summary(&definition)?,
            enabled,
        ));
    }
    // Each column is padded to the maximum cell width (header included); the
    // last column is not padded so rows never carry trailing whitespace.
    let name_width = "NAME"
        .len()
        .max(rows.iter().map(|row| row.0.len()).max().unwrap_or(0));
    let schedule_width = "SCHEDULE"
        .len()
        .max(rows.iter().map(|row| row.1.len()).max().unwrap_or(0));
    let target_width = "TARGET"
        .len()
        .max(rows.iter().map(|row| row.2.len()).max().unwrap_or(0));
    // Fitting is a separate step from padding: when the natural table width
    // (name + 1 + schedule + 1 + target + 1 + 7) exceeds the terminal width,
    // only TARGET absorbs the deficit, leaving it the remaining display
    // columns after NAME, SCHEDULE, the unpadded ENABLED label, and every
    // inter-column space. A budget below one column falls back to the
    // untruncated table, which wraps exactly as it always has.
    let target_budget = width.and_then(|w| {
        let natural = name_width + 1 + schedule_width + 1 + target_width + 1 + 7;
        let budget = (w as usize).saturating_sub(name_width + 1 + schedule_width + 1 + 1 + 7);
        (natural > w as usize && budget >= 1).then_some(budget)
    });
    let target_column = target_budget.unwrap_or(target_width);
    let mut table = String::new();
    writeln!(
        table,
        "{:<name_width$} {:<schedule_width$} {:<target_column$} ENABLED",
        "NAME", "SCHEDULE", "TARGET"
    )
    .unwrap();
    for (name, schedule, target, enabled) in &rows {
        let rendered_target = match target_budget {
            Some(budget) => truncate_display(target, budget),
            None => target.clone(),
        };
        writeln!(
            table,
            "{name:<name_width$} {schedule:<schedule_width$} {rendered_target:<target_column$} {enabled}"
        )
        .unwrap();
    }
    Ok(table)
}

/// Renders a UTC RFC 3339 instant from epoch microseconds, or `unknown` when
/// no instant is recorded.
fn human_instant(epoch_micros: Option<i64>) -> String {
    match epoch_micros {
        Some(micros) => Timestamp::from_epoch_micros(micros).to_string(),
        None => "unknown".into(),
    }
}

/// Abbreviates a canonical UUID to its first 8 characters. The run ID may be
/// abbreviated only in tables; every other human form prints the full ID.
fn abbreviated_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Renders the aligned `history` table (`TIME | JOB | TRIGGER | STATE |
/// DURATION`) for human output: the header always prints, rows are newest
/// first, `TIME` is RFC 3339 UTC of the request instant, `DURATION` renders
/// in the largest whole human unit from request to finish (`-` for active
/// runs), and the run ID may be abbreviated only in this table. The job
/// column prefers the live job name and falls back to the abbreviated job ID
/// for removed jobs. All values derive from the redacted run records.
fn render_history_table(runs: &[Value], names: &BTreeMap<String, String>) -> Result<()> {
    let mut sorted = runs.to_vec();
    sorted.sort_by(|a, b| {
        b["requested_at_us"]
            .as_i64()
            .cmp(&a["requested_at_us"].as_i64())
            .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
    });
    let mut rows: Vec<(String, String, String, String, String)> = Vec::with_capacity(sorted.len());
    for run in &sorted {
        let job_id = run["job_id"].as_str().context("run record lacks job_id")?;
        let job = names
            .get(job_id)
            .cloned()
            .unwrap_or_else(|| abbreviated_id(job_id));
        let duration = match (
            run["requested_at_us"].as_i64(),
            run["finished_at_us"].as_i64(),
        ) {
            (Some(requested), Some(finished)) if finished >= requested => {
                human_duration(u64::try_from(finished - requested).unwrap_or(0))
            }
            _ => "-".to_owned(),
        };
        rows.push((
            human_instant(run["requested_at_us"].as_i64()),
            job,
            run["trigger"].as_str().unwrap_or("unknown").to_owned(),
            run["state"].as_str().unwrap_or("unknown").to_owned(),
            duration,
        ));
    }
    let time_width = "TIME"
        .len()
        .max(rows.iter().map(|row| row.0.len()).max().unwrap_or(0));
    let job_width = "JOB"
        .len()
        .max(rows.iter().map(|row| row.1.len()).max().unwrap_or(0));
    let trigger_width = "TRIGGER"
        .len()
        .max(rows.iter().map(|row| row.2.len()).max().unwrap_or(0));
    let state_width = "STATE"
        .len()
        .max(rows.iter().map(|row| row.3.len()).max().unwrap_or(0));
    println!(
        "{:<time_width$} | {:<job_width$} | {:<trigger_width$} | {:<state_width$} | DURATION",
        "TIME", "JOB", "TRIGGER", "STATE"
    );
    for (time, job, trigger, state, duration) in &rows {
        println!(
            "{time:<time_width$} | {job:<job_width$} | {trigger:<trigger_width$} | {state:<state_width$} | {duration}"
        );
    }
    Ok(())
}

/// Prints the schedule and target summary lines `add` and `update` follow
/// their outcome line with, in the same form the `list` table uses, derived
/// from the redacted definition JSON.
fn render_definition_summary_lines(definition: &Value) -> Result<()> {
    println!("schedule: {}", list_schedule_summary(definition)?);
    println!("target: {}", list_target_summary(definition)?);
    Ok(())
}

/// Joins a job's tags as a comma-separated string, or returns an empty string
/// when the job has no tags.
fn render_tags(job: &Value) -> String {
    serde_json::from_str::<Vec<String>>(job["tags_json"].as_str().unwrap_or("[]"))
        .map(|tags| tags.join(", "))
        .unwrap_or_default()
}

/// Renders the human timezone of a cron schedule from the redacted definition
/// JSON: the IANA name, `local`, or `unknown`.
fn schedule_timezone_summary(schedule: &Value) -> String {
    match schedule
        .get("timezone")
        .and_then(|timezone| timezone.get("mode"))
        .and_then(Value::as_str)
    {
        Some("local") => "local".to_owned(),
        Some("iana") => schedule
            .get("timezone")
            .and_then(|timezone| timezone.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        _ => "unknown".to_owned(),
    }
}

/// Prints the `POLICIES` section lines (overlap, missed run, deadline,
/// retries, timeout, concurrency) from the redacted definition JSON.
fn render_policy_fields(definition: &Value) -> Result<()> {
    let policy = definition
        .get("policy")
        .context("definition lacks policy")?;
    println!(
        "  overlap: {}",
        policy
            .get("overlap")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "  missed run: {}",
        policy
            .get("missed_run")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "  deadline: {}",
        match policy.get("start_deadline").and_then(Value::as_u64) {
            Some(micros) => human_duration(micros),
            None => "none".to_owned(),
        }
    );
    println!(
        "  retries: {}",
        policy.get("retries").and_then(Value::as_u64).unwrap_or(0)
    );
    println!(
        "  timeout: {}",
        match policy.get("timeout").and_then(Value::as_u64) {
            Some(micros) => human_duration(micros),
            None => "none".to_owned(),
        }
    );
    println!(
        "  concurrency: {}",
        policy
            .get("per_job_concurrency")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    Ok(())
}

/// Prints the `SCHEDULE` and `TARGET` labeled sections of `show` from the
/// redacted definition JSON.
fn render_definition_sections(definition: &Value) -> Result<()> {
    let schedule = definition
        .get("schedule")
        .context("definition lacks schedule")?;
    println!("SCHEDULE");
    println!("  schedule: {}", list_schedule_summary(definition)?);
    if schedule.get("kind").and_then(Value::as_str) == Some("cron") {
        println!("  timezone: {}", schedule_timezone_summary(schedule));
    }
    println!("TARGET");
    println!("  target: {}", list_target_summary(definition)?);
    Ok(())
}

/// Renders `show` human output: the labeled sections `JOB`, `SCHEDULE`,
/// `TARGET`, and `POLICIES` with one field per line, derived only from the
/// redacted job record.
fn render_show(job: &Value) -> Result<()> {
    let name = job["name"].as_str().context("job record lacks name")?;
    let id = job["id"].as_str().context("job record lacks id")?;
    let enabled = if job["enabled"].as_bool().unwrap_or(false) {
        "yes"
    } else {
        "no"
    };
    let tags = render_tags(job);
    let revision = job["current_revision"].as_i64().unwrap_or(0);
    println!("JOB");
    println!("  name: {name}");
    println!("  id: {id}");
    println!("  enabled: {enabled}");
    if !tags.is_empty() {
        println!("  tags: {tags}");
    }
    println!("  revision: {revision}");
    let definition: Value = serde_json::from_str(
        job["definition_json"]
            .as_str()
            .context("job record lacks definition_json")?,
    )
    .context("invalid definition_json in job record")?;
    render_definition_sections(&definition)?;
    println!("POLICIES");
    render_policy_fields(&definition)
}

/// Renders `why NAME` human output: labeled sections `JOB`, `SCHEDULE`,
/// `ELIGIBILITY`, `POLICIES`, and `DAEMON` with one field per line. Facts are
/// read from durable state; unknown facts print `unknown`.
fn render_why_job(
    job: &Value,
    definition: &JobDefinition,
    next: Option<&str>,
    active: &[Value],
    daemon_running: bool,
    global_concurrency: u8,
) -> Result<()> {
    let name = job["name"].as_str().context("job record lacks name")?;
    let id = job["id"].as_str().context("job record lacks id")?;
    let enabled = if job["enabled"].as_bool().unwrap_or(false) {
        "yes"
    } else {
        "no"
    };
    let tags = render_tags(job);
    let revision = job["current_revision"].as_i64().unwrap_or(0);
    println!("JOB");
    println!("  name: {name}");
    println!("  id: {id}");
    println!("  enabled: {enabled}");
    if !tags.is_empty() {
        println!("  tags: {tags}");
    }
    println!("  revision: {revision}");
    let cursor = job["cursor_us"].as_i64().filter(|cursor| *cursor > 0);
    println!("SCHEDULE");
    println!("  schedule: {}", schedule_summary(&definition.schedule));
    if let Schedule::Cron { timezone, .. } = &definition.schedule {
        let timezone = match timezone {
            ScheduleTimeZone::Local => "local".to_owned(),
            ScheduleTimeZone::Iana(name) => name.clone(),
        };
        println!("  timezone: {timezone}");
    }
    println!("  cursor: {}", human_instant(cursor));
    println!("  next occurrence: {}", next.unwrap_or("none"));
    let decision = if active.is_empty() {
        "eligible".to_owned()
    } else {
        match definition.policy.overlap {
            OverlapPolicy::Skip => "would skip (overlap policy)".to_owned(),
            OverlapPolicy::Replace => "would replace".to_owned(),
            OverlapPolicy::Allow => "eligible subject to capacity".to_owned(),
        }
    };
    println!("ELIGIBILITY");
    println!("  active runs: {}", active.len());
    println!("  decision: {decision}");
    println!("  global concurrency: {global_concurrency}");
    println!("POLICIES");
    let definition_value: Value = serde_json::from_str(
        job["definition_json"]
            .as_str()
            .context("job record lacks definition_json")?,
    )
    .context("invalid definition_json in job record")?;
    render_policy_fields(&definition_value)?;
    println!("DAEMON");
    println!(
        "  daemon running: {}",
        if daemon_running { "yes" } else { "no" }
    );
    Ok(())
}

/// Renders `why --run RUN_ID` human output: labeled sections `RUN`,
/// `ATTEMPTS`, `EVENTS`, and `TERMINAL REASON` with one field per line.
/// Facts are read from the immutable snapshot and the durable event log;
/// unknown facts print `unknown`.
fn render_why_run(run: &Value, events: &[EventRecord]) -> Result<()> {
    let id = run["id"].as_str().context("run record lacks id")?;
    println!("RUN");
    println!("  run id: {id}");
    println!(
        "  trigger: {}",
        run["trigger"].as_str().unwrap_or("unknown")
    );
    println!(
        "  nominal time: {}",
        run["nominal_us"]
            .as_i64()
            .map_or_else(|| "none".to_owned(), |micros| human_instant(Some(micros)))
    );
    println!(
        "  requested: {}",
        human_instant(run["requested_at_us"].as_i64())
    );
    println!("  state: {}", run["state"].as_str().unwrap_or("unknown"));
    println!(
        "  started: {}",
        human_instant(run["actual_started_at_us"].as_i64())
    );
    println!(
        "  finished: {}",
        human_instant(run["finished_at_us"].as_i64())
    );
    println!(
        "  duration: {}",
        run["duration_us"].as_i64().map_or_else(
            || "unknown".to_owned(),
            |micros| human_duration(u64::try_from(micros.max(0)).unwrap_or(0))
        )
    );
    println!(
        "  outcome: {}",
        run["outcome"].as_str().unwrap_or("unknown")
    );
    println!("ATTEMPTS");
    let attempts = match run["attempts"].as_array() {
        Some(attempts) => attempts.as_slice(),
        None => &[],
    };
    if attempts.is_empty() {
        println!("  (none)");
    }
    for attempt in attempts {
        let number = attempt["attempt_number"].as_i64().unwrap_or(0);
        let state = attempt["state"].as_str().unwrap_or("unknown");
        let duration = attempt["duration_us"]
            .as_i64()
            .map_or_else(String::new, |micros: i64| {
                format!(
                    " ({})",
                    human_duration(u64::try_from(micros.max(0)).unwrap_or(0))
                )
            });
        println!("  attempt {number}: {state}{duration}");
    }
    println!("EVENTS");
    if events.is_empty() {
        println!("  (none)");
    }
    for event in events {
        println!(
            "  {} {}",
            Timestamp::from_epoch_micros(event.occurred_at_us),
            event.kind
        );
    }
    println!("TERMINAL REASON");
    match run["reason"].as_str() {
        Some(reason) if !reason.is_empty() => println!("  reason: {reason}"),
        _ => println!("  reason: unknown"),
    }
    Ok(())
}

/// Renders `doctor` human output: one line per check with an `ok`, `warn`, or
/// `fail` level prefix carrying the check name and the fact or path verified.
fn render_doctor_human(
    paths: &StatePaths,
    settings: &SettingsRecord,
    resolutions: &[Value],
    checks: &[String],
) {
    println!("ok   state dir: {}", paths.root.display());
    println!("ok   database: {}", paths.database.display());
    if daemon_lock_free(paths) {
        println!("warn daemon: not running");
    } else {
        println!("ok   daemon: running");
    }
    if paths.wake_socket.exists() {
        println!("ok   wake socket: {}", paths.wake_socket.display());
    } else {
        println!(
            "warn wake socket: missing ({})",
            paths.wake_socket.display()
        );
    }
    println!("ok   execution path: {}", settings.execution_path);
    for name in settings.environment.keys() {
        println!("ok   environment.{name}: configured (value redacted)");
    }
    for resolution in resolutions {
        let job_name = resolution["job_name"].as_str().unwrap_or("unknown");
        if resolution["status"].as_str() == Some("resolved") {
            let executable = resolution["resolved_executable"]
                .as_str()
                .unwrap_or("unknown");
            println!("ok   process resolution: {job_name} -> {executable}");
        } else {
            let error = resolution["error"].as_str().unwrap_or("unknown error");
            println!("fail process resolution: {job_name} ({error})");
        }
    }
    for check in checks {
        match check.split_once(':') {
            Some(("integrity", " ok")) => println!("ok   integrity: database integrity verified"),
            Some(("integrity", detail)) => println!("fail integrity:{detail}"),
            Some(("foreign_key_violations", " 0")) => println!("ok   foreign key violations: 0"),
            Some(("foreign_key_violations", detail)) => {
                println!("fail foreign key violations:{detail}")
            }
            _ => println!("fail {check}"),
        }
    }
}

/// Counts the planned create/update/no-op actions of an import plan.
fn import_plan_counts(plan: &ImportPlan) -> (usize, usize, usize) {
    let mut created = 0;
    let mut updated = 0;
    let mut unchanged = 0;
    for action in &plan.jobs {
        match action {
            PlannedImportJob::Create { .. } => created += 1,
            PlannedImportJob::Update { .. } => updated += 1,
            PlannedImportJob::NoOp { .. } => unchanged += 1,
        }
    }
    (created, updated, unchanged)
}

/// One human action line per planned import action: `created: NAME (ID)`,
/// `updated: NAME (ID)`, or `unchanged: NAME (ID)`.
fn import_action_lines(plan: &ImportPlan) -> Vec<String> {
    plan.jobs
        .iter()
        .map(|action| match action {
            PlannedImportJob::Create { job, .. } => {
                format!("created: {} ({})", job.name, job.id)
            }
            PlannedImportJob::Update { job, .. } => {
                format!("updated: {} ({})", job.name, job.id)
            }
            PlannedImportJob::NoOp { job, .. } => format!("unchanged: {} ({})", job.name, job.id),
        })
        .collect()
}

/// Renders the human schedule summary (`cron 'EXPR'`, `every DUR`, or
/// `at RFC3339`) from a typed schedule.
fn schedule_summary(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Cron { expression, .. } => format!("cron '{expression}'"),
        Schedule::Every { interval, .. } => format!("every {}", human_duration(interval.get())),
        Schedule::At { at } => format!("at {at}"),
    }
}

/// Renders the human schedule summary (`cron 'EXPR'`, `every DUR`, or
/// `at RFC3339`) from the redacted definition JSON.
fn list_schedule_summary(definition: &Value) -> Result<String> {
    let schedule = definition
        .get("schedule")
        .context("definition lacks schedule")?;
    Ok(match schedule.get("kind").and_then(Value::as_str) {
        Some("cron") => format!(
            "cron '{}'",
            schedule
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
        Some("every") => format!(
            "every {}",
            human_duration(
                schedule
                    .get("interval")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            )
        ),
        Some("at") => {
            let at = schedule
                .get("at")
                .and_then(Value::as_i64)
                .context("at schedule lacks its instant")?;
            format!("at {}", Timestamp::from_epoch_micros(at))
        }
        _ => return Err(anyhow!("unknown schedule kind in definition")),
    })
}

/// Renders the human target summary (`run EXE [ARGS...]`, `shell CMD`, or
/// `http METHOD URL`) from the redacted definition JSON.
fn list_target_summary(definition: &Value) -> Result<String> {
    let target = definition
        .get("target")
        .context("definition lacks target")?;
    Ok(match target.get("kind").and_then(Value::as_str) {
        Some("process") => {
            let executable = target
                .get("executable")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut summary = format!("run {executable}");
            if let Some(args) = target.get("args").and_then(Value::as_array) {
                for arg in args.iter().filter_map(Value::as_str) {
                    summary.push(' ');
                    summary.push_str(arg);
                }
            }
            summary
        }
        Some("shell") => format!(
            "shell {}",
            target.get("command").and_then(Value::as_str).unwrap_or("")
        ),
        Some("http") => format!(
            "http {} {}",
            target.get("method").and_then(Value::as_str).unwrap_or(""),
            target.get("url").and_then(Value::as_str).unwrap_or("")
        ),
        _ => return Err(anyhow!("unknown target kind in definition")),
    })
}

/// Renders a duration in the CLI's input grammar: the largest whole unit
/// (`s`, `m`, `h`, or `d`) that divides the value, or the raw microsecond
/// count as a defensive fallback for sub-second values (which the input
/// grammar can never produce).
fn human_duration(micros: u64) -> String {
    const US_PER_SECOND: u64 = 1_000_000;
    if !micros.is_multiple_of(US_PER_SECOND) {
        return format!("{micros}us");
    }
    let seconds = micros / US_PER_SECOND;
    if seconds.is_multiple_of(86_400) {
        format!("{}d", seconds / 86_400)
    } else if seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn render(format: Format, command: &str, data: Value, warnings: &[&str]) {
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string(&Envelope {
                schema: "locron.cli/v1",
                ok: true,
                command,
                data,
                warnings
            })
            .expect("JSON rendering")
        ),
        Format::Human => {
            println!(
                "{}",
                serde_json::to_string_pretty(&data).expect("JSON rendering")
            );
            for warning in warnings {
                eprintln!("warning: {warning}")
            }
        }
    }
}
fn render_error(format: Format, command: &str, error: &anyhow::Error) {
    if format == Format::Json {
        println!(
            "{}",
            json!({"schema":"locron.cli/v1","ok":false,"command":command,"error":{"code":error_code(error),"message":error.to_string()},"warnings":[]})
        )
    } else {
        eprintln!("error: {error:#}")
    }
}
fn render_stream_result(
    command: &str,
    ok: bool,
    data: Value,
    warnings: &[&str],
    error: Option<Value>,
) {
    let mut record = json!({
        "schema":"locron.stream/v1",
        "record":"result",
        "terminal":true,
        "ok":ok,
        "command":command,
        "data":null,
        "warnings":warnings,
    });
    record["data"] = data;
    if let Some(error) = error {
        record["error"] = error;
    }
    println!("{record}");
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
fn render_stream_error(command: &str, error: &anyhow::Error) {
    let data = error
        .downcast_ref::<TargetOutcomeError>()
        .map_or(Value::Null, |target| {
            json!({
                "run_id":target.run_id,
                "state":target.state,
                "reason":target.reason,
            })
        });
    render_stream_result(
        command,
        false,
        data,
        &[],
        Some(json!({"code":error_code(error),"message":error.to_string()})),
    );
}
fn error_code(error: &anyhow::Error) -> &'static str {
    if error.downcast_ref::<TargetOutcomeError>().is_some() {
        "target_outcome"
    } else if let Some(update) = error.downcast_ref::<SelfUpdateError>() {
        match update {
            SelfUpdateError::UnsupportedPlatform { .. } => "update_unsupported_platform",
            SelfUpdateError::ManagedInstall => "update_managed_install",
            SelfUpdateError::RateLimited => "update_rate_limited",
            SelfUpdateError::Network(_) => "update_network",
            SelfUpdateError::ReleaseMetadata(_) => "update_release_metadata",
            SelfUpdateError::ChecksumMismatch { .. } => "update_checksum_mismatch",
            SelfUpdateError::Io(_) => "update_io",
        }
    } else if let Some(service) = error.downcast_ref::<ServiceError>() {
        match service {
            ServiceError::UnsupportedPlatform { .. } => "service_unsupported_platform",
            ServiceError::ManagedInstall => "service_managed_install",
            ServiceError::CommandFailed { .. } => "service_command_failed",
            ServiceError::Io(_) => "service_io",
        }
    } else if let Some(fetch) = error.downcast_ref::<ImportFetchError>() {
        match fetch {
            ImportFetchError::UnsupportedScheme { .. } => "import_unsupported_scheme",
            ImportFetchError::Network(_) => "import_fetch",
            ImportFetchError::HttpStatus { .. } => "import_http_status",
            ImportFetchError::BodyTooLarge { .. } => "import_body_too_large",
            ImportFetchError::TooManyRedirects => "import_too_many_redirects",
            ImportFetchError::TotalTimeout => "import_timeout",
        }
    } else if let Some(store) = error.downcast_ref::<StoreError>() {
        match store {
            StoreError::NotFound(_) => "not_found",
            StoreError::Conflict(_) => "durable_conflict",
            StoreError::DaemonAlreadyRunning => "daemon_already_running",
            StoreError::MigrationRequiresDaemonRestart => "migration_requires_restart",
            StoreError::SchemaTooNew { .. } => "schema_too_new",
            _ => "state_error",
        }
    } else {
        "invalid_request"
    }
}
fn exit_code(error: &anyhow::Error) -> i32 {
    if error.downcast_ref::<TargetOutcomeError>().is_some() {
        1
    } else if let Some(update) = error.downcast_ref::<SelfUpdateError>() {
        match update {
            SelfUpdateError::UnsupportedPlatform { .. } => 2,
            SelfUpdateError::ManagedInstall => 3,
            SelfUpdateError::RateLimited
            | SelfUpdateError::Network(_)
            | SelfUpdateError::ReleaseMetadata(_)
            | SelfUpdateError::ChecksumMismatch { .. }
            | SelfUpdateError::Io(_) => 5,
        }
    } else if let Some(service) = error.downcast_ref::<ServiceError>() {
        match service {
            ServiceError::UnsupportedPlatform { .. } => 2,
            ServiceError::ManagedInstall => 3,
            ServiceError::CommandFailed { .. } | ServiceError::Io(_) => 5,
        }
    } else if let Some(fetch) = error.downcast_ref::<ImportFetchError>() {
        match fetch {
            ImportFetchError::UnsupportedScheme { .. }
            | ImportFetchError::Network(_)
            | ImportFetchError::HttpStatus { .. }
            | ImportFetchError::BodyTooLarge { .. }
            | ImportFetchError::TooManyRedirects
            | ImportFetchError::TotalTimeout => 5,
        }
    } else if let Some(store) = error.downcast_ref::<StoreError>() {
        match store {
            StoreError::NotFound(_) | StoreError::Conflict(_) => 3,
            StoreError::DaemonAlreadyRunning
            | StoreError::MigrationRequiresDaemonRestart
            | StoreError::SchemaTooNew { .. } => 4,
            _ => 5,
        }
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::sync::atomic::{AtomicI64, AtomicU64};

    #[test]
    fn every_argument_of_every_command_has_a_description() {
        assert_command_descriptions(&Cli::command());
    }

    fn assert_command_descriptions(command: &clap::Command) {
        for argument in command.get_arguments() {
            let id = argument.get_id().as_str();
            if id == "help" || id == "version" {
                continue;
            }
            let help = argument
                .get_help()
                .map(|text| text.to_string().trim().to_string())
                .unwrap_or_default();
            assert!(
                !help.is_empty(),
                "argument {id} of command {} has no help text",
                command.get_name()
            );
        }
        for subcommand in command.get_subcommands() {
            if subcommand.get_name() != "help" {
                assert_command_descriptions(subcommand);
            }
        }
    }

    struct FakeClock {
        wall: AtomicI64,
        monotonic: AtomicU64,
    }

    impl FakeClock {
        fn new(wall: i64, monotonic: u64) -> Self {
            Self {
                wall: AtomicI64::new(wall),
                monotonic: AtomicU64::new(monotonic),
            }
        }

        fn set(&self, wall: i64, monotonic: u64) {
            self.wall.store(wall, Ordering::Release);
            self.monotonic.store(monotonic, Ordering::Release);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Timestamp {
            Timestamp::from_epoch_micros(self.wall.load(Ordering::Acquire))
        }

        fn monotonic_micros(&self) -> u64 {
            self.monotonic.load(Ordering::Acquire)
        }
    }

    struct FakeTimeZoneResolver {
        name: Mutex<String>,
    }

    impl FakeTimeZoneResolver {
        fn new(name: &str) -> Self {
            Self {
                name: Mutex::new(name.into()),
            }
        }

        fn set(&self, name: &str) {
            *self.name.lock().unwrap() = name.into();
        }
    }

    impl TimeZoneResolver for FakeTimeZoneResolver {
        fn local_timezone(&self) -> std::result::Result<jiff::tz::TimeZone, CoreError> {
            jiff::tz::TimeZone::get(self.name.lock().unwrap().as_str())
                .map_err(|error| CoreError::Unavailable(error.to_string()))
        }
    }

    fn test_adapter(
        store: Arc<Store>,
        paths: StatePaths,
        clock: Arc<dyn Clock>,
        timezone: Arc<dyn TimeZoneResolver>,
        first_reconcile: bool,
    ) -> StoreAdapter {
        StoreAdapter {
            store,
            lifetime: SchedulerLifetimeId::new().to_string(),
            paths,
            clock,
            timezone,
            first_reconcile: AtomicBool::new(first_reconcile),
            last_clock_sample: Mutex::new(None),
            compiled_schedules: Mutex::new(BTreeMap::new()),
            wake: Mutex::new(None),
            wake_task: Mutex::new(None),
            lock: Mutex::new(None),
        }
    }

    #[test]
    fn interval_catch_up_keeps_newest_bounded_window() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let occurrences = schedule
            .reconcile(
                Timestamp::from_epoch_micros(0),
                Timestamp::from_epoch_micros(10_000_000),
                MissedRunPolicy::All,
                None,
                3,
                &jiff::tz::TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap()
            .selected;
        assert_eq!(
            occurrences
                .iter()
                .map(|item| item.nominal.epoch_micros())
                .collect::<Vec<_>>(),
            [8_000_000, 9_000_000, 10_000_000]
        );
    }

    #[tokio::test]
    async fn catch_up_limit_one_thousand_materializes_compactly_and_admits_oldest_first() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Arc::new(Store::open(paths.clone(), "test", 0).unwrap());
        let job_id = JobId::new().to_string();
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: "1s".parse().unwrap(),
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Process {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
            },
            cwd: PathBuf::from("/tmp"),
            environment: Environment::default(),
            policy: locron_core::policy::ExecutionPolicy {
                missed_run: MissedRunPolicy::All,
                catch_up_limit: 1_000,
                ..Default::default()
            },
        };
        store
            .create_job(&CreateJob {
                id: job_id.clone(),
                name: "thousand".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: serde_json::to_string(&definition).unwrap(),
                now_us: 0,
                cursor_us: 0,
            })
            .unwrap();
        let now = 2_000_000_000;
        let clock = Arc::new(FakeClock::new(now, u64::try_from(now).unwrap()));
        let adapter = test_adapter(
            Arc::clone(&store),
            paths,
            clock,
            Arc::new(FakeTimeZoneResolver::new("UTC")),
            true,
        );

        assert_eq!(adapter.reconcile().await.unwrap(), 1_000);
        let history = store.history(Some("thousand"), 1_000).unwrap();
        assert_eq!(history.len(), 1_000);
        assert_eq!(history.last().unwrap().nominal_us, Some(1_001_000_000));
        assert_eq!(history.first().unwrap().nominal_us, Some(2_000_000_000));
        let omitted = store
            .events_for_job(&job_id)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "catch_up_omitted")
            .collect::<Vec<_>>();
        assert_eq!(omitted.len(), 1);
        let details: Value = serde_json::from_str(&omitted[0].details_json).unwrap();
        assert_eq!(details["count"], 1_000);
        assert_eq!(details["first_nominal_us"], 1_000_000);
        assert_eq!(details["last_nominal_us"], 1_000_000_000);
        assert_eq!(adapter.reconcile().await.unwrap(), 0);
        assert_eq!(store.history(Some("thousand"), 1_000).unwrap().len(), 1_000);
        assert_eq!(
            store
                .events_for_job(&job_id)
                .unwrap()
                .into_iter()
                .filter(|event| event.kind == "catch_up_omitted")
                .count(),
            1
        );

        let lifetime = SchedulerLifetimeId::new().to_string();
        store.begin_lifetime(&lifetime, now, "test").unwrap();
        for expected in 1_001_i64..=1_003 {
            let admission = store.admit(&lifetime, now, 64).unwrap();
            assert_eq!(admission.attempts.len(), 1);
            let attempt = &admission.attempts[0];
            assert_eq!(attempt.nominal_us, Some(expected * 1_000_000));
            assert_eq!(
                store
                    .mark_attempt_running(&attempt.run_id, attempt.attempt_number, now)
                    .unwrap(),
                locron_store::StartDecision::Ready
            );
            store
                .complete_attempt(&AttemptCompletion {
                    run_id: attempt.run_id.clone(),
                    attempt_number: attempt.attempt_number,
                    now_us: now,
                    duration_us: 0,
                    state: "succeeded".into(),
                    exit_code: Some(0),
                    http_status: None,
                    http_content_type: None,
                    reason: "test completion".into(),
                    retry: None,
                })
                .unwrap();
        }
        assert_eq!(
            store
                .history(Some("thousand"), 1_000)
                .unwrap()
                .into_iter()
                .filter(|run| run.state == "queued")
                .count(),
            997
        );
    }

    #[test]
    fn sparse_cron_does_not_require_full_preview_count() {
        let schedule = Schedule::Cron {
            expression: "0 0 1 1 *".into(),
            timezone: ScheduleTimeZone::Iana("UTC".into()),
        };
        let cursor: Timestamp = "2025-06-01T00:00:00Z".parse().unwrap();
        let now: Timestamp = "2026-06-01T00:00:00Z".parse().unwrap();
        let occurrences = schedule
            .reconcile(
                cursor,
                now,
                MissedRunPolicy::All,
                None,
                100,
                &jiff::tz::TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap()
            .selected;
        assert_eq!(
            occurrences[0].nominal,
            "2026-01-01T00:00:00Z".parse().unwrap()
        );
    }

    #[tokio::test]
    async fn injected_clock_handles_recovery_reenable_and_backward_wall_move() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Arc::new(Store::open(paths.clone(), "test", 0).unwrap());
        let job_id = JobId::new().to_string();
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: "1s".parse().unwrap(),
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Process {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
            },
            cwd: PathBuf::from("/tmp"),
            environment: Environment::default(),
            policy: locron_core::policy::ExecutionPolicy {
                missed_run: MissedRunPolicy::Latest,
                ..Default::default()
            },
        };
        store
            .create_job(&CreateJob {
                id: job_id,
                name: "clocked".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: serde_json::to_string(&definition).unwrap(),
                now_us: 0,
                cursor_us: 0,
            })
            .unwrap();
        let clock = Arc::new(FakeClock::new(10_000_000, 10_000_000));
        let adapter = test_adapter(
            Arc::clone(&store),
            paths,
            clock.clone(),
            Arc::new(FakeTimeZoneResolver::new("UTC")),
            true,
        );

        assert_eq!(adapter.reconcile().await.unwrap(), 1);
        assert_eq!(store.history(Some("clocked"), 10).unwrap().len(), 1);
        assert_eq!(store.job("clocked").unwrap().cursor_us, 10_000_000);

        clock.set(5_000_000, 11_000_000);
        assert_eq!(adapter.reconcile().await.unwrap(), 0);
        assert_eq!(store.job("clocked").unwrap().cursor_us, 10_000_000);

        store.set_enabled("clocked", false, 12_000_000).unwrap();
        store.set_enabled("clocked", true, 13_000_000).unwrap();
        clock.set(20_000_000, 20_000_000);
        assert_eq!(adapter.reconcile().await.unwrap(), 1);
        let history = store.history(Some("clocked"), 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].trigger, "catch_up");
        assert_eq!(history[0].nominal_us, Some(20_000_000));
    }

    #[tokio::test]
    async fn injected_local_timezone_change_recalculates_without_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let cursor: Timestamp = "2026-08-20T00:00:00Z".parse().unwrap();
        let first_now: Timestamp = "2026-08-21T10:00:00Z".parse().unwrap();
        let second_now: Timestamp = "2026-08-22T00:00:00Z".parse().unwrap();
        let store = Arc::new(Store::open(paths.clone(), "test", cursor.epoch_micros()).unwrap());
        let definition = JobDefinition {
            schedule: Schedule::Cron {
                expression: "0 9 * * *".into(),
                timezone: ScheduleTimeZone::Local,
            },
            target: Target::Process {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
            },
            cwd: PathBuf::from("/tmp"),
            environment: Environment::default(),
            policy: locron_core::policy::ExecutionPolicy {
                missed_run: MissedRunPolicy::All,
                ..Default::default()
            },
        };
        store
            .create_job(&CreateJob {
                id: JobId::new().to_string(),
                name: "local-zone".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: serde_json::to_string(&definition).unwrap(),
                now_us: cursor.epoch_micros(),
                cursor_us: cursor.epoch_micros(),
            })
            .unwrap();
        let clock = Arc::new(FakeClock::new(first_now.epoch_micros(), 1_000_000));
        let timezone = Arc::new(FakeTimeZoneResolver::new("UTC"));
        let adapter = test_adapter(
            Arc::clone(&store),
            paths,
            clock.clone(),
            timezone.clone(),
            false,
        );
        assert_eq!(adapter.reconcile().await.unwrap(), 2);

        timezone.set("Asia/Seoul");
        let wall_delta = second_now.epoch_micros() - first_now.epoch_micros();
        clock.set(
            second_now.epoch_micros(),
            1_000_000 + u64::try_from(wall_delta).unwrap(),
        );
        assert_eq!(adapter.reconcile().await.unwrap(), 1);
        let history = store.history(Some("local-zone"), 10).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].nominal_us, Some(second_now.epoch_micros()));
        assert_ne!(history[0].nominal_us, history[1].nominal_us);
        assert_eq!(adapter.compiled_schedules.lock().unwrap().len(), 1);

        store
            .set_enabled("local-zone", false, second_now.epoch_micros() + 1)
            .unwrap();
        assert_eq!(adapter.reconcile().await.unwrap(), 0);
        assert!(adapter.compiled_schedules.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn real_store_completion_command_survives_applied_then_response_lost() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Arc::new(Store::open(paths.clone(), "test", 0).unwrap());
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: "1h".parse().unwrap(),
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Process {
                executable: "/bin/sh".into(),
                args: vec!["-c".into(), "exit 7".into()],
            },
            cwd: temp.path().into(),
            environment: Environment::default(),
            policy: locron_core::policy::ExecutionPolicy {
                retries: 1,
                ..Default::default()
            },
        };
        store
            .create_job(&CreateJob {
                id: JobId::new().to_string(),
                name: "completion-loss".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: serde_json::to_string(&definition).unwrap(),
                now_us: 0,
                cursor_us: 0,
            })
            .unwrap();
        let run_id = Uuid::now_v7().to_string();
        store.enqueue_manual("completion-loss", &run_id, 1).unwrap();
        let clock = Arc::new(FakeClock::new(5_000_000, 5_000_000));
        let adapter = test_adapter(
            Arc::clone(&store),
            paths,
            clock.clone(),
            Arc::new(FakeTimeZoneResolver::new("UTC")),
            false,
        );
        store.begin_lifetime(&adapter.lifetime, 2, "test").unwrap();
        let attempt = adapter.admit(1).await.unwrap().remove(0);
        assert!(adapter.mark_running(&attempt).await.unwrap());
        let outcome = Runner::new(RunnerConfig::default())
            .unwrap()
            .execute(&attempt.target, &attempt.context)
            .await
            .unwrap();
        assert_eq!(outcome.kind, OutcomeKind::FailedRetryable);
        let completed_at = adapter.completion_instant_us();

        let applied_then_lost = async {
            adapter
                .complete(&attempt, &outcome, completed_at)
                .await
                .map_err(|error| error.to_string())?;
            Err::<(), String>("synthetic response loss after commit".into())
        }
        .await;
        assert!(applied_then_lost.is_err());
        clock.set(99_000_000, 99_000_000);
        adapter
            .complete(&attempt, &outcome, completed_at)
            .await
            .unwrap();

        let run = store.run(&run_id).unwrap();
        assert_eq!(run.state, "retry_wait");
        assert_eq!(run.eligible_at_us, 15_000_000);
    }

    #[tokio::test]
    async fn durable_retry_remains_eligible_beyond_original_start_deadline() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Arc::new(Store::open(paths.clone(), "test", 0).unwrap());
        let job_id = JobId::new().to_string();
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: "1h".parse().unwrap(),
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Process {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
            },
            cwd: temp.path().into(),
            environment: Environment::default(),
            policy: locron_core::policy::ExecutionPolicy {
                retries: 1,
                start_deadline: Some("1s".parse().unwrap()),
                ..Default::default()
            },
        };
        let snapshot = serde_json::to_string(&definition).unwrap();
        store
            .create_job(&CreateJob {
                id: job_id.clone(),
                name: "deadline-retry".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: snapshot.clone(),
                now_us: 0,
                cursor_us: 0,
            })
            .unwrap();
        let run_id = Uuid::now_v7().to_string();
        store
            .materialize(
                &job_id,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 0,
                    new_cursor_us: 1_000_000,
                    resolve_one_time: false,
                },
                &[NewScheduledRun {
                    id: run_id.clone(),
                    job_id: job_id.clone(),
                    revision: 1,
                    trigger: "scheduled".into(),
                    nominal_us: 1_000_000,
                    requested_at_us: 1_000_000,
                    eligible_at_us: 1_000_000,
                    snapshot_json: snapshot,
                }],
                1_000_000,
            )
            .unwrap();
        let clock = Arc::new(FakeClock::new(1_000_000, 1_000_000));
        let adapter = test_adapter(
            Arc::clone(&store),
            paths,
            clock.clone(),
            Arc::new(FakeTimeZoneResolver::new("UTC")),
            false,
        );
        store
            .begin_lifetime(&adapter.lifetime, 1_000_000, "test")
            .unwrap();
        let attempt = adapter.admit(64).await.unwrap().remove(0);
        assert!(adapter.mark_running(&attempt).await.unwrap());

        clock.set(100_000_000, 100_000_000);
        let completed_at = adapter.completion_instant_us();
        adapter
            .complete(
                &attempt,
                &locron_engine::runner::ExecutionOutcome {
                    kind: OutcomeKind::FailedRetryable,
                    exit_code: Some(7),
                    http_status: None,
                    http_content_type: None,
                    reason: "known failure".into(),
                    duration_micros: 1,
                    output: locron_engine::OutputStats::default(),
                },
                completed_at,
            )
            .await
            .unwrap();
        assert_eq!(store.run(&run_id).unwrap().state, "retry_wait");
        assert_eq!(store.run(&run_id).unwrap().eligible_at_us, 110_000_000);

        clock.set(110_000_000, 110_000_000);
        let retry = adapter.admit(64).await.unwrap();
        assert_eq!(retry.len(), 1);
        assert_eq!(retry[0].run_id, run_id);
        assert_eq!(retry[0].context.attempt, 2);
    }

    #[test]
    fn http_header_environment_sources_resolve_per_attempt() {
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: "1h".parse().unwrap(),
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Http(HttpTarget {
                method: HttpMethod::Get,
                url: "https://example.test".into(),
                headers: BTreeMap::from([(
                    "X-Token".into(),
                    HttpHeaderSource::Environment("TOKEN".into()),
                )]),
                body: None,
                body_file: None,
                success_statuses: vec![],
                follow_redirects: false,
            }),
            cwd: std::env::current_dir().unwrap(),
            environment: Environment {
                values: BTreeMap::from([("TOKEN".into(), "attempt-value".into())]),
                ..Environment::default()
            },
            policy: Default::default(),
        };
        let attempt = AdmitAttempt {
            run_id: Uuid::now_v7().to_string(),
            job_id: Uuid::now_v7().to_string(),
            attempt_number: 1,
            trigger: "manual".into(),
            nominal_us: None,
            snapshot_json: String::new(),
        };

        let TargetSpec::Http(http) =
            engine_target(&definition, &attempt, &default_settings()).unwrap()
        else {
            panic!("expected HTTP target");
        };
        assert_eq!(
            http.headers.get("X-Token").map(String::as_str),
            Some("attempt-value")
        );

        let mut missing = definition;
        missing.environment.values.clear();
        assert!(
            engine_target(&missing, &attempt, &default_settings())
                .unwrap_err()
                .contains("header environment TOKEN is missing")
        );
    }

    #[test]
    fn process_executable_and_path_list_normalize_against_registration_context() {
        let temp = tempfile::tempdir().unwrap();
        let target = TargetArgs {
            cwd: Some(temp.path().to_path_buf()),
            path: Some(format!("./tools{}../bin", std::path::MAIN_SEPARATOR)),
            command: vec!["./scripts/task".into()],
            ..TargetArgs::default()
        };
        let (definition, _) = normalize_definition(
            None,
            &ScheduleArgs {
                every: Some("1h".into()),
                ..ScheduleArgs::default()
            },
            &target,
            &PolicyArgs::default(),
            16,
            1,
        )
        .unwrap();
        let Target::Process { executable, .. } = definition.target else {
            panic!("expected process target");
        };
        assert_eq!(PathBuf::from(executable), temp.path().join("scripts/task"));
        assert!(
            definition
                .environment
                .path
                .as_deref()
                .is_some_and(|path| Path::new(path).is_absolute())
        );
    }

    // --- Export selection and URL import (2026-08-24) ---

    fn job_record(name: &str, tags: &[&str]) -> JobRecord {
        JobRecord {
            id: format!("id-{name}"),
            name: name.to_owned(),
            description: None,
            tags_json: serde_json::to_string(tags).unwrap(),
            enabled: true,
            removed_at_us: None,
            current_revision: 1,
            definition_json: "{}".into(),
            cursor_us: 0,
            updated_at_us: 0,
            cursor_updated_at_us: 0,
            disabled_since_us: None,
        }
    }

    #[derive(Default)]
    struct CountingPicker {
        selected: Vec<String>,
        calls: std::cell::Cell<usize>,
    }

    impl CountingPicker {
        fn picking(selected: &[&str]) -> Self {
            Self {
                selected: selected.iter().map(|id| format!("id-{id}")).collect(),
                calls: std::cell::Cell::new(0),
            }
        }
    }

    impl JobPicker for CountingPicker {
        fn pick(&self, jobs: &[JobRecord]) -> Result<Vec<String>> {
            self.calls.set(self.calls.get() + 1);
            assert!(
                self.selected.len() <= jobs.len(),
                "fake picked more jobs than exist"
            );
            Ok(self.selected.clone())
        }
    }

    #[test]
    fn export_picker_decision_is_table_tested_across_the_context_matrix() {
        // (stdin_tty, stdout_tty, stderr_tty, ci_set, format, has_selectors)
        let cases: &[(bool, bool, bool, bool, Format, bool, bool)] = &[
            // The single interactive context: three terminals, CI unset,
            // human format, no selector.
            (true, true, true, false, Format::Human, false, true),
            // Each stream alone can suppress the picker.
            (false, true, true, false, Format::Human, false, false),
            (true, false, true, false, Format::Human, false, false),
            (true, true, false, false, Format::Human, false, false),
            // CI, JSON, and selectors each suppress it.
            (true, true, true, true, Format::Human, false, false),
            (true, true, true, false, Format::Json, false, false),
            (true, true, true, false, Format::Human, true, false),
            // Combined hostile contexts stay non-interactive.
            (false, false, false, true, Format::Json, true, false),
            (true, false, false, true, Format::Human, true, false),
            (false, true, true, true, Format::Json, false, false),
        ];
        for (stdin, stdout, stderr, ci, format, selectors, expected) in cases {
            let tty = TerminalState {
                stdin: *stdin,
                stdout: *stdout,
                stderr: *stderr,
            };
            assert_eq!(
                should_show_picker(tty, *ci, *format, *selectors),
                *expected,
                "decision mismatch for stdin={stdin} stdout={stdout} stderr={stderr} ci={ci} format={format:?} selectors={selectors}"
            );
        }
    }

    #[test]
    fn export_selector_union_dedup_and_no_match() {
        let jobs = vec![
            job_record("alpha", &[]),
            job_record("beta", &["nightly"]),
            job_record("gamma", &["nightly", "backup"]),
            job_record("delta", &["backup"]),
            job_record("epsilon", &["x"]),
        ];
        let selectors = ExportSelectors::parse(Some("alpha,gamma"), Some("nightly"));
        let picker = CountingPicker::picking(&[]);
        let selected = select_export_jobs(jobs.clone(), &selectors, true, &picker).unwrap();
        let names: Vec<&str> = selected.iter().map(|job| job.name.as_str()).collect();
        // alpha by name; beta and gamma by tag; store order preserved; no dup.
        assert_eq!(names, ["alpha", "beta", "gamma"]);
        assert_eq!(
            picker.calls.get(),
            0,
            "selectors must never show the picker"
        );

        // A single job satisfying several selector values is not duplicated:
        // gamma carries both --tag values, epsilon matches both --jobs and --tag.
        // delta is included because "backup" matches every job carrying it.
        let selectors = ExportSelectors::parse(Some("epsilon,gamma"), Some("x,backup"));
        let selected = select_export_jobs(jobs.clone(), &selectors, false, &picker).unwrap();
        let names: Vec<&str> = selected.iter().map(|job| job.name.as_str()).collect();
        assert_eq!(names, ["gamma", "delta", "epsilon"]);

        // A tag value must match every job carrying it, even when several
        // jobs share the tag: beta and gamma both carry "nightly".
        let selectors = ExportSelectors::parse(None, Some("nightly"));
        let selected = select_export_jobs(jobs.clone(), &selectors, false, &picker).unwrap();
        let names: Vec<&str> = selected.iter().map(|job| job.name.as_str()).collect();
        assert_eq!(names, ["beta", "gamma"]);
    }

    #[test]
    fn export_selector_no_match_is_a_validation_error_before_output() {
        let jobs = vec![job_record("alpha", &["nightly"])];
        let selectors = ExportSelectors::parse(Some("alpha,ghost"), Some("nightly"));
        let error = select_export_jobs(jobs, &selectors, false, &CountingPicker::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("--jobs ghost"), "unexpected: {error}");
        assert!(!error.contains("--jobs alpha"), "unexpected: {error}");
        assert!(!error.contains("--tag nightly"), "unexpected: {error}");

        let jobs = vec![job_record("alpha", &["nightly"])];
        let selectors = ExportSelectors::parse(None, Some("nightly,ghost"));
        let error = select_export_jobs(jobs, &selectors, false, &CountingPicker::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("--tag ghost"), "unexpected: {error}");

        // A totally unmatched selection fails even when other jobs exist.
        let jobs = vec![job_record("alpha", &["nightly"])];
        let selectors = ExportSelectors::parse(Some("absent"), None);
        let error = select_export_jobs(jobs, &selectors, false, &CountingPicker::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("matched no job"), "unexpected: {error}");
    }

    #[test]
    fn export_fake_picker_drives_selection_without_a_pty() {
        let jobs = vec![
            job_record("alpha", &[]),
            job_record("beta", &[]),
            job_record("gamma", &[]),
        ];
        let picker = CountingPicker::picking(&["alpha", "gamma"]);
        let selected = select_export_jobs(
            jobs.clone(),
            &ExportSelectors::parse(None, None),
            true,
            &picker,
        )
        .unwrap();
        let names: Vec<&str> = selected.iter().map(|job| job.name.as_str()).collect();
        assert_eq!(names, ["alpha", "gamma"]);
        assert_eq!(picker.calls.get(), 1);

        // An empty confirmed selection yields a settings-only export.
        let picker = CountingPicker::picking(&[]);
        let selected = select_export_jobs(
            jobs.clone(),
            &ExportSelectors::parse(None, None),
            true,
            &picker,
        )
        .unwrap();
        assert!(selected.is_empty());
        assert_eq!(picker.calls.get(), 1);
    }

    #[test]
    fn export_non_interactive_never_instantiates_the_picker() {
        let jobs = vec![job_record("alpha", &[])];
        // Non-interactive (pipes, redirection, CI, JSON): full export, no picker.
        let picker = CountingPicker::picking(&[]);
        let selected = select_export_jobs(
            jobs.clone(),
            &ExportSelectors::parse(None, None),
            false,
            &picker,
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(picker.calls.get(), 0);

        // JSON mode is decided non-interactive before any picker exists.
        let tty = TerminalState {
            stdin: true,
            stdout: true,
            stderr: true,
        };
        let json = should_show_picker(tty, false, Format::Json, false);
        assert!(!json);

        // Zero registered jobs skip the picker even when interactive.
        let picker = CountingPicker::picking(&[]);
        let selected = select_export_jobs(
            Vec::new(),
            &ExportSelectors::parse(None, None),
            true,
            &picker,
        )
        .unwrap();
        assert!(selected.is_empty());
        assert_eq!(picker.calls.get(), 0);
    }

    #[test]
    fn scripted_picker_resolves_names_and_rejects_unknown_ones() {
        let jobs = vec![job_record("alpha", &[]), job_record("beta", &[])];
        let picker = ScriptedPicker {
            names: "alpha,alpha,beta".into(),
        };
        let picked = picker.pick(&jobs).unwrap();
        let names: Vec<&str> = jobs
            .iter()
            .filter(|job| picked.contains(&job.id))
            .map(|job| job.name.as_str())
            .collect();
        assert_eq!(names, ["alpha", "beta"]);

        let picker = ScriptedPicker {
            names: "alpha,ghost".into(),
        };
        let error = picker.pick(&jobs).unwrap_err().to_string();
        assert!(error.contains("ghost"), "unexpected: {error}");

        // An empty scripted selection means the user deselected everything.
        let picker = ScriptedPicker {
            names: String::new(),
        };
        assert!(picker.pick(&jobs).unwrap().is_empty());
    }

    #[test]
    fn import_source_classifies_paths_and_urls_explicitly() {
        match import_source(Path::new("backup.json")).unwrap() {
            ImportSource::Path(path) => assert_eq!(path, Path::new("backup.json")),
            ImportSource::Url(_) => panic!("relative name classified as URL"),
        }
        match import_source(Path::new("/tmp/backup.json")).unwrap() {
            ImportSource::Path(path) => assert_eq!(path, Path::new("/tmp/backup.json")),
            ImportSource::Url(_) => panic!("absolute path classified as URL"),
        }
        // A colon in a name without `://` is a path, not a scheme.
        match import_source(Path::new("backup:2024.json")).unwrap() {
            ImportSource::Path(path) => assert_eq!(path, Path::new("backup:2024.json")),
            ImportSource::Url(_) => panic!("colon-name classified as URL"),
        }
        match import_source(Path::new("https://example.test/doc.json")).unwrap() {
            ImportSource::Url(url) => {
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("example.test"));
            }
            ImportSource::Path(_) => panic!("https URL classified as path"),
        }
        let error = import_source(Path::new("https://user:pass@example.test/doc.json"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("userinfo"), "unexpected: {error}");
        let error = import_source(Path::new("ftp://example.test/doc.json"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("scheme"), "unexpected: {error}");
        assert!(import_source(Path::new("https://example.test/")).is_ok());
    }

    fn list_job_record(name: &str, enabled: bool, shell_command: &str) -> Value {
        json!({
            "name": name,
            "enabled": enabled,
            "definition_json": serde_json::to_string(&json!({
                "schedule": {"kind": "every", "interval": 3_600_000_000u64},
                "target": {"kind": "shell", "command": shell_command}
            }))
            .unwrap(),
        })
    }

    #[test]
    fn truncate_display_ascii_fitting_no_fit_and_boundaries() {
        // Fits: returned unchanged, no marker, at and above the exact width.
        assert_eq!(truncate_display("hello", 10), "hello");
        assert_eq!(truncate_display("hello", 5), "hello");
        // No-fit: truncated to max_width - 1 text columns plus the marker.
        assert_eq!(truncate_display("hello", 4), "hel…");
        assert_eq!(truncate_display("hello", 3), "he…");
        assert_eq!(truncate_display("hello", 2), "h…");
        // Minimum widths: one column holds only the marker; zero cannot hold
        // the marker, so the text is returned unchanged.
        assert_eq!(truncate_display("hello", 1), "…");
        assert_eq!(truncate_display("hello", 0), "hello");
        assert_eq!(truncate_display("", 0), "");
    }

    #[test]
    fn truncate_display_uses_display_width_for_cjk_and_emoji() {
        // Width-2 CJK characters count as two columns.
        assert_eq!(truncate_display("한글", 4), "한글");
        assert_eq!(truncate_display("한글", 3), "한…");
        assert_eq!(truncate_display("한글", 2), "…");
        // Emoji also occupy two columns.
        assert_eq!(truncate_display("😀", 2), "😀");
        assert_eq!(truncate_display("😀", 1), "…");
        assert_eq!(truncate_display("😀😀", 3), "😀…");
        // Mixed ASCII and wide characters fit by total display width; a wide
        // character that cannot fit the remaining budget ends the cut.
        assert_eq!(truncate_display("a한b", 4), "a한b");
        assert_eq!(truncate_display("a한b", 3), "a…");
    }

    #[test]
    fn truncate_display_appends_the_marker_only_when_truncating() {
        // An exact fit is not a truncation.
        assert_eq!(
            truncate_display("run /usr/bin/true", 17),
            "run /usr/bin/true"
        );
        // One column over the width truncates and marks the cut.
        assert_eq!(
            truncate_display("run /usr/bin/true", 16),
            "run /usr/bin/tr…"
        );
        // A comfortably fitting value is byte-identical.
        assert_eq!(truncate_display("short", 80), "short");
    }

    #[test]
    fn list_table_with_injected_widths_truncates_only_the_target_column() {
        let jobs = vec![
            list_job_record("a", true, "git push origin main"),
            list_job_record("b", true, "run-a-very-long-backup-job-with-a-silly-name"),
        ];
        // name_width = max(4, 1, 1) = 4; schedule_width = max(8, 8, 8) = 8;
        // target_width = max(6, 26, 50) = 50; the natural table width is
        // 4 + 1 + 8 + 1 + 50 + 1 + 7 = 72.
        let full = format!(
            "{:<4} {:<8} {:<50} ENABLED\n\
             {:<4} {:<8} {:<50} yes\n\
             {:<4} {:<8} {:<50} yes\n",
            "NAME",
            "SCHEDULE",
            "TARGET",
            "a",
            "every 1h",
            "shell git push origin main",
            "b",
            "every 1h",
            "shell run-a-very-long-backup-job-with-a-silly-name",
        );
        // No width: full values, byte-identical to the pre-change table.
        assert_eq!(list_table(&jobs, None).unwrap(), full);
        // A width that fits the natural table width: unchanged.
        assert_eq!(list_table(&jobs, Some(80)).unwrap(), full);
        assert_eq!(list_table(&jobs, Some(72)).unwrap(), full);
        // A width that cannot even hold NAME + SCHEDULE + ENABLED alone: the
        // documented fallback prints the untruncated table (rows wrap).
        assert_eq!(list_table(&jobs, Some(20)).unwrap(), full);
    }

    #[test]
    fn list_table_truncates_the_target_column_to_the_remaining_width() {
        let jobs = vec![
            list_job_record("a", true, "git push origin main"),
            list_job_record("b", true, "run-a-very-long-backup-job-with-a-silly-name"),
        ];
        // At width 40 the fixed prefix (4 + 1 + 8 + 1) and the trailing
        // " ENABLED" (1 + 7) leave 40 - 22 = 18 columns for TARGET; every
        // target value shrinks to 17 text columns plus the marker.
        let expected = format!(
            "{:<4} {:<8} {:<18} ENABLED\n\
             {:<4} {:<8} {:<18} yes\n\
             {:<4} {:<8} {:<18} yes\n",
            "NAME",
            "SCHEDULE",
            "TARGET",
            "a",
            "every 1h",
            "shell git push or…",
            "b",
            "every 1h",
            "shell run-a-very-…",
        );
        assert_eq!(list_table(&jobs, Some(40)).unwrap(), expected);
    }
}
