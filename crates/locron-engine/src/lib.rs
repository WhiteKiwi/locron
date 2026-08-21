//! Long-lived scheduling, execution, and output-capture runtime.

pub mod admission;
pub mod daemon;
pub mod output;
pub mod runner;

pub use daemon::{Daemon, DaemonConfig, DaemonStore, TickResult};
pub use output::{Channel, Frame, OutputStats, OutputWriter, read_frames, repair_partial};
pub use runner::{
    AttemptContext, ExecutionOutcome, HttpSpec, ProcessSpec, Runner, RunnerConfig, TargetSpec,
};
