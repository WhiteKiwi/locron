//! The loopback HTTP management and viewer surface for locron.
//!
//! `locron-server` implements the roadmap-phase-1 web administration dashboard
//! (`docs/dashboard/SPEC.md`): a loopback-only HTTP server exposing the same durable application
//! commands as the CLI through a versioned JSON API, an SSE stream for live run output, and an
//! embedded single-page viewer.
//!
//! The crate depends only on `locron-core` and `locron-store`. It never parses CLI arguments,
//! never owns the daemon scheduler lifetime or a runner lifecycle, and never touches SQLite
//! outside the store boundary. Its composition surface is [`Config`], [`bind`], and [`serve`];
//! the CLI owns startup output, token display, and exit codes.

mod api;
pub mod assets;
pub mod envelope;
pub mod middleware;
pub mod token;
mod transfer;

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use axum::routing::{get, post, put};
use axum::{Extension, Router};
use locron_store::StatePaths;
use tokio::net::TcpListener;

/// The default dashboard port, verified unassigned in the IANA service-names registry
/// (`docs/FINDINGS.md` §14).
pub const DEFAULT_PORT: u16 = 10824;

/// How the preferred port is treated when it is occupied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortPolicy {
    /// Foreground mode: fall back to the next free port (up to ten successive ports, then an
    /// OS-assigned port) and report the chosen port.
    Foreground,
    /// Fixed mode (service mode and explicit `--port`): an occupied port is an error so the
    /// bookmarked address never silently moves.
    Fixed,
}

/// Server configuration: bind addresses, port preference, and token file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Loopback bind addresses (`127.0.0.1` and/or `::1`).
    pub bind: Vec<String>,
    /// Preferred port; `None` selects [`DEFAULT_PORT`] with the policy's fallback behavior.
    pub port: Option<u16>,
    /// Port occupancy behavior.
    pub port_policy: PortPolicy,
    /// Token file name under the state directory.
    pub token_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: vec!["127.0.0.1".to_owned(), "::1".to_owned()],
            port: None,
            port_policy: PortPolicy::Foreground,
            token_file: PathBuf::from(token::TOKEN_FILE_NAME),
        }
    }
}

/// Shared state threaded through the middleware chain and handlers.
#[derive(Clone)]
pub struct AppState {
    /// State directory layout (the store is opened per request from these paths).
    pub paths: StatePaths,
    /// The 64-character hex access token, read at startup.
    pub token: String,
    /// The actually-bound port (after any fallback), used by the Origin check.
    pub bound_port: u16,
}

/// A successfully bound server: the chosen port and the per-address listeners.
pub struct BoundServer {
    /// The actually-bound port (after any fallback).
    pub port: u16,
    /// One address that was actually bound, used for truthful startup output.
    pub address: SocketAddr,
    /// Startup warnings (for example, one loopback family could not be bound).
    pub warnings: Vec<String>,
    listeners: Vec<TcpListener>,
}

/// Binds the configured loopback addresses on the preferred port under the port policy.
///
/// A port conflict under [`PortPolicy::Fixed`] is an error; under [`PortPolicy::Foreground`] the
/// next free ports are tried (up to ten), then an OS-assigned port. If one loopback family cannot
/// be bound the server warns and continues on the other; if none can be bound the underlying
/// error is returned.
pub async fn bind(config: &Config) -> io::Result<BoundServer> {
    bind_with_socket(config, TcpListener::bind::<SocketAddr>).await
}

async fn bind_with_socket<B, F>(config: &Config, mut socket_bind: B) -> io::Result<BoundServer>
where
    B: FnMut(SocketAddr) -> F,
    F: std::future::Future<Output = io::Result<TcpListener>>,
{
    let addresses = config
        .bind
        .iter()
        .map(|value| value.parse::<IpAddr>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "at least one bind address is required",
        ));
    }
    // Validate every address before opening any socket; callers of the library
    // must not be able to bypass the CLI's loopback-only policy.
    if let Some(address) = addresses.iter().find(|address| !address.is_loopback()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("dashboard bind address must be loopback: {address}"),
        ));
    }
    let preferred = config.port.unwrap_or(DEFAULT_PORT);
    let mut warnings = Vec::new();
    match config.port_policy {
        PortPolicy::Fixed => {
            let (address, listeners) =
                bind_all(&addresses, preferred, &mut warnings, &mut socket_bind).await?;
            Ok(BoundServer {
                port: address.port(),
                address,
                warnings,
                listeners,
            })
        }
        PortPolicy::Foreground => {
            let mut last_error = None;
            // At the top of the u16 range, exhaust the remaining valid ports
            // before requesting an ephemeral port; never overflow or wrap.
            for port in preferred..=preferred.saturating_add(9) {
                match bind_all(&addresses, port, &mut warnings, &mut socket_bind).await {
                    Ok((address, listeners)) => {
                        return Ok(BoundServer {
                            port: address.port(),
                            address,
                            warnings,
                            listeners,
                        });
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            let (address, listeners) = bind_all(&addresses, 0, &mut warnings, &mut socket_bind)
                .await
                .map_err(|error| last_error.unwrap_or(error))?;
            Ok(BoundServer {
                port: address.port(),
                address,
                warnings,
                listeners,
            })
        }
    }
}

async fn bind_all<B, F>(
    addresses: &[IpAddr],
    port: u16,
    warnings: &mut Vec<String>,
    socket_bind: &mut B,
) -> io::Result<(SocketAddr, Vec<TcpListener>)>
where
    B: FnMut(SocketAddr) -> F,
    F: std::future::Future<Output = io::Result<TcpListener>>,
{
    let mut listeners = Vec::new();
    let mut failures = Vec::new();
    for address in addresses {
        let candidate_port = if port == 0 {
            listeners
                .first()
                .and_then(|listener: &TcpListener| listener.local_addr().ok())
                .map_or(0, |local| local.port())
        } else {
            port
        };
        match socket_bind(SocketAddr::new(*address, candidate_port)).await {
            Ok(listener) => listeners.push(listener),
            Err(error) => failures.push((*address, candidate_port, error)),
        }
    }
    if listeners.is_empty() {
        let (address, failed_port, error) = failures
            .into_iter()
            .next()
            .expect("at least one bind address was provided");
        return Err(io::Error::new(
            error.kind(),
            format!("could not bind {address}:{failed_port}: {error}"),
        ));
    }
    for (address, failed_port, error) in failures {
        warnings.push(format!("could not bind {address}:{failed_port}: {error}"));
    }
    let bound = listeners[0].local_addr()?;
    Ok((bound, listeners))
}

/// Builds the router with the full middleware chain applied after all routes.
///
/// The middleware layers capture their own state at construction; `.with_state` finalizes the
/// router state consumed by the handlers' `State<AppState>` extractors and yields the default
/// `Router<()>` that `axum::serve` accepts.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(assets::entry))
        .route("/{*path}", get(assets::assets))
        .route(
            "/api/v1/session",
            post(api::session_create).get(api::session_status),
        )
        .route("/api/v1/jobs", get(api::jobs_list).post(api::jobs_create))
        .route(
            "/api/v1/jobs/{id}",
            get(api::jobs_show).put(api::jobs_update).delete(api::jobs_remove),
        )
        .route("/api/v1/jobs/{id}/enable", post(api::jobs_enable))
        .route("/api/v1/jobs/{id}/disable", post(api::jobs_disable))
        .route("/api/v1/jobs/{id}/run", post(api::jobs_run))
        .route("/api/v1/jobs/{id}/preview", get(api::jobs_preview))
        .route("/api/v1/jobs/{id}/why", get(api::jobs_why))
        .route("/api/v1/schedule/preview", post(api::schedule_preview))
        .route("/api/v1/runs", get(api::runs_history))
        .route("/api/v1/runs/{id}", get(api::runs_show))
        .route("/api/v1/runs/{id}/cancel", post(api::runs_cancel))
        .route("/api/v1/runs/{id}/logs", get(api::runs_logs))
        .route("/api/v1/runs/{id}/stream", get(api::runs_stream))
        .route("/api/v1/runs/{id}/why", get(api::runs_why))
        .route("/api/v1/settings", get(api::settings_get))
        .route(
            "/api/v1/settings/{key}",
            put(api::settings_put).delete(api::settings_delete),
        )
        .route("/api/v1/export", get(api::export_document))
        .route("/api/v1/import", post(api::import_document))
        .route("/api/v1/prune", post(api::prune))
        .route("/api/v1/diagnostics", get(api::diagnostics))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::csrf,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::authenticate,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::origin,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::host,
        ))
        // Applied last so it wraps the whole chain: even responses short-circuited by an inner
        // middleware carry the header.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::referrer_policy,
        ))
        .with_state(state)
}

