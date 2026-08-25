//! Manual browser checklist server (step 7 evidence).
//!
//! Binds the dashboard API on a state directory so a real browser can walk
//! the viewer end to end: token paste, cookie handoff, list/detail/history,
//! job creation with dry-run preview, a live follow, and a cancellation.
//! Run `locron daemon run` against the same state directory (or start this
//! example first and the daemon later) so scheduled jobs execute for real.
//!
//! Usage:
//!
//! ```text
//! LOCRON_STATE_DIR=/tmp/locron-walk cargo run -p locron-server --example dashboard_walk
//! ```
//!
//! The printed access URL and access token are the ones to paste into the
//! browser; the token also lives at `<state-dir>/dashboard.token`.

use std::io::{self, Write};

use locron_server::{Config, PortPolicy, bind, serve, token};
use locron_store::StatePaths;

#[tokio::main]
async fn main() -> io::Result<()> {
    let paths = StatePaths::discover(None).map_err(io::Error::other)?;
    let config = Config {
        port: Some(10824),
        port_policy: PortPolicy::Fixed,
        ..Config::default()
    };
    let bound = bind(&config).await?;
    let access_token = token::ensure(&paths)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "access url: http://{}", bound.address)?;
    writeln!(stdout, "access token: {access_token}")?;
    for warning in &bound.warnings {
        writeln!(stdout, "warning: {warning}")?;
    }
    stdout.flush()?;
    serve(bound, paths).await
}
