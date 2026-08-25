//! Export-document construction and import planning (the `locron.export/v1`
//! transfer surface).
//!
//! The export document and its validation rules mirror the CLI's
//! `locron.export/v1` contract (`crates/locron-cli/src/main.rs`): the schema
//! must be exact, plaintext values require an explicit acceptance flag, a
//! redacted document must not contain omitted values, and job ids must be
//! lowercase canonical UUID text. Import planning resolves each source job to
//! a local destination by source id, then by source name, before creating with
//! a preserved or freshly allocated id, exactly like the CLI's `plan_import`;
//! the store rechecks every mapping inside the apply transaction
//! (`Store::apply_import`).
//!
//! Server-side URL import implements the documented bounds verbatim
//! (`docs/dashboard/IMPLEMENTATION.md`): mandatory TLS verification (the
//! rustls-backed workspace reqwest), a 10-redirect cap, a 30-second timeout, a
//! 16 MiB streaming cap, and userinfo rejection. The CLI's `fetch_import_url`
//! does not exist on main yet; the reconciliation note in the planning
//! document records this duplicate.

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use locron_core::ValidationError;
use locron_core::command::JobDefinition;
use locron_store::{
    CreateJob, ImportBatch, ImportJob, ImportResolution, SettingsRecord, Store, StoreError,
    UpdateJob,
};

use crate::envelope;

/// The export document schema identifier.
pub const EXPORT_SCHEMA: &str = "locron.export/v1";

/// How secret values are represented in an export document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuesMode {
    /// Secret values are removed from the document and listed in
    /// `omitted_values`.
    Redacted,
    /// Secret values are included verbatim; importing them requires the
    /// `accept_plaintext_values` flag.
    Plaintext,
}

/// One job in an export document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportJob {
    /// Lowercase canonical UUID.
    pub id: String,
    /// Live job name.
    pub name: String,
    /// Human description, or null.
    pub description: Option<String>,
    /// Tag list.
    pub tags: Vec<String>,
    /// Enabled flag.
    pub enabled: bool,
    /// Full normalized job definition.
    pub definition: JobDefinition,
    /// Dotted paths of removed secret values (`redacted` mode only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_values: Vec<String>,
}

/// The `locron.export/v1` document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportDocument {
    /// Must be [`EXPORT_SCHEMA`].
    pub schema: String,
    /// Secret-value representation.
    pub values_mode: ValuesMode,
    /// Global settings.
    pub settings: SettingsRecord,
    /// Jobs in name order.
    pub jobs: Vec<ExportJob>,
    /// Dotted paths of removed global settings values (`redacted` mode only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_values: Vec<String>,
}

/// The CLI's defaults, used when no state directory exists yet.
#[must_use]
pub fn default_settings() -> SettingsRecord {
    SettingsRecord {
        global_concurrency: 16,
        execution_path: "/usr/local/bin:/usr/bin:/bin".to_owned(),
        run_retention_count: 10_000,
        run_retention_age_us: Some(7_776_000_000_000),
        output_limit_bytes: 268_435_456,
        per_run_output_limit_bytes: 10_485_760,
        environment: BTreeMap::new(),
    }
}