/// Runs the dashboard server on the bound listeners until Ctrl-C, then shuts down gracefully.
///
/// The token file is read (or generated on first use) before listening. All blocking store work
/// happens per request; this function only runs the HTTP stack.
pub async fn serve(bound: BoundServer, paths: StatePaths) -> io::Result<()> {
    serve_until(bound, paths, std::future::pending()).await
}

/// Runs the HTTP stack until Ctrl-C or composition-owned cooperative shutdown.
///
/// The supplied future owns no scheduler lifetime and carries no application command.
pub async fn serve_until<F>(bound: BoundServer, paths: StatePaths, shutdown: F) -> io::Result<()>
where
    F: std::future::Future<Output = ()> + Send,
{
    let token = token::ensure(&paths)?;
    let state = AppState {
        paths,
        token,
        bound_port: bound.port,
    };
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let app = router(state).layer(Extension(shutdown_rx));
    let mut tasks = tokio::task::JoinSet::new();
    for listener in bound.listeners {
        let app = app.clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tasks.spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    wait_for_server_shutdown(Some(shutdown_rx)).await;
                })
                .await
        });
    }
    supervise_listener_tasks(tasks, shutdown_tx, shutdown, dashboard_ctrl_c()).await
}

async fn supervise_listener_tasks<S, C>(
    mut tasks: tokio::task::JoinSet<io::Result<()>>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown: S,
    signal: C,
) -> io::Result<()>
where
    S: std::future::Future<Output = ()> + Send,
    C: std::future::Future<Output = io::Result<()>> + Send,
{
    // A failed listener must wake the owner just like an explicit stop. Retain
    // the first error, but always notify and drain the remaining listeners.
    let mut failure = tokio::select! {
        result = signal => result.err(),
        () = shutdown => None,
        result = tasks.join_next() => Some(match result {
            Some(Ok(Err(error))) => error,
            Some(Err(error)) => io::Error::other(format!("dashboard listener task failed: {error}")),
            Some(Ok(Ok(()))) | None => io::Error::other("dashboard listener exited before shutdown"),
        }),
    };
    shutdown_tx.send_replace(true);
    let drained = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(result) = tasks.join_next().await {
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(error) => Some(io::Error::other(format!(
                    "dashboard listener task failed: {error}"
                ))),
            };
            if let Some(error) = error {
                if failure.is_none() {
                    failure = Some(error);
                } else {
                    tracing::warn!(%error, "additional dashboard listener failure during drain");
                }
            }
        }
    })
    .await;
    if drained.is_ok() {
        failure.map_or(Ok(()), Err)
    } else {
        if let Some(error) = failure {
            tracing::warn!(%error, "dashboard drain timed out after listener or signal failure");
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "dashboard connection drain exceeded its deadline",
        ))
    }
}

pub(crate) async fn wait_for_server_shutdown(receiver: Option<tokio::sync::watch::Receiver<bool>>) {
    let Some(mut receiver) = receiver else {
        return std::future::pending().await;
    };
    let stopped = *receiver.borrow_and_update();
    if !stopped {
        let _ = receiver.changed().await;
    }
}

async fn dashboard_ctrl_c() -> io::Result<()> {
    let result = tokio::signal::ctrl_c().await;
    #[cfg(windows)]
    if let Err(error) = result {
        tracing::warn!(%error, "console shutdown unavailable; cooperative role control remains active");
        return std::future::pending().await;
    }
    result
}

#[cfg(test)]
mod listener_control_tests {
    use super::{
        Config, PortPolicy, bind, bind_with_socket, supervise_listener_tasks,
        wait_for_server_shutdown,
    };
    use futures_util::FutureExt;
    use std::io;
    use std::net::{IpAddr, SocketAddr};
    use std::panic::AssertUnwindSafe;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;
    use tokio::sync::{Notify, oneshot, watch};
    use tokio::task::JoinSet;

