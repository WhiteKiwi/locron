//! API route handlers.
//!
//! One handler family per durable application command (IMPLEMENTATION.md §5):
//! jobs, runs, settings, export/import, prune, and diagnostics, all wrapped in
//! the versioned `locron.api/v1` envelope. Store access happens on the
//! blocking pool with a store opened per request (`Store::open`), so handlers
//! never block the reactor. Response payloads mirror the CLI's machine JSON
//! through the shared `locron-core` redaction boundary; dry-run parity and the
//! CLI-category-to-HTTP-status mapping follow `docs/CLI.md` and
//! `docs/dashboard/IMPLEMENTATION.md` §5.

use std::collections::BTreeSet;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{CookieJar, SameSite};
use base64::Engine;
use cookie::Cookie;
use serde::Deserialize;
use serde_json::{Value, json};

use locron_core::command::JobDefinition;
use locron_core::redact::{
    redact_definition, redacted_job_document, redacted_observable_run_document,
    redacted_settings_document, terminal_run_state,
};
use locron_core::schedule::Schedule;
use locron_core::target::{Target, is_valid_environment_name};
use locron_core::{Timestamp, ValidationError};
use locron_store::{
    CancelOutcome, CreateJob, DaemonLock, FrameChannel, FrameReader, StatePaths, Store, StoreError,
    UpdateJob,
};

use crate::AppState;
use crate::envelope;
use crate::middleware::{CSRF_COOKIE, SESSION_COOKIE, constant_time_eq};
use crate::token;
use crate::transfer::{self, ApiTransferError};

/// The history endpoint caps the store's window at this many runs and warns
/// when the requested window exceeds it.
const HISTORY_CAP: usize = 1000;
/// Default history limit when `limit` is absent.
const DEFAULT_HISTORY_LIMIT: usize = 20;
/// Default schedule preview count when `count` is absent.
const DEFAULT_PREVIEW_COUNT: usize = 5;
/// The daemon lock probe used by the run endpoint to warn when a run would
/// stay durably queued.
const DAEMON_NOT_RUNNING_WARNING: &str = "daemon is not running; run remains durably queued";

// ---------------------------------------------------------------------------
// Error plumbing
// ---------------------------------------------------------------------------

/// Handler errors mapped to envelopes with the CLI category table.
pub(crate) enum ApiError {
    /// A store failure (CLI category table).
    Store(StoreError),
    /// A validation failure (`invalid_request`, 400).
    Validation(ValidationError),
    /// An explicit status, CLI code, and message.
    Message(StatusCode, &'static str, String),
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ValidationError> for ApiError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<ApiTransferError> for ApiError {
    fn from(error: ApiTransferError) -> Self {
        match error {
            ApiTransferError::Store(error) => Self::Store(error),
            ApiTransferError::Validation(error) => Self::Validation(error),
            ApiTransferError::Message(status, code, message) => {
                Self::Message(status, code, message)
            }
        }
    }
}

impl ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Store(error) => store_error_response(&error),
            Self::Validation(error) => envelope::error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.to_string(),
            ),
            Self::Message(status, code, message) => envelope::error(status, code, message),
        }
    }
}

/// Maps a store error to the envelope with the CLI category table
/// (IMPLEMENTATION.md §5): `not_found` → 404, `durable_conflict` → 409,
/// daemon-required categories → 503, everything else → 500 `state_error`.
pub(crate) fn store_error_response(error: &StoreError) -> Response {
    let (status, code, message) = match error {
        StoreError::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message.clone()),
        StoreError::Conflict(message) => {
            (StatusCode::CONFLICT, "durable_conflict", message.clone())
        }
        StoreError::DaemonAlreadyRunning => (
            StatusCode::SERVICE_UNAVAILABLE,
            "daemon_already_running",
            error.to_string(),
        ),
        StoreError::MigrationRequiresDaemonRestart => (
            StatusCode::SERVICE_UNAVAILABLE,
            "migration_requires_restart",
            error.to_string(),
        ),
        StoreError::SchemaTooNew { .. } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "schema_too_new",
            error.to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "state_error",
            error.to_string(),
        ),
    };
    envelope::error(status, code, message)
}

/// Wraps a handler result in the versioned envelope.
fn respond(result: Result<Value, ApiError>, warnings: &[String]) -> Response {
    match result {
        Ok(data) => Json(envelope::ok(&data, warnings)).into_response(),
        Err(error) => error.into_response(),
    }
}

/// Wraps a handler result that carries its own warnings.
fn respond_pair(result: Result<(Value, Vec<String>), ApiError>) -> Response {
    match result {
        Ok((data, warnings)) => respond(Ok(data), &warnings),
        Err(error) => error.into_response(),
    }
}

/// Serializes a record through the store's JSON error channel.
fn to_json(value: impl serde::Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(StoreError::Json)
        .map_err(ApiError::Store)
}

// ---------------------------------------------------------------------------
// Blocking-pool store access
// ---------------------------------------------------------------------------

/// Wall-clock microseconds since the Unix epoch.
fn now_us() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock precedes the Unix epoch")
        .as_micros()
        .try_into()
        .expect("system clock exceeds the i64 microsecond range")
}

/// Best-effort wake hint to a running daemon; the command is already durable
/// when the socket is unavailable, so failures are ignored.
fn send_wake(paths: &StatePaths) {
    use std::os::unix::net::UnixDatagram;
    let _ = UnixDatagram::unbound().and_then(|socket| {
        socket.connect(&paths.wake_socket)?;
        socket.send(b"locron-wake/v1").map(|_| ())
    });
}

