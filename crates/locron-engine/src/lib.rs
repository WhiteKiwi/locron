//! Long-lived scheduling, execution, and output-capture runtime.

pub mod admission;
pub mod daemon;
pub mod output;
pub mod runner;

pub use daemon::{CompletionError, Daemon, DaemonConfig, DaemonStore, TickResult};
pub use output::{Channel, Frame, OutputStats, OutputWriter, read_frames, repair_partial};
pub use runner::{
    AttemptContext, ExecutionOutcome, HttpSpec, ProcessSpec, Runner, RunnerConfig, TargetSpec,
};

#[cfg(debug_assertions)]
async fn test_crash_boundary(boundary: &str) {
    if std::env::var("LOCRON_TEST_CRASH_POINT").as_deref() != Ok(boundary) {
        return;
    }
    let ready = std::env::var_os("LOCRON_TEST_CRASH_READY")
        .expect("LOCRON_TEST_CRASH_READY must accompany LOCRON_TEST_CRASH_POINT");
    std::fs::write(ready, boundary).expect("write crash-test rendezvous");
    std::future::pending::<()>().await;
}

#[cfg(not(debug_assertions))]
async fn test_crash_boundary(_: &str) {}
