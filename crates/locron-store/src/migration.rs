use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{StoreError, StoreResult};

pub const APPLICATION_ID: i32 = 0x4c4f_4352; // "LOCR"
pub const LATEST_SCHEMA_VERSION: i64 = 1;
const MIGRATION_NAME: &str = "initial durable state";

const INITIAL_SCHEMA: &str = include_str!("../migrations/0001_initial.sql");

pub(crate) fn migrate(
    connection: &mut Connection,
    binary_version: &str,
    now_us: i64,
) -> StoreResult<()> {
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(StoreError::NotLocronDatabase(application_id));
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > LATEST_SCHEMA_VERSION {
        return Err(StoreError::SchemaTooNew {
            found: version,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    if version == 0 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checked_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if checked_version != 0 {
            return Err(StoreError::MigrationConflict);
        }
        tx.execute_batch(INITIAL_SCHEMA)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)?;
        let checksum = checksum(INITIAL_SCHEMA);
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (1, ?1, ?2, ?3, ?4)",
            params![MIGRATION_NAME, checksum, binary_version, now_us],
        )?;
        tx.commit()?;
    } else {
        verify_migration(connection)?;
    }
    Ok(())
}

fn verify_migration(connection: &Connection) -> StoreResult<()> {
    let recorded: Option<String> = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let expected = checksum(INITIAL_SCHEMA);
    match recorded {
        Some(value) if value == expected => Ok(()),
        Some(value) => Err(StoreError::MigrationChecksumMismatch {
            version: 1,
            expected,
            found: value,
        }),
        None => Err(StoreError::MissingMigration(1)),
    }
}

fn checksum(sql: &str) -> String {
    let digest = crc32fast::hash(sql.as_bytes());
    format!("crc32:{digest:08x}")
}