/// Builds an export document from the live jobs, applying the viewer's
/// selection union (`jobs` references by name or id, `tag` membership) with
/// strict no-match validation, and redacting secret values unless both
/// plaintext flags are set.
pub fn build_export_document(
    store: &Store,
    jobs: &[String],
    tags: &[String],
    plaintext: bool,
) -> Result<ExportDocument, ApiTransferError> {
    let mut records = store.list_jobs(true)?;
    if !jobs.is_empty() || !tags.is_empty() {
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for reference in jobs {
            let matched = records
                .iter()
                .filter(|record| record.name == *reference || record.id == *reference)
                .collect::<Vec<_>>();
            if matched.is_empty() {
                return Err(ApiTransferError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    format!("no job matches {reference}"),
                ));
            }
            for record in matched {
                if seen.insert(record.id.clone()) {
                    selected.push(record.clone());
                }
            }
        }
        for tag in tags {
            let matched = records
                .iter()
                .filter(|record| {
                    serde_json::from_str::<Vec<String>>(&record.tags_json)
                        .is_ok_and(|record_tags| record_tags.iter().any(|t| t == tag))
                })
                .collect::<Vec<_>>();
            if matched.is_empty() {
                return Err(ApiTransferError::Message(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    format!("no jobs match tag {tag}"),
                ));
            }
            for record in matched {
                if seen.insert(record.id.clone()) {
                    selected.push(record.clone());
                }
            }
        }
        records = selected;
    }
    records.sort_by(|left, right| left.name.cmp(&right.name));
    let settings = store.settings()?;
    let mut omitted = BTreeSet::new();
    let settings_out = if plaintext {
        settings
    } else {
        let mut redacted = settings.clone();
        for name in settings.environment.keys() {
            omitted.insert(format!("settings.environment.{name}"));
        }
        redacted.environment.clear();
        redacted
    };
    let mut jobs_out = Vec::with_capacity(records.len());
    for record in records {
        let definition: Value =
            serde_json::from_str(&record.definition_json).map_err(StoreError::Json)?;
        let (definition_out, job_omitted) = if plaintext {
            (definition, Vec::new())
        } else {
            redact_export_definition(&definition)
        };
        jobs_out.push(ExportJob {
            id: record.id,
            name: record.name,
            description: record.description,
            tags: serde_json::from_str(&record.tags_json).map_err(StoreError::Json)?,
            enabled: record.enabled,
            definition: serde_json::from_value(definition_out).map_err(StoreError::Json)?,
            omitted_values: job_omitted,
        });
    }
    Ok(ExportDocument {
        schema: EXPORT_SCHEMA.to_owned(),
        values_mode: if plaintext {
            ValuesMode::Plaintext
        } else {
            ValuesMode::Redacted
        },
        settings: settings_out,
        jobs: jobs_out,
        omitted_values: omitted.into_iter().collect(),
    })
}

/// Removes secret values from a serialized `JobDefinition`, returning the
/// redacted value and the sorted dotted paths of everything removed.
fn redact_export_definition(definition: &Value) -> (Value, Vec<String>) {
    let mut omitted = BTreeSet::new();
    let mut redacted = definition.clone();
    if let Some(values) = redacted
        .pointer_mut("/environment/values")
        .and_then(Value::as_object_mut)
    {
        for name in values.keys() {
            omitted.insert(format!("definition.environment.values.{name}"));
        }
        values.clear();
    }
    if let Some(headers) = redacted
        .pointer_mut("/target/headers")
        .and_then(Value::as_object_mut)
    {
        let mut to_remove = Vec::new();
        for (name, header) in &*headers {
            if header.get("source").and_then(Value::as_str) == Some("inline") {
                omitted.insert(format!("definition.target.headers.{name}"));
                to_remove.push(name.clone());
            }
        }
        for name in to_remove {
            headers.remove(&name);
        }
    }
    if redacted
        .pointer("/target/body")
        .is_some_and(Value::is_object)
    {
        // The body serializes as a JSON array of bytes; null is the absent
        // form. A non-null value is a secret and must be removed.
        omitted.insert("definition.target.body".to_owned());
        if let Some(body) = redacted.pointer_mut("/target/body") {
            *body = Value::Null;
        }
    }
    (redacted, omitted.into_iter().collect())
}

