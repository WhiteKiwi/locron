//! API route handlers.
//!
//! Step 4 ships the session surface (the entry-page paste and the session status check); the
//! durable command families land in the API step on top of the same envelope and security chain.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{CookieJar, SameSite};
use cookie::Cookie;
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::envelope;
use crate::middleware::{CSRF_COOKIE, SESSION_COOKIE, constant_time_eq};
use crate::token;

/// Body of the one-time token paste.
#[derive(Debug, Deserialize)]
pub struct SessionRequest {
    /// The 64-character hex access token.
    pub token: String,
}

fn session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token.to_owned()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::days(90))
        .build()
}

fn csrf_cookie(value: String) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, value))
        .http_only(false)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::days(90))
        .build()
}

/// `GET /api/v1/session`: reports the authenticated state. The middleware only lets this through
/// when a session cookie or bearer token validated, so a reachable handler is always
/// authenticated; the csrf_token cookie is re-issued when missing so mutations keep working.
pub async fn session_status(jar: CookieJar) -> Response {
    let mut jar = jar;
    if jar.get(CSRF_COOKIE).is_none() {
        jar = jar.add(csrf_cookie(token::random_hex_32()));
    }
    (
        jar,
        Json(envelope::ok(&json!({"authenticated": true}), &[])),
    )
        .into_response()
}

/// `POST /api/v1/session`: the entry-page token paste. A matching token sets the session cookie
/// and a fresh CSRF cookie; the token never appears in a URL or in a response.
pub async fn session_create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<SessionRequest>,
) -> Response {
    let mut jar = jar;
    if !constant_time_eq(&body.token, &state.token) {
        return envelope::error(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "access token rejected",
        );
    }
    jar = jar.add(session_cookie(&state.token));
    jar = jar.add(csrf_cookie(token::random_hex_32()));
    (
        jar,
        Json(envelope::ok(&json!({"authenticated": true}), &[])),
    )
        .into_response()
}