    const UPPER_PORTS: &[(u16, &[u16])] = &[
        (
            65_526,
            &[
                65_526, 65_527, 65_528, 65_529, 65_530, 65_531, 65_532, 65_533, 65_534, 65_535,
            ],
        ),
        (
            65_527,
            &[
                65_527, 65_528, 65_529, 65_530, 65_531, 65_532, 65_533, 65_534, 65_535,
            ],
        ),
        (
            65_528,
            &[
                65_528, 65_529, 65_530, 65_531, 65_532, 65_533, 65_534, 65_535,
            ],
        ),
        (
            65_529,
            &[65_529, 65_530, 65_531, 65_532, 65_533, 65_534, 65_535],
        ),
        (65_530, &[65_530, 65_531, 65_532, 65_533, 65_534, 65_535]),
        (65_531, &[65_531, 65_532, 65_533, 65_534, 65_535]),
        (65_532, &[65_532, 65_533, 65_534, 65_535]),
        (65_533, &[65_533, 65_534, 65_535]),
        (65_534, &[65_534, 65_535]),
        (65_535, &[65_535]),
    ];

    fn config(addresses: &[&str], port: u16, port_policy: PortPolicy) -> Config {
        Config {
            bind: addresses
                .iter()
                .map(|address| (*address).to_owned())
                .collect(),
            port: Some(port),
            port_policy,
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn bind_refuses_non_loopback_and_mixed_before_socket_attempts() {
        let invalid: &[&[&str]] = &[
            &["192.0.2.1"],
            &["2001:db8::1"],
            &["0.0.0.0"],
            &["::"],
            &["127.0.0.1", "192.0.2.1"],
            &["192.0.2.1", "127.0.0.1"],
            &["::1", "2001:db8::1"],
            &["2001:db8::1", "::1"],
            &["127.0.0.1", "::1", "192.0.2.1"],
            &["2001:db8::1", "::1", "127.0.0.1"],
        ];
        for addresses in invalid {
            let configuration = config(addresses, 0, PortPolicy::Fixed);
            let Err(error) = bind(&configuration).await else {
                panic!("public bind accepted a non-loopback list");
            };
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            let mut attempts = 0;
            let result = bind_with_socket(&configuration, |_address| {
                attempts += 1;
                std::future::ready(Err(io::Error::other("unexpected socket admission")))
            })
            .await;
            let Err(error) = result else {
                panic!("shared bind accepted a non-loopback list");
            };
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(attempts, 0, "preflight must precede every socket attempt");
        }
    }

    #[tokio::test]
    async fn bind_ipv4_and_ipv6_report_actual_loopback_identity() {
        for address in ["127.0.0.1", "::1"] {
            let expected: IpAddr = address.parse().expect("fixed loopback fixture");
            let bound = bind(&config(&[address], 0, PortPolicy::Fixed))
                .await
                .expect("native single-family loopback binding");
            assert_eq!(bound.listeners.len(), 1);
            let actual = bound.listeners[0]
                .local_addr()
                .expect("owned native listener");
            assert_eq!(actual, bound.address);
            assert_eq!(actual.ip(), expected);
            assert_eq!(actual.port(), bound.port);
            assert!(actual.port() > 0);
            assert!(actual.ip().is_loopback());
        }
    }

    #[tokio::test]
    async fn foreground_upper_ports_never_wrap_and_exhaust_to_ephemeral() {
        let mut nonzero_attempts = 0;
        let mut ephemeral_attempts = 0;
        for &(preferred, suffix) in UPPER_PORTS {
            let configuration = config(&["127.0.0.1"], preferred, PortPolicy::Foreground);
            let expected: Vec<u16> = suffix.iter().copied().chain(std::iter::once(0)).collect();
            let mut attempts = Vec::new();
            let bound = bind_with_socket(&configuration, |address: SocketAddr| {
                attempts.push(address.port());
                async move {
                    if address.port() == 0 {
                        TcpListener::bind(address).await
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "controlled occupancy",
                        ))
                    }
                }
            })
            .await
            .expect("actual ephemeral listener after controlled exhaustion");
            assert_eq!(attempts, expected);
            nonzero_attempts += attempts.iter().filter(|port| **port != 0).count();
            ephemeral_attempts += attempts.iter().filter(|port| **port == 0).count();
            assert_eq!(bound.listeners.len(), 1);
            assert_eq!(bound.listeners[0].local_addr().unwrap(), bound.address);
            assert_eq!(bound.port, bound.address.port());
            assert!(bound.port > 0);
            drop(bound);

            let mut native_attempts = Vec::new();
            let bound = bind_with_socket(&configuration, |address: SocketAddr| {
                native_attempts.push(address.port());
                TcpListener::bind(address)
            })
            .await
            .expect("native upper-range binding or final ephemeral fallback");
            assert!(!native_attempts.is_empty());
            if native_attempts.last() == Some(&0) {
                assert_eq!(native_attempts, expected);
            } else {
                assert!(native_attempts.len() <= suffix.len());
                assert_eq!(native_attempts, suffix[..native_attempts.len()]);
                assert_eq!(native_attempts.last(), Some(&bound.port));
            }
            assert_eq!(bound.listeners.len(), 1);
            assert_eq!(bound.listeners[0].local_addr().unwrap(), bound.address);
            assert_eq!(bound.address.ip(), "127.0.0.1".parse::<IpAddr>().unwrap());
            assert_eq!(bound.port, bound.address.port());
            assert!(bound.port > 0);
        }
        assert_eq!(nonzero_attempts, 55);
        assert_eq!(ephemeral_attempts, 10);
    }

    #[derive(Default)]
    struct TaskReceipts {
        dropped: AtomicU8,
        returned: AtomicU8,
        changed: Notify,
    }

    struct TaskDrop {
        receipts: Arc<TaskReceipts>,
        bit: u8,
    }

    impl Drop for TaskDrop {
        fn drop(&mut self) {
            self.receipts.dropped.fetch_or(self.bit, Ordering::SeqCst);
            self.receipts.changed.notify_one();
        }
    }

    #[derive(Clone, Copy)]
    enum OriginExit {
        Io,
        Clean,
        Panic,
    }

    #[derive(Clone, Copy)]
    enum SurvivorExit {
        Healthy,
        Error,
        Pending,
    }

    #[derive(Clone, Copy)]
    enum Trigger {
        Origin,
        SignalOk,
        SignalError,
        Cooperative,
    }

    struct Survivor {
        ready: oneshot::Receiver<()>,
        observed: oneshot::Receiver<bool>,
        release: Option<oneshot::Sender<()>>,
        returned: oneshot::Receiver<()>,
    }

    struct Origin {
        ready: oneshot::Receiver<()>,
        release: Option<oneshot::Sender<()>>,
    }

