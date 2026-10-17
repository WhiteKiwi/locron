//! The versioned `locron.api/v1` response envelope.
//!
//! Success responses are `{"schema": "locron.api/v1", "ok": true, "data": ..., "warnings": [...]}`;
//! error responses are `{"schema": "locron.api/v1", "ok": false, "error": {"code", "message"}}`
//! with additive `error.storage` for safe structured persistence diagnostics,
//! with the stable CLI error categories carried verbatim in `code` and mapped to HTTP statuses per
//! `docs/CLI.md` (400 `invalid_request`, 401 `unauthenticated`, 403 `refused`, 404 `not_found`,
//! 409 `durable_conflict`, 503 daemon-required/state unavailable, 500 `state_error`).

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::{Value, json};

/// Builds a success envelope response body.
#[must_use]
pub fn ok(data: &Value, warnings: &[String]) -> Value {
    json!({
        "schema": "locron.api/v1",
        "ok": true,
        "data": data,
        "warnings": warnings,
    })
}

/// Builds an error envelope response with the stable CLI error category as `code`.
#[must_use]
pub fn error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> axum::response::Response {
    (
        status,
        Json(json!({
            "schema": "locron.api/v1",
            "ok": false,
            "error": {"code": code, "message": message.into()},
        })),
    )
        .into_response()
}

/// Builds the additive safe persistence failure envelope without adapter sources.
#[must_use]
pub fn storage_error(storage: &locron_core::PersistenceDiagnostic) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "schema": "locron.api/v1",
            "ok": false,
            "error": {
                "code": "state_error",
                "message": storage.to_string(),
                "storage": storage,
            },
        })),
    )
        .into_response()
}
