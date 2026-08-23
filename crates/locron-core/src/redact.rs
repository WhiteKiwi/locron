//! The shared redaction boundary over serialized documents.
//!
//! Every user-facing surface (CLI, MCP, HTTP API) renders durable records through these functions
//! so that inline environment values, sensitive headers, and body content never appear in normal
//! output. The functions operate on `serde_json::Value` documents rather than on record types so
//! this crate — the shared dependency of every surface — does not need to know any other crate's
//! record shapes.
//!
//! The CLI keeps thin serialization adapters over these functions; the API (roadmap phase 1,
//! `docs/dashboard/SPEC.md`) calls them directly on its payload documents.

use serde_json::{Value, json};

/// Redacts sensitive material inside a serialized definition document.
///
/// Handles the CLI's flag-level wrapper shape (`{"definition": ...}`) by recursing, then replaces
/// every environment value (inline values and environment references alike) with the literal
/// `<redacted>`, every inline-sourced header value with `<redacted>`, and every non-null target
/// body with `<redacted>`. Header references are left untouched.
pub fn redact_definition(mut definition: Value) -> Value {
    if let Some(nested) = definition.get_mut("definition") {
        *nested = redact_definition(nested.take());
        return definition;
    }
    if let Some(values) = definition
        .get_mut("environment")
        .and_then(|environment| environment.get_mut("values"))
        .and_then(Value::as_object_mut)
    {
        for value in values.values_mut() {
            *value = Value::String("<redacted>".into());
        }
    }
    if let Some(headers) = definition
        .get_mut("target")
        .and_then(|target| target.get_mut("headers"))
        .and_then(Value::as_object_mut)
    {
        for value in headers.values_mut() {
            if value.get("source").and_then(Value::as_str) == Some("inline")
                && let Some(inline) = value.get_mut("value")
            {
                *inline = Value::String("<redacted>".into());
            }
        }
    }
    if let Some(body) = definition
        .get_mut("target")
        .and_then(|target| target.get_mut("body"))
        && !body.is_null()
    {
        *body = Value::String("<redacted>".into());
    }
    definition
}

/// Whether a run state is terminal.
#[must_use]
pub fn terminal_run_state(state: &str) -> bool {
    matches!(
        state,
        "succeeded"
            | "failed"
            | "timed_out"
            | "cancelled"
            | "skipped_overlap"
            | "skipped_concurrency"
            | "interrupted_unknown"
    )
}

