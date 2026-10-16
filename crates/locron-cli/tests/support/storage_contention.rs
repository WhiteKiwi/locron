//! Owned disabled, secret-bearing fixtures and real external SQLite writer controls.

use std::collections::BTreeMap;

use locron_core::command::{CompletionAction, JobDefinition};
use locron_core::policy::ExecutionPolicy;
use locron_core::schedule::Schedule;
use locron_core::target::{Environment, HttpHeaderSource, HttpMethod, HttpTarget, Target};
use locron_core::{DurationMicros, Timestamp};
use locron_store::{CreateJob, StatePaths, Store};
use serde_json::Value;

pub const SECRETS: [&str; 4] = [
    "private-contention-argv-canary",
    "private-contention-env-canary",
    "private-contention-header-canary",
    "private-contention-body-canary",
];

pub fn seed(paths: &StatePaths) {
    let store = Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1).unwrap();
    let targets = [
        (
            "00000000-0000-7000-8000-000000000180",
            "private-process",
            Target::Process {
                executable: std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                args: vec![SECRETS[0].into()],
            },
        ),
        (
            "00000000-0000-7000-8000-000000000181",
            "private-http",
            Target::Http(HttpTarget {
                method: HttpMethod::Post,
                url: "http://127.0.0.1:1/never-executed".into(),
                headers: BTreeMap::from([(
                    "Authorization".into(),
                    HttpHeaderSource::Inline(SECRETS[2].into()),
                )]),
                body: Some(SECRETS[3].as_bytes().to_vec()),
                body_file: None,
                success_statuses: vec![200],
                follow_redirects: false,
            }),
        ),
    ];
    for (id, name, target) in targets {
        let definition = JobDefinition {
            schedule: Schedule::Every {
                interval: DurationMicros::new(900_000_000),
                anchor: Timestamp::from_epoch_micros(1),
            },
            target,
            cwd: paths.root.clone(),
            environment: Environment {
                values: BTreeMap::from([("PRIVATE_TOKEN".into(), SECRETS[1].into())]),
                ..Environment::default()
            },
            policy: ExecutionPolicy::default(),
            completion_action: CompletionAction::Retain,
        };
        store
            .create_job(&CreateJob {
                id: id.into(),
                name: name.into(),
                description: None,
                tags_json: "[]".into(),
                enabled: false,
                definition_json: serde_json::to_string(&definition).unwrap(),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
    }
}

pub fn hold_writer(paths: &StatePaths) -> rusqlite::Connection {
    let writer = rusqlite::Connection::open(&paths.database).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
}

pub fn reject_private_mutations(paths: &StatePaths) {
    let connection = rusqlite::Connection::open(&paths.database).unwrap();
    // The raw adapter message deliberately contains every class of secret.
    for (trigger, table, action) in [
        ("reject_private_removal", "jobs", "UPDATE OF removed_at_us"),
        ("reject_private_enqueue", "runs", "INSERT"),
    ] {
        connection
            .execute_batch(&format!(
                "CREATE TRIGGER {trigger} BEFORE {action} ON {table} \
             BEGIN SELECT RAISE(ABORT, '{}'); END;",
                SECRETS.join(" "),
            ))
            .unwrap();
    }
}

pub fn allow_private_mutations(paths: &StatePaths) {
    rusqlite::Connection::open(&paths.database)
        .unwrap()
        .execute_batch("DROP TRIGGER reject_private_removal; DROP TRIGGER reject_private_enqueue;")
        .unwrap();
}

pub fn assert_rejected(error: &Value) {
    let storage = &error["storage"];
    assert_eq!(error["code"], "state_error");
    assert_eq!(storage["kind"], "sqlite");
    assert_eq!(storage["stage"], "execute");
    assert_eq!(storage["sqlite_primary_code"], 19);
    assert_eq!(storage["sqlite_extended_code"], 1811);
    assert_eq!(storage["retries"], 0);
    assert_eq!(storage["retryable"], false);
    assert_eq!(storage["durable_outcome"], "not_committed");
    assert_private(&serde_json::to_vec(error).unwrap());
}

pub fn assert_private(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for secret in SECRETS {
        assert!(
            !text.contains(secret),
            "diagnostic exposed configured secret"
        );
    }
}

pub fn assert_busy(error: &Value) {
    assert_eq!(error["code"], "state_error");
    let storage = &error["storage"];
    assert_eq!(storage["kind"], "busy");
    assert_eq!(storage["sqlite_primary_code"], 5);
    assert_eq!(storage["sqlite_extended_code"].as_i64().unwrap() & 255, 5);
    assert_eq!(storage["durable_outcome"], "not_committed");
    assert_eq!(storage["lock_owner"], "unknown");
    assert_eq!(storage["retryable"], true);
    assert!(storage["operation"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(matches!(
        storage["stage"].as_str(),
        Some("begin" | "configuration" | "migration")
    ));
    assert!(storage["elapsed_us"].as_u64().unwrap() >= 4_800_000);
    assert!(storage["sqlite_wait_us"].as_u64().unwrap() >= 4_500_000);
    assert!(storage["retries"].as_u64().unwrap() > 0);
    for field in [
        "admission_wait_us",
        "connection_wait_us",
        "transaction_us",
        "commit_us",
    ] {
        assert!(storage[field].is_u64(), "missing measurement: {field}");
    }
    assert!(storage.get("os_error_code").is_some());
    let message = error["message"].as_str().unwrap();
    assert!(message.contains(storage["stage"].as_str().unwrap()));
    assert!(message.contains("SQLite primary 5"));
    assert!(message.contains("retry"));
    assert_private(&serde_json::to_vec(error).unwrap());
}
