//! Safe Windows child and process-tree ownership shared by jobs and registered roles.

use std::io;
use std::process::ExitStatus;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use process_wrap::tokio::{
    ChildWrapper, CommandWrap, CommandWrapper, CreationFlags, JobObject, KillOnDrop,
};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use win32job::{ExtendedLimitInfo, Job};

/// Explicit console-window policy composed with temporary suspended creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildWindow {
    /// Start a console executable without attaching or creating a console window.
    Hidden,
    /// Preserve the runner's existing default window inheritance policy.
    Inherit,
}

fn creation_flags(window: ChildWindow) -> CreationFlags {
    let mut flags = CreationFlags(Default::default());
    flags.0.0 = match window {
        ChildWindow::Hidden => 0x0800_0000, // CREATE_NO_WINDOW
        ChildWindow::Inherit => 0,
    };
    flags
}

#[derive(Debug)]
struct Enroll {
    job: Arc<Job>,
    reject_after_enrollment: bool,
}

impl CommandWrapper for Enroll {
    fn post_spawn(
        &mut self,
        _command: &mut Command,
        child: &mut Child,
        _core: &CommandWrap,
    ) -> io::Result<()> {
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("suspended child has no process handle"))?;
        if let Err(error) = self.job.assign_process(handle as isize) {
            let _ = child.start_kill();
            return Err(io::Error::other(error));
        }
        if self.reject_after_enrollment {
            let _ = child.start_kill();
            return Err(io::Error::other("owned-child enrollment fault fixture"));
        }
        Ok(())
    }
}

/// Cleanup ownership after a spawned child's native wait capability was lost.
///
/// Keep this value through refusal/quarantine. Empty Job membership alone cannot establish
/// root reaping; dropping the guard triggers emergency kernel containment only.
#[derive(Debug)]
pub struct SpawnContainment {
    job: Arc<Job>,
    pid: Option<u32>,
}

impl SpawnContainment {
    /// Returns the original spawned PID for diagnostics, never PID termination authority.
    #[must_use]
    pub const fn id(&self) -> Option<u32> {
        self.pid
    }

    /// Queries membership without claiming that the lost root was reaped.
    pub fn tree_empty(&self) -> io::Result<bool> {
        job_empty(&self.job)
    }
}

/// Whether spawn failed before a native child existed or left unconfirmed cleanup ownership.
#[derive(Debug, thiserror::Error)]
pub enum SpawnFailure {
    /// No native child was created and no external execution can have started.
    #[error("owned child was not started: {0}")]
    NotStarted(#[source] io::Error),
    /// A native child was created; its retained Job is containment, not completion proof.
    #[error("owned child spawn cleanup remains unconfirmed: {error}")]
    ExecutionMayHaveStarted {
        /// Enrollment, wrapper, or resume error.
        #[source]
        error: io::Error,
        /// Retained cleanup ownership which must not be discarded to enable a retry.
        containment: SpawnContainment,
    },
}

fn job_empty(job: &Job) -> io::Result<bool> {
    job.query_process_id_list()
        .map(|processes| processes.is_empty())
        .map_err(io::Error::other)
}

/// One root child and the independently retained Job enrolled before its first instruction.
#[derive(Debug)]
pub struct OwnedChild {
    child: Box<dyn ChildWrapper>,
    job: Arc<Job>,
    pid: Option<u32>,
    root_status: Option<ExitStatus>,
}

impl OwnedChild {
    /// Spawns exact caller argv/environment/cwd/stdio with the explicit window policy.
    ///
    /// Raw creation flags on `command` are unsupported; the audited wrapper supplies them.
    pub fn spawn(command: Command, window: ChildWindow) -> Result<Self, SpawnFailure> {
        Self::spawn_inner(command, window, false)
    }

