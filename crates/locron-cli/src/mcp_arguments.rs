//! Validate server-declared mutation inputs without opening durable state.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, ensure};
use serde_json::Value;

/// This deliberately implements only the primitive shapes used by our tool
/// catalogue. It is not a general JSON Schema interpreter. Unknown shapes
/// refuse rather than silently allowing a newly declared input type.
fn matches_shape(value: &Value, rule: &Value) -> bool {
    let typed = match rule.get("type") {
        Some(Value::String(kind)) => match kind.as_str() {
            "boolean" => value.is_boolean(),
            "string" => value.is_string(),
            "integer" => value.as_u64().is_some_and(|number| {
                rule.get("minimum")
                    .is_none_or(|min| min.as_u64().is_some_and(|min| number >= min))
                    && rule
                        .get("maximum")
                        .is_none_or(|max| max.as_u64().is_some_and(|max| number <= max))
            }),
            "array"
                if rule.get("items").and_then(|items| items.get("type"))
                    == Some(&Value::String("string".into())) =>
            {
                value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
            }
            _ => false,
        },
        // Description null means absent on creation and explicit clearing on
        // update. No boolean, integer or array field treats null as omission.
        Some(Value::Array(types))
            if types.len() == 2 && types[0] == "string" && types[1] == "null" =>
        {
            value.is_string() || value.is_null()
        }
        _ => false,
    };
    typed
        && rule.get("enum").is_none_or(|options| {
            options
                .as_array()
                .is_some_and(|options| options.contains(value))
        })
}

pub(super) fn validate(tool: &str, arguments: &Value) -> Result<()> {
    let object = arguments
        .as_object()
        .ok_or_else(|| anyhow!("tool arguments must be an object"))?;
    let catalogue = super::handle_tools_list();
    let schema = catalogue["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|entry| entry["name"] == tool))
        .and_then(|entry| entry.get("inputSchema"))
        .ok_or_else(|| anyhow!("mutation tool has no declared input schema"))?;
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(|| anyhow!("mutation tool has no declared input properties"))?;
    if let Some(required) = schema.get("required") {
        for field in required
            .as_array()
            .ok_or_else(|| anyhow!("invalid declared required fields"))?
        {
            let name = field
                .as_str()
                .ok_or_else(|| anyhow!("invalid declared field name"))?;
            ensure!(
                object.contains_key(name),
                "missing required parameter: {name}"
            );
        }
    }
    for (name, rule) in properties {
        if let Some(value) = object.get(name) {
            // Keep actionable range guidance without reflecting supplied data
            // or duplicating the catalogue's accepted numeric bounds.
            if rule["type"] == "integer"
                && let (Some(minimum), Some(maximum)) =
                    (rule["minimum"].as_u64(), rule["maximum"].as_u64())
            {
                ensure!(
                    matches_shape(value, rule),
                    "parameter {name} must be an integer from {minimum} through {maximum}"
                );
            }
            ensure!(
                matches_shape(value, rule),
                "parameter {name} must match its declared type and constraints"
            );
        }
    }
    // tools/list has no additionalProperties:false restriction. Preserve
    // unrelated extension fields rather than imposing a new closed schema.
    Ok(())
}

/// Called before Store opening/enqueue/wake, even for an explicit unused wait
/// duration. There is no product maximum: the platform clock must represent it.
pub(super) fn wait_deadline(arguments: &Value, now: Instant) -> Result<Instant> {
    let seconds = super::parse_timeout_seconds(arguments.get("timeout_seconds"))?.unwrap_or(30);
    now.checked_add(Duration::from_secs(seconds))
        .ok_or_else(|| anyhow!("timeout_seconds exceeds the platform wait clock range"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn schema_admission_is_strict_and_does_not_reflect_supplied_values() {
        for bad in [
            json!(null),
            json!("secret-canary"),
            json!(0),
            json!([]),
            json!({}),
        ] {
            let error = validate("locron_run_job", &json!({"job":"test", "dry_run":bad}))
                .unwrap_err()
                .to_string();
            assert!(error.contains("dry_run"));
            assert!(!error.contains("secret-canary"));
        }
        for value in [
            json!({"job":"test"}),
            json!({"job":"test", "wait":false}),
            json!({"job":"test", "wait":true, "dry_run":true, "extension":42}),
        ] {
            validate("locron_run_job", &value).unwrap();
        }
        assert!(validate("locron_run_job", &json!({})).is_err());
        assert!(validate("locron_run_job", &json!([])).is_err());
    }

    #[test]
    fn unknown_declared_shapes_and_mixed_string_arrays_refuse() {
        assert!(!matches_shape(&json!({}), &json!({"type":"object"})));
        assert!(!matches_shape(
            &json!([1]),
            &json!({"type":"array","items":{"type":"integer"}})
        ));
        assert!(!matches_shape(
            &json!(["ok", 1]),
            &json!({"type":"array","items":{"type":"string"}})
        ));
        assert!(matches_shape(
            &json!([]),
            &json!({"type":"array","items":{"type":"string"}})
        ));
        assert!(!matches_shape(&json!(1.0), &json!({"type":"integer"})));
    }

    #[test]
    fn deadline_uses_one_checked_clock_without_an_arbitrary_wait_cap() {
        let now = Instant::now();
        assert_eq!(
            wait_deadline(&json!({}), now).unwrap().duration_since(now),
            Duration::from_secs(30)
        );
        for seconds in [1, 86400] {
            assert_eq!(
                wait_deadline(&json!({"timeout_seconds":seconds}), now)
                    .unwrap()
                    .duration_since(now),
                Duration::from_secs(seconds)
            );
        }
        for bad in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("30"),
            json!(null),
            json!(u64::MAX),
        ] {
            assert!(wait_deadline(&json!({"timeout_seconds":bad}), now).is_err());
        }
    }
}
