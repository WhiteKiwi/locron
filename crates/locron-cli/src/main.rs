//! `locron` command-line composition root.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use locron_core::command::JobDefinition;
use locron_core::policy::{BackoffMode, MissedRunPolicy, OverlapPolicy};
use locron_core::schedule::{Schedule, ScheduleTimeZone};
use locron_core::target::{
    Environment, HttpHeaderSource, HttpMethod, HttpTarget, Target, is_valid_environment_name,
    is_valid_http_header_name,
};
use locron_core::{JobId, SchedulerLifetimeId, Timestamp};
use locron_engine::daemon::{AdmittedAttempt, DaemonStore};
use locron_engine::runner::{OutcomeKind, RunnerConfig};
use locron_engine::{
    AttemptContext, Daemon, DaemonConfig, HttpSpec, OutputWriter, ProcessSpec, Runner, TargetSpec,
};
use locron_store::{
    AdmitAttempt, AttemptCompletion, CreateJob, CursorUpdate, ImportBatch, ImportJob,
    ImportResolution, JobRecord, LockMetadata, NewScheduledRun, OutputRecord, RetryPlan, RunRecord,
    SettingsRecord, StatePaths, Store, StoreError, UpdateJob,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "locron",
    version,
    about = "A predictable local-first job scheduler"
)]
struct Cli {
    #[arg(long, global = true, env = "LOCRON_STATE_DIR")]
    state_dir: Option<PathBuf>,
    #[arg(long, global = true, value_enum, default_value = "human")]
    format: Format,
    #[arg(long, global = true, action = ArgAction::SetTrue, conflicts_with = "format")]
    json: bool,
    #[arg(short = 'v', long, global = true, action = ArgAction::Count)]
    verbose: u8,
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum Format {
    Human,
    Json,
}

#[derive(Subcommand, Debug)]
enum Command {
    Add(AddArgs),
    Update(UpdateArgs),
    List {
        #[arg(long)]
        all: bool,
    },
    Show {
        name: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Remove {
        name: String,
    },
    Preview(PreviewArgs),
    Run {
        name: String,
        #[arg(long)]
        wait: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Cancel {
        run_id: String,
    },
    History {
        name: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Logs {
        run_id: String,
        #[arg(long)]
        attempt: Option<u16>,
        #[arg(long)]
        follow: bool,
        #[arg(long, value_enum, default_value = "all")]
        channel: LogChannel,
    },
    Why {
        name: Option<String>,
        #[arg(long)]
        run: Option<String>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Export {
        #[arg(long)]
        include_values: bool,
        #[arg(long)]
        acknowledge_plaintext: bool,
        #[arg(long)]
        include_history: bool,
    },
    Import {
        path: PathBuf,
        #[arg(long)]
        accept_plaintext_values: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Prune {
        #[arg(long)]
        dry_run: bool,
    },
    Doctor,
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Get {
        key: Option<String>,
    },
    Set {
        key: String,
        value: String,
        #[arg(long)]
        dry_run: bool,
    },
}
#[derive(Subcommand, Debug)]
enum DaemonCommand {
    Run,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum LogChannel {
    All,
    Stdout,
    Stderr,
    Body,
}

#[derive(Args, Debug, Clone, Default)]
struct ScheduleArgs {
    #[arg(long)]
    cron: Option<String>,
    #[arg(long)]
    every: Option<String>,
    #[arg(long)]
    at: Option<String>,
    #[arg(long)]
    timezone: Option<String>,
    #[arg(long)]
    anchor: Option<String>,
}

#[derive(Args, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct TargetArgs {
    #[arg(long)]
    shell: Option<String>,
    #[arg(long, num_args = 2)]
    http: Option<Vec<String>>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long = "env", value_parser = parse_key_value)]
    env: Vec<(String, String)>,
    #[arg(long)]
    unset_env: Vec<String>,
    #[arg(long)]
    clear_env: bool,
    #[arg(long)]
    env_file: Option<PathBuf>,
    #[arg(long)]
    no_env_file: bool,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    no_path: bool,
    #[arg(long)]
    shell_executable: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["body_file", "json_body", "clear_body"])]
    body: Option<String>,
    #[arg(long, conflicts_with_all = ["body", "json_body", "clear_body"])]
    body_file: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["body", "body_file", "clear_body"])]
    json_body: Option<String>,
    #[arg(long, conflicts_with_all = ["body", "body_file", "json_body"])]
    clear_body: bool,
    #[arg(long, value_parser = parse_key_value)]
    header: Vec<(String, String)>,
    #[arg(long, value_parser = parse_key_value)]
    header_env: Vec<(String, String)>,
    #[arg(long)]
    unset_header: Vec<String>,
    #[arg(long)]
    clear_headers: bool,
    #[arg(long)]
    success_status: Vec<String>,
    #[arg(long)]
    clear_success_statuses: bool,
    #[arg(long, conflicts_with = "no_follow_redirects")]
    follow_redirects: bool,
    #[arg(long, conflicts_with = "follow_redirects")]
    no_follow_redirects: bool,
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct PolicyArgs {
    #[arg(long, value_enum)]
    overlap: Option<OverlapArg>,
    #[arg(long, value_enum)]
    missed_run: Option<MissedArg>,
    #[arg(long)]
    start_deadline: Option<String>,
    #[arg(long, conflicts_with = "start_deadline")]
    no_start_deadline: bool,
    #[arg(long)]
    catch_up_limit: Option<u16>,
    #[arg(long)]
    retries: Option<u8>,
    #[arg(long, value_enum)]
    backoff: Option<BackoffArg>,
    #[arg(long)]
    retry_delay: Option<String>,
    #[arg(long)]
    retry_cap: Option<String>,
    #[arg(long, conflicts_with = "no_timeout")]
    timeout: Option<String>,
    #[arg(long, conflicts_with = "timeout")]
    no_timeout: bool,
    #[arg(long, conflicts_with = "no_retry_timeout")]
    retry_timeout: bool,
    #[arg(long, conflicts_with = "retry_timeout")]
    no_retry_timeout: bool,
    #[arg(long)]
    termination_grace: Option<String>,
    #[arg(long)]
    per_job_concurrency: Option<u8>,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OverlapArg {
    Skip,
    Replace,
    Allow,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum MissedArg {
    Skip,
    Latest,
    All,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackoffArg {
    Fixed,
    Exponential,
}

#[derive(Args, Debug)]
struct AddArgs {
    name: String,
    #[command(flatten)]
    schedule: ScheduleArgs,
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    policy: PolicyArgs,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    disabled: bool,
    #[arg(long)]
    dry_run: bool,
}
#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct UpdateArgs {
    name: String,
    #[command(flatten)]
    schedule: ScheduleArgs,
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    policy: PolicyArgs,
    #[arg(long)]
    rename: Option<String>,
    #[arg(long, conflicts_with = "clear_description")]
    description: Option<String>,
    #[arg(long, conflicts_with = "description")]
    clear_description: bool,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long, conflicts_with = "tag")]
    clear_tags: bool,
    #[arg(long, conflicts_with = "disabled")]
    enabled: bool,
    #[arg(long, conflicts_with = "enabled")]
    disabled: bool,
    #[arg(long)]
    dry_run: bool,
}
#[derive(Args, Debug)]
struct PreviewArgs {
    value: Option<String>,
    #[command(flatten)]
    schedule: ScheduleArgs,
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose, cli.debug);
    let format = if cli.json { Format::Json } else { cli.format };
    let command_name = command_name(&cli.command);
    if let Err(error) = execute(cli, format).await {
        render_error(format, command_name, &error);
        std::process::exit(exit_code(&error));
    }
}