    fn spawn_inner(
        command: Command,
        window: ChildWindow,
        reject_after_enrollment: bool,
    ) -> Result<Self, SpawnFailure> {
        let mut limits = ExtendedLimitInfo::default();
        limits.limit_kill_on_job_close();
        let job = Arc::new(
            Job::create_with_limit_info(&limits)
                .map_err(|error| SpawnFailure::NotStarted(io::Error::other(error)))?,
        );
        let created = AtomicBool::new(false);
        let mut pid = None;
        let mut wrapped = CommandWrap::from(command);
        wrapped
            .wrap(creation_flags(window))
            .wrap(KillOnDrop)
            .wrap(JobObject)
            .wrap(Enroll {
                job: Arc::clone(&job),
                reject_after_enrollment,
            });
        let child = wrapped
            .spawn_with(|command| {
                let child = command.spawn()?;
                pid = child.id();
                created.store(true, Ordering::Release);
                Ok(child)
            })
            .map_err(|error| {
                if created.load(Ordering::Acquire) {
                    SpawnFailure::ExecutionMayHaveStarted {
                        error,
                        containment: SpawnContainment {
                            job: Arc::clone(&job),
                            pid,
                        },
                    }
                } else {
                    SpawnFailure::NotStarted(error)
                }
            })?;
        Ok(Self {
            child,
            job,
            pid,
            root_status: None,
        })
    }

    /// Returns the original root PID; it is not proof of a live process.
    #[must_use]
    pub const fn id(&self) -> Option<u32> {
        self.pid
    }

    /// Polls and caches the root's exit status; descendants are checked separately.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.root_status.is_none() {
            self.root_status = self.child.try_wait()?;
        }
        Ok(self.root_status)
    }

    /// Queries the authoritative retained Job membership; an error stays unconfirmed.
    pub fn tree_empty(&self) -> io::Result<bool> {
        job_empty(&self.job)
    }

    /// Requests termination of the owned Job without claiming that it has exited.
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.child.start_kill()
    }

    /// Confirms both root reaping and empty Job membership under one supplied deadline.
    pub async fn confirm_exit_until(&mut self, deadline: Instant) -> io::Result<ExitStatus> {
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "owned child root/tree exit remains unconfirmed",
                ));
            }
            if let Some(status) = self.try_wait()?
                && self.tree_empty()?
            {
                return Ok(status);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::sleep(remaining.min(Duration::from_millis(10))).await;
        }
    }

    /// Requests hard termination and confirms root plus tree exit under the same deadline.
    pub async fn terminate_until(&mut self, deadline: Instant) -> io::Result<ExitStatus> {
        self.start_kill()?;
        self.confirm_exit_until(deadline).await
    }

    pub(crate) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }
}

#[cfg(test)]
mod tests {
    use super::{ChildWindow, OwnedChild, SpawnFailure, creation_flags};
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use tokio::process::Command;

    const MODE: &str = "LOCRON_OWNED_CHILD_FIXTURE";
    const ROOT: &str = "LOCRON_OWNED_CHILD_FIXTURE_ROOT";
    const FIXTURE: &str = "windows_child::tests::native_child_fixture";

    struct FixtureRoot {
        _temporary: tempfile::TempDir,
        path: PathBuf,
    }

