//! `locron service` and `locron dashboard enable|disable|status` — register,
//! unregister, and inspect the per-user daemon and dashboard services.
//!
//! Both services are registered with the operating system's per-user service
//! manager through a small [`ServicePort`]: launchd LaunchAgents on macOS and
//! systemd user units on Linux, both driven as child processes, plus a
//! deterministic fake for tests. The flows themselves ([`install`],
//! [`uninstall`], [`status`]) are manager-independent: idempotent registration,
//! refresh and graceful restart when the service is already loaded, deferral
//! when a manual daemon holds the state lock, and graceful stop-then-cleanup on
//! uninstall.
//!
//! The two registration targets ([`Target::Daemon`] and [`Target::Dashboard`])
//! are independent: each flow addresses exactly one target and never touches
//! the other's registration.
//!
//! Registration always uses the canonicalized path of the running binary, so
//! repeating it repairs a registration whose binary moved or was replaced.

use std::env;
use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use clap::Subcommand;
use locron_server::{Config, PortPolicy};
use locron_store::{DaemonLock, StatePaths, StoreError};
use serde_json::{Value, json};

use crate::{Format, render};

/// launchd label of the registered daemon (kept on all test builds so the
/// plist template tests run everywhere).
#[cfg(any(target_os = "macos", test))]
const DAEMON_LABEL: &str = "dev.locron.daemon";
/// launchd label of the registered dashboard (kept on all test builds so the
/// plist template tests run everywhere).
#[cfg(any(target_os = "macos", test))]
const DASHBOARD_LABEL: &str = "dev.locron.dashboard";
/// Name of the daemon's systemd user unit.
#[cfg(target_os = "linux")]
const DAEMON_UNIT: &str = "locron.service";
/// Name of the dashboard's systemd user unit.
#[cfg(target_os = "linux")]
const DASHBOARD_UNIT: &str = "locron-dashboard.service";
/// Plist file name of the daemon inside `~/Library/LaunchAgents`.
#[cfg(target_os = "macos")]
const DAEMON_PLIST_NAME: &str = "dev.locron.daemon.plist";
/// Plist file name of the dashboard inside `~/Library/LaunchAgents`.
#[cfg(target_os = "macos")]
const DASHBOARD_PLIST_NAME: &str = "dev.locron.dashboard.plist";
/// Log directory inside the home directory (the Homebrew default-log-path convention).
#[cfg(any(target_os = "macos", test))]
const LOG_DIR: &str = "Library/Logs/locron";
/// Log file written by the daemon service.
#[cfg(any(target_os = "macos", test))]
const DAEMON_LOG_FILE: &str = "daemon.log";
/// Log file written by the dashboard service.
#[cfg(any(target_os = "macos", test))]
const DASHBOARD_LOG_FILE: &str = "dashboard.log";
/// How long uninstall waits for a signaled daemon to leave the manager.
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll interval while waiting for the daemon to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The registration targets on the service-manager port.
///
/// Each target carries its own label/unit name, registration file name, log
/// file, service arguments, and deferral behavior; the two registrations are
/// independent and never touch each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// The scheduler daemon (`locron daemon run`).
    Daemon,
    /// The web dashboard (`locron dashboard serve`).
    Dashboard,
}

impl Target {
    /// The name the target carries in the current platform's manager.
    pub(crate) fn service_name(self) -> &'static str {
        match self {
            Target::Daemon => {
                #[cfg(target_os = "macos")]
                {
                    DAEMON_LABEL
                }
                #[cfg(target_os = "linux")]
                {
                    DAEMON_UNIT
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    "locron.service"
                }
            }
            Target::Dashboard => {
                #[cfg(target_os = "macos")]
                {
                    DASHBOARD_LABEL
                }
                #[cfg(target_os = "linux")]
                {
                    DASHBOARD_UNIT
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    "locron-dashboard.service"
                }
            }
        }
    }

    /// The plist file name inside `~/Library/LaunchAgents`.
    #[cfg(target_os = "macos")]
    fn plist_name(self) -> &'static str {
        match self {
            Target::Daemon => DAEMON_PLIST_NAME,
            Target::Dashboard => DASHBOARD_PLIST_NAME,
        }
    }

    /// The log file written by the service.
    #[cfg(any(target_os = "macos", test))]
    fn log_file(self) -> &'static str {
        match self {
            Target::Daemon => DAEMON_LOG_FILE,
            Target::Dashboard => DASHBOARD_LOG_FILE,
        }
    }

    /// The unit description.
    #[cfg(any(target_os = "linux", test))]
    fn description(self) -> &'static str {
        match self {
            Target::Daemon => "locron scheduler daemon",
            Target::Dashboard => "locron web dashboard",
        }
    }

    /// Whether the start may be deferred when a manual daemon holds the state
    /// lock. Only the daemon holds that lock; the dashboard never does.
    fn defers_to_daemon_lock(self) -> bool {
        matches!(self, Target::Daemon)
    }
}

/// Guidance printed when no per-user service-manager session is available.
/// The deb/rpm package postinst prints the same text.
const NO_SESSION_GUIDANCE: &str = "\
The locron daemon was not registered because no per-user service manager session \
is available here (for example an SSH session, a container, or a machine without \
systemd). The daemon stops at logout and starts again at the next login. To \
register and start the daemon after your next interactive login, run: \
'locron service install'. To run the scheduler right now, start the daemon in \
the foreground: 'locron daemon run'. Keeping the daemon running after logout is \
optional: 'loginctl enable-linger'.";

/// Note shown when the start is deferred because a manual daemon holds the lock.
const DEFERRED_GUIDANCE: &str = "\
The daemon service is registered and enabled, but its start is deferred because \
a manually started locron daemon currently holds the state-directory lock. The \
service starts when that daemon exits.";

/// Stable `service` failure categories.
#[derive(Debug)]
pub(crate) enum ServiceError {
    /// The platform has no supported per-user service manager.
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },
    /// A package-manager marker next to the executable refuses service management.
    ManagedInstall,
    /// A service-manager subprocess failed.
    CommandFailed {
        tool: &'static str,
        args: String,
        status: Option<i32>,
        stderr: String,
    },
    /// Filesystem or environment failure.
    Io(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::UnsupportedPlatform { os, arch } => write!(
                formatter,
                "service management is supported on macOS (launchd) and Linux with systemd; \
                 this is a {os}/{arch} build"
            ),
            ServiceError::ManagedInstall => write!(
                formatter,
                "this locron is installed by a package manager and its service is managed by \
                 that package manager; use 'brew services start locron', 'brew services restart \
                 locron', and 'brew services stop locron' instead of 'locron service'"
            ),
            ServiceError::CommandFailed {
                tool,
                args,
                status,
                stderr,
            } => {
                let status = match status {
                    Some(code) => format!("exit status {code}"),
                    None => "no exit status".to_owned(),
                };
                if stderr.trim().is_empty() {
                    write!(formatter, "{tool} {args} failed ({status})")
                } else {
                    write!(
                        formatter,
                        "{tool} {args} failed ({status}): {}",
                        stderr.trim()
                    )
                }
            }
            ServiceError::Io(message) => {
                write!(formatter, "service management failed: {message}")
            }
        }
    }
}

impl StdError for ServiceError {}

const SERVICE_INSTALL_HELP: &str = "\
Examples:
  locron service install

Registers the daemon as a per-user service and starts it. Repeating it is
safe: a loaded service is restarted onto the current binary, and a fresh
registration starts unless a manually started daemon holds the state lock
(the start is then deferred until that daemon exits).

Navigation:
  Run 'locron service --help' for service commands or 'locron --help' for all commands.";
const SERVICE_UNINSTALL_HELP: &str = "\
Examples:
  locron service uninstall

Unregisters the daemon service: it stops a running service gracefully,
removes it from the service manager, and deletes the registration file. The
binary itself is never touched.

Navigation:
  Run 'locron service --help' for service commands or 'locron --help' for all commands.";
const SERVICE_STATUS_HELP: &str = "\
Examples:
  locron service status

Reports whether the daemon is registered, loaded by the service manager, and
(enabled) for the current platform.

Navigation:
  Run 'locron service --help' for service commands or 'locron --help' for all commands.";