/// Whether a daemon currently owns the state directory, probed through the
/// exclusive lock (any probe failure counts as running, like the CLI).
fn daemon_running(paths: &StatePaths) -> bool {
    DaemonLock::try_prove_free(&paths.daemon_lock).is_err()
}

/// The configured global concurrency, or the CLI default of 16 when no state
/// database exists.
fn configured_global_concurrency(store: Option<&Store>) -> Result<u8, ApiError> {
    let settings = match store {
        Some(store) => store.settings()?,
        None => transfer::default_settings(),
    };
    u8::try_from(settings.global_concurrency).map_err(|_| {
        ApiError::Message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "state_error",
            "configured global concurrency is out of range".to_owned(),
        )
    })
}

/// Validates job metadata the way the CLI does before the store sees it.
fn validate_metadata(
    name: &str,
    description: Option<&str>,
    tags: &[String],
) -> Result<(), ApiError> {
    if name.trim().is_empty() || name.contains('\0') {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "job name must be non-empty and contain no NUL".to_owned(),
        ));
    }
    if tags
        .iter()
        .any(|tag| tag.trim().is_empty() || tag.contains('\0'))
    {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "tags must be non-empty and contain no NUL".to_owned(),
        ));
    }
    if description.is_some_and(|description| description.contains('\0')) {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "job description must not contain NUL".to_owned(),
        ));
    }
    Ok(())
}

/// Runs `f` on the blocking pool with a freshly opened store, so handlers
/// never block the reactor thread.
async fn with_store<T>(
    state: &AppState,
    f: impl FnOnce(&Store) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || {
        let store = Store::open(paths, env!("CARGO_PKG_VERSION"), now_us())?;
        f(&store)
    })
    .await
    .map_err(|error| {
        ApiError::Message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "state_error",
            format!("store task failed: {error}"),
        )
    })?
}

/// Runs `f` on the blocking pool with the dry-run store: read-only when the
/// state database exists, `None` (defaults) when it does not.
async fn with_dry_store<T>(
    state: &AppState,
    f: impl FnOnce(Option<&Store>) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || {
        let store = if paths.database.is_file() {
            Some(Store::open_read_only(&paths.database)?)
        } else {
            None
        };
        f(store.as_ref())
    })
    .await
    .map_err(|error| {
        ApiError::Message(
            StatusCode::INTERNAL_SERVER_ERROR,
            "state_error",
            format!("store task failed: {error}"),
        )
    })?
}

/// Runs `f` on the blocking pool with a live store when `dry` is false and
/// the dry-run store when it is true; the live branch never sees `None`.
async fn with_store_for<T>(
    state: &AppState,
    dry: bool,
    f: impl FnOnce(Option<&Store>) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    if dry {
        with_dry_store(state, f).await
    } else {
        with_store(state, move |store| f(Some(store))).await
    }
}

// ---------------------------------------------------------------------------
// Query and body shapes
// ---------------------------------------------------------------------------

/// Deserializes bare query flags (`?wait`), empty flags (`?wait=`), and
/// explicit booleans.
fn deserialize_flag<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    match raw.as_deref() {
        None | Some("" | "true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(other) => Err(serde::de::Error::custom(format!(
            "invalid boolean flag {other:?}"
        ))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ListJobsQuery {
    #[serde(default, deserialize_with = "deserialize_flag")]
    all: bool,
}

