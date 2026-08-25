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

use axum::Router;
use axum::routing::{get, post, put};
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
    let preferred = config.port.unwrap_or(DEFAULT_PORT);
    let mut warnings = Vec::new();
    match config.port_policy {
        PortPolicy::Fixed => {
            let (address, listeners) = bind_all(&addresses, preferred, &mut warnings).await?;
            Ok(BoundServer {
                port: address.port(),
                address,
                warnings,
                listeners,
            })
        }
        PortPolicy::Foreground => {
            let mut last_error = None;
            for port in preferred..preferred + 10 {
                match bind_all(&addresses, port, &mut warnings).await {
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
            let (address, listeners) = bind_all(&addresses, 0, &mut warnings)
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

async fn bind_all(
    addresses: &[IpAddr],
    port: u16,
    warnings: &mut Vec<String>,
) -> io::Result<(SocketAddr, Vec<TcpListener>)> {
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
        match TcpListener::bind(SocketAddr::new(*address, candidate_port)).await {
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
    let token = token::ensure(&paths)?;
    let state = AppState {
        paths,
        token,
        bound_port: bound.port,
    };
    let app = router(state);
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let mut tasks = tokio::task::JoinSet::new();
    for listener in bound.listeners {
        let app = app.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        tasks.spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.recv().await;
                })
                .await
        });
    }
    tokio::signal::ctrl_c().await?;
    let _ = shutdown_tx.send(());
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error),
            Err(_) => return Err(io::Error::other("dashboard listener task failed")),
        }
    }
    Ok(())
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