/// The `locron service` subcommands.
#[derive(Clone, Copy, Subcommand, Debug)]
pub(crate) enum ServiceCommand {
    #[command(about = "Register and start the daemon as a per-user service", after_help = SERVICE_INSTALL_HELP)]
    Install,
    #[command(about = "Unregister the daemon service", after_help = SERVICE_UNINSTALL_HELP)]
    Uninstall,
    #[command(about = "Report the daemon service registration state", after_help = SERVICE_STATUS_HELP)]
    Status,
}

/// Everything a backend needs to register the running binary as a service.
struct ServiceContext {
    executable: PathBuf,
    home: PathBuf,
    /// The registration target addressed by the flow.
    target: Target,
    /// Launchd addresses the service as `gui/<uid>/<label>`; the systemd user
    /// manager has no per-domain user id, so the field is dead on Linux.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    uid: u32,
    paths: Option<StatePaths>,
}

impl ServiceContext {
    fn new(state_dir: Option<PathBuf>, target: Target) -> Result<Self, ServiceError> {
        let executable = canonical_executable()?;
        let home = env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            ServiceError::Io("cannot determine the home directory (HOME is unset)".to_owned())
        })?;
        let uid = resolve_uid()?;
        // The lock probe needs the state paths, but the service commands also
        // run on machines with no state directory yet; a missing state
        // directory simply means no daemon can be holding the lock.
        let paths = match StatePaths::discover(state_dir.as_deref()) {
            Ok(paths) => Some(paths),
            Err(_) => state_dir.map(StatePaths::new),
        };
        Ok(Self {
            executable,
            home,
            target,
            uid,
            paths,
        })
    }
}

