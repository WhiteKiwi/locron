//! Core-local adapter ownership; Core never depends on the Engine process factory.

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
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use win32job::{ExtendedLimitInfo, Job};

use super::remaining;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_SUSPENDED: u32 = 0x0000_0004;

#[derive(Debug)]
struct Enroll(Arc<Job>);

impl CommandWrapper for Enroll {
    fn post_spawn(
        &mut self,
        _command: &mut Command,
        child: &mut Child,
        _core: &CommandWrap,
    ) -> io::Result<()> {
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("suspended adapter has no handle"))?;
        self.0.assign_process(handle as isize).map_err(|error| {
            let _ = child.start_kill();
            io::Error::other(error)
        })
    }
}

pub(super) enum SpawnFailure {
    NotStarted(io::Error),
    Unconfirmed { error: io::Error, _job: Arc<Job> },
}

pub(super) struct OwnedChild {
    child: Box<dyn ChildWrapper>,
    job: Arc<Job>,
    status: Option<ExitStatus>,
    #[cfg(test)]
    creation_flags_at_spawn: u32,
}

fn hidden_flags() -> CreationFlags {
    let mut flags = CreationFlags(Default::default());
    flags.0.0 = CREATE_NO_WINDOW;
    flags
}

impl OwnedChild {
    pub(super) fn spawn(command: Command) -> Result<Self, SpawnFailure> {
        let mut limits = ExtendedLimitInfo::default();
        limits.limit_kill_on_job_close();
        let job = Arc::new(
            Job::create_with_limit_info(&limits)
                .map_err(|error| SpawnFailure::NotStarted(io::Error::other(error)))?,
        );
        let created = AtomicBool::new(false);
        let native_flags = CREATE_NO_WINDOW | CREATE_SUSPENDED;
        #[cfg(test)]
        let mut creation_flags_at_spawn = 0;
        let mut command = CommandWrap::from(command);
        command
            .wrap(hidden_flags())
            .wrap(KillOnDrop)
            .wrap(JobObject)
            .wrap(Enroll(Arc::clone(&job)));
        let child = command
            .spawn_with(|command| {
                // Apply the actual native mask after every wrapper pre_spawn hook. The logical
                // wrapper stays unsuspended so JobObject resumes only after both enrollments.
                command.creation_flags(native_flags);
                #[cfg(test)]
                {
                    creation_flags_at_spawn = native_flags;
                }
                let child = command.spawn()?;
                created.store(true, Ordering::Release);
                Ok(child)
            })
            .map_err(|error| {
                if created.load(Ordering::Acquire) {
                    SpawnFailure::Unconfirmed {
                        error,
                        _job: Arc::clone(&job),
                    }
                } else {
                    SpawnFailure::NotStarted(error)
                }
            })?;
        Ok(Self {
            child,
            job,
            status: None,
            #[cfg(test)]
            creation_flags_at_spawn,
        })
    }

    #[cfg(test)]
    pub(super) fn creation_flags_at_spawn(&self) -> u32 {
        self.creation_flags_at_spawn
    }

    pub(super) fn id(&self) -> Option<u32> {
        self.child.id()
    }
    pub(super) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin().take()
    }
    pub(super) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }
    pub(super) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }
    pub(super) fn start_kill(&mut self) -> io::Result<()> {
        self.child.start_kill()
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.status.is_none() {
            self.status = self.child.try_wait()?;
        }
        Ok(self.status)
    }

    pub(super) fn tree_empty(&self) -> io::Result<bool> {
        self.job
            .query_process_id_list()
            .map(|list| list.is_empty())
            .map_err(io::Error::other)
    }

    pub(super) async fn confirm_exit_until(&mut self, deadline: Instant) -> io::Result<ExitStatus> {
        loop {
            remaining(deadline)?;
            let status = self.try_wait()?;
            remaining(deadline)?;
            let empty = self.tree_empty()?;
            remaining(deadline)?;
            if let Some(status) = status
                && empty
            {
                return Ok(status);
            }
            tokio::time::sleep(remaining(deadline)?.min(Duration::from_millis(10))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hidden_flags;
    use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject};
    use tokio::process::Command;

    #[test]
    fn adapter_job_composes_hidden_flags_before_temporary_suspension() {
        let mut command = CommandWrap::from(Command::new("fixture.exe"));
        command.wrap(hidden_flags()).wrap(JobObject);
        assert!(command.has_wrap::<JobObject>());
        assert_eq!(
            command.get_wrap::<CreationFlags>().unwrap().0.0,
            0x0800_0000
        );
    }
}