/// Redacts the serialized job record document in place of the CLI's `redacted_job`.
///
/// The `definition_json` string field is parsed, redacted, and re-serialized so the
/// `definition_json` field keeps its string shape; an unparseable definition propagates the parse
/// error rather than silently weakening redaction.
pub fn redacted_job_document(mut value: Value) -> serde_json::Result<Value> {
    if let Some(definition) = value.get_mut("definition_json") {
        let source = definition.as_str().unwrap_or("{}");
        *definition = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

/// Redacts the serialized run record document in place of the CLI's `redacted_run`.
///
/// Equivalent to [`redacted_job_document`] for the `snapshot_json` field.
pub fn redacted_run_document(mut value: Value) -> serde_json::Result<Value> {
    if let Some(snapshot) = value.get_mut("snapshot_json") {
        let source = snapshot.as_str().unwrap_or("{}");
        *snapshot = Value::String(serde_json::to_string(&redact_definition(
            serde_json::from_str(source)?,
        ))?);
    }
    Ok(value)
}

/// Enriches a redacted run document with observable summary facts.
///
/// In place of the CLI's `redacted_observable_run`, which fetches attempts through the store: the
/// caller serializes the run record and the attempts array and passes both in. The document gains
/// the `source` trigger, a terminal `outcome`, `actual_started_at_us` (the earliest attempt start
/// time), `duration_us` (finished minus that start, non-negative), and the `attempts` array.
///
/// # Panics
///
/// Panics if `run` is not a JSON object (run records always serialize as objects).
pub fn redacted_observable_run_document(run: Value, attempts: Value) -> serde_json::Result<Value> {
    let source = run
        .get("trigger")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    let finished_at_us = run.get("finished_at_us").and_then(Value::as_i64);
    let state = run.get("state").and_then(Value::as_str).unwrap_or_default();
    let outcome = terminal_run_state(state).then(|| state.to_owned());
    let actual_started_at_us = attempts.as_array().and_then(|attempts| {
        attempts
            .iter()
            .filter_map(|attempt| attempt["running_at_us"].as_i64())
            .min()
    });
    let duration_us = actual_started_at_us
        .zip(finished_at_us)
        .and_then(|(started, finished)| finished.checked_sub(started))
        .filter(|duration| *duration >= 0);
    let mut value = redacted_run_document(run)?;
    let object = value
        .as_object_mut()
        .expect("run records serialize as objects");
    object.insert("source".into(), json!(source));
    object.insert("outcome".into(), json!(outcome));
    object.insert("actual_started_at_us".into(), json!(actual_started_at_us));
    object.insert("duration_us".into(), json!(duration_us));
    object.insert("attempts".into(), attempts);
    Ok(value)
}

/// Replaces the environment map of a serialized settings document with redaction markers.
///
/// In place of the CLI's `redacted_settings_value`: each configured environment variable name
/// becomes `{"configured": true, "value_redacted": true}` and no value is retained.
///
/// # Panics
///
/// Panics if `value` is not a JSON object (settings records always serialize as objects).
pub fn redacted_settings_document(mut value: Value) -> Value {
    let environment = value
        .get("environment")
        .and_then(Value::as_object)
        .map(|environment| {
            environment
                .keys()
                .map(|name| {
                    (
                        name.clone(),
                        json!({"configured": true, "value_redacted": true}),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
        })
        .unwrap_or_default();
    value
        .as_object_mut()
        .expect("settings serialize as an object")
        .insert("environment".into(), Value::Object(environment));
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_all_environment_values_including_references() {
        let definition = json!({
            "environment": {"values": {
                "SECRET": "s3cret",
                "REF": {"reference": "env", "name": "OTHER"},
            }},
            "target": {"kind": "process", "headers": {}, "body": null},
        });
        let redacted = redact_definition(definition);
        assert_eq!(redacted["environment"]["values"]["SECRET"], "<redacted>");
        assert_eq!(redacted["environment"]["values"]["REF"], "<redacted>");
    }

    #[test]
    fn redacts_inline_headers_and_non_null_body_but_keeps_references() {
        let definition = json!({
            "environment": {"values": {}},
            "target": {
                "kind": "http",
                "headers": {
                    "Authorization": {"source": "inline", "value": "Bearer sekrit"},
                    "X-Referenced": {"source": "reference", "name": "TOKEN"},
                },
                "body": {"raw": "payload"},
            },
        });
        let redacted = redact_definition(definition);
        assert_eq!(
            redacted["target"]["headers"]["Authorization"]["value"],
            "<redacted>"
        );
        assert_eq!(
            redacted["target"]["headers"]["X-Referenced"],
            json!({"source": "reference", "name": "TOKEN"})
        );
        assert_eq!(redacted["target"]["body"], "<redacted>");
    }

    #[test]
    fn redacts_nested_definition_wrappers() {
        let wrapped = json!({"definition": {"environment": {"values": {"K": "v"}}}});
        let redacted = redact_definition(wrapped);
        assert_eq!(
            redacted["definition"]["environment"]["values"]["K"],
            "<redacted>"
        );
    }

    #[test]
    fn terminal_states_are_recognized() {
        for state in [
            "succeeded",
            "failed",
            "timed_out",
            "cancelled",
            "skipped_overlap",
            "skipped_concurrency",
            "interrupted_unknown",
        ] {
            assert!(terminal_run_state(state), "{state} should be terminal");
        }
        for state in ["queued", "starting", "running", "retry_wait"] {
            assert!(!terminal_run_state(state), "{state} should not be terminal");
        }
    }

    #[test]
    fn job_document_redaction_keeps_definition_json_shape() {
        let value = json!({
            "id": "00000000-0000-7000-8000-000000000000",
            "definition_json": "{\"environment\":{\"values\":{\"K\":\"v\"}}}",
        });
        let redacted = redacted_job_document(value).expect("parse");
        assert_eq!(redacted["id"], "00000000-0000-7000-8000-000000000000");
        assert_eq!(
            redacted["definition_json"],
            json!(r#"{"environment":{"values":{"K":"<redacted>"}}}"#)
        );
    }

    #[test]
    fn observable_run_document_enriches_with_attempt_facts() {
        let run = json!({
            "id": "00000000-0000-7000-8000-000000000000",
            "trigger": "manual",
            "state": "succeeded",
            "finished_at_us": 200,
            "snapshot_json": "{\"environment\":{\"values\":{\"K\":\"v\"}}}",
        });
        let attempts = json!([
            {"running_at_us": 50},
            {"running_at_us": null},
        ]);
        let redacted = redacted_observable_run_document(run, attempts).expect("parse");
        assert_eq!(redacted["source"], "manual");
        assert_eq!(redacted["outcome"], "succeeded");
        assert_eq!(redacted["actual_started_at_us"], 50);
        assert_eq!(redacted["duration_us"], 150);
        assert_eq!(redacted["attempts"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn settings_document_replaces_environment_with_markers() {
        let value = json!({"global_concurrency": 4, "environment": {"A": "x", "B": "y"}});
        let redacted = redacted_settings_document(value);
        assert_eq!(redacted["global_concurrency"], 4);
        assert_eq!(
            redacted["environment"],
            json!({"A": {"configured": true, "value_redacted": true},
                   "B": {"configured": true, "value_redacted": true}})
        );
        assert!(
            redacted["environment"]["A"]["value_redacted"]
                .as_bool()
                .unwrap()
        );
    }
}