/// The small service-manager port shared by the launchd, systemd-user, and fake
/// backends. Every method drives the real manager as a child process; nothing
/// here depends on a shell.
trait ServicePort {
    /// True when a per-user service-manager session is available here.
    fn session_available(&self, ctx: &ServiceContext) -> Result<bool, ServiceError>;
    /// Write the registration file (plist or unit) for the current executable.
    fn write_registration(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// Remove the registration file; true when a file was actually removed.
    fn remove_registration(&self, ctx: &ServiceContext) -> Result<bool, ServiceError>;
    /// Refresh the manager's view of its registration files.
    fn reload(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// True when the service is loaded by the manager.
    fn is_loaded(&self, ctx: &ServiceContext) -> Result<bool, ServiceError>;
    /// Enable the service without starting it (start deferred or later).
    fn enable(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// Start the service; reports the domain it landed in when the manager has
    /// domains.
    fn start(&self, ctx: &ServiceContext) -> Result<StartedService, ServiceError>;
    /// Gracefully stop a loaded service.
    fn stop(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// Restart a loaded service onto the current registration (graceful
    /// signal, never a kill of in-flight work).
    fn restart(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// Unload the service from the manager without touching the registration file.
    fn unload(&self, ctx: &ServiceContext) -> Result<(), ServiceError>;
    /// Report the current registration and loaded state.
    fn status(&self, ctx: &ServiceContext) -> Result<ServiceStatus, ServiceError>;
}

/// The domain a started service lives in, when the manager has domains.
struct StartedService {
    domain: Option<String>,
}

/// Status fields common to both backends; platform-specific fields stay null
/// on the platform that cannot report them.
#[derive(Debug, Default)]
struct ServiceStatus {
    registered: bool,
    loaded: bool,
    enabled: Option<bool>,
    domain: Option<String>,
    pid: Option<u32>,
    executable: Option<String>,
    session_available: bool,
}

/// Outcome of a successful `service install`.
#[derive(Debug)]
struct InstallOutcome {
    registered: bool,
    restarted: bool,
    deferred: bool,
    domain: Option<String>,
    guidance: Option<String>,
}

/// Outcome of a successful `service uninstall`.
#[derive(Debug)]
struct UninstallOutcome {
    removed: bool,
    stopped: bool,
}

/// `locron service install`: write the registration and make the daemon run.
///
/// Repeating an install is safe: a loaded service is refreshed and restarted
/// onto the current binary; an unloaded service is enabled and started unless
/// a manual daemon holds the state lock, in which case the start is deferred.
fn install(ctx: &ServiceContext, port: &dyn ServicePort) -> Result<InstallOutcome, ServiceError> {
    refuse_managed_install(ctx)?;
    if !port.session_available(ctx)? {
        return Ok(InstallOutcome {
            registered: false,
            restarted: false,
            deferred: false,
            domain: None,
            guidance: Some(NO_SESSION_GUIDANCE.to_owned()),
        });
    }
    port.write_registration(ctx)?;
    port.reload(ctx)?;
    if port.is_loaded(ctx)? {
        port.restart(ctx)?;
        let domain = port.status(ctx)?.domain;
        return Ok(InstallOutcome {
            registered: true,
            restarted: true,
            deferred: false,
            domain,
            guidance: None,
        });
    }
    // Only the daemon defers to the daemon lock: the dashboard holds no lock,
    // so a manually started daemon never defers its start.
    let lock_held = if ctx.target.defers_to_daemon_lock() {
        match &ctx.paths {
            Some(paths) => daemon_lock_held(paths)?,
            None => false,
        }
    } else {
        false
    };
    port.enable(ctx)?;
    if lock_held {
        return Ok(InstallOutcome {
            registered: true,
            restarted: false,
            deferred: true,
            domain: None,
            guidance: Some(DEFERRED_GUIDANCE.to_owned()),
        });
    }
    let started = port.start(ctx)?;
    Ok(InstallOutcome {
        registered: true,
        restarted: false,
        deferred: false,
        domain: started.domain,
        guidance: None,
    })
}

/// `locron service uninstall`: stop a loaded service gracefully, unload it,
/// and remove the registration. The binary itself is never touched.
fn uninstall(
    ctx: &ServiceContext,
    port: &dyn ServicePort,
) -> Result<UninstallOutcome, ServiceError> {
    refuse_managed_install(ctx)?;
    let session = port.session_available(ctx)?;
    let mut stopped = false;
    if session {
        if port.is_loaded(ctx)? {
            // The pid of the process about to be signaled, so the wait can
            // recognize a KeepAlive respawn as "the old process is gone".
            let original_pid = port.status(ctx)?.pid;
            port.stop(ctx)?;
            wait_until_stopped(ctx, port, original_pid)?;
            stopped = true;
        }
        port.unload(ctx)?;
    }
    let removed = port.remove_registration(ctx)?;
    if session {
        port.reload(ctx)?;
    }
    Ok(UninstallOutcome { removed, stopped })
}

/// `locron service status`: report the registration and loaded state.
fn status(ctx: &ServiceContext, port: &dyn ServicePort) -> Result<ServiceStatus, ServiceError> {
    port.status(ctx)
}

/// Refuse install and uninstall on package-manager-managed binaries: the
/// package manager owns the service lifecycle there.
fn refuse_managed_install(ctx: &ServiceContext) -> Result<(), ServiceError> {
    if managed_marker_path(&ctx.executable).exists() {
        return Err(ServiceError::ManagedInstall);
    }
    Ok(())
}

/// The package-manager marker next to a canonicalized executable.
fn managed_marker_path(executable: &Path) -> PathBuf {
    executable
        .parent()
        .unwrap_or_else(|| Path::new("/"))
        .join("../lib/.disable-self-update")
}

/// The canonicalized absolute path of the running binary.
fn canonical_executable() -> Result<PathBuf, ServiceError> {
    let executable = env::current_exe().map_err(|error| {
        ServiceError::Io(format!("cannot locate the current executable: {error}"))
    })?;
    fs::canonicalize(&executable)
        .map_err(|error| ServiceError::Io(format!("cannot locate the current executable: {error}")))
}

/// Resolve the real user id through `id -u` (no libc dependency in this crate).
fn resolve_uid() -> Result<u32, ServiceError> {
    let output =
        Command::new("id")
            .arg("-u")
            .output()
            .map_err(|error| ServiceError::CommandFailed {
                tool: "id",
                args: "-u".to_owned(),
                status: None,
                stderr: error.to_string(),
            })?;
    if !output.status.success() {
        return Err(ServiceError::CommandFailed {
            tool: "id",
            args: "-u".to_owned(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| ServiceError::Io("cannot determine the user id (id -u)".to_owned()))
}

/// True when a daemon currently holds the state-directory lock. A missing
/// state directory means no daemon can hold it.
fn daemon_lock_held(paths: &StatePaths) -> Result<bool, ServiceError> {
    match DaemonLock::try_prove_free(&paths.daemon_lock) {
        Ok(()) => Ok(false),
        Err(StoreError::MigrationRequiresDaemonRestart) => Ok(true),
        Err(StoreError::DaemonAlreadyRunning) => Ok(true),
        Err(StoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ServiceError::Io(format!(
            "cannot probe the daemon state lock: {error}"
        ))),
    }
}

/// Wait (bounded) for the signaled daemon process to exit. The job itself may
/// stay loaded (launchd KeepAlive keeps it in the domain and respawns it), so
/// the wait watches the process: it is gone when no pid is reported or when a
/// different process replaced it. A service that ignores the signal is
/// unloaded anyway by the caller as cleanup.
fn wait_until_stopped(
    ctx: &ServiceContext,
    port: &dyn ServicePort,
    original_pid: Option<u32>,
) -> Result<(), ServiceError> {
    let deadline = Instant::now() + UNLOAD_TIMEOUT;
    while Instant::now() < deadline {
        let pid = port.status(ctx)?.pid;
        if pid.is_none() || (original_pid.is_some() && pid != original_pid) {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

/// Select the service-manager backend for this platform. `LOCRON_SERVICE_BACKEND`
/// (auto|launchd|systemd|fake) forces a backend for tests; `LOCRON_SERVICE_FAKE_STATE`
/// and `LOCRON_SERVICE_FAKE_LOG` configure the deterministic fake.
fn select_port() -> Result<Box<dyn ServicePort>, ServiceError> {
    #[cfg(target_os = "macos")]
    use launchd::LaunchdPort;
    #[cfg(target_os = "linux")]
    use systemd::SystemdPort;

    match env::var("LOCRON_SERVICE_BACKEND").as_deref() {
        Ok("fake") => {
            return FakeServicePort::from_env().map(|port| Box::new(port) as Box<dyn ServicePort>);
        }
        Ok("launchd") if cfg!(target_os = "macos") => {}
        Ok("systemd") if cfg!(target_os = "linux") => {}
        Ok(_) => {
            return Err(ServiceError::UnsupportedPlatform {
                os: env::consts::OS,
                arch: env::consts::ARCH,
            });
        }
        _ => {}
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(LaunchdPort))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(SystemdPort))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(ServiceError::UnsupportedPlatform {
            os: env::consts::OS,
            arch: env::consts::ARCH,
        })
    }
}

/// Run the requested service subcommand and render its result.
pub(crate) fn execute(
    state_dir: Option<PathBuf>,
    command: ServiceCommand,
    format: Format,
) -> Result<()> {
    let port = select_port()?;
    let ctx = ServiceContext::new(state_dir, Target::Daemon)?;
    match command {
        ServiceCommand::Install => {
            let outcome = install(&ctx, port.as_ref())?;
            render_install(format, Target::Daemon, "service install", &outcome);
        }
        ServiceCommand::Uninstall => {
            let outcome = uninstall(&ctx, port.as_ref())?;
            render_uninstall(format, Target::Daemon, "service uninstall", &outcome);
        }
        ServiceCommand::Status => {
            let outcome = status(&ctx, port.as_ref())?;
            render_status(format, Target::Daemon, "service status", &outcome);
        }
    }
    Ok(())
}

const DASHBOARD_ENABLE_HELP: &str = "\
Examples:
  locron dashboard enable
  locron dashboard enable --reset

Registers the dashboard as a per-user service and starts it, generating the
access token when absent. Repeating it refreshes and repairs the registration.
--reset regenerates the token first, then refreshes and restarts the service,
invalidating the old token and any outstanding session cookies.

Navigation:
  Run 'locron dashboard --help' for dashboard commands or 'locron --help' for all commands.";
const DASHBOARD_DISABLE_HELP: &str = "\
Examples:
  locron dashboard disable

Unregisters the dashboard service: it stops a running service gracefully,
removes it from the service manager, deletes the registration, and removes the
access token. A foreground dashboard the user started themselves is never
stopped; a warning names it.

Navigation:
  Run 'locron dashboard --help' for dashboard commands or 'locron --help' for all commands.";
const DASHBOARD_STATUS_HELP: &str = "\
Examples:
  locron dashboard status

Reports whether the dashboard is registered, loaded by the service manager,
and (enabled) for the current platform, together with the access URL and
access-token facts (presence and file-permission posture only, never the
token value).

Navigation:
  Run 'locron dashboard --help' for dashboard commands or 'locron --help' for all commands.";
const DASHBOARD_SERVE_HELP: &str = "\
Examples:
  locron dashboard
  locron dashboard --port 9000 --bind 127.0.0.1

Serves the web dashboard in the foreground: binds loopback only, prints the
access URL, and serves until a signal. The bare 'locron dashboard' form is
identical. An occupied default port falls back to the next free port when
run by the user and fails in the explicitly marked registered-service mode,
where the fixed address must never move;
an explicit --port is always strict. The --bind option accepts only the
loopback addresses 127.0.0.1 and ::1; any other value is refused.

Navigation:
  Run 'locron dashboard --help' for dashboard commands or 'locron --help' for all commands.";
const DASHBOARD_TOKEN_HELP: &str = "\
Examples:
  locron dashboard token

Re-displays the 64-character access token stored in the state directory,
generating it when absent. This is the only output that shows the token
value (besides the first-run foreground line); every other surface reports
only token facts. The token is accepted by the entry-page paste box and the
Authorization: token header.

Navigation:
  Run 'locron dashboard --help' for dashboard commands or 'locron --help' for all commands.";

/// The `locron dashboard` subcommands. The bare `locron dashboard` form (no
/// subcommand) serves in the foreground, identical to [`DashboardCommand::Serve`].
#[derive(Clone, Copy, Subcommand, Debug)]
pub(crate) enum DashboardCommand {
    #[command(about = "Run the dashboard server in the foreground", after_help = DASHBOARD_SERVE_HELP)]
    Serve {
        /// Internal marker used only by the registered per-user service
        #[arg(long, hide = true)]
        service_mode: bool,
    },
    #[command(about = "Register and start the dashboard as a per-user service", after_help = DASHBOARD_ENABLE_HELP)]
    Enable {
        /// Regenerate the access token, then refresh and restart the service
        #[arg(long)]
        reset: bool,
    },
    #[command(about = "Unregister the dashboard service and remove the access token", after_help = DASHBOARD_DISABLE_HELP)]
    Disable,
    #[command(about = "Report the dashboard service registration state and access facts", after_help = DASHBOARD_STATUS_HELP)]
    Status,
    #[command(about = "Display the dashboard access token", after_help = DASHBOARD_TOKEN_HELP)]
    Token,
}

/// The fixed access URL of the service-mode dashboard.
fn access_url() -> String {
    format!("http://127.0.0.1:{}/", locron_server::DEFAULT_PORT)
}

/// The state paths the dashboard flows operate on; a service context without
/// paths cannot host the access token.
fn dashboard_paths(ctx: &ServiceContext) -> Result<&StatePaths, ServiceError> {
    ctx.paths
        .as_ref()
        .ok_or_else(|| ServiceError::Io("the dashboard state directory is unavailable".to_owned()))
}

/// Access-token facts for `dashboard status`: presence and permission posture
/// only — the token value never leaves the token file.
fn token_facts(paths: &StatePaths) -> Result<Value, ServiceError> {
    let path = locron_server::token::token_path(paths);
    match std::fs::metadata(&path) {
        Ok(metadata) => {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = 0o600;
            Ok(json!({
                "present": true,
                "permissions": if mode.trailing_zeros() >= 6 { "owner_only" } else { "world_readable" },
            }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "present": false, "permissions": "missing" }))
        }
        Err(error) => Err(ServiceError::Io(format!(
            "cannot inspect the access token file {}: {error}",
            path.display()
        ))),
    }
}

/// True when something still accepts connections on the dashboard port — the
/// fingerprint of a foreground `locron dashboard` the user started themselves.
fn foreground_listener_present() -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, locron_server::DEFAULT_PORT)),
        Duration::from_millis(200),
    )
    .is_ok()
}

/// `locron dashboard enable [--reset]`: ensure (or regenerate) the access
/// token, then register/refresh/start the dashboard service. The dashboard
/// holds no daemon lock, so the start never defers to a manual daemon.
fn dashboard_enable(
    ctx: &ServiceContext,
    port: &dyn ServicePort,
    reset: bool,
) -> Result<InstallOutcome, ServiceError> {
    refuse_managed_install(ctx)?;
    let paths = dashboard_paths(ctx)?;
    if reset {
        locron_server::token::regenerate(paths).map_err(|error| {
            ServiceError::Io(format!("cannot regenerate the access token: {error}"))
        })?;
    } else {
        locron_server::token::ensure(paths).map_err(|error| {
            ServiceError::Io(format!("cannot ensure the access token: {error}"))
        })?;
    }
    install(ctx, port)
}

/// `locron dashboard disable`: unregister the service, then remove the access
/// token, warning when a foreground instance may still be running.
fn dashboard_disable(
    ctx: &ServiceContext,
    port: &dyn ServicePort,
) -> Result<(UninstallOutcome, Option<String>), ServiceError> {
    refuse_managed_install(ctx)?;
    let outcome = uninstall(ctx, port)?;
    let paths = dashboard_paths(ctx)?;
    locron_server::token::remove(paths)
        .map_err(|error| ServiceError::Io(format!("cannot remove the access token: {error}")))?;
    let guidance = foreground_listener_present().then(|| {
        "a foreground dashboard still listens on http://127.0.0.1:10824/; \
             stop it yourself (Ctrl-C on its terminal)"
            .to_owned()
    });
    Ok((outcome, guidance))
}

/// `locron dashboard status`: report the registration state, the access URL,
/// and token facts.
fn dashboard_status(
    ctx: &ServiceContext,
    port: &dyn ServicePort,
) -> Result<(ServiceStatus, Value), ServiceError> {
    let outcome = status(ctx, port)?;
    let paths = dashboard_paths(ctx)?;
    let token = token_facts(paths)?;
    Ok((outcome, token))
}

/// The port policy for foreground serving: an explicit `--port` is always
/// strict; without one, a user invocation falls back to the next free port
/// while the explicit registered-service mode keeps the fixed default so the
/// bookmarked address never moves.
fn port_policy(explicit_port: Option<u16>, service_mode: bool) -> PortPolicy {
    if explicit_port.is_some() || service_mode {
        PortPolicy::Fixed
    } else {
        PortPolicy::Foreground
    }
}

/// Validate the `--bind` values: only the loopback literals `127.0.0.1` and
/// `::1` are accepted (comma separated), matching the fixed loopback-only
/// contract; anything else is a stable usage error.
fn parse_bind(bind: Option<&str>) -> Result<Vec<String>, String> {
    let Some(bind) = bind else {
        return Ok(vec!["127.0.0.1".to_owned(), "::1".to_owned()]);
    };
    let addresses = bind
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("--bind requires at least one loopback address (127.0.0.1, ::1)".to_owned());
    }
    for address in &addresses {
        if !matches!(*address, "127.0.0.1" | "::1") {
            return Err(format!(
                "--bind accepts only the loopback addresses 127.0.0.1 and ::1; refused {address}"
            ));
        }
    }
    Ok(addresses.into_iter().map(str::to_owned).collect())
}

/// The state paths for foreground serving, tolerating a missing platform
/// default only when an explicit state directory was given.
fn serve_paths(state_dir: Option<PathBuf>) -> Result<StatePaths, ServiceError> {
    match StatePaths::discover(state_dir.as_deref()) {
        Ok(paths) => Ok(paths),
        Err(_) => state_dir
            .map(StatePaths::new)
            .ok_or_else(|| ServiceError::Io("cannot determine the state directory".to_owned())),
    }
}

/// `locron dashboard` / `locron dashboard serve`: bind loopback, print the
/// access URL, then serve until a signal.
async fn foreground_serve(
    state_dir: Option<PathBuf>,
    port_arg: Option<u16>,
    bind_arg: Option<String>,
    service_mode: bool,
    format: Format,
) -> Result<()> {
    let paths = serve_paths(state_dir)?;
    let bind = parse_bind(bind_arg.as_deref()).map_err(|message| anyhow!(message))?;
    let config = Config {
        bind,
        port: port_arg,
        port_policy: port_policy(port_arg, service_mode),
        token_file: locron_server::token::TOKEN_FILE_NAME.into(),
    };
    let bound = locron_server::bind(&config)
        .await
        .map_err(|error| ServiceError::Io(format!("cannot bind the dashboard server: {error}")))?;
    let generated = !locron_server::token::token_path(&paths).exists();
    let token = locron_server::token::ensure(&paths)
        .map_err(|error| ServiceError::Io(format!("cannot ensure the access token: {error}")))?;
    let url = format!("http://{}/", bound.address);
    match format {
        Format::Human => {
            println!("Dashboard URL: {url}");
            for warning in &bound.warnings {
                eprintln!("warning: {warning}");
            }
            if generated {
                println!(
                    "Access token (newly generated; the entry-page paste box accepts it): {token}"
                );
            }
        }
        Format::Json => {
            let mut token_data = token_facts(&paths)?;
            token_data["generated"] = json!(generated);
            let warnings = bound
                .warnings
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            render(
                format,
                "dashboard",
                json!({
                    "access_url": url,
                    "token": token_data,
                }),
                &warnings,
            );
        }
    }
    locron_server::serve(bound, paths)
        .await
        .map_err(|error| ServiceError::Io(format!("the dashboard server failed: {error}")))?;
    Ok(())
}

/// `locron dashboard token`: ensure and re-display the access token — the only
/// output that shows the token value.
fn dashboard_token(state_dir: Option<PathBuf>, format: Format) -> Result<(), ServiceError> {
    let ctx = ServiceContext::new(state_dir, Target::Dashboard)?;
    let paths = dashboard_paths(&ctx)?;
    let token = locron_server::token::ensure(paths)
        .map_err(|error| ServiceError::Io(format!("cannot read the access token: {error}")))?;
    render(
        format,
        "dashboard token",
        json!({
            "access_url": access_url(),
            "token": token,
        }),
        &[],
    );
    Ok(())
}

/// Dashboard exposure facts for `locron doctor`: token posture and whether a
/// dashboard service is registered (never the token value).
pub(crate) fn dashboard_doctor_facts(state_dir: Option<PathBuf>) -> Result<Value, ServiceError> {
    let ctx = ServiceContext::new(state_dir, Target::Dashboard)?;
    let paths = dashboard_paths(&ctx)?;
    let token = token_facts(paths)?;
    let outcome = status(&ctx, select_port()?.as_ref())?;
    Ok(json!({
        "access_url": access_url(),
        "token": token,
        "registered": outcome.registered,
        "loaded": outcome.loaded,
    }))
}

/// Run the requested dashboard subcommand and render its result. The bare
/// `locron dashboard` form (no subcommand) serves in the foreground.
pub(crate) async fn execute_dashboard(
    state_dir: Option<PathBuf>,
    port_arg: Option<u16>,
    bind_arg: Option<String>,
    command: Option<DashboardCommand>,
    format: Format,
) -> Result<()> {
    match command {
        None => foreground_serve(state_dir, port_arg, bind_arg, false, format).await,
        Some(DashboardCommand::Serve { service_mode }) => {
            foreground_serve(state_dir, port_arg, bind_arg, service_mode, format).await
        }
        Some(DashboardCommand::Token) => dashboard_token(state_dir, format).map_err(Into::into),
        Some(
            DashboardCommand::Enable { .. } | DashboardCommand::Disable | DashboardCommand::Status,
        ) => {
            if port_arg.is_some() || bind_arg.is_some() {
                return Err(anyhow!(
                    "the --port and --bind options apply only to foreground serving; \
                     run 'locron dashboard --port N' without a subcommand"
                ));
            }
            let port = select_port()?;
            let ctx = ServiceContext::new(state_dir, Target::Dashboard)?;
            match command {
                Some(DashboardCommand::Enable { reset }) => {
                    let outcome = dashboard_enable(&ctx, port.as_ref(), reset)?;
                    render_install(format, Target::Dashboard, "dashboard enable", &outcome);
                }
                Some(DashboardCommand::Disable) => {
                    let (outcome, guidance) = dashboard_disable(&ctx, port.as_ref())?;
                    let mut data = json!({
                        "removed": outcome.removed,
                        "stopped": outcome.stopped,
                        "token_removed": true,
                        "service_name": Target::Dashboard.service_name(),
                    });
                    if let Some(guidance) = guidance {
                        data["guidance"] = json!(guidance);
                        if format == Format::Human {
                            eprintln!("\n{guidance}");
                        }
                    }
                    render(format, "dashboard disable", data, &[]);
                }
                Some(DashboardCommand::Status) => {
                    let (outcome, token) = dashboard_status(&ctx, port.as_ref())?;
                    let mut data = json!({
                        "registered": outcome.registered,
                        "loaded": outcome.loaded,
                        "enabled": outcome.enabled,
                        "domain": outcome.domain,
                        "pid": outcome.pid,
                        "executable": outcome.executable,
                        "session_available": outcome.session_available,
                        "service_name": Target::Dashboard.service_name(),
                        "access_url": access_url(),
                        "token": token,
                    });
                    if !outcome.loaded && outcome.registered {
                        data["guidance"] = json!(
                            "the service is registered but not running; an occupied port at the access URL or a stopped service are the usual causes"
                        );
                        if format == Format::Human {
                            eprintln!("\n{}", data["guidance"]);
                        }
                    }
                    render(format, "dashboard status", data, &[]);
                }
                _ => unreachable!("matched a service-management dashboard command"),
            }
            Ok(())
        }
    }
}

fn render_install(format: Format, target: Target, command: &str, outcome: &InstallOutcome) {
    let mut data = json!({
        "registered": outcome.registered,
        "restarted": outcome.restarted,
        "deferred": outcome.deferred,
        "service_name": target.service_name(),
    });
    if let Some(domain) = &outcome.domain {
        data["domain"] = json!(domain);
    }
    if let Some(guidance) = &outcome.guidance {
        data["guidance"] = json!(guidance);
        if format == Format::Human {
            eprintln!("\n{guidance}");
        }
    }
    render(format, command, data, &[]);
}

fn render_uninstall(format: Format, target: Target, command: &str, outcome: &UninstallOutcome) {
    render(
        format,
        command,
        json!({
            "removed": outcome.removed,
            "stopped": outcome.stopped,
            "service_name": target.service_name(),
        }),
        &[],
    );
}

fn render_status(format: Format, target: Target, command: &str, outcome: &ServiceStatus) {
    render(
        format,
        command,
        json!({
            "registered": outcome.registered,
            "loaded": outcome.loaded,
            "enabled": outcome.enabled,
            "domain": outcome.domain,
            "pid": outcome.pid,
            "executable": outcome.executable,
            "session_available": outcome.session_available,
            "service_name": target.service_name(),
        }),
        &[],
    );
}

/// Write a registration file owned by the user with 0644 permissions (launchd
/// and systemd both refuse registration files writable by group or others).
fn write_private(path: &Path, content: &[u8]) -> Result<(), ServiceError> {
    let mut file = fs::File::create(path)
        .map_err(|error| ServiceError::Io(format!("cannot write {}: {error}", path.display())))?;
    file.write_all(content)
        .map_err(|error| ServiceError::Io(format!("cannot write {}: {error}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o644);
        fs::set_permissions(path, permissions).map_err(|error| {
            ServiceError::Io(format!("cannot write {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

/// XML-escape a value for embedding in the plist (kept on all test builds so
/// the plist template tests run everywhere).
#[cfg(any(target_os = "macos", test))]
fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Render the LaunchAgent plist for the canonicalized binary and target (kept
/// on all test builds so the plist template tests run everywhere).
#[cfg(any(target_os = "macos", test))]
fn render_plist(ctx: &ServiceContext) -> Result<String, ServiceError> {
    let label = ctx.target.service_name();
    let executable = escape_xml(&ctx.executable.display().to_string());
    let log = escape_xml(
        &ctx.home
            .join(LOG_DIR)
            .join(ctx.target.log_file())
            .display()
            .to_string(),
    );
    let arguments = match ctx.target {
        Target::Daemon => "    <string>daemon</string>\n    <string>run</string>".to_owned(),
        Target::Dashboard => {
            let state_dir = escape_xml(&dashboard_paths(ctx)?.root.display().to_string());
            format!(
                "    <string>--state-dir</string>\n    <string>{state_dir}</string>\n    <string>dashboard</string>\n    <string>serve</string>\n    <string>--service-mode</string>"
            )
        }
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
{arguments}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#,
    ))
}

/// Escape a path for use inside a double-quoted systemd `ExecStart` argument:
/// systemd performs backslash, dollar, backtick, and specifier expansion.
#[cfg(any(target_os = "linux", test))]
fn escape_systemd_path(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len() + 8);
    for ch in path.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '`' => escaped.push_str("\\`"),
            '%' => escaped.push_str("%%"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Render the systemd user unit for the canonicalized binary and target.
#[cfg(any(target_os = "linux", test))]
fn render_unit(ctx: &ServiceContext) -> Result<String, ServiceError> {
    let executable = escape_systemd_path(&ctx.executable.display().to_string());
    let arguments = match ctx.target {
        Target::Daemon => "daemon run".to_owned(),
        Target::Dashboard => {
            let state_dir = escape_systemd_path(&dashboard_paths(ctx)?.root.display().to_string());
            format!("--state-dir \"{state_dir}\" dashboard serve --service-mode")
        }
    };
    Ok(format!(
        r#"# Managed by 'locron service install' / 'locron dashboard enable'; remove with
# 'locron service uninstall' / 'locron dashboard disable'.
[Unit]
Description={description}

[Service]
ExecStart="{executable}" {arguments}
Restart=on-failure
RestartSec=1

[Install]
WantedBy=default.target
"#,
        description = ctx.target.description(),
    ))
}

#[cfg(target_os = "macos")]
mod launchd {
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};

    use super::{
        LOG_DIR, ServiceContext, ServiceError, ServicePort, ServiceStatus, StartedService,
        render_plist, write_private,
    };

    /// The launchd backend: `enable`, `bootstrap`, `print`, `kill`, and
    /// `bootout` in the `gui/<uid>` domain with a `user/<uid>` fallback for
    /// sessions without a GUI (for example SSH).
    pub(crate) struct LaunchdPort;

    fn domains(ctx: &ServiceContext) -> [String; 2] {
        [format!("gui/{}", ctx.uid), format!("user/{}", ctx.uid)]
    }

    fn plist_path(ctx: &ServiceContext) -> PathBuf {
        ctx.home
            .join("Library/LaunchAgents")
            .join(ctx.target.plist_name())
    }

    fn launchctl(args: &[&str]) -> Result<Output, ServiceError> {
        let output = Command::new("launchctl")
            .args(args)
            .output()
            .map_err(|error| ServiceError::CommandFailed {
                tool: "launchctl",
                args: args.join(" "),
                status: None,
                stderr: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(ServiceError::CommandFailed {
                tool: "launchctl",
                args: args.join(" "),
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output)
    }

    /// Run launchctl and report success without surfacing the error.
    fn launchctl_ok(args: &[&str]) -> bool {
        Command::new("launchctl")
            .args(args)
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// `launchctl kill` and `bootout` fail with exit 3 when the job has no
    /// process to signal (KeepAlive respawn window) or has already left the
    /// domain; both are the state the caller wants, so exit 3 is tolerated.
    fn launchctl_ok_or_absent(args: &[&str]) -> Result<(), ServiceError> {
        match launchctl(args) {
            Ok(_) => Ok(()),
            Err(ServiceError::CommandFailed {
                tool: "launchctl",
                status: Some(3),
                ..
            }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// The domain the service is currently loaded in, if any.
    fn loaded_domain(ctx: &ServiceContext) -> Option<String> {
        for domain in domains(ctx) {
            let target = format!("{domain}/{}", ctx.target.service_name());
            if launchctl_ok(&["print", &target]) {
                return Some(domain);
            }
        }
        None
    }

    impl ServicePort for LaunchdPort {
        fn session_available(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
            // launchd is always present per-user on macOS; a GUI-less session
            // is handled by the domain fallback, not by skipping registration.
            Ok(true)
        }

        fn write_registration(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            let directory = ctx.home.join("Library/LaunchAgents");
            fs::create_dir_all(&directory).map_err(|error| {
                ServiceError::Io(format!("cannot create {}: {error}", directory.display()))
            })?;
            let log_directory = ctx.home.join(LOG_DIR);
            fs::create_dir_all(&log_directory).map_err(|error| {
                ServiceError::Io(format!(
                    "cannot create {}: {error}",
                    log_directory.display()
                ))
            })?;
            let plist = render_plist(ctx)?;
            write_private(&plist_path(ctx), plist.as_bytes())
        }

        fn remove_registration(&self, ctx: &ServiceContext) -> Result<bool, ServiceError> {
            let path = plist_path(ctx);
            match fs::remove_file(&path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(ServiceError::Io(format!(
                    "cannot remove {}: {error}",
                    path.display()
                ))),
            }
        }

        fn reload(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
            // launchd reads the plist at bootstrap; no manager reload exists.
            Ok(())
        }

        fn is_loaded(&self, ctx: &ServiceContext) -> Result<bool, ServiceError> {
            Ok(loaded_domain(ctx).is_some())
        }

        fn enable(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            // Enable in the gui domain; the fallback bootstrap keeps the user
            // domain consistent when the gui domain is unavailable.
            let target = format!("gui/{}/{}", ctx.uid, ctx.target.service_name());
            launchctl(&["enable", &target]).map(|_| ())
        }

        fn start(&self, ctx: &ServiceContext) -> Result<StartedService, ServiceError> {
            let plist = plist_path(ctx);
            let plist = plist.to_string_lossy();
            let mut last_error = None;
            for domain in domains(ctx) {
                match launchctl(&["bootstrap", &domain, &plist]) {
                    Ok(_) => {
                        return Ok(StartedService {
                            domain: Some(domain),
                        });
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            Err(last_error
                .unwrap_or_else(|| ServiceError::Io("launchctl bootstrap failed".to_owned())))
        }

        fn stop(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            let Some(domain) = loaded_domain(ctx) else {
                return Ok(());
            };
            let target = format!("{domain}/{}", ctx.target.service_name());
            launchctl_ok_or_absent(&["kill", "SIGTERM", &target])
        }

        fn restart(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            // SIGTERM; KeepAlive relaunches the job onto the refreshed plist.
            self.stop(ctx)
        }

        fn unload(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            let Some(domain) = loaded_domain(ctx) else {
                return Ok(());
            };
            let target = format!("{domain}/{}", ctx.target.service_name());
            launchctl_ok_or_absent(&["bootout", &target])
        }

        fn status(&self, ctx: &ServiceContext) -> Result<ServiceStatus, ServiceError> {
            let mut status = ServiceStatus {
                registered: plist_path(ctx).exists(),
                session_available: true,
                ..Default::default()
            };
            let Some(domain) = loaded_domain(ctx) else {
                return Ok(status);
            };
            let target = format!("{domain}/{}", ctx.target.service_name());
            let output = launchctl(&["print", &target])?;
            let output = String::from_utf8_lossy(&output.stdout);
            status.loaded = true;
            status.domain = Some(domain);
            status.pid = output.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("pid = ")?.trim().parse().ok()
            });
            status.executable = output.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("program = ").map(str::to_owned)
            });
            Ok(status)
        }
    }
}

#[cfg(target_os = "linux")]
mod systemd {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};

    use super::{
        ServiceContext, ServiceError, ServicePort, ServiceStatus, StartedService, render_unit,
        write_private,
    };

    /// The systemd-user backend: `daemon-reload`, `enable --now`, `stop`,
    /// `disable`, and `is-active`/`is-enabled` against the user manager.
    pub(crate) struct SystemdPort;

    fn unit_dir(ctx: &ServiceContext) -> PathBuf {
        let config = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map_or_else(|| ctx.home.join(".config"), PathBuf::from);
        config.join("systemd").join("user")
    }

    fn unit_path(ctx: &ServiceContext) -> PathBuf {
        unit_dir(ctx).join(ctx.target.service_name())
    }

    fn systemctl(args: &[&str]) -> Result<Output, ServiceError> {
        let output = Command::new("systemctl")
            .args(args)
            .output()
            .map_err(|error| ServiceError::CommandFailed {
                tool: "systemctl",
                args: args.join(" "),
                status: None,
                stderr: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(ServiceError::CommandFailed {
                tool: "systemctl",
                args: args.join(" "),
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output)
    }

    /// Run systemctl and report success without surfacing the error.
    fn systemctl_ok(args: &[&str]) -> bool {
        Command::new("systemctl")
            .args(args)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    impl ServicePort for SystemdPort {
        fn session_available(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
            // A usable user manager needs a runtime directory and a reachable
            // user bus: SSH sessions, containers, and cron jobs typically have
            // neither.
            let runtime = env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty());
            if runtime.is_none() {
                return Ok(false);
            }
            Ok(systemctl_ok(&["--user", "show-environment"]))
        }

        fn write_registration(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            let directory = unit_dir(ctx);
            fs::create_dir_all(&directory).map_err(|error| {
                ServiceError::Io(format!("cannot create {}: {error}", directory.display()))
            })?;
            let unit = render_unit(ctx)?;
            write_private(&unit_path(ctx), unit.as_bytes())
        }

        fn remove_registration(&self, ctx: &ServiceContext) -> Result<bool, ServiceError> {
            let path = unit_path(ctx);
            match fs::remove_file(&path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(ServiceError::Io(format!(
                    "cannot remove {}: {error}",
                    path.display()
                ))),
            }
        }

        fn reload(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
            systemctl(&["--user", "daemon-reload"]).map(|_| ())
        }

        fn is_loaded(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
            Ok(systemctl_ok(&[
                "--user",
                "is-active",
                ctx.target.service_name(),
            ]))
        }

        fn enable(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
            systemctl(&["--user", "enable", ctx.target.service_name()]).map(|_| ())
        }

        fn start(&self, _ctx: &ServiceContext) -> Result<StartedService, ServiceError> {
            systemctl(&["--user", "enable", "--now", ctx.target.service_name()])
                .map(|_| StartedService { domain: None })
        }

        fn stop(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
            systemctl(&["--user", "stop", ctx.target.service_name()]).map(|_| ())
        }

        fn restart(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
            // `enable --now` on an already-active unit is a no-op (systemd
            // start never restarts an active unit), so the refresh stops first.
            self.stop(ctx)?;
            self.start(ctx)?;
            Ok(())
        }

        fn unload(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
            systemctl(&["--user", "disable", ctx.target.service_name()]).map(|_| ())
        }

        fn status(&self, ctx: &ServiceContext) -> Result<ServiceStatus, ServiceError> {
            let session_available = self.session_available(ctx)?;
            let registered = unit_path(ctx).exists();
            let mut status = ServiceStatus {
                registered,
                session_available,
                ..Default::default()
            };
            if !session_available {
                return Ok(status);
            }
            status.loaded = systemctl_ok(&["--user", "is-active", ctx.target.service_name()]);
            status.enabled = Some(systemctl_ok(&[
                "--user",
                "is-enabled",
                ctx.target.service_name(),
            ]));
            Ok(status)
        }
    }
}

/// The deterministic fake service manager for contract tests.
///
/// `LOCRON_SERVICE_FAKE_STATE` points at a JSON file with the initial
/// `{"session":bool,"registered":bool,"loaded":bool,"enabled":bool}` state;
/// `LOCRON_SERVICE_FAKE_LOG` points at a file that receives one call name per
/// line in call order.
struct FakeServicePort {
    inner: std::sync::Mutex<FakeInner>,
}

// The fake models four independent boolean manager states; the field count is
// the point of the struct, not an accident.
#[allow(clippy::struct_excessive_bools)]
struct FakeInner {
    session: bool,
    registered: bool,
    loaded: bool,
    enabled: bool,
    calls: Vec<String>,
    log: Option<PathBuf>,
}

impl FakeServicePort {
    fn from_env() -> Result<Self, ServiceError> {
        let mut inner = FakeInner {
            session: false,
            registered: false,
            loaded: false,
            enabled: false,
            calls: Vec::new(),
            log: env::var("LOCRON_SERVICE_FAKE_LOG").ok().map(PathBuf::from),
        };
        if let Ok(path) = env::var("LOCRON_SERVICE_FAKE_STATE") {
            let text = fs::read_to_string(&path).map_err(|error| {
                ServiceError::Io(format!(
                    "cannot read the fake service state {path}: {error}"
                ))
            })?;
            let value: Value = serde_json::from_str(&text).map_err(|error| {
                ServiceError::Io(format!(
                    "cannot read the fake service state {path}: {error}"
                ))
            })?;
            inner.session = value
                .get("session")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            inner.registered = value
                .get("registered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            inner.loaded = value
                .get("loaded")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            inner.enabled = value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        }
        Ok(Self {
            inner: std::sync::Mutex::new(inner),
        })
    }

    fn record(&self, call: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.calls.push(call.to_owned());
        if let Some(path) = inner.log.clone()
            && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
        {
            let _ = writeln!(file, "{call}");
        }
    }
}

impl ServicePort for FakeServicePort {
    fn session_available(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
        self.record("session_available");
        Ok(self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .session)
    }

    fn write_registration(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("write_registration");
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .registered = true;
        Ok(())
    }

    fn remove_registration(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
        self.record("remove_registration");
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let existed = inner.registered;
        inner.registered = false;
        Ok(existed)
    }

    fn reload(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("reload");
        Ok(())
    }

    fn is_loaded(&self, _ctx: &ServiceContext) -> Result<bool, ServiceError> {
        self.record("is_loaded");
        Ok(self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded)
    }

    fn enable(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("enable");
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .enabled = true;
        Ok(())
    }

    fn start(&self, _ctx: &ServiceContext) -> Result<StartedService, ServiceError> {
        self.record("start");
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.loaded = true;
        inner.enabled = true;
        Ok(StartedService {
            domain: Some("fake/domain".to_owned()),
        })
    }

    fn stop(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("stop");
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded = false;
        Ok(())
    }

    fn restart(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("restart");
        Ok(())
    }

    fn unload(&self, _ctx: &ServiceContext) -> Result<(), ServiceError> {
        self.record("unload");
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded = false;
        Ok(())
    }

    fn status(&self, _ctx: &ServiceContext) -> Result<ServiceStatus, ServiceError> {
        self.record("status");
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(ServiceStatus {
            registered: inner.registered,
            loaded: inner.loaded,
            enabled: Some(inner.enabled),
            domain: Some("fake/domain".to_owned()),
            pid: None,
            executable: None,
            session_available: inner.session,
        })
    }
}

#[cfg(test)]
mod tests {
    use locron_store::LockMetadata;

    use super::*;

    #[allow(clippy::fn_params_excessive_bools)]
    fn fake(session: bool, loaded: bool, enabled: bool, registered: bool) -> FakeServicePort {
        FakeServicePort {
            inner: std::sync::Mutex::new(FakeInner {
                session,
                registered,
                loaded,
                enabled,
                calls: Vec::new(),
                log: None,
            }),
        }
    }

    fn ctx_with(tmp: &Path, executable: &Path) -> ServiceContext {
        ServiceContext {
            executable: executable.to_path_buf(),
            home: tmp.to_path_buf(),
            target: Target::Daemon,
            uid: 501,
            paths: None,
        }
    }

    fn ctx_with_target(tmp: &Path, executable: &Path, target: Target) -> ServiceContext {
        ServiceContext {
            executable: executable.to_path_buf(),
            home: tmp.to_path_buf(),
            target,
            uid: 501,
            paths: Some(StatePaths::new(tmp.join("state"))),
        }
    }

    fn calls(port: &FakeServicePort) -> Vec<String> {
        port.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .calls
            .clone()
    }

    #[test]
    fn plist_template_renders_required_keys_and_paths() {
        let executable = Path::new("/opt/locron/bin/locron");
        let home = Path::new("/Users/tester");
        let ctx = ctx_with_target(home, executable, Target::Daemon);
        let plist = render_plist(&ctx).unwrap();
        assert!(plist.contains("<string>dev.locron.daemon</string>"));
        assert!(plist.contains("<string>/opt/locron/bin/locron</string>"));
        assert!(plist.contains("<string>daemon</string>"));
        assert!(plist.contains("<string>run</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert_eq!(plist.matches("<true/>").count(), 2);
        let log = format!(
            "<string>{}</string>",
            home.join("Library/Logs/locron/daemon.log").display()
        );
        assert_eq!(plist.matches(&log).count(), 2);
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    }

    #[test]
    fn dashboard_plist_template_uses_dashboard_label_args_and_log() {
        let executable = Path::new("/opt/locron/bin/locron");
        let home = Path::new("/Users/tester");
        let ctx = ctx_with_target(home, executable, Target::Dashboard);
        let plist = render_plist(&ctx).unwrap();
        assert!(plist.contains("<string>dev.locron.dashboard</string>"));
        assert!(plist.contains("<string>/opt/locron/bin/locron</string>"));
        assert!(plist.contains("<string>dashboard</string>"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<string>--service-mode</string>"));
        assert!(plist.contains("<string>--state-dir</string>"));
        assert!(plist.contains("<string>/Users/tester/state</string>"));
        assert!(!plist.contains("dev.locron.daemon"));
        assert!(!plist.contains("<string>daemon</string>\n    <string>run</string>"));
        let log = format!(
            "<string>{}</string>",
            home.join("Library/Logs/locron/dashboard.log").display()
        );
        assert_eq!(plist.matches(&log).count(), 2);
    }

    #[test]
    fn plist_template_xml_escapes_special_paths() {
        let executable = Path::new("/tmp/a&b <c>/locron");
        let home = Path::new("/Users/te'ster");
        let ctx = ctx_with_target(home, executable, Target::Daemon);
        let plist = render_plist(&ctx).unwrap();
        assert!(plist.contains("/tmp/a&amp;b &lt;c&gt;/locron"));
        assert!(plist.contains("te&apos;ster"));
        assert!(!plist.contains("/tmp/a&b"));
        assert!(!plist.contains("<c>"));
    }

    #[test]
    fn unit_template_renders_required_keys_and_paths() {
        let ctx = ctx_with_target(
            Path::new("/home/tester"),
            Path::new("/opt/locron/bin/locron"),
            Target::Daemon,
        );
        let unit = render_unit(&ctx).unwrap();
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("Description=locron scheduler daemon"));
        assert!(unit.contains("ExecStart=\"/opt/locron/bin/locron\" daemon run"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn dashboard_unit_template_uses_dashboard_description_and_args() {
        let ctx = ctx_with_target(
            Path::new("/home/tester"),
            Path::new("/opt/locron/bin/locron"),
            Target::Dashboard,
        );
        let unit = render_unit(&ctx).unwrap();
        assert!(unit.contains("Description=locron web dashboard"));
        assert!(unit.contains(
            "ExecStart=\"/opt/locron/bin/locron\" --state-dir \"/home/tester/state\" dashboard serve --service-mode"
        ));
        assert!(!unit.contains("daemon run"));
        assert!(!unit.contains("locron scheduler daemon"));
    }

    #[test]
    fn unit_template_escapes_systemd_specials() {
        let ctx = ctx_with_target(
            Path::new("/home/tester"),
            Path::new("/tmp/a\"b $c/locron"),
            Target::Daemon,
        );
        let unit = render_unit(&ctx).unwrap();
        assert!(unit.contains("ExecStart=\"/tmp/a\\\"b \\$c/locron\" daemon run"));
    }

    #[test]
    fn dashboard_enable_generates_the_token_then_registers_and_starts() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_target(
            tmp.path(),
            Path::new("/opt/locron/bin/locron"),
            Target::Dashboard,
        );
        let port = fake(true, false, false, false);
        let outcome = dashboard_enable(&ctx, &port, false).unwrap();
        assert!(outcome.registered);
        assert!(!outcome.restarted);
        assert!(!outcome.deferred);
        assert_eq!(outcome.guidance, None);
        let paths = StatePaths::new(tmp.path().join("state"));
        let token = std::fs::read_to_string(paths.root.join("dashboard.token")).unwrap();
        assert_eq!(token.len(), 64);
        // The dashboard never probes the daemon lock: no status call for it,
        // and the start is never deferred.
        assert_eq!(
            calls(&port),
            [
                "session_available",
                "write_registration",
                "reload",
                "is_loaded",
                "enable",
                "start",
            ]
        );
    }

    #[test]
    fn dashboard_enable_reuses_the_token_unless_reset() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(tmp.path().join("state"));
        let first = locron_server::token::ensure(&paths).unwrap();
        let ctx = ctx_with_target(
            tmp.path(),
            Path::new("/opt/locron/bin/locron"),
            Target::Dashboard,
        );
        let port = fake(true, true, true, true);
        let outcome = dashboard_enable(&ctx, &port, false).unwrap();
        assert!(outcome.restarted, "repeated enable refreshes and restarts");
        assert_eq!(
            std::fs::read_to_string(paths.root.join("dashboard.token")).unwrap(),
            first,
            "enable without --reset must reuse the stored token"
        );
        let outcome = dashboard_enable(&ctx, &port, true).unwrap();
        assert!(outcome.restarted);
        let second = std::fs::read_to_string(paths.root.join("dashboard.token")).unwrap();
        assert_ne!(first, second, "--reset must regenerate the token");
        assert_eq!(second.len(), 64);
    }

    #[test]
    fn dashboard_disable_unregisters_then_removes_the_token() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(tmp.path().join("state"));
        locron_server::token::ensure(&paths).unwrap();
        let ctx = ctx_with_target(
            tmp.path(),
            Path::new("/opt/locron/bin/locron"),
            Target::Dashboard,
        );
        let port = fake(true, true, true, true);
        let (outcome, guidance) = dashboard_disable(&ctx, &port).unwrap();
        assert!(outcome.removed);
        assert!(outcome.stopped);
        assert!(
            !paths.root.join("dashboard.token").exists(),
            "disable must remove the access token"
        );
        let calls = calls(&port);
        let stop_at = calls.iter().position(|call| call == "stop").unwrap();
        let unload_at = calls.iter().position(|call| call == "unload").unwrap();
        let remove_at = calls
            .iter()
            .position(|call| call == "remove_registration")
            .unwrap();
        assert!(stop_at < unload_at && unload_at < remove_at);
        // The foreground-listener probe is best-effort; the guidance may or
        // may not fire, but the flow must never error on it.
        let _ = guidance;
    }

    #[test]
    fn dashboard_flows_refuse_managed_binaries_before_touching_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let marker_dir = tmp.path().join("lib");
        fs::create_dir_all(&marker_dir).unwrap();
        fs::write(marker_dir.join(".disable-self-update"), "").unwrap();
        let ctx = ctx_with_target(tmp.path(), &bin.join("locron"), Target::Dashboard);
        let port = fake(true, false, false, false);
        let error = dashboard_enable(&ctx, &port, false).unwrap_err();
        assert!(matches!(error, ServiceError::ManagedInstall));
        let paths = StatePaths::new(tmp.path().join("state"));
        assert!(
            !paths.root.join("dashboard.token").exists(),
            "refusal must happen before the token is generated"
        );
        let error = dashboard_disable(&ctx, &port).unwrap_err();
        assert!(matches!(error, ServiceError::ManagedInstall));
        assert_eq!(calls(&port), Vec::<String>::new());
    }

    #[test]
    fn dashboard_status_reports_backend_fields_and_token_facts() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(tmp.path().join("state"));
        locron_server::token::ensure(&paths).unwrap();
        let ctx = ctx_with_target(
            tmp.path(),
            Path::new("/opt/locron/bin/locron"),
            Target::Dashboard,
        );
        let port = fake(true, true, true, true);
        let (outcome, token) = dashboard_status(&ctx, &port).unwrap();
        assert!(outcome.registered);
        assert!(outcome.loaded);
        assert_eq!(outcome.enabled, Some(true));
        assert_eq!(outcome.domain.as_deref(), Some("fake/domain"));
        assert_eq!(token["present"], true);
        assert_eq!(token["permissions"], "owner_only");
        assert_eq!(access_url(), "http://127.0.0.1:10824/");
    }

    #[test]
    fn install_first_registration_orders_write_reload_probe_enable_start() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(true, false, false, false);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = install(&ctx, &port).unwrap();
        assert!(outcome.registered);
        assert!(!outcome.restarted);
        assert!(!outcome.deferred);
        assert_eq!(outcome.guidance, None);
        assert_eq!(outcome.domain.as_deref(), Some("fake/domain"));
        assert_eq!(
            calls(&port),
            [
                "session_available",
                "write_registration",
                "reload",
                "is_loaded",
                "enable",
                "start",
            ]
        );
    }

    #[test]
    fn install_refreshes_and_restarts_a_loaded_service() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(true, true, true, true);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = install(&ctx, &port).unwrap();
        assert!(outcome.registered);
        assert!(outcome.restarted);
        assert!(!outcome.deferred);
        assert_eq!(
            calls(&port),
            [
                "session_available",
                "write_registration",
                "reload",
                "is_loaded",
                "restart",
                "status",
            ]
        );
    }

    #[test]
    fn install_defers_start_when_a_manual_daemon_holds_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(tmp.path().to_path_buf());
        let _lock = DaemonLock::acquire(
            &paths.daemon_lock,
            &LockMetadata {
                pid: 4242,
                lifetime_id: "test".to_owned(),
                started_at_us: 0,
                binary_version: env!("CARGO_PKG_VERSION").to_owned(),
            },
        )
        .unwrap();
        let port = fake(true, false, false, false);
        let ctx = ServiceContext {
            executable: PathBuf::from("/opt/locron/bin/locron"),
            home: tmp.path().to_path_buf(),
            target: Target::Daemon,
            uid: 501,
            paths: Some(paths),
        };
        let outcome = install(&ctx, &port).unwrap();
        assert!(outcome.registered);
        assert!(!outcome.restarted);
        assert!(outcome.deferred);
        assert!(outcome.guidance.is_some());
        assert_eq!(outcome.domain, None);
        assert_eq!(
            calls(&port),
            [
                "session_available",
                "write_registration",
                "reload",
                "is_loaded",
                "enable",
            ]
        );
    }

    #[test]
    fn install_without_a_state_directory_treats_the_lock_as_free() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(true, false, false, false);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = install(&ctx, &port).unwrap();
        assert!(!outcome.deferred);
        assert_eq!(calls(&port).last().map(String::as_str), Some("start"));
    }

    #[test]
    fn install_without_a_session_registers_nothing_and_prints_guidance() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(false, false, false, false);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = install(&ctx, &port).unwrap();
        assert!(!outcome.registered);
        assert!(!outcome.deferred);
        assert!(outcome.guidance.is_some());
        assert_eq!(calls(&port), ["session_available"]);
    }

    #[test]
    fn uninstall_stops_before_unloading_and_removes_the_registration() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(true, true, true, true);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = uninstall(&ctx, &port).unwrap();
        assert!(outcome.removed);
        assert!(outcome.stopped);
        let calls = calls(&port);
        assert_eq!(
            calls,
            [
                "session_available",
                "is_loaded",
                "status",
                "stop",
                "status",
                "unload",
                "remove_registration",
                "reload",
            ]
        );
        let stop_at = calls.iter().position(|call| call == "stop").unwrap();
        let unload_at = calls.iter().position(|call| call == "unload").unwrap();
        let remove_at = calls
            .iter()
            .position(|call| call == "remove_registration")
            .unwrap();
        assert!(stop_at < unload_at && unload_at < remove_at);
    }

    #[test]
    fn uninstall_without_a_session_removes_a_stale_registration() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(false, false, false, true);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = uninstall(&ctx, &port).unwrap();
        assert!(outcome.removed);
        assert!(!outcome.stopped);
        assert_eq!(calls(&port), ["session_available", "remove_registration"]);
    }

    #[test]
    fn uninstall_of_nothing_reports_removed_false() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(false, false, false, false);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = uninstall(&ctx, &port).unwrap();
        assert!(!outcome.removed);
        assert!(!outcome.stopped);
    }

    #[test]
    fn status_reports_backend_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let port = fake(true, true, true, true);
        let ctx = ctx_with(tmp.path(), Path::new("/opt/locron/bin/locron"));
        let outcome = status(&ctx, &port).unwrap();
        assert!(outcome.registered);
        assert!(outcome.loaded);
        assert_eq!(outcome.enabled, Some(true));
        assert_eq!(outcome.domain.as_deref(), Some("fake/domain"));
        assert!(outcome.session_available);
    }

    #[test]
    fn managed_install_refuses_with_brew_guidance() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let marker_dir = tmp.path().join("lib");
        fs::create_dir_all(&marker_dir).unwrap();
        fs::write(marker_dir.join(".disable-self-update"), "").unwrap();
        let port = fake(true, false, false, false);
        let ctx = ctx_with(tmp.path(), &bin.join("locron"));
        let error = install(&ctx, &port).unwrap_err();
        assert!(matches!(error, ServiceError::ManagedInstall));
        assert!(error.to_string().contains("brew services"));
        assert_eq!(calls(&port), Vec::<String>::new());
    }

    #[test]
    fn lock_probe_treats_a_missing_state_directory_as_free() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(tmp.path().join("absent"));
        assert!(!daemon_lock_held(&paths).unwrap());
    }
}
