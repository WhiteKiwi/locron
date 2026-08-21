//! `locron` command-line composition root.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use locron_core::command::JobDefinition;
use locron_core::policy::{ExecutionPolicy, MissedRunPolicy, OverlapPolicy};
use locron_core::schedule::{Schedule, ScheduleTimeZone};
use locron_core::target::{Environment, HttpMethod, HttpTarget, Target};
use locron_core::{JobId, SchedulerLifetimeId, Timestamp};
use locron_engine::daemon::{AdmittedAttempt, DaemonStore};
use locron_engine::runner::{OutcomeKind, RunnerConfig};
use locron_engine::{
    AttemptContext, Daemon, DaemonConfig, HttpSpec, OutputWriter, ProcessSpec, Runner, TargetSpec,
};
use locron_store::{
    AdmitAttempt, AttemptCompletion, CreateJob, CursorUpdate, JobRecord, LockMetadata,
    NewScheduledRun, OutputRecord, RetryPlan, RunRecord, StatePaths, Store, StoreError, UpdateJob,
};
use serde::Serialize;
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

#[derive(Args, Debug, Clone)]
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

#[derive(Args, Debug, Clone)]
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
    env_file: Option<PathBuf>,
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args, Debug, Clone)]
struct PolicyArgs {
    #[arg(long, value_enum, default_value = "skip")]
    overlap: OverlapArg,
    #[arg(long, value_enum)]
    missed_run: Option<MissedArg>,
    #[arg(long, default_value_t = 100)]
    catch_up_limit: u16,
    #[arg(long, default_value_t = 0)]
    retries: u8,
    #[arg(long, default_value = "60s")]
    timeout: String,
    #[arg(long)]
    no_timeout: bool,
    #[arg(long)]
    retry_timeout: bool,
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
struct UpdateArgs {
    name: String,
    #[arg(long)]
    rename: Option<String>,
    #[arg(long)]
    description: Option<String>,
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
        Command::Update(args) => update(&paths, args, format),
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
            include_history,
        } => export(&paths, include_values, include_history, format),
        Command::Import {
            path,
            accept_plaintext_values,
            dry_run,
        } => import(&path, accept_plaintext_values, dry_run, format),
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
    let definition = build_definition(&args.schedule, &args.target, &args.policy)?;
    if args.dry_run {
        render(
            format,
            "add",
            json!({"dry_run":true,"normalized":{"name":args.name,"enabled":!args.disabled,"definition":redact_definition(serde_json::to_value(&definition)?)},"id":"<non-durable>"}),
            &[],
        );
        return Ok(());
    }
    let store = open(paths)?;
    let now = now_us();
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
    render(format, "add", redacted_job(record)?, &[]);
    Ok(())
}

fn update(paths: &StatePaths, args: UpdateArgs, format: Format) -> Result<()> {
    let store = if args.dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let current = store.job(&args.name)?;
    let name = args.rename.unwrap_or(current.name.clone());
    let description = args.description.or(current.description.clone());
    if args.dry_run {
        render(
            format,
            "update",
            json!({"dry_run":true,"id":current.id,"revision":current.current_revision+1,"name":name,"description":description}),
            &[],
        );
        return Ok(());
    }
    let now = now_us();
    let record = store.update_job(&UpdateJob {
        id: current.id,
        expected_revision: current.current_revision,
        name,
        description,
        tags_json: current.tags_json,
        enabled: current.enabled,
        definition_json: current.definition_json,
        now_us: now,
        cursor_us: now,
    })?;
    send_wake(paths);
    render(format, "update", redacted_job(record)?, &[]);
    Ok(())
}