fn default_history_limit() -> usize {
    DEFAULT_HISTORY_LIMIT
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct HistoryQuery {
    job: Option<String>,
    #[serde(default = "default_history_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_attempt() -> u16 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct LogsQuery {
    #[serde(default = "default_attempt")]
    attempt: u16,
    #[serde(default)]
    channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct RunJobQuery {
    #[serde(default, deserialize_with = "deserialize_flag")]
    wait: bool,
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

fn default_preview_count() -> usize {
    DEFAULT_PREVIEW_COUNT
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PreviewCountQuery {
    #[serde(default = "default_preview_count")]
    count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CancelQuery {
    #[serde(default, deserialize_with = "deserialize_flag")]
    acknowledge_unconfirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ExportQuery {
    #[serde(default)]
    jobs: String,
    #[serde(default)]
    tag: String,
    #[serde(default, deserialize_with = "deserialize_flag")]
    include_values: bool,
    #[serde(default, deserialize_with = "deserialize_flag")]
    acknowledge_plaintext: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ImportQuery {
    #[serde(default, deserialize_with = "deserialize_flag")]
    accept_plaintext_values: bool,
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PruneQuery {
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobCreateRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    definition: JobDefinition,
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

fn default_true() -> bool {
    true
}

/// How the description field changes on an update: absent means keep the
/// current value, `null` clears it, a string replaces it.
#[derive(Debug, Clone, Default)]
enum DescriptionUpdate {
    /// Field absent from the request: keep the current value.
    #[default]
    Absent,
    /// Field present as `null`: clear the description.
    Clear,
    /// Field present as a string: replace the description.
    Set(String),
}

impl<'de> Deserialize<'de> for DescriptionUpdate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match Option::<Value>::deserialize(deserializer)? {
            None => Ok(Self::Clear),
            Some(Value::String(value)) => Ok(Self::Set(value)),
            Some(_) => Err(serde::de::Error::custom(
                "description must be a string or null",
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobUpdateRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: DescriptionUpdate,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    definition: Option<JobDefinition>,
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SchedulePreviewRequest {
    #[serde(default)]
    job: Option<String>,
    #[serde(default)]
    schedule: Option<Schedule>,
    #[serde(default = "default_preview_count")]
    count: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SettingsPutRequest {
    value: String,
    #[serde(default, deserialize_with = "deserialize_flag")]
    dry_run: bool,
}

// ---------------------------------------------------------------------------
// Session surface
// ---------------------------------------------------------------------------

/// Body of the one-time token paste.
#[derive(Debug, Deserialize)]
pub(crate) struct SessionRequest {
    /// The 64-character hex access token.
    token: String,
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
pub(crate) async fn session_status(jar: CookieJar) -> Response {
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
pub(crate) async fn session_create(
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

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

/// `GET /api/v1/jobs`: lists jobs (active by default), redacted through the
/// shared core boundary exactly like the CLI's `list`.
pub(crate) async fn jobs_list(
    State(state): State<AppState>,
    Query(query): Query<ListJobsQuery>,
) -> Response {
    let result = with_store(&state, move |store| {
        let jobs = store.list_jobs(query.all)?;
        jobs.into_iter()
            .map(|job| {
                redacted_job_document(serde_json::to_value(&job).map_err(StoreError::Json)?)
                    .map_err(StoreError::Json)
            })
            .collect::<Result<Vec<_>, StoreError>>()
            .map(Value::Array)
            .map_err(ApiError::from)
    })
    .await;
    respond(result, &[])
}

/// `POST /api/v1/jobs`: creates a job, or dry-runs the creation without a
/// durable side effect. The dry-run response carries the `<non-durable>`
/// placeholder id exactly like the CLI's `add --dry-run`.
pub(crate) async fn jobs_create(
    State(state): State<AppState>,
    Json(body): Json<JobCreateRequest>,
) -> Response {
    let result = with_store_for(&state, body.dry_run, move |store| {
        let global_concurrency = configured_global_concurrency(store)?;
        body.definition.validate(global_concurrency)?;
        validate_metadata(&body.name, body.description.as_deref(), &body.tags)?;
        let definition_json = serde_json::to_string(&body.definition).map_err(StoreError::Json)?;
        let warnings = environment_warnings(&body.definition.environment);
        if store.is_none() {
            let normalized = redact_definition(to_json(&body.definition)?);
            return Ok((
                json!({
                    "dry_run": true,
                    "id": "<non-durable>",
                    "name": body.name,
                    "description": body.description,
                    "tags": body.tags,
                    "enabled": body.enabled,
                    "definition": normalized,
                }),
                warnings,
            ));
        }
        let store = store.expect("live store");
        let now = now_us();
        let record = store.create_job(&CreateJob {
            id: uuid::Uuid::now_v7().to_string(),
            name: body.name,
            description: body.description,
            tags_json: serde_json::to_string(&body.tags).map_err(StoreError::Json)?,
            enabled: body.enabled,
            definition_json,
            now_us: now,
            cursor_us: now,
        })?;
        send_wake(store.paths());
        let job = redacted_job_document(to_json(&record)?).map_err(StoreError::Json)?;
        Ok((job, warnings))
    })
    .await;
    respond_pair(result)
}

/// `GET /api/v1/jobs/{id}`: shows one job by name or UUID, redacted.
pub(crate) async fn jobs_show(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Response {
    let result = with_store(&state, move |store| {
        let record = store.job(&reference)?;
        redacted_job_document(to_json(&record)?)
            .map_err(StoreError::Json)
            .map_err(ApiError::from)
    })
    .await;
    respond(result, &[])
}

/// `PUT /api/v1/jobs/{id}`: replaces fields with immutable-revision
/// semantics, mirroring the CLI's `update`: a no-op update is a 409
/// `durable_conflict`, and dry-run responses carry `changed_fields`,
/// `schedule_changed`, and redacted before/after documents.
pub(crate) async fn jobs_update(
    State(state): State<AppState>,
    Path(reference): Path<String>,
    Json(body): Json<JobUpdateRequest>,
) -> Response {
    let result = with_store_for(&state, body.dry_run, move |store| {
        let Some(store) = store else {
            return Err(ApiError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "state database does not exist".to_owned(),
            ));
        };
        let current = store.job(&reference)?;
        let current_definition: JobDefinition =
            serde_json::from_str(&current.definition_json).map_err(StoreError::Json)?;
        let current_tags: Vec<String> =
            serde_json::from_str(&current.tags_json).map_err(StoreError::Json)?;
        let now = now_us();
        let global_concurrency = configured_global_concurrency(Some(store))?;
        let new_name = body.name.clone().unwrap_or_else(|| current.name.clone());
        let new_description = match &body.description {
            DescriptionUpdate::Absent => current.description.clone(),
            DescriptionUpdate::Clear => None,
            DescriptionUpdate::Set(value) => Some(value.clone()),
        };
        let new_tags = body.tags.clone().unwrap_or_else(|| current_tags.clone());
        let new_enabled = body.enabled.unwrap_or(current.enabled);
        let new_definition = body
            .definition
            .clone()
            .unwrap_or_else(|| current_definition.clone());
        new_definition.validate(global_concurrency)?;
        validate_metadata(&new_name, new_description.as_deref(), &new_tags)?;
        let schedule_changed = new_definition.schedule != current_definition.schedule;
        let before = job_fields(
            &current.name,
            current.description.as_deref(),
            &current_tags,
            current.enabled,
            &current_definition,
        )?;
        let after = job_fields(
            &new_name,
            new_description.as_deref(),
            &new_tags,
            new_enabled,
            &new_definition,
        )?;
        let changed_fields = changed_field_paths(&before, &after);
        if changed_fields.is_empty() {
            return Err(ApiError::Message(
                StatusCode::CONFLICT,
                "durable_conflict",
                "update does not change any field".to_owned(),
            ));
        }
        let warnings = environment_warnings(&new_definition.environment);
        let cursor_us = if schedule_changed {
            now
        } else {
            current.cursor_us
        };
        if body.dry_run {
            return Ok((
                json!({
                    "dry_run": true,
                    "id": current.id,
                    "revision": current.current_revision + 1,
                    "schedule_changed": schedule_changed,
                    "changed_fields": changed_fields,
                    "before": redact_definition(before),
                    "after": redact_definition(after),
                    "cursor_us": cursor_us,
                }),
                warnings,
            ));
        }
        let record = store.update_job(&UpdateJob {
            id: current.id.clone(),
            expected_revision: current.current_revision,
            name: new_name,
            description: new_description,
            tags_json: serde_json::to_string(&new_tags).map_err(StoreError::Json)?,
            enabled: new_enabled,
            definition_json: serde_json::to_string(&new_definition).map_err(StoreError::Json)?,
            now_us: now,
            cursor_us,
        })?;
        send_wake(store.paths());
        let job = redacted_job_document(to_json(&record)?).map_err(StoreError::Json)?;
        Ok((job, warnings))
    })
    .await;
    respond_pair(result)
}

/// `POST /api/v1/jobs/{id}/enable` and `.../disable`: toggles a job.
async fn toggle_job(state: &AppState, reference: String, enabled: bool) -> Response {
    let result = with_store(state, move |store| {
        let record = store.set_enabled(&reference, enabled, now_us())?;
        send_wake(store.paths());
        redacted_job_document(to_json(&record)?)
            .map_err(StoreError::Json)
            .map_err(ApiError::from)
    })
    .await;
    respond(result, &[])
}

/// `POST /api/v1/jobs/{id}/enable`
pub(crate) async fn jobs_enable(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Response {
    toggle_job(&state, reference, true).await
}

/// `POST /api/v1/jobs/{id}/disable`
pub(crate) async fn jobs_disable(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Response {
    toggle_job(&state, reference, false).await
}

/// `DELETE /api/v1/jobs/{id}`: soft-deletes a job by name or UUID.
pub(crate) async fn jobs_remove(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Response {
    let result = with_store(&state, move |store| {
        store.remove_job(&reference, now_us())?;
        send_wake(store.paths());
        Ok(json!({"name": reference, "removed": true}))
    })
    .await;
    respond(result, &[])
}

/// `POST /api/v1/jobs/{id}/run?wait&dry-run`: queues a manual run, dry-runs
/// the admission decision, or waits for the terminal state. The `wait` mode
/// returns 200 with the terminal state (a CLI exit-1 `target_outcome` does
/// not map to an HTTP category; recorded in IMPLEMENTATION.md §5).
pub(crate) async fn jobs_run(
    State(state): State<AppState>,
    Path(reference): Path<String>,
    Query(query): Query<RunJobQuery>,
) -> Response {
    if query.dry_run {
        let result = with_dry_store(&state, move |store| {
            let Some(store) = store else {
                return Err(ApiError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "state database does not exist".to_owned(),
                ));
            };
            let job = store.job(&reference)?;
            let active = store
                .history(Some(&reference), 100)?
                .into_iter()
                .filter(|run| {
                    matches!(
                        run.state.as_str(),
                        "queued" | "starting" | "running" | "retry_wait"
                    )
                })
                .count();
            let definition: JobDefinition =
                serde_json::from_str(&job.definition_json).map_err(StoreError::Json)?;
            let decision = if active == 0 {
                "eligible"
            } else {
                match definition.policy.overlap {
                    locron_core::policy::OverlapPolicy::Skip => "would_skip_overlap",
                    locron_core::policy::OverlapPolicy::Replace => "would_replace",
                    locron_core::policy::OverlapPolicy::Allow => "eligible_subject_to_capacity",
                }
            };
            Ok(json!({
                "dry_run": true,
                "durable": false,
                "decision": decision,
                "capacity_reserved": false,
            }))
        })
        .await;
        return respond(result, &[]);
    }

    let run_id = uuid::Uuid::now_v7().to_string();
    let enqueue_run_id = run_id.clone();
    let enqueued = with_store(&state, move |store| {
        let run = store.enqueue_manual(&reference, &enqueue_run_id, now_us())?;
        send_wake(store.paths());
        let warnings = if daemon_running(store.paths()) {
            Vec::new()
        } else {
            vec![DAEMON_NOT_RUNNING_WARNING.to_owned()]
        };
        Ok((run, warnings))
    })
    .await;

    if !query.wait {
        return match enqueued {
            Ok((run, warnings)) => {
                respond(Ok(json!({"run_id": run.id, "state": run.state})), &warnings)
            }
            Err(error) => error.into_response(),
        };
    }

    let (run, warnings) = match enqueued {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let mut run_state = run.state;
    let mut reason = run.reason;
    while !terminal_run_state(&run_state) {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let probe_id = run_id.clone();
        let current = with_store(&state, move |store| {
            let record = store.run(&probe_id)?;
            Ok((record.state, record.reason))
        })
        .await;
        match current {
            Ok((next_state, next_reason)) => {
                run_state = next_state;
                reason = next_reason;
            }
            Err(error) => return error.into_response(),
        }
    }
    respond(
        Ok(json!({"run_id": run_id, "state": run_state, "reason": reason})),
        &warnings,
    )
}

/// `GET /api/v1/jobs/{id}/preview?count=` and
/// `POST /api/v1/schedule/preview`: enumerate the next schedule occurrences
/// as RFC 3339 strings, exactly like the CLI's `preview`.
fn preview_occurrences(schedule: &Schedule, count: usize) -> Result<Value, ApiError> {
    let occurrences = schedule.next(Timestamp::from_epoch_micros(now_us()), count)?;
    Ok(json!({
        "occurrences": occurrences
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    }))
}

/// `GET /api/v1/jobs/{id}/preview?count=`
pub(crate) async fn jobs_preview(
    State(state): State<AppState>,
    Path(reference): Path<String>,
    Query(query): Query<PreviewCountQuery>,
) -> Response {
    let result = with_store(&state, move |store| {
        let job = store.job(&reference)?;
        let definition: JobDefinition =
            serde_json::from_str(&job.definition_json).map_err(StoreError::Json)?;
        preview_occurrences(&definition.schedule, query.count)
    })
    .await;
    respond(result, &[])
}

/// `POST /api/v1/schedule/preview`: preview a schedule literal or a job's
/// schedule.
pub(crate) async fn schedule_preview(
    State(state): State<AppState>,
    Json(body): Json<SchedulePreviewRequest>,
) -> Response {
    let schedule = match (body.job, body.schedule) {
        (Some(_), Some(_)) => {
            return envelope::error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "provide a job name or a schedule, not both",
            );
        }
        (None, None) => {
            return envelope::error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "provide a job name or a schedule",
            );
        }
        (Some(reference), None) => {
            let result = with_store(&state, move |store| {
                let job = store.job(&reference)?;
                let definition: JobDefinition =
                    serde_json::from_str(&job.definition_json).map_err(StoreError::Json)?;
                preview_occurrences(&definition.schedule, body.count)
            })
            .await;
            return respond(result, &[]);
        }
        (None, Some(schedule)) => schedule,
    };
    respond(preview_occurrences(&schedule, body.count), &[])
}

/// `GET /api/v1/jobs/{id}/why`: durable facts about a job and its next
/// occurrence, mirroring the CLI's `why` fields and explanation verbatim.
pub(crate) async fn jobs_why(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Response {
    let result = with_store(&state, move |store| {
        let job = store.job(&reference)?;
        let definition: JobDefinition =
            serde_json::from_str(&job.definition_json).map_err(StoreError::Json)?;
        let next = definition
            .schedule
            .next(Timestamp::from_epoch_micros(now_us()), 1)?
            .first()
            .map(ToString::to_string);
        let active = store
            .history(Some(&reference), 100)?
            .into_iter()
            .filter(|run| {
                matches!(
                    run.state.as_str(),
                    "queued" | "starting" | "running" | "retry_wait"
                )
            })
            .map(|run| {
                locron_core::redact::redacted_run_document(
                    serde_json::to_value(&run).map_err(StoreError::Json)?,
                )
                .map_err(StoreError::Json)
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let job = redacted_job_document(to_json(&job)?).map_err(StoreError::Json)?;
        Ok(json!({
            "job": job,
            "next_occurrence": next,
            "active_runs": active,
            "overlap": definition.policy.overlap,
            "daemon_running": daemon_running(store.paths()),
            "explanation": "facts are read from durable state; unknown execution facts are not inferred",
        }))
    })
    .await;
    respond(result, &[])
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

/// `GET /api/v1/runs?job=&limit=&offset=`: paginated observable run history.
/// The store window is capped at 1000 runs; a truncation warning is emitted
/// when the requested window exceeds it (recorded in IMPLEMENTATION.md §5).
pub(crate) async fn runs_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let result = with_store(&state, move |store| {
        let total = store.count_runs(query.job.as_deref())?;
        let fetched = store.history(
            query.job.as_deref(),
            (query.limit + query.offset).min(HISTORY_CAP),
        )?;
        let runs = fetched
            .into_iter()
            .skip(query.offset)
            .map(|run| {
                let attempts = serde_json::to_value(&store.attempts_for_run(&run.id)?)
                    .map_err(StoreError::Json)?;
                redacted_observable_run_document(
                    serde_json::to_value(&run).map_err(StoreError::Json)?,
                    attempts,
                )
                .map_err(StoreError::Json)
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok((
            json!({
                "runs": runs,
                "total": total,
                "limit": query.limit,
                "offset": query.offset,
            }),
            if query.limit + query.offset > HISTORY_CAP {
                vec!["history is capped at 1000 runs".to_owned()]
            } else {
                Vec::new()
            },
        ))
    })
    .await;
    respond_pair(result)
}

/// `GET /api/v1/runs/{id}`: one observable run with its ordered attempts.
pub(crate) async fn runs_show(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let result = with_store(&state, move |store| {
        let run = store.run(&id)?;
        let attempts = to_json(&store.attempts_for_run(&run.id)?)?;
        redacted_observable_run_document(to_json(&run)?, attempts)
            .map_err(StoreError::Json)
            .map_err(ApiError::from)
    })
    .await;
    respond(result, &[])
}

/// `POST /api/v1/runs/{id}/cancel?acknowledge_unconfirmed`: cancels a run
/// with the CLI's outcome shapes.
pub(crate) async fn runs_cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<CancelQuery>,
) -> Response {
    if uuid::Uuid::parse_str(&id).is_err() {
        return envelope::error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "invalid run UUID",
        );
    }
    let result = with_store(&state, move |store| {
        let outcome =
            store.cancel_with_acknowledgement(&id, now_us(), query.acknowledge_unconfirmed)?;
        send_wake(store.paths());
        Ok(match outcome {
            CancelOutcome::CancelledBeforeExecution => {
                json!({"run_id": id, "requested": true, "cancelled": true, "before_execution": true})
            }
            CancelOutcome::CancellationRequested => json!({"run_id": id, "requested": true}),
            CancelOutcome::AcknowledgedUnconfirmed => {
                json!({"run_id": id, "acknowledged_unconfirmed": true, "state": "interrupted_unknown"})
            }
        })
    })
    .await;
    respond(result, &[])
}

/// `GET /api/v1/runs/{id}/logs?attempt=&channel=`: the framed output of one
/// attempt, base64 payloads, from the final output artifact.
pub(crate) async fn runs_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Response {
    let result = with_store(&state, move |store| {
        let run = store.run(&id)?;
        let attempt_state = store
            .attempts_for_run(&run.id)?
            .into_iter()
            .find(|attempt| attempt.attempt_number == i64::from(query.attempt))
            .map(|attempt| attempt.state);
        let path = store.paths().final_output(&run.id, query.attempt)?;
        let frames = match FrameReader::open(&path) {
            Ok(mut reader) => {
                let mut frames = Vec::new();
                while let Some(frame) = reader.next_frame().map_err(StoreError::Io)? {
                    if channel_selected(&query.channel, frame.channel) {
                        frames.push(json!({
                            "channel": format!("{:?}", frame.channel).to_lowercase(),
                            "sequence": frame.sequence,
                            "elapsed_micros": frame.elapsed_us,
                            "bytes": base64::engine::general_purpose::STANDARD
                                .encode(&frame.payload),
                            "encoding": "base64",
                        }));
                    }
                }
                frames
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ApiError::Message(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    "output not found".to_owned(),
                ));
            }
            Err(error) => return Err(ApiError::Store(StoreError::Io(error))),
        };
        Ok(json!({
            "run_id": run.id,
            "attempt": query.attempt,
            "attempt_state": attempt_state,
            "frames": frames,
        }))
    })
    .await;
    respond(result, &[])
}

/// `GET /api/v1/runs/{id}/why`: durable run facts, ordered attempts, and
/// audit events, mirroring the CLI's `why --run` fields verbatim.
pub(crate) async fn runs_why(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let result = with_store(&state, move |store| {
        let events = store.events_for_run(&id)?;
        let run = store.run(&id)?;
        let attempts = to_json(&store.attempts_for_run(&run.id)?)?;
        let run = redacted_observable_run_document(to_json(&run)?, attempts)
            .map_err(StoreError::Json)?;
        Ok(json!({
            "run": run,
            "events": events,
            "daemon_running": daemon_running(store.paths()),
            "explanation": "terminal reason, immutable snapshot, ordered attempts, and audit events are durable facts",
        }))
    })
    .await;
    respond(result, &[])
}

/// Whether the channel query selects the frame channel.
fn channel_selected(query: &str, frame: FrameChannel) -> bool {
    let wanted = query.to_ascii_lowercase();
    if wanted.is_empty() || wanted == "all" {
        return true;
    }
    let actual = format!("{frame:?}").to_lowercase();
    actual == wanted
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// `GET /api/v1/settings`: the full redacted settings document (the CLI's
/// `config get` without a key).
pub(crate) async fn settings_get(State(state): State<AppState>) -> Response {
    let result = with_dry_store(&state, |store| {
        let settings = match store {
            Some(store) => store.settings()?,
            None => transfer::default_settings(),
        };
        Ok(redacted_settings_document(to_json(&settings)?))
    })
    .await;
    respond(result, &[])
}

/// `PUT /api/v1/settings/{key}`: sets a typed setting or an
/// `environment.NAME` value, with the CLI's config surface, grammar, and
/// redaction preserved.
pub(crate) async fn settings_put(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SettingsPutRequest>,
) -> Response {
    let result = with_store_for(&state, body.dry_run, move |store| {
        if let Some(name) = environment_config_name(&key)? {
            validate_environment_value(name, &body.value)?;
            let before = dry_settings(store)?;
            let action = if before.environment.contains_key(name) {
                "replaced"
            } else {
                "created"
            };
            if let Some(store) = store {
                store.set_environment(name, Some(&body.value), now_us())?;
                send_wake(store.paths());
            }
            return Ok(json!({
                "key": key,
                "action": action,
                "configured": true,
                "value_redacted": true,
                "dry_run": body.dry_run,
            }));
        }
        if body.dry_run {
            validate_config_value(&key, &body.value)?;
            return Ok(json!({"key": key, "value": body.value, "dry_run": true}));
        }
        let store = store.expect("live store");
        let settings = store.set_setting(&key, &body.value, now_us())?;
        send_wake(store.paths());
        Ok(redacted_settings_document(to_json(&settings)?))
    })
    .await;
    respond(result, &[])
}

/// `DELETE /api/v1/settings/{key}`: unsets an `environment.NAME` value.
pub(crate) async fn settings_delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
    let result = with_store(&state, move |store| {
        let name = environment_config_name(&key)?.ok_or_else(|| {
            ApiError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "only environment.NAME settings can be unset".to_owned(),
            )
        })?;
        let before = store.settings()?;
        let action = if before.environment.contains_key(name) {
            "removed"
        } else {
            "unchanged"
        };
        store.set_environment(name, None, now_us())?;
        send_wake(store.paths());
        Ok(json!({
            "key": key,
            "action": action,
            "configured": false,
            "value_redacted": true,
            "dry_run": false,
        }))
    })
    .await;
    respond(result, &[])
}

/// Extracts the `environment.NAME` grammar from a settings key, mirroring
/// the CLI's `environment_config_name`.
fn environment_config_name(key: &str) -> Result<Option<&str>, ApiError> {
    let Some(name) = key.strip_prefix("environment.") else {
        if key == "environment" {
            return Err(ApiError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "environment requires a named environment.NAME key".to_owned(),
            ));
        }
        return Ok(None);
    };
    if !is_valid_environment_name(name) || name.starts_with("LOCRON_") {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!("invalid or reserved environment name {name}"),
        ));
    }
    Ok(Some(name))
}

/// Environment values must not contain NUL, like the CLI.
fn validate_environment_value(name: &str, value: &str) -> Result<(), ApiError> {
    if value.contains('\0') {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!("environment value for {name} contains NUL"),
        ));
    }
    Ok(())
}

/// The settings record a dry-run decision should be based on: the existing
/// database when present, defaults otherwise.
fn dry_settings(store: Option<&Store>) -> Result<locron_store::SettingsRecord, ApiError> {
    match store {
        Some(store) => Ok(store.settings()?),
        None => Ok(transfer::default_settings()),
    }
}

/// Validates a typed config key against the CLI's config surface.
fn validate_config_value(key: &str, value: &str) -> Result<(), ApiError> {
    let parse_i64 = |name: &str| -> Result<i64, ApiError> {
        value.parse::<i64>().map_err(|_| {
            ApiError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                format!("{name} must be a non-negative integer"),
            )
        })
    };
    match key {
        "global_concurrency" => {
            let parsed = value.parse::<i64>().map_err(|_| {
                ApiError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "global_concurrency must be an integer".to_owned(),
                )
            })?;
            if !(1..=64).contains(&parsed) {
                return Err(ApiError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "global_concurrency must be from 1 through 64".to_owned(),
                ));
            }
        }
        "execution_path" => {}
        "run_retention_count" | "output_limit_bytes" | "per_run_output_limit_bytes" => {
            let parsed = parse_i64(key)?;
            if parsed < 0 {
                return Err(ApiError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    format!("{key} must be non-negative"),
                ));
            }
        }
        _ => {
            return Err(ApiError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "unknown configuration key".to_owned(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Export and import
// ---------------------------------------------------------------------------

/// `GET /api/v1/export?jobs=&tag=&include-values&acknowledge-plaintext`:
/// streams the `locron.export/v1` document as an attachment. Plaintext
/// values require both flags, exactly like the CLI.
pub(crate) async fn export_document(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Response {
    if query.include_values != query.acknowledge_plaintext {
        return envelope::error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "plaintext export requires both --include-values and --acknowledge-plaintext",
        );
    }
    let jobs = split_references(&query.jobs);
    let tags = split_references(&query.tag);
    let result = with_store(&state, move |store| {
        let document = transfer::build_export_document(store, &jobs, &tags, query.include_values)?;
        serde_json::to_vec(&document)
            .map_err(StoreError::Json)
            .map_err(ApiError::from)
    })
    .await;
    match result {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/json".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"locron.export.json\"".to_owned(),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

/// Splits a comma-separated query reference list, dropping empties.
fn split_references(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `POST /api/v1/import?accept-plaintext-values&dry-run`: applies an export
/// document, or fetches one from a URL via `{"url": "…"}` with the documented
/// server-side bounds (TLS verification, 16 MiB streaming cap, 10-redirect
/// cap, 30-second timeout, userinfo rejection).
pub(crate) async fn import_document(
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Response {
    let bytes = match import_source(&body).await {
        Ok(bytes) => bytes,
        Err(error) => return error.into_response(),
    };
    let document = match transfer::parse_import_document(&bytes, query.accept_plaintext_values) {
        Ok(document) => document,
        Err(error) => return transfer::transfer_error_response(error),
    };
    let result = with_store_for(&state, query.dry_run, move |store| {
        let plan = transfer::plan_import(&document, store, now_us())?;
        if query.dry_run {
            return Ok(json!({
                "dry_run": true,
                "settings_changed": plan.settings_changed,
                "actions": plan.actions,
            }));
        }
        let store = store.expect("live store");
        let summary = match plan.batch {
            Some(batch) => {
                let summary = store.apply_import(&batch)?;
                send_wake(store.paths());
                summary
            }
            None => locron_store::ImportSummary::default(),
        };
        Ok(json!({
            "created": summary.created,
            "updated": summary.updated,
            "no_op": document.jobs.len() - summary.created - summary.updated,
            "settings_changed": plan.settings_changed,
        }))
    })
    .await;
    respond(result, &[])
}

/// Resolves the import body: either the export document bytes verbatim, or a
/// server-side URL fetch when the body is exactly `{"url": "…"}`.
async fn import_source(body: &Bytes) -> Result<Vec<u8>, ApiError> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Ok(body.to_vec());
    };
    let Some(url) = value.get("url").and_then(Value::as_str) else {
        return Ok(body.to_vec());
    };
    if value.as_object().is_none_or(|object| object.len() != 1) {
        return Err(ApiError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "import body must be an export document or {\"url\": \"…\"}".to_owned(),
        ));
    }
    match transfer::fetch_import_url(url).await {
        Ok(bytes) => Ok(bytes),
        Err(error) => Err(error.into()),
    }
}

// ---------------------------------------------------------------------------
// Prune and diagnostics
// ---------------------------------------------------------------------------

/// `POST /api/v1/prune?dry-run`: prunes finalized output artifacts beyond
/// the 30-day age bound or the global output cap, mirroring the CLI's prune
/// selection and its refusal to remove symbolic-link or non-file outputs.
pub(crate) async fn prune(
    State(state): State<AppState>,
    Query(query): Query<PruneQuery>,
) -> Response {
    let result = with_store_for(&state, query.dry_run, move |store| {
        let Some(store) = store else {
            return Ok(json!({"dry_run": true, "candidate_count": 0, "bytes": 0}));
        };
        let settings = store.settings()?;
        let mut retained = store.retained_output_bytes()?;
        let age_cutoff = now_us().saturating_sub(30_i64 * 24 * 60 * 60 * 1_000_000);
        let candidates = store
            .output_retention_candidates(100)?
            .into_iter()
            .filter(|candidate| {
                candidate.finalized_at_us < age_cutoff || retained > settings.output_limit_bytes
            })
            .collect::<Vec<_>>();
        if query.dry_run {
            return Ok(json!({
                "dry_run": true,
                "candidate_count": candidates.len(),
                "bytes": candidates.iter().map(|candidate| candidate.physical_bytes).sum::<i64>(),
            }));
        }
        for candidate in &candidates {
            store.mark_output_prune_pending(candidate, now_us())?;
            let path = store.paths().outputs.join(&candidate.relative_path);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(ApiError::Message(
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "refusing to prune symbolic-link output".to_owned(),
                    ));
                }
                Ok(metadata) if metadata.is_file() => {
                    std::fs::remove_file(&path).map_err(StoreError::Io)?
                }
                Ok(_) => {
                    return Err(ApiError::Message(
                        StatusCode::BAD_REQUEST,
                        "invalid_request",
                        "refusing to prune non-file output".to_owned(),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ApiError::Store(StoreError::Io(error))),
            }
            store.finish_output_prune(candidate, now_us())?;
            retained = retained.saturating_sub(candidate.physical_bytes);
        }
        Ok(json!({
            "dry_run": false,
            "candidate_count": candidates.len(),
            "bytes": candidates.iter().map(|candidate| candidate.physical_bytes).sum::<i64>(),
        }))
    })
    .await;
    respond(result, &[])
}

/// `GET /api/v1/diagnostics`: scheduler health facts mirroring the CLI's
/// `doctor` fields. Executable resolution walks the configured execution
/// path here rather than reusing locron-engine's target resolution (the
/// server does not depend on locron-engine; recorded in IMPLEMENTATION.md).
pub(crate) async fn diagnostics(State(state): State<AppState>) -> Response {
    let paths = state.paths.clone();
    let result = with_dry_store(&state, move |store| {
        let settings = match store {
            Some(store) => store.settings()?,
            None => transfer::default_settings(),
        };
        let checks = match store {
            Some(store) => store.integrity_check()?,
            None => Vec::new(),
        };
        let mut resolutions = Vec::new();
        if let Some(store) = store {
            for job in store.list_jobs(true)? {
                let definition: JobDefinition =
                    serde_json::from_str(&job.definition_json).map_err(StoreError::Json)?;
                let requested = match &definition.target {
                    Target::Process { executable, .. } => executable.clone(),
                    Target::Shell { shell, .. } => shell.display().to_string(),
                    Target::Http(_) => continue,
                };
                match resolve_executable(&requested, &settings.execution_path) {
                    Some(resolved) => resolutions.push(json!({
                        "job_id": job.id,
                        "job_name": job.name,
                        "requested_executable": requested,
                        "effective_path": settings.execution_path,
                        "resolved_executable": resolved,
                        "status": "resolved",
                    })),
                    None => resolutions.push(json!({
                        "job_id": job.id,
                        "job_name": job.name,
                        "requested_executable": requested,
                        "status": "unresolved",
                        "error": "executable not found in execution path",
                    })),
                }
            }
        }
        Ok(json!({
            "state_dir": paths.root,
            "database": paths.database,
            "daemon_running": daemon_running(&paths),
            "wake_socket": paths.wake_socket.exists(),
            "execution_path": settings.execution_path,
            "global_environment_names": settings.environment.keys().cloned().collect::<Vec<_>>(),
            "process_resolution": resolutions,
            "checks": checks,
        }))
    })
    .await;
    respond(result, &[])
}

/// Resolves an executable against the execution path: paths containing a
/// separator resolve directly, bare names search each `:`-separated entry.
fn resolve_executable(executable: &str, execution_path: &str) -> Option<String> {
    if executable.contains('/') {
        return std::path::Path::new(executable)
            .is_file()
            .then(|| executable.to_owned());
    }
    execution_path.split(':').find_map(|directory| {
        let candidate = std::path::Path::new(directory).join(executable);
        candidate.is_file().then(|| candidate.display().to_string())
    })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The redacted job-field document the CLI diffs for `changed_fields`.
fn job_fields(
    name: &str,
    description: Option<&str>,
    tags: &[String],
    enabled: bool,
    definition: &JobDefinition,
) -> Result<Value, ApiError> {
    Ok(json!({
        "name": name,
        "description": description,
        "tags": tags,
        "enabled": enabled,
        "definition": to_json(definition)?,
    }))
}

/// Dotted-path differences between two documents, mirroring the CLI's
/// `changed_fields` algorithm.
fn changed_field_paths(before: &Value, after: &Value) -> Vec<String> {
    fn collect(
        path: &str,
        before: Option<&Value>,
        after: Option<&Value>,
        output: &mut Vec<String>,
    ) {
        if before == after {
            return;
        }
        match (
            before.and_then(Value::as_object),
            after.and_then(Value::as_object),
        ) {
            (Some(before), Some(after)) => {
                let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
                for key in keys {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    collect(&child, before.get(key), after.get(key), output);
                }
            }
            _ => output.push(path.to_owned()),
        }
    }
    let mut output = Vec::new();
    collect("", Some(before), Some(after), &mut output);
    output
}

/// Warnings about the definition's environment file permissions, mirroring
/// the CLI's `environment_warnings`.
fn environment_warnings(environment: &locron_core::target::Environment) -> Vec<String> {
    let Some(path) = &environment.file else {
        return Vec::new();
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.permissions().mode() & 0o077 != 0
        {
            return vec!["environment file is readable or writable by group/others".to_owned()];
        }
    }
    Vec::new()
}