async fn execute(cli: Cli, format: Format) -> Result<()> {
    let paths = StatePaths::discover(cli.state_dir.as_deref())?;
    match cli.command {
        Command::Add(args) => add(&paths, args, format),
        Command::Update(args) => update(&paths, &args, format),
        Command::List { all } => {
            let jobs = open(&paths)?
                .list_jobs(all)?
                .into_iter()
                .map(redacted_job)
                .collect::<Result<Vec<_>>>()?;
            render(format, "list", json!(jobs), &[]);
            Ok(())
        }
        Command::Show { name } => {
            render(
                format,
                "show",
                redacted_job(open(&paths)?.job(&name)?)?,
                &[],
            );
            Ok(())
        }
        Command::Enable { name } => toggle(&paths, &name, true, format),
        Command::Disable { name } => toggle(&paths, &name, false, format),
        Command::Remove { name } => {
            open(&paths)?.remove_job(&name, now_us())?;
            send_wake(&paths);
            render(format, "remove", json!({"name":name,"removed":true}), &[]);
            Ok(())
        }
        Command::Preview(args) => preview(&paths, args, format),
        Command::Run {
            name,
            wait,
            dry_run,
        } => run_job(&paths, &name, wait, dry_run, format).await,
        Command::Cancel { run_id } => {
            Uuid::parse_str(&run_id).context("invalid run UUID")?;
            open(&paths)?.cancel(&run_id, now_us())?;
            send_wake(&paths);
            render(
                format,
                "cancel",
                json!({"run_id":run_id,"requested":true}),
                &[],
            );
            Ok(())
        }
        Command::History { name, limit } => {
            let runs = open(&paths)?
                .history(name.as_deref(), limit)?
                .into_iter()
                .map(redacted_run)
                .collect::<Result<Vec<_>>>()?;
            render(format, "history", json!(runs), &[]);
            Ok(())
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
            include_values,
            acknowledge_plaintext,
            include_history,
        } => export(
            &paths,
            include_values,
            acknowledge_plaintext,
            include_history,
            format,
        ),
        Command::Import {
            path,
            accept_plaintext_values,
            dry_run,
        } => import(&paths, &path, accept_plaintext_values, dry_run, format),
        Command::Prune { dry_run } => prune(&paths, dry_run, format),
        Command::Doctor => doctor(&paths, format),
        Command::Daemon {
            command: DaemonCommand::Run,
        } => daemon(paths).await,
    }
}

