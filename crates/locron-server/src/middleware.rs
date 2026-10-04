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
use axum::http::{Method, StatusCode};
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

/// Distinguishes a genuinely absent field from invalid or ambiguous supplied values.
fn single_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, ()> {
    let all = headers.get_all(name);
    let mut values = all.iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    value.to_str().map(Some).map_err(|_| ())
}

/// Parses only the supported loopback authorities, returning the effective HTTP port.
/// No URL normalization, ignored suffixes, userinfo, path, query or fragment is accepted.
fn loopback_port(authority: &str) -> Option<u16> {
    let suffix = if let Some(suffix) = authority.strip_prefix("[::1]") {
        suffix
    } else {
        let end = authority.find(':').unwrap_or(authority.len());
        let hostname = &authority[..end];
        if !hostname.eq_ignore_ascii_case("localhost") && hostname != "127.0.0.1" {
            return None;
        }
        &authority[end..]
    };
    if suffix.is_empty() {
        return Some(80);
    }
    let port = suffix.strip_prefix(':')?;
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    port.parse().ok()
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

/// Host allowlist: require one well-formed supported loopback authority before routing.
/// Host ports remain ignored for compatibility; the Origin check enforces the bound port.
pub async fn host(State(state): State<AppState>, request: Request, next: Next) -> Response {
    match single_header(request.headers(), header::HOST) {
        Ok(Some(value)) if loopback_port(value).is_some() => {}
        _ => {
            return envelope::error(
                StatusCode::FORBIDDEN,
                "refused",
                "request requires one valid loopback Host header",
            );
        }
    }
    let _ = state;
    next.run(request).await
}

/// Origin check on unsafe methods: a present Origin must be one complete loopback server
/// origin (http, allowlisted hostname, bound port). A genuinely absent Origin remains allowed.
pub async fn origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) {
        match single_header(request.headers(), header::ORIGIN) {
            Ok(None) => {}
            Ok(Some(value)) if origin_matches(&state, value) => {}
            _ => {
                return envelope::error(
                    StatusCode::FORBIDDEN,
                    "refused",
                    "Origin must be one valid origin of this loopback server",
                );
            }
        }
    }
    next.run(request).await
}

fn origin_matches(state: &AppState, origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .and_then(loopback_port)
        .is_some_and(|port| port == state.bound_port)
}

/// Token authentication: `Authorization: token <t>` or the session cookie, with GETs outside
/// `/api/` (the entry page and the static viewer bundle it references) and the one-time paste
/// (`POST /api/v1/session`) as the only unauthenticated routes. The bundle must be public because
/// it loads before any token exists — the paste form is served by `app.js` — and it carries no
/// data; every `/api/v1` route is token-gated.
pub async fn authenticate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let auth = match single_header(&parts.headers, header::AUTHORIZATION) {
        Ok(Some(bearer)) => {
            if let Some(token) = bearer
                .strip_prefix("token ")
                .or_else(|| bearer.strip_prefix("Token "))
            {
                if constant_time_eq(token, &state.token) {
                    AuthKind::Bearer
                } else {
                    return unauthorized();
                }
            } else {
                return unauthorized();
            }
        }
        Ok(None) => match cookie_value(&parts.headers, SESSION_COOKIE) {
            Some(cookie) if constant_time_eq(&cookie, &state.token) => AuthKind::Session,
            _ => AuthKind::Unauthenticated,
        },
        Err(()) => return unauthorized(),
    };
    let entry_request = parts.method == Method::GET && !parts.uri.path().starts_with("/api/");
    let paste_request = parts.method == Method::POST && parts.uri.path() == "/api/v1/session";
    if auth == AuthKind::Unauthenticated && !entry_request && !paste_request {
        return unauthorized();
    }
    parts.extensions.insert(auth);
    next.run(Request::from_parts(parts, body)).await
}