    struct Controls {
        survivors: Vec<Survivor>,
        origin: Option<Origin>,
        stop: Option<oneshot::Sender<()>>,
        signal: Option<oneshot::Sender<io::Result<()>>>,
        trigger: Trigger,
        pending: bool,
    }

    fn spawn_survivor(
        tasks: &mut JoinSet<io::Result<()>>,
        receiver: watch::Receiver<bool>,
        receipts: Arc<TaskReceipts>,
        bit: u8,
        exit: SurvivorExit,
    ) -> Survivor {
        let (ready_tx, ready) = oneshot::channel();
        let (observed_tx, observed) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let (returned_tx, returned) = oneshot::channel();
        // Capture the guard before spawning, so abort before first poll still
        // leaves a real per-case release receipt.
        let drop_guard = TaskDrop { receipts, bit };
        tasks.spawn(async move {
            let guard = drop_guard;
            assert!(
                !*receiver.borrow(),
                "survivor must be ready before shutdown"
            );
            ready_tx.send(()).expect("survivor readiness receiver");
            let observation = receiver.clone();
            wait_for_server_shutdown(Some(receiver)).await;
            observed_tx
                .send(*observation.borrow())
                .expect("shutdown observer");
            let result = match exit {
                SurvivorExit::Pending => std::future::pending::<io::Result<()>>().await,
                SurvivorExit::Healthy | SurvivorExit::Error => {
                    release_rx.await.expect("owned survivor release");
                    if matches!(exit, SurvivorExit::Error) {
                        Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "later listener failure",
                        ))
                    } else {
                        Ok(())
                    }
                }
            };
            guard.receipts.returned.fetch_or(bit, Ordering::SeqCst);
            returned_tx.send(()).expect("survivor returned receiver");
            result
        });
        Survivor {
            ready,
            observed,
            release: Some(release_tx),
            returned,
        }
    }

    fn spawn_origin(
        tasks: &mut JoinSet<io::Result<()>>,
        receiver: watch::Receiver<bool>,
        receipts: Arc<TaskReceipts>,
        exit: OriginExit,
    ) -> Origin {
        let (ready_tx, ready) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let drop_guard = TaskDrop {
            receipts,
            bit: 0b100,
        };
        tasks.spawn(async move {
            let guard = drop_guard;
            assert!(!*receiver.borrow(), "origin must be ready before shutdown");
            ready_tx.send(()).expect("origin readiness receiver");
            release_rx.await.expect("controlled origin release");
            guard
                .receipts
                .returned
                .fetch_or(guard.bit, Ordering::SeqCst);
            match exit {
                OriginExit::Io => Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "origin listener failure",
                )),
                OriginExit::Clean => Ok(()),
                OriginExit::Panic => panic!("listener control origin panic"),
            }
        });
        Origin {
            ready,
            release: Some(release_tx),
        }
    }

    async fn exercise_controls(
        controls: &mut Controls,
        result_rx: &mut oneshot::Receiver<io::Result<()>>,
    ) -> (io::Result<()>, Duration) {
        for survivor in &mut controls.survivors {
            (&mut survivor.ready)
                .await
                .expect("actual survivor readiness");
        }
        if let Some(origin) = &mut controls.origin {
            (&mut origin.ready).await.expect("actual origin readiness");
        }
        let started = Instant::now();
        match controls.trigger {
            Trigger::Origin => controls
                .origin
                .as_mut()
                .expect("owned origin")
                .release
                .take()
                .expect("origin released once")
                .send(())
                .expect("origin remains live"),
            Trigger::SignalOk => controls
                .signal
                .take()
                .expect("signal returned once")
                .send(Ok(()))
                .expect("controlled signal remains live"),
            Trigger::SignalError => controls
                .signal
                .take()
                .expect("signal returned once")
                .send(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "origin signal failure",
                )))
                .expect("controlled signal remains live"),
            Trigger::Cooperative => controls
                .stop
                .take()
                .expect("composition stop once")
                .send(())
                .expect("controlled composition remains live"),
        }
        for survivor in &mut controls.survivors {
            assert!(
                (&mut survivor.observed)
                    .await
                    .expect("actual shutdown notification")
            );
        }
        assert!(
            matches!(
                result_rx.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "controller returned while both survivors remained held"
        );
        if !controls.pending {
            for (index, survivor) in controls.survivors.iter_mut().enumerate() {
                survivor
                    .release
                    .take()
                    .expect("survivor released once")
                    .send(())
                    .expect("actual survivor remains held");
                (&mut survivor.returned)
                    .await
                    .expect("actual survivor return boundary");
                if index == 0 {
                    assert!(
                        matches!(
                            result_rx.try_recv(),
                            Err(oneshot::error::TryRecvError::Empty)
                        ),
                        "controller returned before the healthy survivor was released"
                    );
                }
            }
        }
        let result = result_rx.await.expect("owned controller result");
        (result, started.elapsed())
    }

    async fn wait_for_drops(receipts: &TaskReceipts, expected: u8) {
        while receipts.dropped.load(Ordering::SeqCst) != expected {
            receipts.changed.notified().await;
        }
    }

    async fn listener_case(
        origin_exit: Option<OriginExit>,
        trigger: Trigger,
        later_failure: bool,
        pending: bool,
    ) -> (io::Result<()>, Duration) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(13);
        let receipts = Arc::new(TaskReceipts::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut tasks = JoinSet::new();
        let survivor_exits = if pending {
            [SurvivorExit::Pending, SurvivorExit::Pending]
        } else if later_failure {
            [SurvivorExit::Error, SurvivorExit::Healthy]
        } else {
            [SurvivorExit::Healthy, SurvivorExit::Healthy]
        };
        let survivors = survivor_exits
            .into_iter()
            .enumerate()
            .map(|(index, exit)| {
                spawn_survivor(
                    &mut tasks,
                    shutdown_rx.clone(),
                    Arc::clone(&receipts),
                    1 << index,
                    exit,
                )
            })
            .collect();
        let origin = origin_exit
            .map(|exit| spawn_origin(&mut tasks, shutdown_rx, Arc::clone(&receipts), exit));
        let expected_drops = if origin_exit.is_some() { 0b111 } else { 0b011 };
        let expected_returns = if pending { 0b100 } else { expected_drops };
        let (stop_tx, stop_rx) = oneshot::channel();
        let (signal_tx, signal_rx) = oneshot::channel();
        let (result_tx, mut result_rx) = oneshot::channel();
        let mut controllers = JoinSet::new();
        controllers.spawn(async move {
            let result = supervise_listener_tasks(
                tasks,
                shutdown_tx,
                async { stop_rx.await.expect("controlled composition future") },
                async { signal_rx.await.expect("controlled signal future") },
            )
            .await;
            let _ = result_tx.send(result);
        });
        let mut controls = Controls {
            survivors,
            origin,
            stop: Some(stop_tx),
            signal: Some(signal_tx),
            trigger,
            pending,
        };
        // Keep the owning controller set outside the assertion future. A failed
        // oracle or outer guard cannot detach the controller during unwinding.
        let checked = tokio::time::timeout_at(
            deadline,
            AssertUnwindSafe(exercise_controls(&mut controls, &mut result_rx)).catch_unwind(),
        )
        .await;
        let interrupted = !matches!(&checked, Ok(Ok(_)));
        if interrupted {
            controllers.abort_all();
        }
        let mut joined = 0;
        let mut join_failed = false;
        while let Some(result) = controllers.join_next().await {
            joined += 1;
            join_failed |= result.is_err();
        }
        if interrupted {
            // These captured tasks contain only yielding channel/watch/pending
            // futures. Confirm their real Drop receipts after controller reap.
            wait_for_drops(&receipts, expected_drops).await;
        }
        let value = match checked {
            Ok(Ok(value)) => value,
            Ok(Err(payload)) => std::panic::resume_unwind(payload),
            Err(_) => panic!("owned listener control exceeded its thirteen-second fixture guard"),
        };
        assert_eq!(joined, 1, "reap the actual owned controller");
        assert!(
            !join_failed,
            "owned controller must finish without panic or abort"
        );
        assert_eq!(receipts.dropped.load(Ordering::SeqCst), expected_drops);
        assert_eq!(
            receipts.returned.load(Ordering::SeqCst),
            expected_returns,
            "healthy held survivors must return naturally, not be aborted on an early error"
        );
        value
    }

    fn expect_error(result: io::Result<()>, kind: io::ErrorKind, message: &str) {
        let error = result.expect_err("controlled failure must remain visible");
        assert_eq!(error.kind(), kind);
        assert_eq!(error.to_string(), message);
    }

    #[tokio::test]
    async fn io_failure_broadcasts_and_joins_every_survivor() {
        let (result, _) = listener_case(Some(OriginExit::Io), Trigger::Origin, true, false).await;
        expect_error(
            result,
            io::ErrorKind::ConnectionReset,
            "origin listener failure",
        );
    }

    #[tokio::test]
    async fn clean_listener_exit_broadcasts_and_joins_every_survivor() {
        let (result, _) =
            listener_case(Some(OriginExit::Clean), Trigger::Origin, true, false).await;
        expect_error(
            result,
            io::ErrorKind::Other,
            "dashboard listener exited before shutdown",
        );
    }

    #[tokio::test]
    async fn listener_panic_broadcasts_and_joins_every_survivor() {
        let (result, _) =
            listener_case(Some(OriginExit::Panic), Trigger::Origin, true, false).await;
        let error = result.expect_err("actual listener panic must remain visible");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        let message = error.to_string();
        assert!(message.starts_with("dashboard listener task failed: "));
        assert!(message.contains("listener control origin panic"));
    }

    #[tokio::test]
    async fn signal_error_broadcasts_and_preserves_origin_while_draining() {
        let (result, _) = listener_case(None, Trigger::SignalError, true, false).await;
        expect_error(
            result,
            io::ErrorKind::PermissionDenied,
            "origin signal failure",
        );
    }

    #[tokio::test]
    async fn cooperative_stop_retains_drain_error_and_joins_every_survivor() {
        let (result, _) = listener_case(None, Trigger::Cooperative, true, false).await;
        expect_error(
            result,
            io::ErrorKind::ConnectionAborted,
            "later listener failure",
        );
    }

    #[tokio::test]
    async fn successful_signal_broadcasts_and_joins_every_survivor() {
        let (result, _) = listener_case(None, Trigger::SignalOk, false, false).await;
        result.expect("successful signal drains every healthy listener");
    }

    #[tokio::test]
    async fn cooperative_stop_broadcasts_and_joins_every_survivor() {
        let (result, _) = listener_case(None, Trigger::Cooperative, false, false).await;
        result.expect("composition stop drains every healthy listener");
    }

    #[tokio::test]
    async fn drain_timeout_aborts_and_joins_every_survivor_after_origin_error() {
        let (result, elapsed) =
            listener_case(Some(OriginExit::Io), Trigger::Origin, false, true).await;
        assert!(
            elapsed >= Duration::from_secs(10),
            "exercise the real production drain timer"
        );
        expect_error(
            result,
            io::ErrorKind::TimedOut,
            "dashboard connection drain exceeded its deadline",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{HeaderMap, Request, StatusCode, header};
    use std::path::PathBuf;
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const SESSION_COOKIE_NAME: &str = "locron_session";
    const CSRF_COOKIE_NAME: &str = "csrf_token";

    fn held_ipv4_port_with_probe_room() -> std::net::TcpListener {
        loop {
            let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            if held.local_addr().unwrap().port() <= u16::MAX - 10 {
                return held;
            }
        }
    }

    fn test_state() -> AppState {
        AppState {
            paths: StatePaths::new(PathBuf::from("/nonexistent/test-state")),
            token: TOKEN.to_owned(),
            bound_port: 10_824,
        }
    }

    /// Sends a request through the full middleware chain; `body` is optional (POST bodies are
    /// sent when the test needs the handler to parse them).
    async fn request(
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:10824");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = match body {
            Some(body) => builder.body(Body::from(body.to_owned())).expect("request"),
            None => builder.body(Body::empty()).expect("request"),
        };
        let response = router(test_state())
            .oneshot(request)
            .await
            .expect("router responds");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, headers, bytes.to_vec())
    }

    fn session_cookies(headers: &HeaderMap) -> (Option<String>, Option<String>) {
        let mut session = None;
        let mut csrf = None;
        for value in headers.get_all(header::SET_COOKIE) {
            let value = value.to_str().expect("ascii");
            let name = value.split('=').next().expect("name");
            let content = value.split_once(';').map_or(value, |(content, _)| content);
            let cookie_value = content
                .split_once('=')
                .map_or("", |(_, cookie_value)| cookie_value);
            if name == SESSION_COOKIE_NAME {
                session = Some(cookie_value.to_owned());
            } else if name == CSRF_COOKIE_NAME {
                csrf = Some(cookie_value.to_owned());
            }
        }
        (session, csrf)
    }

    fn paste_body() -> String {
        format!(r#"{{"token":"{TOKEN}"}}"#)
    }

    fn built_asset(html: &str, marker: &str) -> String {
        let start = html.find(marker).expect("built asset marker") + marker.len();
        let rest = &html[start..];
        format!(
            "/assets/{}",
            rest.split('"').next().expect("built asset path")
        )
    }

    async fn authenticated_paste() -> (String, String) {
        let (_, headers, _) = request(
            "POST",
            "/api/v1/session",
            &[
                ("content-type", "application/json"),
                ("origin", "http://localhost:10824"),
            ],
            Some(&paste_body()),
        )
        .await;
        let (session, csrf) = session_cookies(&headers);
        (session.expect("session cookie"), csrf.expect("csrf cookie"))
    }

    #[tokio::test]
    async fn entry_page_is_the_only_unauthenticated_response() {
        let (status, headers, body) = request("GET", "/", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["content-type"], "text/html");
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains("id=\"root\""),
            "entry page hosts the React root"
        );
        assert!(
            html.contains("locron.theme"),
            "entry page applies theme before paint"
        );
        assert!(!html.contains(TOKEN), "entry page never contains the token");

        // The viewer bundle is public so the entry page can boot (the paste
        // form is served by app.js); every /api/v1 route is token-gated.
        let script = built_asset(&html, "src=\"/assets/");
        let stylesheet = built_asset(&html, "href=\"/assets/");
        for (method, path) in [
            ("GET", script.as_str()),
            ("GET", stylesheet.as_str()),
            ("GET", "/favicon.svg"),
        ] {
            let (status, _, _) = request(method, path, &[], None).await;
            assert_eq!(status, StatusCode::OK, "{method} {path} must be public");
        }
        for (method, path) in [
            ("GET", "/api/v1/session"),
            ("GET", "/api/v1/jobs"),
            ("POST", "/api/v1/jobs"),
            ("POST", "/api/v1/prune"),
        ] {
            let (status, _, body) = request(method, path, &[], None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
            assert_eq!(value["schema"], "locron.api/v1");
            assert_eq!(value["ok"], false);
            assert_eq!(value["error"]["code"], "unauthenticated");
        }

        // A token in a URL query is not authentication and never leaks into a response.
        let (status, _, _) = request("GET", "/?token=secret", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = request("GET", "/api/v1/jobs?token=secret", &[], None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_allowlist_accepts_loopback_forms_and_refuses_everything_else() {
        for host in [
            "localhost",
            "localhost:10824",
            "LOCALHOST",
            "127.0.0.1",
            "127.0.0.1:9999",
            "[::1]",
            "[::1]:10824",
        ] {
            let builder = Request::builder()
                .uri("/")
                .header(header::HOST, host)
                .body(Body::empty())
                .expect("request");
            let response = router(test_state())
                .oneshot(builder)
                .await
                .expect("router responds");
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "host {host} must be allowed"
            );
        }
        for host in [
            "attacker.example",
            "attacker.example:10824",
            "192.168.1.10",
            "0.0.0.0",
            "localhost.attacker.example",
            "10.0.0.1:80",
        ] {
            let builder = Request::builder()
                .uri("/")
                .header(header::HOST, host)
                .body(Body::empty())
                .expect("request");
            let response = router(test_state())
                .oneshot(builder)
                .await
                .expect("router responds");
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "host {host} must be refused"
            );
        }
        let builder = Request::builder()
            .uri("/")
            .body(Body::empty())
            .expect("request");
        let response = router(test_state())
            .oneshot(builder)
            .await
            .expect("router responds");
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "missing Host refused"
        );
    }

    #[tokio::test]
    async fn origin_mismatch_on_unsafe_methods_is_refused() {
        for origin in [
            "http://localhost:10824",
            "http://127.0.0.1:10824",
            "http://[::1]:10824",
        ] {
            let (status, _, _) = request(
                "POST",
                "/api/v1/session",
                &[("content-type", "application/json"), ("origin", origin)],
                Some(&paste_body()),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "origin {origin} must match");
        }
        for origin in [
            "http://localhost:9999",
            "http://127.0.0.1:9999",
            "https://localhost:10824",
            "http://attacker.example",
            "http://localhost.attacker.example:10824",
        ] {
            let (status, _, body) = request(
                "POST",
                "/api/v1/session",
                &[("content-type", "application/json"), ("origin", origin)],
                Some(&paste_body()),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "origin {origin} must be refused"
            );
            let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
            assert_eq!(value["error"]["code"], "refused");
        }
        // Absent Origin is allowed (curl, same-origin navigations).
        let (status, _, _) = request(
            "POST",
            "/api/v1/session",
            &[("content-type", "application/json")],
            Some(&paste_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        // An Origin on a safe method is not refused.
        let (status, _, _) =
            request("GET", "/", &[("origin", "http://attacker.example")], None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_token_accepts_and_rejects() {
        let (status, _, body) = request(
            "GET",
            "/api/v1/session",
            &[("authorization", &format!("token {TOKEN}"))],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
        assert_eq!(value["data"]["authenticated"], true);

        for wrong in ["token wrong", "Token wrong", "bearer wrong", "token "] {
            let (status, _, _) =
                request("GET", "/api/v1/session", &[("authorization", wrong)], None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "wrong bearer {wrong:?}");
        }
        let wrong = format!("token {}", &TOKEN[..63]);
        let (status, _, _) = request(
            "GET",
            "/api/v1/session",
            &[("authorization", wrong.as_str())],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "truncated token rejected");
        let (status, _, _) = request("GET", "/", &[("authorization", "token wrong")], None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn entry_paste_sets_session_and_csrf_cookies() {
        let (status, headers, _) = request(
            "POST",
            "/api/v1/session",
            &[
                ("content-type", "application/json"),
                ("origin", "http://localhost:10824"),
            ],
            Some(&paste_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (session, csrf) = session_cookies(&headers);
        let session = session.expect("session cookie set");
        let csrf = csrf.expect("csrf cookie set");
        assert_eq!(session, TOKEN);
        assert_eq!(csrf.len(), 64);

        let cookie_header = format!("locron_session={session}; csrf_token={csrf}");
        let (_, _, index) = request("GET", "/", &[], None).await;
        let index = String::from_utf8_lossy(&index);
        let script = built_asset(&index, "src=\"/assets/");
        let (status, _, body) =
            request("GET", &script, &[("cookie", cookie_header.as_str())], None).await;
        assert_eq!(status, StatusCode::OK, "authenticated assets load");
        assert!(
            String::from_utf8_lossy(&body).contains("authenticated"),
            "the app shell boots from the authenticated session response"
        );

        // Wrong pasted token is rejected (401 from the handler, before any cookie is set).
        let wrong = format!(r#"{{"token":"{}"}}"#, "f".repeat(64));
        let (status, _, body) = request(
            "POST",
            "/api/v1/session",
            &[
                ("content-type", "application/json"),
                ("origin", "http://localhost:10824"),
            ],
            Some(&wrong),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
        assert_eq!(value["error"]["code"], "unauthenticated");
    }

    /// Cookie-authenticated POST to the session endpoint with explicit content type, extra
    /// headers, and a body. The content type is passed explicitly because the shared `request`
    /// helper appends header values (a duplicate content-type would leave the middleware
    /// sniffing the first value).
    async fn session_post(
        cookie: &str,
        content_type: &str,
        extra: &[(&str, &str)],
        body: Option<&str>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut headers = vec![
            ("content-type", content_type),
            ("cookie", cookie),
            ("origin", "http://localhost:10824"),
        ];
        headers.extend_from_slice(extra);
        request("POST", "/api/v1/session", &headers, body).await
    }

    #[tokio::test]
    async fn csrf_double_submit_with_bearer_exemption() {
        let (session, csrf) = authenticated_paste().await;
        let cookie = format!("locron_session={session}; csrf_token={csrf}");

        let (status, _, _) = session_post(
            &cookie,
            "application/json",
            &[("x-csrf-token", csrf.as_str())],
            Some(&paste_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "matching CSRF header accepted");

        for bad in ["wrong", ""] {
            let (status, _, body) = session_post(
                &cookie,
                "application/json",
                &[("x-csrf-token", bad)],
                Some(&paste_body()),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "CSRF mismatch {bad:?}");
            let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
            assert_eq!(value["error"]["code"], "refused");
        }
        let (status, _, body) =
            session_post(&cookie, "application/json", &[], Some(&paste_body())).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "missing CSRF header refused");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("envelope");
        assert_eq!(value["error"]["code"], "refused");

        // The urlencoded form-field echo passes the CSRF check (the handler's Json extractor then
        // rejects the non-JSON content type with 415 — never 403 — proving the middleware let it
        // through).
        let form = format!("csrf_token={csrf}");
        let (status, _, _) = session_post(
            &cookie,
            "application/x-www-form-urlencoded",
            &[],
            Some(&form),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "form field accepted by CSRF"
        );

        // Bearer-authenticated mutations skip the CSRF check entirely.
        let (status, _, _) = request(
            "POST",
            "/api/v1/session",
            &[
                ("content-type", "application/json"),
                ("authorization", &format!("token {TOKEN}")),
            ],
            Some(&paste_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "bearer exemption");
    }

    #[tokio::test]
    async fn referrer_policy_is_set_on_every_response() {
        let (_, headers, _) = request("GET", "/", &[], None).await;
        assert_eq!(headers["referrer-policy"], "no-referrer");
        let (_, headers, _) = request(
            "GET",
            "/api/v1/session",
            &[("authorization", &format!("token {TOKEN}"))],
            None,
        )
        .await;
        assert_eq!(headers["referrer-policy"], "no-referrer");
        let (_, headers, _) = request(
            "GET",
            "/api/v1/jobs",
            &[("authorization", "token wrong")],
            None,
        )
        .await;
        assert_eq!(
            headers["referrer-policy"], "no-referrer",
            "error responses too"
        );
    }

    #[tokio::test]
    async fn bind_policies_select_ports() {
        let config = Config {
            port: Some(0),
            port_policy: PortPolicy::Fixed,
            ..Config::default()
        };
        let bound = bind(&config).await.expect("bind");
        assert_eq!(bound.warnings.len(), 0);
        assert!(bound.port > 0);
        assert_eq!(bound.address.port(), bound.port);
        assert!(
            bound.listeners.iter().all(|listener| {
                listener
                    .local_addr()
                    .is_ok_and(|address| address.port() == bound.port)
            }),
            "all loopback listeners share the reported OS-assigned port"
        );
    }

    #[tokio::test]
    async fn foreground_policy_falls_back_from_owned_ipv4_port() {
        let held = held_ipv4_port_with_probe_room();
        let port = held.local_addr().unwrap().port();
        let config = Config {
            bind: vec![std::net::Ipv4Addr::LOCALHOST.to_string()],
            port: Some(port),
            port_policy: PortPolicy::Foreground,
            ..Config::default()
        };

        let bound = bind(&config).await.expect("foreground fallback bind");

        assert_ne!(bound.port, port);
        assert!(bound.address.is_ipv4());
        assert!(bound.warnings.is_empty());
        drop(held);
    }

    #[tokio::test]
    async fn fixed_policy_rejects_owned_ipv4_port() {
        let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let config = Config {
            bind: vec![std::net::Ipv4Addr::LOCALHOST.to_string()],
            port: Some(port),
            port_policy: PortPolicy::Fixed,
            ..Config::default()
        };

        let Err(error) = bind(&config).await else {
            panic!("fixed bind must reject conflict");
        };

        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
        drop(held);
    }

    #[tokio::test]
    async fn bound_address_reports_ipv6_when_ipv4_is_unavailable() {
        let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        if std::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port)).is_err() {
            eprintln!("SKIPPED: IPv6 loopback is unavailable");
            return;
        }
        let config = Config {
            port: Some(port),
            port_policy: PortPolicy::Fixed,
            ..Config::default()
        };
        let bound = bind(&config).await.expect("IPv6 fallback bind");
        assert_eq!(
            bound.address,
            SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port))
        );
        assert_eq!(
            format!("http://{}/", bound.address),
            format!("http://[::1]:{port}/")
        );
        assert!(
            bound
                .warnings
                .iter()
                .any(|warning| warning.contains("127.0.0.1")),
            "the failed IPv4 bind remains visible: {:?}",
            bound.warnings
        );
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use futures_util::StreamExt;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    struct RunningServer {
        paths: StatePaths,
        address: SocketAddr,
        token: String,
        client: reqwest::Client,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
        task: Option<tokio::task::JoinHandle<io::Result<()>>>,
        _temporary: tempfile::TempDir,
    }

    impl RunningServer {
        async fn start() -> Self {
            let temporary = tempfile::tempdir().expect("temporary parent");
            let root = temporary.path().join("private-dashboard");
            let guard = locron_core::filesystem::DirectoryGuard::private(&root)
                .expect("private dashboard fixture");
            let paths = StatePaths::new(guard.normalized_path().to_path_buf());
            paths.ensure().expect("state layout");
            let token = token::ensure(&paths).expect("private token");
            let bound = bind(&Config {
                bind: vec![std::net::Ipv4Addr::LOCALHOST.to_string()],
                port: Some(0),
                port_policy: PortPolicy::Fixed,
                ..Config::default()
            })
            .await
            .expect("ephemeral dashboard listener");
            let address = bound.address;
            let (stop, stopped) = tokio::sync::oneshot::channel();
            let state_paths = paths.clone();
            let task = tokio::spawn(serve_until(bound, state_paths, async {
                let _ = stopped.await;
            }));
            Self {
                paths,
                address,
                token,
                client: reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .expect("loopback client"),
                stop: Some(stop),
                task: Some(task),
                _temporary: temporary,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{path}", self.address)
        }

        async fn post(&self, path: &str, body: Option<&Value>) -> Value {
            let mut request = self
                .client
                .post(self.url(path))
                .header("authorization", format!("token {}", self.token));
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.expect("dashboard mutation");
            let status = response.status();
            let body: Value = response.json().await.expect("mutation envelope");
            assert!(status.is_success(), "{status}: {body}");
            assert_eq!(body["ok"], true, "{body}");
            body
        }

        async fn queue_run(&self) -> String {
            let job = self
                .post(
                    "/api/v1/jobs",
                    Some(&json!({
                        "name": "dashboard shutdown fixture",
                        "description": null,
                        "tags": [],
                        "enabled": true,
                        "definition": {
                            "schedule": {"kind": "cron", "expression": "* * * * *", "timezone": {"mode": "local"}},
                            "target": {"kind": "process", "executable": std::env::current_exe().expect("native executable"), "args": []},
                            "cwd": self.paths.root,
                            "environment": {"values": {}},
                            "policy": {
                                "overlap": "skip", "missed_run": "skip", "catch_up_limit": 10,
                                "retries": 0, "retry_delay": 0, "retry_cap": 0,
                                "backoff": "exponential", "retry_timeout": false,
                                "timeout": null, "start_deadline": null,
                                "termination_grace": 0, "per_job_concurrency": 1
                            }
                        }
                    })),
                )
                .await;
            let job_id = job["data"]["id"].as_str().expect("job id");
            let run = self.post(&format!("/api/v1/jobs/{job_id}/run"), None).await;
            assert_eq!(run["data"]["state"], "queued");
            run["data"]["run_id"].as_str().expect("run id").to_owned()
        }

        async fn shutdown(&mut self, deadline: Duration) -> io::Result<()> {
            self.stop.take().expect("owned stop sender").send(()).ok();
            let mut task = self.task.take().expect("owned server task");
            if let Ok(result) = tokio::time::timeout(deadline, &mut task).await {
                result.expect("server task did not panic")
            } else {
                task.abort();
                let _ = task.await;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "dashboard fixture exceeded its outer deadline",
                ))
            }
        }
    }

    impl Drop for RunningServer {
        fn drop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cooperative_shutdown_closes_active_sse_without_cancelling_a_durable_run() {
        let mut server = RunningServer::start().await;
        let run_id = server.queue_run().await;
        let response = server
            .client
            .get(server.url(&format!("/api/v1/runs/{run_id}/stream")))
            .header("authorization", format!("token {}", server.token))
            .send()
            .await
            .expect("active SSE request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let mut stream = response.bytes_stream();
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut bytes = Vec::new();
            while !String::from_utf8_lossy(&bytes).contains("event: run") {
                bytes.extend_from_slice(
                    &stream
                        .next()
                        .await
                        .expect("initial SSE event")
                        .expect("SSE bytes"),
                );
            }
        })
        .await
        .expect("SSE fixture was active before shutdown");

        server
            .shutdown(Duration::from_secs(3))
            .await
            .expect("active SSE must permit graceful dashboard shutdown");
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(chunk) = stream.next().await {
                chunk.expect("finite SSE completion");
            }
        })
        .await
        .expect("SSE response closes after dashboard shutdown");

        let store = locron_store::Store::open(server.paths.clone(), "test", 1)
            .expect("durable state after dashboard exit");
        let run = store.run(&run_id).expect("queued run survives");
        assert_eq!(run.state, "queued");
        assert_eq!(run.finished_at_us, None);
        assert!(
            !store
                .cancellation_requested(&run_id)
                .expect("cancellation fact")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stalled_headers_and_request_bodies_have_a_finite_dashboard_drain() {
        let mut server = RunningServer::start().await;
        let mut headers = tokio::net::TcpStream::connect(server.address)
            .await
            .expect("stalled header client");
        headers
            .write_all(b"GET /api/v1/jobs HTTP/1.1\r\nHost: 127.0.0.1\r\n")
            .await
            .expect("partial headers");
        let mut body = tokio::net::TcpStream::connect(server.address)
            .await
            .expect("stalled body client");
        body.write_all(
            format!(
                "POST /api/v1/jobs HTTP/1.1\r\nHost: {}\r\nAuthorization: token {}\r\nContent-Type: application/json\r\nContent-Length: 10000\r\n\r\n{{",
                server.address, server.token
            )
            .as_bytes(),
        )
        .await
        .expect("partial body");
        let response = server
            .client
            .get(server.url("/api/v1/session"))
            .header("authorization", format!("token {}", server.token))
            .send()
            .await
            .expect("listener remains responsive");
        assert!(response.status().is_success());
        tokio::time::sleep(Duration::from_millis(50)).await;

        let start = std::time::Instant::now();
        let result = server.shutdown(Duration::from_secs(13)).await;
        if let Err(error) = result {
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert_eq!(
                error.to_string(),
                "dashboard connection drain exceeded its deadline"
            );
        }
        assert!(start.elapsed() < Duration::from_secs(12));
        // The transport fixture observes serve_until. Actual role/launcher exit is checked by
        // the CLI lifecycle suite, since axum can own detached connection tasks until runtime drop.
        drop((headers, body));
    }
}