fn open(paths: &StatePaths) -> Result<Store> {
    Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), now_us()).map_err(Into::into)
}
fn open_read_only(paths: &StatePaths) -> Result<Store> {
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
        render(
            format,
            "add",
            json!({"dry_run":true,"normalized":{"name":args.name,"enabled":!args.disabled,"definition":redact_definition(serde_json::to_value(&definition)?)},"id":"<non-durable>"}),
            &warnings,
        );
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
    render(format, "add", redacted_job(record)?, &warnings);
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
    render(format, "update", redacted_job(record)?, &warnings);
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
    render(
        format,
        if enabled { "enable" } else { "disable" },
        redacted_job(record)?,
        &[],
    );
    Ok(())
}

fn preview(paths: &StatePaths, args: PreviewArgs, format: Format) -> Result<()> {
    let schedule = if let Some(name) = args.value {
        let job = open(paths)?.job(&name)?;
        serde_json::from_str::<JobDefinition>(&job.definition_json)?.schedule
    } else {
        build_schedule_only(&args.schedule)?
    };
    let next = schedule.next(Timestamp::from_epoch_micros(now_us()), args.count)?;
    render(
        format,
        "preview",
        json!({"occurrences":next.iter().map(ToString::to_string).collect::<Vec<_>>()}),
        &[],
    );
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
        render(
            format,
            "run",
            json!({"dry_run":true,"durable":false,"decision":decision,"capacity_reserved":false}),
            &[],
        );
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
    render(
        format,
        "run",
        json!({"run_id":run.id,"state":run.state}),
        &warnings,
    );
    if wait {
        wait_run(&store, &run_id, format).await?
    }
    Ok(())
}