    impl FixtureRoot {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("private 子 root");
            let _guard = locron_core::filesystem::DirectoryGuard::private(&path).unwrap();
            Self {
                _temporary: temporary,
                path,
            }
        }
    }

    fn command(root: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", FIXTURE, "--nocapture", "--test-threads=1"])
            .env(MODE, mode)
            .env(ROOT, root)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn grandchild(root: &Path, mode: &str) -> std::process::Child {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", FIXTURE, "--nocapture", "--test-threads=1"])
            .env(MODE, mode)
            .env(ROOT, root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn native_child_fixture() {
        let Ok(mode) = std::env::var(MODE) else {
            return;
        };
        let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
        match mode.as_str() {
            "zero" => std::fs::write(root.join("started"), b"started").unwrap(),
            "nonzero" => std::process::exit(7),
            "root-first" => {
                let child = grandchild(&root, "await-release");
                std::fs::write(root.join("descendant.pid"), child.id().to_string()).unwrap();
                std::process::exit(0);
            }
            "await-release" => {
                while !root.join("descendant-release").exists() {
                    std::thread::sleep(Duration::from_millis(10));
                }
                std::fs::write(root.join("descendant-finished"), b"finished").unwrap();
            }
            "tree" => {
                let _child = grandchild(&root, "heartbeat");
                std::fs::write(root.join("started"), b"started").unwrap();
                std::thread::sleep(Duration::from_secs(30));
            }
            "heartbeat" => loop {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("heartbeat"))
                    .unwrap()
                    .write_all(b"x")
                    .unwrap();
                std::thread::sleep(Duration::from_millis(20));
            },
            "console" => {
                let console = std::fs::OpenOptions::new()
                    .write(true)
                    .open(r"\\.\CONOUT$")
                    .is_ok();
                std::fs::write(root.join("console"), if console { "yes" } else { "no" }).unwrap();
            }
            other => panic!("unknown owned-child fixture mode {other}"),
        }
    }

    async fn wait_for(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(Instant::now() < deadline, "fixture did not create {path:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn owned_child_preserves_exit_status_and_hidden_console_policy() {
        let root = FixtureRoot::new();
        assert_eq!(creation_flags(ChildWindow::Hidden).0.0, 0x0800_0000);
        assert_eq!(creation_flags(ChildWindow::Inherit).0.0, 0);
        let mut child =
            OwnedChild::spawn(command(&root.path, "console"), ChildWindow::Hidden).unwrap();
        assert!(child.id().is_some());
        let status = child
            .confirm_exit_until(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read(root.path.join("console")).unwrap(), b"no");
        assert!(child.tree_empty().unwrap());
        assert_eq!(child.try_wait().unwrap(), Some(status));
        let mut nonzero =
            OwnedChild::spawn(command(&root.path, "nonzero"), ChildWindow::Inherit).unwrap();
        assert_eq!(
            nonzero
                .confirm_exit_until(Instant::now() + Duration::from_secs(5))
                .await
                .unwrap()
                .code(),
            Some(7)
        );
    }

    #[tokio::test]
    async fn root_exit_alone_never_confirms_a_live_descendant() {
        let root = FixtureRoot::new();
        let mut child =
            OwnedChild::spawn(command(&root.path, "root-first"), ChildWindow::Hidden).unwrap();
        wait_for(&root.path.join("descendant.pid")).await;
        let root_deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < root_deadline, "fixture root did not exit");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let result = child
            .confirm_exit_until(Instant::now() + Duration::from_millis(100))
            .await;
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert!(child.try_wait().unwrap().is_some());
        assert!(!child.tree_empty().unwrap());
        std::fs::write(root.path.join("descendant-release"), b"release").unwrap();
        assert!(
            child
                .confirm_exit_until(Instant::now() + Duration::from_secs(5))
                .await
                .unwrap()
                .success()
        );
        assert!(root.path.join("descendant-finished").exists());
    }

    #[tokio::test]
    async fn hard_stop_confirms_the_whole_job_with_one_deadline() {
        let root = FixtureRoot::new();
        let mut child =
            OwnedChild::spawn(command(&root.path, "tree"), ChildWindow::Hidden).unwrap();
        wait_for(&root.path.join("heartbeat")).await;
        let start = Instant::now();
        child
            .terminate_until(start + Duration::from_secs(3))
            .await
            .unwrap();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(child.try_wait().unwrap().is_some());
        assert!(child.tree_empty().unwrap());
        let length = std::fs::metadata(root.path.join("heartbeat"))
            .unwrap()
            .len();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            std::fs::metadata(root.path.join("heartbeat"))
                .unwrap()
                .len(),
            length
        );
    }

    #[tokio::test]
    async fn failed_enrollment_keeps_containment_and_never_runs_the_target() {
        let root = FixtureRoot::new();
        let failure =
            OwnedChild::spawn_inner(command(&root.path, "zero"), ChildWindow::Hidden, true)
                .unwrap_err();
        let SpawnFailure::ExecutionMayHaveStarted { containment, .. } = failure else {
            panic!("spawned suspended child was misclassified as not started");
        };
        assert!(containment.id().is_some());
        let deadline = Instant::now() + Duration::from_secs(3);
        while !containment.tree_empty().unwrap() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!root.path.join("started").exists());
        // This proves containment only: the API intentionally cannot report root reaping here.
        drop(containment);
        let mut missing = Command::new(root.path.join("missing-executable.exe"));
        missing
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(matches!(
            OwnedChild::spawn(missing, ChildWindow::Hidden),
            Err(SpawnFailure::NotStarted(_))
        ));
    }
}