fn build_definition(
    schedule: &ScheduleArgs,
    target: &TargetArgs,
    policy: &PolicyArgs,
) -> Result<JobDefinition> {
    let now = Timestamp::from_epoch_micros(now_us());
    let schedule = match (&schedule.cron, &schedule.every, &schedule.at) {
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
        _ => {
            return Err(anyhow!(
                "exactly one of --cron, --every, or --at is required"
            ));
        }
    };
    let target_kind = match (&target.shell, &target.http, target.command.is_empty()) {
        (Some(command), None, true) => Target::Shell {
            command: command.clone(),
            shell: "/bin/sh".into(),
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
        _ => {
            return Err(anyhow!(
                "exactly one of -- COMMAND, --shell, or --http is required"
            ));
        }
    };
    let cwd = match &target.cwd {
        Some(path) => normalize_path(path)?,
        None => std::env::current_dir()?,
    };
    let mut execution = ExecutionPolicy::default();
    execution.overlap = match policy.overlap {
        OverlapArg::Skip => OverlapPolicy::Skip,
        OverlapArg::Replace => OverlapPolicy::Replace,
        OverlapArg::Allow => OverlapPolicy::Allow,
    };
    execution.missed_run = match policy.missed_run {
        Some(MissedArg::Skip) => MissedRunPolicy::Skip,
        Some(MissedArg::Latest) => MissedRunPolicy::Latest,
        Some(MissedArg::All) => MissedRunPolicy::All,
        None if matches!(schedule, Schedule::At { .. }) => MissedRunPolicy::Latest,
        None => MissedRunPolicy::Skip,
    };
    execution.catch_up_limit = policy.catch_up_limit;
    execution.retries = policy.retries;
    execution.timeout = if policy.no_timeout {
        None
    } else {
        Some(policy.timeout.parse()?)
    };
    execution.retry_timeout = policy.retry_timeout;
    execution.per_job_concurrency =
        policy
            .per_job_concurrency
            .unwrap_or(if execution.overlap == OverlapPolicy::Allow {
                2
            } else {
                1
            });
    let definition = JobDefinition {
        schedule,
        target: target_kind,
        cwd,
        environment: Environment {
            file: target
                .env_file
                .clone()
                .map(|path| normalize_path(&path))
                .transpose()?,
            values: target.env.iter().cloned().collect(),
            path: None,
        },
        policy: execution,
    };
    definition.validate(16)?;
    Ok(definition)
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
        http: None,
        cwd: Some(std::env::current_dir()?),
        env: vec![],
        env_file: None,
        command: vec![],
    };
    let policy = PolicyArgs {
        overlap: OverlapArg::Skip,
        missed_run: None,
        catch_up_limit: 100,
        retries: 0,
        timeout: "60s".into(),
        no_timeout: false,
        retry_timeout: false,
        per_job_concurrency: None,
    };
    Ok(build_definition(args, &target, &policy)?.schedule)
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
    include_history: bool,
    format: Format,
) -> Result<()> {
    if include_values {
        return Err(anyhow!(
            "--include-values requires an explicit non-interactive acknowledgement not yet exposed"
        ));
    }
    let store = open(paths)?;
    let history = if include_history {
        json!(
            store
                .history(None, 1000)?
                .into_iter()
                .map(redacted_run)
                .collect::<Result<Vec<_>>>()?
        )
    } else {
        Value::Null
    };
    render(
        format,
        "export",
        json!({"schema":"locron.export/v1","jobs":store.list_jobs(true)?.into_iter().map(redacted_job).collect::<Result<Vec<_>>>()?,"history":history,"values_redacted":true}),
        &[],
    );
    Ok(())
}
fn import(path: &Path, accept: bool, dry_run: bool, format: Format) -> Result<()> {
    let value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    if value.get("schema") != Some(&Value::String("locron.export/v1".into())) {
        return Err(anyhow!("unsupported export schema"));
    }
    if !accept && value.get("values_redacted") == Some(&Value::Bool(false)) {
        return Err(anyhow!(
            "plaintext values require --accept-plaintext-values"
        ));
    }
    if !dry_run {
        return Err(anyhow!(
            "atomic import application is not implemented yet; use --dry-run to validate"
        ));
    }
    render(format, "import", json!({"dry_run":true,"valid":true}), &[]);
    Ok(())
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
            if key.starts_with("LOCRON_") {
                return Err(format!("reserved environment name {key}"));
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
        Target::Http(http) => Ok(TargetSpec::Http(HttpSpec {
            method: http.method.as_str().into(),
            url: http.url.parse().map_err(|e| format!("{e}"))?,
            headers: http.headers.clone(),
            body: match (&http.body, &http.body_file) {
                (Some(body), None) => Some(body.clone()),
                (None, Some(path)) => {
                    Some(std::fs::read(path).map_err(|error| format!("HTTP body file: {error}"))?)
                }
                (None, None) => None,
                (Some(_), Some(_)) => return Err("conflicting HTTP body sources".into()),
            },
            success_statuses: http.success_statuses.clone(),
            follow_redirects: http.follow_redirects,
        })),
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
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    })
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
            *value = Value::String("<redacted>".into());
        }
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
}