/// One planned import action, in the CLI's dry-run shape.
#[derive(Clone, Debug, Serialize)]
pub struct PlannedAction {
    /// `create`, `update`, or `no_op`.
    pub action: &'static str,
    /// Job id in the exporting database.
    pub source_id: String,
    /// Job id the source maps to locally (the literal `<non-durable:{id}>`
    /// in a dry run when a fresh id would be allocated).
    pub destination_id: String,
    /// Job name; omitted for `no_op` actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The result of planning an import.
#[derive(Debug)]
pub struct ImportPlan {
    /// Whether the document settings differ from the current settings.
    pub settings_changed: bool,
    /// Per-job actions in document order (dry-run surface).
    pub actions: Vec<PlannedAction>,
    /// The batch to apply, or `None` when nothing would change.
    pub batch: Option<ImportBatch>,
}

/// Errors produced while building or planning transfers; mapped to envelopes
/// by the API layer.
#[derive(Debug)]
pub enum ApiTransferError {
    /// A store failure (mapped with the CLI category table).
    Store(StoreError),
    /// A validation failure (`invalid_request`, 400).
    Validation(ValidationError),
    /// An explicit code and message with the given HTTP status.
    Message(StatusCode, &'static str, String),
}

impl From<StoreError> for ApiTransferError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Parses and validates an export document for import. The rules mirror the
/// CLI's `parse_import_document`: exact schema, plaintext acceptance, no
/// omitted values in either mode, lowercase canonical job ids, no duplicate
/// ids or names, normalized and validated definitions, and — for redacted
/// documents — no inline plaintext values.
pub fn parse_import_document(
    bytes: &[u8],
    accept_plaintext: bool,
) -> Result<ExportDocument, ApiTransferError> {
    let document: ExportDocument = serde_json::from_slice(bytes).map_err(|error| {
        ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            error.to_string(),
        )
    })?;
    if document.schema != EXPORT_SCHEMA {
        return Err(ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!("unsupported export schema: {}", document.schema),
        ));
    }
    let plaintext = document.values_mode == ValuesMode::Plaintext;
    if plaintext && !accept_plaintext {
        return Err(ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "plaintext values require --accept-plaintext-values".to_owned(),
        ));
    }
    if !document.omitted_values.is_empty() {
        if plaintext {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "plaintext export must not contain omitted_values entries".to_owned(),
            ));
        }
        return Err(ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redacted export contains omitted values and cannot be imported faithfully".to_owned(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for job in &document.jobs {
        if job.id.len() != 36 || !lowercase_canonical_uuid(&job.id) {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "imported job UUID must be lowercase canonical text".to_owned(),
            ));
        }
        if !ids.insert(job.id.clone()) {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                format!("duplicate imported job ID: {}", job.id),
            ));
        }
        if !names.insert(job.name.clone()) {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                format!("duplicate imported job name: {}", job.name),
            ));
        }
        let definition = serde_json::to_value(&job.definition).map_err(|error| {
            ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.to_string(),
            )
        })?;
        if !plaintext && contains_inline_plaintext(&definition) {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "redacted export unexpectedly contains inline plaintext values".to_owned(),
            ));
        }
        if let Err(error) = job
            .definition
            .validate(u8::try_from(document.settings.global_concurrency).unwrap_or(64))
        {
            return Err(ApiTransferError::Validation(error));
        }
    }
    Ok(document)
}

fn lowercase_canonical_uuid(value: &str) -> bool {
    value
        .as_bytes()
        .iter()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f' | b'-'))
        && value.split('-').map(str::len).collect::<Vec<_>>() == [8, 4, 4, 4, 12]
}

/// Whether a serialized definition still carries secret values (redacted
/// imports must not): environment values, inline headers, or a non-null body.
fn contains_inline_plaintext(definition: &Value) -> bool {
    definition
        .pointer("/environment/values")
        .and_then(Value::as_object)
        .is_some_and(|values| !values.is_empty())
        || definition
            .pointer("/target/headers")
            .and_then(Value::as_object)
            .is_some_and(|headers| {
                headers
                    .values()
                    .any(|header| header.get("source").and_then(Value::as_str) == Some("inline"))
            })
        || definition
            .pointer("/target/body")
            .is_some_and(|body| !body.is_null())
}