async fn wait_run(store: &Store, id: &str, format: Format) -> Result<()> {
    loop {
        let run = store.run(id)?;
        if !matches!(
            run.state.as_str(),
            "queued" | "starting" | "running" | "retry_wait"
        ) {
            if format == Format::Human {
                println!("{}: {}", run.id, run.state);
            }
            if run.state != "succeeded" {
                return Err(anyhow!("target finished with {}", run.state));
            }
            return Ok(());
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
    let path = paths.final_output(run_id, attempt.unwrap_or(1))?;
    loop {
        if path.exists() {
            for frame in locron_engine::read_frames(&path)? {
                let selected = matches!(channel, LogChannel::All)
                    || matches!(
                        (channel, frame.channel),
                        (LogChannel::Stdout, locron_engine::Channel::Stdout)
                            | (LogChannel::Stderr, locron_engine::Channel::Stderr)
                            | (LogChannel::Body, locron_engine::Channel::Body)
                    );
                if selected {
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
        if !follow {
            return Err(anyhow!("output not found"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await
    }
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
            render(
                format,
                "why",
                json!({"job":job,"next_occurrence":next,"active_runs":active,"overlap":definition.policy.overlap,"daemon_running":!daemon_lock_free(paths),"explanation":"facts are read from durable state; unknown execution facts are not inferred"}),
                &[],
            );
            Ok(())
        }
        (None, Some(id)) => {
            render(
                format,
                "why",
                json!({"run":redacted_run(store.run(&id)?)?,"daemon_running":!daemon_lock_free(paths),"explanation":"terminal reason and immutable snapshot are durable facts"}),
                &[],
            );
            Ok(())
        }
        _ => Err(anyhow!("provide a job name or --run RUN_ID")),
    }
}

fn config(paths: &StatePaths, command: ConfigCommand, format: Format) -> Result<()> {
    match command {
        ConfigCommand::Get { key } => {
            let store = open(paths)?;
            let settings = serde_json::to_value(store.settings()?)?;
            let data = match key {
                Some(key) => {
                    json!({"key":key,"value":settings.get(&key).ok_or_else(||anyhow!("unknown configuration key"))?})
                }
                None => settings,
            };
            render(format, "config get", data, &[]);
        }
        ConfigCommand::Set {
            key,
            value,
            dry_run,
        } => {
            if dry_run {
                validate_config_value(&key, &value)?;
                render(
                    format,
                    "config set",
                    json!({"key":key,"value":value,"dry_run":true}),
                    &[],
                );
            } else {
                let store = open(paths)?;
                let settings = store.set_setting(&key, &value, now_us())?;
                send_wake(paths);
                render(format, "config set", json!(settings), &[]);
            }
        }
    }
    Ok(())
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
    let store = open(paths)?;
    let jobs = store
        .list_jobs(true)?
        .into_iter()
        .map(|job| export_job(job, values_mode))
        .collect::<Result<Vec<_>>>()?;
    let document = ExportDocument {
        schema: "locron.export/v1".into(),
        values_mode,
        settings: store.settings()?,
        jobs,
    };
    match format {
        Format::Json => render(format, "export", serde_json::to_value(&document)?, &[]),
        Format::Human => println!("{}", serde_json::to_string_pretty(&document)?),
    }
    Ok(())
}
fn import(
    paths: &StatePaths,
    path: &Path,
    accept: bool,
    dry_run: bool,
    format: Format,
) -> Result<()> {
    let document = parse_import_document(path, accept)?;
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
    let data = import_plan_value(&plan, dry_run);
    if dry_run {
        render(format, "import", data, &[]);
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
    if no_op == mutations.len() && !plan.settings_changed {
        render(
            format,
            "import",
            json!({"created":0,"updated":0,"no_op":no_op,"settings_changed":false}),
            &[],
        );
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
    render(
        format,
        "import",
        json!({"created":summary.created,"updated":summary.updated,"no_op":no_op,"settings_changed":plan.settings_changed}),
        &[],
    );
    Ok(())
}

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
    let mut document: ExportDocument =
        serde_json::from_slice(&std::fs::read(path)?).context("invalid export document")?;
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
            if document
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
            if document
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
    render(
        format,
        "doctor",
        json!({"state_dir":paths.root,"database":paths.database,"daemon_running":!daemon_lock_free(paths),"wake_socket":paths.wake_socket.exists(),"checks":store.integrity_check()?}),
        &[],
    );
    Ok(())
}

fn prune(paths: &StatePaths, dry_run: bool, format: Format) -> Result<()> {
    if dry_run && !paths.database.is_file() {
        render(
            format,
            "prune",
            json!({"dry_run":true,"candidate_count":0,"bytes":0}),
            &[],
        );
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
    render(
        format,
        "prune",
        json!({"dry_run":dry_run,"candidate_count":candidates.len(),"bytes":candidates.iter().map(|candidate|candidate.physical_bytes).sum::<i64>()}),
        &[],
    );
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

fn send_wake(paths: &StatePaths) {
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
    wake: Mutex<Option<Arc<tokio::sync::Notify>>>,
    wake_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    lock: Mutex<Option<locron_store::DaemonLock>>,
}

impl StoreAdapter {
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
                now_us(),
                &reason,
            )
            .map_err(|error| error.to_string())
    }
}

#[async_trait::async_trait]
impl DaemonStore for StoreAdapter {
    async fn begin_lifetime(&self) -> Result<(), String> {
        let metadata = LockMetadata {
            pid: std::process::id(),
            lifetime_id: self.lifetime.clone(),
            started_at_us: now_us(),
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
            .begin_lifetime(&self.lifetime, now_us(), env!("CARGO_PKG_VERSION"))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    async fn reconcile(&self) -> Result<usize, String> {
        let now = now_us();
        let mut total = 0;
        for job in self.store.list_jobs(false).map_err(|e| e.to_string())? {
            let definition: JobDefinition =
                serde_json::from_str(&job.definition_json).map_err(|e| e.to_string())?;
            let mut occurrences = due_occurrences(
                &definition.schedule,
                job.cursor_us,
                now,
                usize::from(definition.policy.catch_up_limit),
            )?;
            if let Some(deadline) = definition.policy.start_deadline {
                let cutoff = now.saturating_sub(i64::try_from(deadline.get()).unwrap_or(i64::MAX));
                occurrences.retain(|at| at.epoch_micros() >= cutoff);
            }
            let latest_is_normal = occurrences
                .last()
                .is_some_and(|at| now.saturating_sub(at.epoch_micros()) <= 31_000_000);
            let selected: Vec<_> = match definition.policy.missed_run {
                MissedRunPolicy::Skip if latest_is_normal => {
                    occurrences.last().copied().into_iter().collect()
                }
                MissedRunPolicy::Skip => Vec::new(),
                MissedRunPolicy::Latest => occurrences.last().copied().into_iter().collect(),
                MissedRunPolicy::All => occurrences,
            };
            let runs = selected
                .into_iter()
                .map(|at| NewScheduledRun {
                    id: Uuid::now_v7().to_string(),
                    job_id: job.id.clone(),
                    revision: job.current_revision,
                    trigger: if now.saturating_sub(at.epoch_micros()) > 31_000_000 {
                        "catch_up".into()
                    } else {
                        "scheduled".into()
                    },
                    nominal_us: at.epoch_micros(),
                    requested_at_us: now,
                    eligible_at_us: now,
                    snapshot_json: job.definition_json.clone(),
                })
                .collect::<Vec<_>>();
            total += self
                .store
                .materialize(
                    &job.id,
                    CursorUpdate {
                        expected_cursor_us: job.cursor_us,
                        new_cursor_us: now,
                        resolve_one_time: matches!(
                            definition.schedule,
                            Schedule::At { at } if at.epoch_micros() <= now
                        ),
                    },
                    &runs,
                    now,
                )
                .map_err(|e| e.to_string())?
                .inserted;
        }
        Ok(total)
    }
    async fn admit(&self, capacity: usize) -> Result<Vec<AdmittedAttempt>, String> {
        let settings = self.store.settings().map_err(|e| e.to_string())?;
        let execution_path = settings.execution_path.clone();
        let global_retained = self
            .store
            .retained_output_bytes()
            .map_err(|e| e.to_string())?;
        let attempts = self
            .store
            .admit(&self.lifetime, now_us(), capacity)
            .map_err(|e| e.to_string())?
            .attempts;
        let mut runnable = Vec::with_capacity(attempts.len());
        for attempt in attempts {
            let prepared = (|| {
                let definition: JobDefinition =
                    serde_json::from_str(&attempt.snapshot_json).map_err(|e| e.to_string())?;
                let target = engine_target(&definition, &attempt, &execution_path)?;
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
    async fn complete(
        &self,
        attempt: &AdmittedAttempt,
        outcome: &locron_engine::runner::ExecutionOutcome,
    ) -> Result<(), String> {
        let run = self.store.run(&attempt.run_id).map_err(|e| e.to_string())?;
        let definition: JobDefinition =
            serde_json::from_str(&run.snapshot_json).map_err(|e| e.to_string())?;
        let retryable = outcome.kind == OutcomeKind::FailedRetryable
            || (outcome.kind == OutcomeKind::TimedOut && definition.policy.retry_timeout);
        let retry = if retryable && attempt.context.attempt <= u32::from(definition.policy.retries)
        {
            let shift = attempt.context.attempt.saturating_sub(1).min(31);
            let delay = definition
                .policy
                .retry_delay
                .get()
                .saturating_mul(1_u64 << shift)
                .min(definition.policy.retry_cap.get());
            Some(RetryPlan {
                not_before_us: now_us().saturating_add(i64::try_from(delay).unwrap_or(i64::MAX)),
                classification: format!("{:?}", outcome.kind).to_lowercase(),
            })
        } else {
            None
        };
        let state = match outcome.kind {
            OutcomeKind::Succeeded => "succeeded",
            OutcomeKind::TimedOut => "timed_out",
            OutcomeKind::Cancelled => "cancelled",
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
                now_us(),
            )
            .map_err(|e| e.to_string())?;
        self.store
            .complete_attempt(&AttemptCompletion {
                run_id: attempt.run_id.clone(),
                attempt_number: i64::from(attempt.context.attempt),
                now_us: now_us(),
                duration_us: i64::try_from(outcome.duration_micros).unwrap_or(i64::MAX),
                state: state.into(),
                exit_code: outcome.exit_code,
                http_status: outcome.http_status,
                reason: outcome.reason.clone(),
                retry,
            })
            .map_err(|e| e.to_string())
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
            .end_lifetime(&self.lifetime, now_us())
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

fn due_occurrences(
    schedule: &Schedule,
    cursor_us: i64,
    now_us: i64,
    limit: usize,
) -> Result<Vec<Timestamp>, String> {
    if let Schedule::Every { interval, anchor } = schedule {
        let step = i64::try_from(interval.get()).map_err(|_| "interval is too large")?;
        let first_index = cursor_us
            .saturating_sub(anchor.epoch_micros())
            .div_euclid(step)
            .saturating_add(1)
            .max(0);
        let last_index = now_us
            .saturating_sub(anchor.epoch_micros())
            .div_euclid(step);
        if last_index < first_index {
            return Ok(Vec::new());
        }
        let bounded_first =
            first_index.max(last_index.saturating_sub(limit.saturating_sub(1) as i64));
        return (bounded_first..=last_index)
            .map(|index| {
                anchor
                    .epoch_micros()
                    .checked_add(index.saturating_mul(step))
                    .map(Timestamp::from_epoch_micros)
                    .ok_or_else(|| "occurrence time overflow".to_string())
            })
            .collect();
    }
    let mut found = Vec::new();
    let mut cursor = Timestamp::from_epoch_micros(cursor_us);
    // Calendar and one-time schedules are evaluated one occurrence at a time so
    // sparse expressions do not require finding `limit` occurrences up front.
    for _ in 0..=limit {
        let Some(next) = schedule
            .next(cursor, 1)
            .map_err(|error| error.to_string())?
            .first()
            .copied()
        else {
            break;
        };
        if next.epoch_micros() > now_us {
            break;
        }
        found.push(next);
        cursor = next;
    }
    if found.len() > limit {
        found.remove(0);
    }
    Ok(found)
}

fn engine_target(
    definition: &JobDefinition,
    attempt: &locron_store::AdmitAttempt,
    execution_path: &str,
) -> Result<TargetSpec, String> {
    let mut env = minimal_env();
    env.insert(
        "PATH".into(),
        definition
            .environment
            .path
            .clone()
            .unwrap_or_else(|| execution_path.to_owned()),
    );
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
        Target::Process { executable, args } => Ok(TargetSpec::Process(ProcessSpec {
            executable: executable.clone(),
            args: args.clone(),
            cwd: definition.cwd.clone(),
            env,
        })),
        Target::Shell { command, shell } => Ok(TargetSpec::Process(ProcessSpec {
            executable: shell.display().to_string(),
            args: vec!["-c".into(), command.clone()],
            cwd: definition.cwd.clone(),
            env,
        })),
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

fn configured_global_concurrency(paths: &StatePaths) -> Result<u8> {
    if !paths.database.is_file() {
        return Ok(16);
    }
    let value = open_read_only(paths)?.settings()?.global_concurrency;
    u8::try_from(value).context("configured global concurrency is out of range")
}

fn validate_metadata(name: &str, description: Option<&str>, tags: &[String]) -> Result<()> {
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

fn environment_warnings(environment: &Environment) -> Vec<&'static str> {
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
fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| {
            i64::try_from(value.as_micros()).unwrap_or(i64::MAX)
        })
}
fn daemon_lock_free(paths: &StatePaths) -> bool {
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

fn redacted_job(job: JobRecord) -> Result<Value> {
    let mut value = serde_json::to_value(job)?;
    if let Some(definition) = value.get_mut("definition_json") {
        let source = definition.as_str().unwrap_or("{}");
        *definition = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

fn redacted_run(run: RunRecord) -> Result<Value> {
    let mut value = serde_json::to_value(run)?;
    if let Some(snapshot) = value.get_mut("snapshot_json") {
        let source = snapshot.as_str().unwrap_or("{}");
        *snapshot = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

fn redact_definition(mut definition: Value) -> Value {
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
fn error_code(error: &anyhow::Error) -> &'static str {
    if let Some(store) = error.downcast_ref::<StoreError>() {
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
    if let Some(store) = error.downcast_ref::<StoreError>() {
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

    #[test]
    fn interval_catch_up_keeps_newest_bounded_window() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let occurrences = due_occurrences(&schedule, 0, 10_000_000, 3).unwrap();
        assert_eq!(
            occurrences
                .iter()
                .map(|at| at.epoch_micros())
                .collect::<Vec<_>>(),
            [8_000_000, 9_000_000, 10_000_000]
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
        let occurrences =
            due_occurrences(&schedule, cursor.epoch_micros(), now.epoch_micros(), 100).unwrap();
        assert_eq!(occurrences, ["2026-01-01T00:00:00Z".parse().unwrap()]);
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

        let TargetSpec::Http(http) = engine_target(&definition, &attempt, "/usr/bin").unwrap()
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
            engine_target(&missing, &attempt, "/usr/bin")
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
}
