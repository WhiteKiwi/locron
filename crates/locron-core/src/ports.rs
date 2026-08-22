//! Runtime-neutral ports used by orchestration layers.

use std::future::Future;

use jiff::tz::TimeZone;

use crate::command::ApplicationCommand;
use crate::{CoreError, Result, Timestamp};

/// Applies one normalized application command to durable state.
///
/// The command's associated result prevents adapters from returning storage
/// rows or presentation values across the application boundary.
pub trait PersistencePort<C>: Send + Sync
where
    C: ApplicationCommand,
{
    /// Adapter-native error retained until the application boundary maps it.
    type Error: std::error::Error + Send + Sync + 'static;

    fn apply(&self, command: C) -> std::result::Result<C::Result, Self::Error>;

    /// Converts adapter details into the stable application error vocabulary.
    fn map_error(error: Self::Error) -> CoreError;
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
        type Error = CoreError;

        fn apply(&self, command: AddJob) -> Result<AddJobResult> {
            Ok(AddJobResult {
                job_id: command.id,
                revision: crate::RevisionNumber::new(1).unwrap(),
            })
        }

        fn map_error(error: Self::Error) -> CoreError {
            error
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

    #[test]
    fn persistence_errors_have_an_explicit_core_mapping() {
        let error = FakePersistence::map_error(CoreError::Conflict("duplicate".into()));
        assert!(matches!(error, CoreError::Conflict(message) if message == "duplicate"));
    }
}
