//! The security middleware chain: Host allowlist, Origin check, token authentication, CSRF
//! double-submit, and `Referrer-Policy` injection.
//!
//! The chain is applied with `Router::layer` after all routes are registered so every route and
//! the asset fallback pass through it. Execution order is [`referrer_policy`] (outermost), then
//! [`host`], then [`origin`], then [`authenticate`], then [`csrf`] (innermost), matching the order
//! the layers are applied (the last applied layer is the outermost in axum). `referrer_policy` is
//! outermost so every response carries the header, including responses short-circuited by the
//! inner middleware.
//!
//! Authentication outcome (`[`AuthKind`]`) is recorded in request extensions by [`authenticate`]
//! and read by [`csrf`]. The only unauthenticated access is the entry page and the static viewer
//! bundle it references (GETs outside `/api/`) and the one-time token paste
//! (`POST /api/v1/session`); every `/api/v1` route returns 401 without a token.

use axum::extract::{Request, State};
use axum::http::header::{self, HeaderMap, HeaderName};
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;
use crate::envelope;

/// Name of the session cookie (value is the access token itself).
pub const SESSION_COOKIE: &str = "locron_session";
/// Name of the double-submit CSRF cookie.
pub const CSRF_COOKIE: &str = "csrf_token";
/// Header carrying the CSRF value on cookie-authenticated mutations.
pub const CSRF_HEADER: &str = "x-csrf-token";
/// Maximum body bytes buffered when sniffing the CSRF value from a form field.
const CSRF_FORM_LIMIT: usize = 1_048_576;

/// How a request authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    /// No valid bearer token and no valid session cookie.
    Unauthenticated,
    /// Valid `Authorization: token <t>` header.
    Bearer,
    /// Valid `locron_session` cookie.
    Session,
}

/// Hosts accepted by the allowlist; the hostname is compared case-insensitively with the port
/// ignored and IPv6 in canonical bracket form.
const ALLOWED_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Extracts the hostname from a Host header value (`localhost:10824`, `[::1]:10824`, bare).
fn hostname_from_host_header(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split_once(']').map_or(rest, |(inner, _)| inner);
    }
    match host.rsplit_once(':') {
        Some((hostname, port))
            if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            hostname
        }
        _ => host,
    }
}

fn hostname_is_allowed(hostname: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|allowed| hostname.eq_ignore_ascii_case(allowed))
}

/// Parses a named cookie value from a Cookie header; values are plain hex, no escaping involved.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

/// Constant-time comparison for the token; both sides are always 64 hex characters.
pub(crate) fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (a, b) in left.bytes().zip(right.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Host allowlist: the hostname must be a loopback name; anything else is refused before routing,
/// which defeats DNS-rebinding attacks.
pub async fn host(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return envelope::error(
            StatusCode::FORBIDDEN,
            "refused",
            "request has no Host header",
        );
    };
    if !hostname_is_allowed(hostname_from_host_header(host)) {
        return envelope::error(
            StatusCode::FORBIDDEN,
            "refused",
            format!("Host {host} is not a loopback name of this server"),
        );
    }
    let _ = state;
    next.run(request).await
}

/// Origin check on unsafe methods: a present Origin must be the loopback server origin (http,
/// allowlisted hostname, bound port). An absent Origin is allowed (same-origin navigations, curl,
/// EventSource).
pub async fn origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) && let Some(value) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        && !origin_matches(&state, value)
    {
        return envelope::error(
            StatusCode::FORBIDDEN,
            "refused",
            format!("Origin {value} is not the server's loopback origin"),
        );
    }
    next.run(request).await
}

fn origin_matches(state: &AppState, origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let host_port = rest
        .split_once('/')
        .map_or(rest, |(host_port, _)| host_port);
    let hostname = hostname_from_host_header(host_port);
    if !hostname_is_allowed(hostname) {
        return false;
    }
    match host_port.rsplit_once(':') {
        Some((_, port)) => port
            .parse::<u16>()
            .is_ok_and(|parsed| parsed == state.bound_port),
        None => state.bound_port == 80,
    }
}

