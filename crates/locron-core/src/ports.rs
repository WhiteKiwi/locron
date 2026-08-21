//! Runtime-neutral ports used by orchestration layers.

use std::future::Future;

use jiff::tz::TimeZone;

use crate::command::ApplicationCommand;
use crate::{Result, Timestamp};

/// Applies one normalized application command to durable state.
///
/// The command's associated result prevents adapters from returning storage
/// rows or presentation values across the application boundary.
pub trait PersistencePort<C>: Send + Sync
where
    C: ApplicationCommand,
{
    fn apply(&self, command: C) -> Result<C::Result>;
}

/// Injected wall-clock source for deterministic scheduling tests.
pub trait Clock: Send + Sync {
    /// Current durable UTC instant.
    fn now(&self) -> Timestamp;
    /// Process-local monotonic elapsed time for wall-clock discontinuity
    /// detection between reconciliation samples.
    fn monotonic_micros(&self) -> u64;
}

/// Resolves the symbolic system-local timezone once per engine pass.
pub trait TimeZoneResolver: Send + Sync {
    /// Current system-local timezone snapshot.
    fn local_timezone(&self) -> Result<TimeZone>;
}

/// Executes a target without coupling the core boundary to an async runtime.
///
/// Engine adapters choose their request and output values. The returned
/// future is a standard-library value and may be driven by any executor.
pub trait ExecutorPort: Send + Sync {
    type Request: Send;
    type Output: Send;

    fn execute(&self, request: Self::Request) -> impl Future<Output = Result<Self::Output>> + Send;
}

/// Minimal persistence health operation shared by applications.
pub trait HealthPort: Send + Sync {
    /// Validates durable state and returns an operator-facing report.
    fn check(&self) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use super::*;
    use crate::command::{AddJob, AddJobResult};

    struct FakePersistence;

    impl PersistencePort<AddJob> for FakePersistence {
        fn apply(&self, command: AddJob) -> Result<AddJobResult> {
            Ok(AddJobResult {
                job_id: command.id,
                revision: 1,
            })
        }
    }

    struct FakeExecutor;

    impl ExecutorPort for FakeExecutor {
        type Request = u32;
        type Output = u32;

        fn execute(
            &self,
            request: Self::Request,
        ) -> impl Future<Output = Result<Self::Output>> + Send {
            ready(Ok(request + 1))
        }
    }

    #[test]
    fn ports_are_storage_and_runtime_neutral() {
        fn assert_port<P: PersistencePort<AddJob>>() {}
        fn assert_executor<P: ExecutorPort>() {}

        assert_port::<FakePersistence>();
        assert_executor::<FakeExecutor>();
        let _future = FakeExecutor.execute(41);
    }
}
