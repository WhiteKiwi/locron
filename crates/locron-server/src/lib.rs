//! The loopback HTTP management and viewer surface for locron.
//!
//! `locron-server` implements the roadmap-phase-1 web administration dashboard
//! (`docs/dashboard/SPEC.md`): a loopback-only HTTP server exposing the same durable application
//! commands as the CLI through a versioned JSON API, an SSE stream for live run output, and an
//! embedded single-page viewer.
//!
//! The crate depends only on `locron-core` and `locron-store`. It never parses CLI arguments,
//! never owns the daemon scheduler lifetime or a runner lifecycle, and never touches SQLite
//! outside the store boundary. Its single composition entry is [`serve`]; the CLI owns startup
//! output, token display, and exit codes.

use std::path::PathBuf;

/// Server configuration: bind addresses, port preference, and token file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Loopback bind addresses (`127.0.0.1` and/or `::1`).
    pub bind: Vec<String>,
    /// Preferred port; `None` selects the default port with foreground fallback behavior.
    pub port: Option<u16>,
    /// Token file name under the state directory.
    pub token_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: vec!["127.0.0.1".to_owned(), "::1".to_owned()],
            port: None,
            token_file: PathBuf::from("dashboard.token"),
        }
    }
}

/// Runs the dashboard server until shutdown or a process signal.
///
/// This is the crate's only composition entry. The middleware stack, API routes, SSE stream, and
/// embedded viewer land in later roadmap-backlog steps; until then this returns an unsupported
/// error so the CLI's `dashboard` family has an honest failure mode.
pub fn serve(_config: &Config) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "dashboard server not implemented yet",
    ))
}