/// Plans the import: resolves each source job to a local destination and
/// builds the batch, mirroring the CLI's `plan_import` resolution order and
/// conflict rules. `store` is `None` when no state directory exists yet
/// (defaults are compared and every job is created).
pub fn plan_import(
    document: &ExportDocument,
    store: Option<&Store>,
    now_us: i64,
) -> Result<ImportPlan, ApiTransferError> {
    let identities = match store {
        Some(store) => store.job_identities()?,
        None => Vec::new(),
    };
    let live_by_id = identities
        .iter()
        .filter(|identity| !identity.removed)
        .map(|identity| (identity.id.clone(), identity.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let live_by_name = identities
        .iter()
        .filter(|identity| !identity.removed)
        .map(|identity| (identity.name.clone(), identity.id.clone()))
        .collect::<BTreeMap<_, _>>();
    let owned = identities
        .iter()
        .map(|identity| identity.id.clone())
        .collect::<BTreeSet<_>>();

    let current = match store {
        Some(store) => store.settings()?,
        None => default_settings(),
    };
    let settings_changed = settings_changed(&current, &document.settings);

    let mut jobs = document.jobs.clone();
    jobs.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    let mut actions = Vec::with_capacity(jobs.len());
    let mut batch = Vec::with_capacity(jobs.len());
    let mut destination_sources = BTreeMap::<String, String>::new();

    for job in jobs {
        let by_id = live_by_id
            .get(&job.id)
            .and_then(|name| live_by_name.get(name))
            .cloned();
        let by_name = live_by_name.get(&job.name).cloned();
        let destination = match (by_id, by_name) {
            (Some(id_dest), Some(name_dest)) if id_dest != name_dest => {
                return Err(ApiTransferError::Message(
                    StatusCode::CONFLICT,
                    "durable_conflict",
                    format!(
                        "source ID {} and name {} resolve to different destination jobs",
                        job.id, job.name
                    ),
                ));
            }
            (id_dest, name_dest) => id_dest.or(name_dest),
        };
        let Some(destination_id) = destination else {
            let preserve = !owned.contains(&job.id);
            let destination_id = if preserve {
                job.id.clone()
            } else {
                format!("<non-durable:{}>", job.id)
            };
            if let Some(previous) =
                destination_sources.insert(destination_id.clone(), job.id.clone())
                && previous != job.id
            {
                return Err(ApiTransferError::Message(
                    StatusCode::CONFLICT,
                    "durable_conflict",
                    format!("multiple imported jobs resolve to destination {destination_id}"),
                ));
            }
            actions.push(PlannedAction {
                action: "create",
                source_id: job.id.clone(),
                destination_id: destination_id.clone(),
                name: Some(job.name.clone()),
            });
            batch.push(ImportJob::Create {
                job: CreateJob {
                    id: if preserve {
                        job.id.clone()
                    } else {
                        uuid::Uuid::now_v7().to_string()
                    },
                    name: job.name.clone(),
                    description: job.description.clone(),
                    tags_json: serde_json::to_string(&job.tags).map_err(|error| {
                        ApiTransferError::Message(
                            StatusCode::BAD_REQUEST,
                            "invalid_request",
                            error.to_string(),
                        )
                    })?,
                    enabled: job.enabled,
                    definition_json: serde_json::to_string(&job.definition).map_err(|error| {
                        ApiTransferError::Message(
                            StatusCode::BAD_REQUEST,
                            "invalid_request",
                            error.to_string(),
                        )
                    })?,
                    now_us,
                    cursor_us: now_us,
                },
                resolution: ImportResolution {
                    source_id: job.id.clone(),
                    source_name: job.name.clone(),
                    expected_id_destination: None,
                    expected_name_destination: None,
                },
            });
            continue;
        };
        if let Some(previous) = destination_sources.insert(destination_id.clone(), job.id.clone())
            && previous != job.id
        {
            return Err(ApiTransferError::Message(
                StatusCode::CONFLICT,
                "durable_conflict",
                format!("multiple imported jobs resolve to destination {destination_id}"),
            ));
        }
        let current = store
            .expect("a resolved destination implies a live store")
            .job(&destination_id)?;
        let current_definition: Value =
            serde_json::from_str(&current.definition_json).map_err(|error| {
                ApiTransferError::Message(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "state_error",
                    error.to_string(),
                )
            })?;
        let source_definition = serde_json::to_value(&job.definition).map_err(|error| {
            ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.to_string(),
            )
        })?;
        let schedule_changed = serde_json::from_str::<JobDefinition>(&current.definition_json)
            .map_or(true, |parsed| parsed.schedule != job.definition.schedule);
        let identical = current.name == job.name
            && current.description == job.description
            && serde_json::from_str::<Vec<String>>(&current.tags_json)
                .is_ok_and(|tags| tags == job.tags)
            && current.enabled == job.enabled
            && current_definition == source_definition
            && !schedule_changed;
        let expected_id_destination = live_by_id
            .contains_key(&job.id)
            .then_some(destination_id.clone());
        let expected_name_destination = live_by_name
            .contains_key(&job.name)
            .then_some(destination_id.clone());
        let (action, job_value) = if identical {
            (
                "no_op",
                UpdateJob {
                    id: destination_id.clone(),
                    expected_revision: current.current_revision,
                    name: current.name.clone(),
                    description: current.description.clone(),
                    tags_json: current.tags_json.clone(),
                    enabled: current.enabled,
                    definition_json: current.definition_json.clone(),
                    now_us,
                    cursor_us: current.cursor_us,
                },
            )
        } else {
            (
                "update",
                UpdateJob {
                    id: destination_id.clone(),
                    expected_revision: current.current_revision,
                    name: job.name.clone(),
                    description: job.description.clone(),
                    tags_json: serde_json::to_string(&job.tags).map_err(|error| {
                        ApiTransferError::Message(
                            StatusCode::BAD_REQUEST,
                            "invalid_request",
                            error.to_string(),
                        )
                    })?,
                    enabled: job.enabled,
                    definition_json: serde_json::to_string(&job.definition).map_err(|error| {
                        ApiTransferError::Message(
                            StatusCode::BAD_REQUEST,
                            "invalid_request",
                            error.to_string(),
                        )
                    })?,
                    now_us,
                    cursor_us: if schedule_changed {
                        now_us
                    } else {
                        current.cursor_us
                    },
                },
            )
        };
        actions.push(PlannedAction {
            action,
            source_id: job.id.clone(),
            destination_id: destination_id.clone(),
            name: (action != "no_op").then_some(job.name.clone()),
        });
        if action == "no_op" {
            batch.push(ImportJob::Verify {
                job: job_value,
                resolution: ImportResolution {
                    source_id: job.id.clone(),
                    source_name: job.name.clone(),
                    expected_id_destination,
                    expected_name_destination,
                },
            });
        } else {
            batch.push(ImportJob::Update {
                job: job_value,
                resolution: ImportResolution {
                    source_id: job.id.clone(),
                    source_name: job.name.clone(),
                    expected_id_destination,
                    expected_name_destination,
                },
            });
        }
    }
    let batch = if actions.iter().any(|action| action.action != "no_op") || settings_changed {
        Some(ImportBatch {
            settings: document.settings.clone(),
            jobs: batch,
            now_us,
        })
    } else {
        None
    };
    Ok(ImportPlan {
        settings_changed,
        actions,
        batch,
    })
}

/// Fetches an import document from a URL with the documented bounds: TLS
/// verification (rustls), a 10-redirect cap, a 30-second timeout, a streaming
/// 16 MiB cap, and userinfo rejection.
pub async fn fetch_import_url(url: &str) -> Result<Vec<u8>, ApiTransferError> {
    let parsed = reqwest::Url::parse(url).map_err(|error| {
        ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!("invalid import URL: {error}"),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "import URL must be http or https".to_owned(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiTransferError::Message(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "import URL must not contain userinfo".to_owned(),
        ));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| {
            ApiTransferError::Message(
                StatusCode::INTERNAL_SERVER_ERROR,
                "state_error",
                format!("could not build import client: {error}"),
            )
        })?;
    let response = client.get(url).send().await.map_err(|error| {
        ApiTransferError::Message(
            StatusCode::BAD_GATEWAY,
            "state_error",
            format!("import fetch failed: {error}"),
        )
    })?;
    if !response.status().is_success() {
        return Err(ApiTransferError::Message(
            StatusCode::BAD_GATEWAY,
            "state_error",
            format!("import fetch returned {}", response.status()),
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            ApiTransferError::Message(
                StatusCode::BAD_GATEWAY,
                "state_error",
                format!("import fetch stream failed: {error}"),
            )
        })?;
        if bytes.len() + chunk.len() > 16 * 1024 * 1024 {
            return Err(ApiTransferError::Message(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "import document exceeds the 16 MiB cap".to_owned(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Whether the document settings differ from the current settings.
fn settings_changed(current: &SettingsRecord, imported: &SettingsRecord) -> bool {
    current.global_concurrency != imported.global_concurrency
        || current.execution_path != imported.execution_path
        || current.run_retention_count != imported.run_retention_count
        || current.run_retention_age_us != imported.run_retention_age_us
        || current.output_limit_bytes != imported.output_limit_bytes
        || current.per_run_output_limit_bytes != imported.per_run_output_limit_bytes
        || current.environment != imported.environment
}

/// Maps a transfer error to the envelope response.
pub fn transfer_error_response(error: ApiTransferError) -> axum::response::Response {
    match error {
        ApiTransferError::Store(error) => crate::api::store_error_response(&error),
        ApiTransferError::Validation(error) => envelope::error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            error.to_string(),
        ),
        ApiTransferError::Message(status, code, message) => envelope::error(status, code, message),
    }
}