fn unauthorized() -> Response {
    // A mistakenly supplied query token (or secret in a path/header) must not be
    // reflected into the error envelope. Input values are not needed for this diagnosis.
    envelope::error(
        StatusCode::UNAUTHORIZED,
        "unauthenticated",
        "a valid access token or session cookie is required",
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

#[cfg(test)]
mod admission_tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::future::poll_fn;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Poll;

    use axum::body::{Body, Bytes};
    use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
    use futures_util::stream;
    use locron_store::StatePaths;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const CSRF: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const HOST_MESSAGE: &str = "request requires one valid loopback Host header";
    const ORIGIN_MESSAGE: &str = "Origin must be one valid origin of this loopback server";
    const AUTH_MESSAGE: &str = "a valid access token or session cookie is required";

    #[derive(Clone, Copy, Debug)]
    enum Fields<'a> {
        Absent,
        Single(&'a str),
        Repeated(&'a str, &'a str),
        NonAscii,
    }

    fn append_fields(headers: &mut HeaderMap, name: HeaderName, fields: Fields<'_>) {
        match fields {
            Fields::Absent => {}
            Fields::Single(value) => {
                headers.append(name, HeaderValue::from_str(value).expect("header value"));
            }
            Fields::Repeated(first, second) => {
                headers.append(
                    name.clone(),
                    HeaderValue::from_str(first).expect("first header value"),
                );
                headers.append(
                    name,
                    HeaderValue::from_str(second).expect("second header value"),
                );
            }
            Fields::NonAscii => {
                headers.append(
                    name,
                    HeaderValue::from_bytes(b"HEADERCANARY\xff").expect("opaque header bytes"),
                );
            }
        }
    }

    fn loopback_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        append_fields(
            &mut headers,
            header::HOST,
            Fields::Single("localhost:10824"),
        );
        headers
    }

    fn valid_cookie() -> String {
        format!("locron_session={TOKEN}; csrf_token={CSRF}")
    }

    fn session_uri() -> String {
        format!("/api/v1/session?path=PATHCANARY&query=QUERYCANARY&token={TOKEN}")
    }

    struct Observed {
        status: StatusCode,
        headers: HeaderMap,
        body: Vec<u8>,
        body_polls: usize,
    }

    /// The stream has one item. Its future increments only when the real request body is polled.
    /// Refusals never poll it; the JSON paste extractor must poll it exactly once on success.
    async fn observe(
        method: Method,
        uri: &str,
        mut headers: HeaderMap,
        bound_port: u16,
    ) -> Observed {
        let polls = Arc::new(AtomicUsize::new(0));
        let body_polls = Arc::clone(&polls);
        let mut item = Some(Bytes::from(format!(r#"{{"token":"{TOKEN}"}}"#)));
        let body = Body::from_stream(stream::once(poll_fn(move |_| {
            body_polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Ok::<Bytes, std::io::Error>(
                item.take()
                    .expect("the one-item body future is polled once"),
            ))
        })));
        headers.append(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.append(
            HeaderName::from_static("x-admission-canary"),
            HeaderValue::from_static("HEADERCANARY"),
        );
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .body(body)
            .expect("request");
        *request.headers_mut() = headers;
        let state = crate::AppState {
            paths: StatePaths::new("admission-unopened-PATHCANARY".into()),
            token: TOKEN.to_owned(),
            bound_port,
        };
        let response = crate::router(state)
            .oneshot(request)
            .await
            .expect("full router response");
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), 1_048_576)
            .await
            .expect("bounded response body")
            .to_vec();
        Observed {
            status,
            headers,
            body,
            body_polls: polls.load(Ordering::SeqCst),
        }
    }

    fn assert_private_body(id: &str, observed: &Observed) {
        let text = std::str::from_utf8(&observed.body).expect("UTF-8 response body");
        for canary in [
            TOKEN,
            "PATHCANARY",
            "QUERYCANARY",
            "HEADERCANARY",
            "WRONGCANARY",
            "suffix-canary",
        ] {
            assert!(!text.contains(canary), "{id}: response contains {canary}");
        }
        assert_eq!(
            observed.headers.get("referrer-policy").expect("policy"),
            "no-referrer",
            "{id}"
        );
    }

    fn assert_refusal(
        id: &str,
        observed: &Observed,
        status: StatusCode,
        code: &str,
        message: &str,
    ) {
        assert_eq!(observed.status, status, "{id}");
        assert_eq!(observed.body_polls, 0, "{id}: refusal polled request body");
        assert_eq!(
            observed.headers.get_all(header::SET_COOKIE).iter().count(),
            0,
            "{id}: refusal emitted a cookie"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&observed.body).expect("error JSON"),
            json!({
                "schema": "locron.api/v1",
                "ok": false,
                "error": {"code": code, "message": message}
            }),
            "{id}"
        );
        assert_private_body(id, observed);
    }

    fn assert_session(id: &str, observed: &Observed, pasted: bool) {
        assert_eq!(observed.status, StatusCode::OK, "{id}");
        assert_eq!(observed.body_polls, usize::from(pasted), "{id}");
        assert_eq!(
            serde_json::from_slice::<Value>(&observed.body).expect("session JSON"),
            json!({
                "schema": "locron.api/v1",
                "ok": true,
                "data": {"authenticated": true},
                "warnings": []
            }),
            "{id}"
        );
        assert_private_body(id, observed);
        let fields = observed.headers.get_all(header::SET_COOKIE);
        assert_eq!(fields.iter().count(), if pasted { 2 } else { 0 }, "{id}");
        if pasted {
            let mut cookies = BTreeMap::new();
            for field in fields {
                let pair = field
                    .to_str()
                    .expect("cookie ASCII")
                    .split(';')
                    .next()
                    .unwrap();
                let (name, value) = pair.split_once('=').expect("cookie pair");
                assert!(
                    cookies.insert(name, value).is_none(),
                    "{id}: duplicate cookie"
                );
            }
            assert_eq!(
                cookies.keys().copied().collect::<BTreeSet<_>>(),
                BTreeSet::from(["locron_session", "csrf_token"]),
                "{id}"
            );
            assert_eq!(cookies["locron_session"], TOKEN, "{id}");
            let csrf = cookies["csrf_token"];
            assert_eq!(csrf.len(), 64, "{id}: CSRF cookie length");
            assert!(
                csrf.bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
                "{id}: CSRF cookie value"
            );
        }
    }

    fn complete(seen: &mut BTreeSet<String>, id: &str) {
        assert!(seen.insert(id.to_owned()), "duplicate completion: {id}");
    }

    #[tokio::test]
    async fn host_admission_executes_all_36_rows_before_body_polling() {
        let cases = [
            ("host-01", Fields::Single("localhost"), true),
            ("host-02", Fields::Single("LOCALHOST"), true),
            ("host-03", Fields::Single("LocalHost:10824"), true),
            ("host-04", Fields::Single("localhost:0"), true),
            ("host-05", Fields::Single("localhost:65535"), true),
            ("host-06", Fields::Single("localhost:00080"), true),
            ("host-07", Fields::Single("127.0.0.1"), true),
            ("host-08", Fields::Single("127.0.0.1:10824"), true),
            ("host-09", Fields::Single("127.0.0.1:9999"), true),
            ("host-10", Fields::Single("[::1]"), true),
            ("host-11", Fields::Single("[::1]:10824"), true),
            ("host-12", Fields::Single("[::1]:0"), true),
            ("host-13", Fields::Single("[::1]:65535"), true),
            ("host-14", Fields::Single("localhost:"), false),
            ("host-15", Fields::Single("localhost:+1"), false),
            ("host-16", Fields::Single("localhost:-1"), false),
            ("host-17", Fields::Single("localhost:65536"), false),
            ("host-18", Fields::Single("localhost:1:2"), false),
            ("host-19", Fields::Single("localhost/PATHCANARY"), false),
            ("host-20", Fields::Single("localhost?QUERYCANARY"), false),
            ("host-21", Fields::Single("localhost#QUERYCANARY"), false),
            ("host-22", Fields::Single("HEADERCANARY@localhost"), false),
            ("host-23", Fields::Single("localhost,127.0.0.1"), false),
            ("host-24", Fields::Single("127.0.0.01"), false),
            ("host-25", Fields::Single("[::1]HEADERCANARY"), false),
            ("host-26", Fields::Single("[::1"), false),
            ("host-27", Fields::Single("[::1]]"), false),
            ("host-28", Fields::Single("::1"), false),
            ("host-29", Fields::Single("[::1]:10824/PATHCANARY"), false),
            ("host-30", Fields::Single("[::1]:+1"), false),
            ("host-31", Fields::Single("HEADERCANARY.example"), false),
            ("host-32", Fields::Absent, false),
            ("host-33", Fields::Single(""), false),
            ("host-34", Fields::Repeated("localhost", "localhost"), false),
            (
                "host-35",
                Fields::Repeated("localhost", "HEADERCANARY.example"),
                false,
            ),
            ("host-36", Fields::NonAscii, false),
        ];
        assert_eq!(cases.len(), 36);
        assert_eq!(
            cases.iter().filter(|(_, _, accepted)| *accepted).count(),
            13
        );
        let mut seen = BTreeSet::new();
        for (id, fields, accepted) in cases {
            let mut headers = HeaderMap::new();
            append_fields(&mut headers, header::HOST, fields);
            let observed = observe(Method::POST, &session_uri(), headers, 10_824).await;
            if accepted {
                assert_session(id, &observed, true);
            } else {
                assert_refusal(
                    id,
                    &observed,
                    StatusCode::FORBIDDEN,
                    "refused",
                    HOST_MESSAGE,
                );
            }
            complete(&mut seen, id);
        }
        let expected: BTreeSet<_> = (1..=36).map(|row| format!("host-{row:02}")).collect();
        assert_eq!(seen, expected, "all 36 Host rows must complete");
    }

    #[tokio::test]
    async fn origin_admission_executes_all_96_unsafe_safe_and_positive_rows() {
        let accepted = [
            ("origin-accept-01", 10_824, Fields::Absent),
            (
                "origin-accept-02",
                10_824,
                Fields::Single("http://localhost:10824"),
            ),
            (
                "origin-accept-03",
                10_824,
                Fields::Single("http://LOCALHOST:10824"),
            ),
            (
                "origin-accept-04",
                10_824,
                Fields::Single("http://127.0.0.1:10824"),
            ),
            (
                "origin-accept-05",
                10_824,
                Fields::Single("http://[::1]:10824"),
            ),
            (
                "origin-accept-06",
                10_824,
                Fields::Single("http://localhost:010824"),
            ),
            (
                "origin-accept-07",
                10_824,
                Fields::Single("http://[::1]:010824"),
            ),
            ("origin-accept-08", 80, Fields::Single("http://localhost")),
            ("origin-accept-09", 80, Fields::Single("http://127.0.0.1")),
            ("origin-accept-10", 80, Fields::Single("http://[::1]")),
            (
                "origin-accept-11",
                80,
                Fields::Single("http://localhost:00080"),
            ),
        ];
        let refused = [
            ("origin-reject-01", Fields::Single("")),
            ("origin-reject-02", Fields::Single("null")),
            (
                "origin-reject-03",
                Fields::Single("https://localhost:10824"),
            ),
            ("origin-reject-04", Fields::Single("http://localhost")),
            ("origin-reject-05", Fields::Single("http://localhost:10825")),
            (
                "origin-reject-06",
                Fields::Single("http://HEADERCANARY@localhost:10824"),
            ),
            (
                "origin-reject-07",
                Fields::Single("http://localhost:10824/PATHCANARY"),
            ),
            (
                "origin-reject-08",
                Fields::Single("http://localhost:10824?QUERYCANARY"),
            ),
            (
                "origin-reject-09",
                Fields::Single("http://localhost:10824#QUERYCANARY"),
            ),
            (
                "origin-reject-10",
                Fields::Single("http://localhost:10824 http://127.0.0.1:10824"),
            ),
            (
                "origin-reject-11",
                Fields::Single("http://[::1]HEADERCANARY"),
            ),
            ("origin-reject-12", Fields::Single("http://localhost:65536")),
            (
                "origin-reject-13",
                Fields::Single("http://localhost:+10824"),
            ),
            (
                "origin-reject-14",
                Fields::Single("http://HEADERCANARY.example:10824"),
            ),
            (
                "origin-reject-15",
                Fields::Repeated("http://localhost:10824", "http://localhost:10824"),
            ),
            (
                "origin-reject-16",
                Fields::Repeated(
                    "http://localhost:10824",
                    "http://HEADERCANARY.example:10824",
                ),
            ),
            ("origin-reject-17", Fields::NonAscii),
        ];
        assert_eq!(accepted.len(), 11);
        assert_eq!(refused.len(), 17);
        let mut seen = BTreeSet::new();
        for (id, port, fields) in accepted {
            let mut headers = loopback_headers();
            append_fields(&mut headers, header::ORIGIN, fields);
            let observed = observe(Method::POST, &session_uri(), headers, port).await;
            assert_session(id, &observed, true);
            complete(&mut seen, id);
        }
        for (id, fields) in refused {
            for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
                let row_id = format!("{id}-{method}");
                let mut headers = loopback_headers();
                append_fields(&mut headers, header::ORIGIN, fields);
                let observed = observe(method, &session_uri(), headers, 10_824).await;
                assert_refusal(
                    &row_id,
                    &observed,
                    StatusCode::FORBIDDEN,
                    "refused",
                    ORIGIN_MESSAGE,
                );
                complete(&mut seen, &row_id);
            }
            let row_id = format!("{id}-GET");
            let mut headers = loopback_headers();
            append_fields(&mut headers, header::ORIGIN, fields);
            append_fields(
                &mut headers,
                header::AUTHORIZATION,
                Fields::Single(&format!("token {TOKEN}")),
            );
            append_fields(
                &mut headers,
                header::COOKIE,
                Fields::Single(&valid_cookie()),
            );
            let observed = observe(Method::GET, &session_uri(), headers, 10_824).await;
            assert_session(&row_id, &observed, false);
            complete(&mut seen, &row_id);
        }
        let mut expected = BTreeSet::new();
        for row in 1..=11 {
            expected.insert(format!("origin-accept-{row:02}"));
        }
        for row in 1..=17 {
            for method in ["POST", "PUT", "PATCH", "DELETE", "GET"] {
                expected.insert(format!("origin-reject-{row:02}-{method}"));
            }
        }
        assert_eq!(expected.len(), 96);
        assert_eq!(seen, expected, "all 96 Origin rows must complete");
    }

    #[tokio::test]
    async fn authorization_admission_never_falls_back_from_supplied_invalid_fields() {
        let lower = format!("token {TOKEN}");
        let capitalized = format!("Token {TOKEN}");
        let unsupported = format!("Bearer {TOKEN}");
        let upper = format!("TOKEN {TOKEN}");
        let suffix = format!("token {TOKEN} suffix-canary");
        let combined = format!("token {TOKEN}, Token {TOKEN}");
        let refused = [
            ("auth-01", Fields::Single("")),
            ("auth-02", Fields::Single(&unsupported)),
            ("auth-03", Fields::Single(&upper)),
            ("auth-04", Fields::Single("token WRONGCANARY")),
            ("auth-05", Fields::Single(&suffix)),
            ("auth-06", Fields::Single(&combined)),
            ("auth-07", Fields::Single("token")),
            ("auth-08", Fields::Repeated(&lower, &lower)),
            ("auth-09", Fields::Repeated(&lower, "token WRONGCANARY")),
            ("auth-10", Fields::Repeated("token WRONGCANARY", &lower)),
            ("auth-11", Fields::NonAscii),
        ];
        assert_eq!(refused.len(), 11);
        let mut seen = BTreeSet::new();
        for (id, fields) in refused {
            let mut headers = loopback_headers();
            append_fields(&mut headers, header::AUTHORIZATION, fields);
            append_fields(
                &mut headers,
                header::COOKIE,
                Fields::Single(&valid_cookie()),
            );
            let observed = observe(Method::GET, &session_uri(), headers, 10_824).await;
            assert_refusal(
                id,
                &observed,
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                AUTH_MESSAGE,
            );
            complete(&mut seen, id);
        }
        for (id, fields, cookie) in [
            (
                "auth-12",
                Fields::Single(&lower),
                format!("csrf_token={CSRF}"),
            ),
            (
                "auth-13",
                Fields::Single(&capitalized),
                format!("csrf_token={CSRF}"),
            ),
            ("auth-14", Fields::Absent, valid_cookie()),
        ] {
            let mut headers = loopback_headers();
            append_fields(&mut headers, header::AUTHORIZATION, fields);
            append_fields(&mut headers, header::COOKIE, Fields::Single(&cookie));
            let observed = observe(Method::GET, &session_uri(), headers, 10_824).await;
            assert_session(id, &observed, false);
            complete(&mut seen, id);
        }
        let observed = observe(Method::GET, &session_uri(), loopback_headers(), 10_824).await;
        assert_refusal(
            "auth-15",
            &observed,
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            AUTH_MESSAGE,
        );
        complete(&mut seen, "auth-15");
        let entry = crate::assets::Assets::get("index.html").expect("embedded entry");
        let html = std::str::from_utf8(entry.data.as_ref()).expect("entry UTF-8");
        let scripts = html
            .split("<script")
            .skip(1)
            .filter_map(|element| {
                let opening = element.split_once('>').expect("script opening tag").0;
                let (_, src) = opening.split_once("src=\"")?;
                assert_eq!(opening.matches("src=\"").count(), 1, "one src per tag");
                Some(src.split_once('"').expect("script src closing quote").0)
            })
            .collect::<Vec<_>>();
        assert_eq!(scripts.len(), 1, "exactly one external script reference");
        let script_path = scripts[0];
        assert!(
            script_path.starts_with("/assets/")
                && script_path
                    .rsplit_once('.')
                    .is_some_and(|(_, extension)| extension == "js")
        );
        let script = crate::assets::Assets::get(script_path.trim_start_matches('/'))
            .expect("entry-referenced embedded script");
        assert!(!entry.data.is_empty() && !script.data.is_empty());
        for (id, path, expected_body) in [
            ("auth-16", "/", entry.data.as_ref()),
            ("auth-17", script_path, script.data.as_ref()),
        ] {
            let observed = observe(Method::GET, path, loopback_headers(), 10_824).await;
            assert_eq!(observed.status, StatusCode::OK, "{id}");
            assert_eq!(observed.body_polls, 0, "{id}");
            assert_eq!(
                observed.body.as_slice(),
                expected_body,
                "{id}: embedded bytes"
            );
            assert_eq!(
                observed.headers.get_all(header::SET_COOKIE).iter().count(),
                0,
                "{id}: public bundle emitted a cookie"
            );
            assert_private_body(id, &observed);
            complete(&mut seen, id);
        }
        let expected: BTreeSet<_> = (1..=17).map(|row| format!("auth-{row:02}")).collect();
        assert_eq!(
            seen, expected,
            "all 17 authentication/public rows must complete"
        );
    }
}