/// Token authentication: `Authorization: token <t>` or the session cookie, with GETs outside
/// `/api/` (the entry page and the static viewer bundle it references) and the one-time paste
/// (`POST /api/v1/session`) as the only unauthenticated routes. The bundle must be public because
/// it loads before any token exists — the paste form is served by `app.js` — and it carries no
/// data; every `/api/v1` route is token-gated.
pub async fn authenticate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let auth = match parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        Some(bearer) => {
            if let Some(token) = bearer
                .strip_prefix("token ")
                .or_else(|| bearer.strip_prefix("Token "))
            {
                if constant_time_eq(token, &state.token) {
                    AuthKind::Bearer
                } else {
                    return unauthorized(&parts.uri);
                }
            } else {
                return unauthorized(&parts.uri);
            }
        }
        None => match cookie_value(&parts.headers, SESSION_COOKIE) {
            Some(cookie) if constant_time_eq(&cookie, &state.token) => AuthKind::Session,
            _ => AuthKind::Unauthenticated,
        },
    };
    let entry_request = parts.method == Method::GET && !parts.uri.path().starts_with("/api/");
    let paste_request = parts.method == Method::POST && parts.uri.path() == "/api/v1/session";
    if auth == AuthKind::Unauthenticated && !entry_request && !paste_request {
        return unauthorized(&parts.uri);
    }
    parts.extensions.insert(auth);
    next.run(Request::from_parts(parts, body)).await
}

fn unauthorized(uri: &Uri) -> Response {
    envelope::error(
        StatusCode::UNAUTHORIZED,
        "unauthenticated",
        format!("a valid access token or session cookie is required for {uri}"),
    )
}

/// CSRF double-submit: a cookie-authenticated unsafe request must echo the `csrf_token` cookie
/// value in the `X-CSRF-Token` header or an urlencoded `csrf_token` form field. Bearer-token
/// requests are exempt (a cross-site page cannot attach the Authorization header), as is the
/// unauthenticated token paste.
pub async fn csrf(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let auth = request
        .extensions()
        .get::<AuthKind>()
        .copied()
        .unwrap_or(AuthKind::Unauthenticated);
    let unsafe_method = matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    );
    if unsafe_method && auth == AuthKind::Session {
        let Some(cookie) = request
            .headers()
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|pair| {
                    let (key, value) = pair.trim().split_once('=')?;
                    (key == CSRF_COOKIE).then(|| value.to_owned())
                })
            })
        else {
            return envelope::error(
                StatusCode::FORBIDDEN,
                "refused",
                "cookie-authenticated mutations require a CSRF token",
            );
        };
        let echoed = match request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|value| value.to_str().ok())
        {
            Some(value) => Some(value.to_owned()),
            None => csrf_from_form_field(&mut request).await,
        };
        if !echoed.is_some_and(|value| constant_time_eq(&value, &cookie)) {
            return envelope::error(
                StatusCode::FORBIDDEN,
                "refused",
                "X-CSRF-Token does not match the csrf_token cookie",
            );
        }
    }
    let _ = state;
    next.run(request).await
}

/// Buffers an urlencoded body (bounded) and returns its `csrf_token` field, if present. CSRF
/// values are 64 hex characters, so no percent-decoding is needed.
async fn csrf_from_form_field(request: &mut Request) -> Option<String> {
    let is_form = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("application/x-www-form-urlencoded"));
    if !is_form {
        return None;
    }
    let (parts, body) = std::mem::take(request).into_parts();
    let bytes = axum::body::to_bytes(body, CSRF_FORM_LIMIT).await.ok()?;
    let body_str = String::from_utf8_lossy(&bytes);
    let field = body_str.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "csrf_token").then(|| value.to_owned())
    });
    *request = Request::from_parts(parts, axum::body::Body::from(bytes));
    field
}

/// Injects `Referrer-Policy: no-referrer` on every response. Applied as the outermost layer so
/// responses short-circuited by the security middleware also carry it.
///
/// # Panics
///
/// Never panics in practice: the static header value always parses.
pub async fn referrer_policy(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let _ = state;
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        "no-referrer".parse().unwrap(),
    );
    response
}
