use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{StoreError, StoreResult};

pub const APPLICATION_ID: i32 = 0x4c4f_4352; // "LOCR"
pub const LATEST_SCHEMA_VERSION: i64 = 5;
const INITIAL_MIGRATION_NAME: &str = "initial durable state";
const DISABLED_CURSOR_MIGRATION_NAME: &str = "record disabled cursor intervals";
const RETENTION_RECOVERY_MIGRATION_NAME: &str = "bound retention and recovery";
const GLOBAL_ENVIRONMENT_MIGRATION_NAME: &str = "persist global environment";
const HTTP_CONTENT_TYPE_MIGRATION_NAME: &str = "persist HTTP response content type";

const INITIAL_SCHEMA: &str = include_str!("../migrations/0001_initial.sql");
const DISABLED_CURSOR_SCHEMA: &str = include_str!("../migrations/0002_disabled_cursor.sql");
const RETENTION_RECOVERY_SCHEMA: &str = include_str!("../migrations/0003_retention_recovery.sql");
const GLOBAL_ENVIRONMENT_SCHEMA: &str = include_str!("../migrations/0004_global_environment.sql");
const HTTP_CONTENT_TYPE_SCHEMA: &str = include_str!("../migrations/0005_http_content_type.sql");

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
        tx.pragma_update(None, "user_version", 1)?;
        let checksum = checksum(INITIAL_SCHEMA);
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (1, ?1, ?2, ?3, ?4)",
            params![INITIAL_MIGRATION_NAME, checksum, binary_version, now_us],
        )?;
        tx.commit()?;
    }
    verify_migration(connection, 1, INITIAL_SCHEMA)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 2 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checked_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if checked_version != 1 {
            return Err(StoreError::MigrationConflict);
        }
        tx.execute_batch(DISABLED_CURSOR_SCHEMA)?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (2, ?1, ?2, ?3, ?4)",
            params![DISABLED_CURSOR_MIGRATION_NAME, checksum(DISABLED_CURSOR_SCHEMA), binary_version, now_us],
        )?;
        tx.commit()?;
    }
    verify_migration(connection, 2, DISABLED_CURSOR_SCHEMA)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 3 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checked_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if checked_version != 2 {
            return Err(StoreError::MigrationConflict);
        }
        tx.execute_batch(RETENTION_RECOVERY_SCHEMA)?;
        tx.pragma_update(None, "user_version", 3)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (3, ?1, ?2, ?3, ?4)",
            params![RETENTION_RECOVERY_MIGRATION_NAME, checksum(RETENTION_RECOVERY_SCHEMA), binary_version, now_us],
        )?;
        tx.commit()?;
    }
    verify_migration(connection, 3, RETENTION_RECOVERY_SCHEMA)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 4 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checked_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if checked_version != 3 {
            return Err(StoreError::MigrationConflict);
        }
        tx.execute_batch(GLOBAL_ENVIRONMENT_SCHEMA)?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (4, ?1, ?2, ?3, ?4)",
            params![GLOBAL_ENVIRONMENT_MIGRATION_NAME, checksum(GLOBAL_ENVIRONMENT_SCHEMA), binary_version, now_us],
        )?;
        tx.commit()?;
    }
    verify_migration(connection, 4, GLOBAL_ENVIRONMENT_SCHEMA)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 5 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checked_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if checked_version != 4 {
            return Err(StoreError::MigrationConflict);
        }
        tx.execute_batch(HTTP_CONTENT_TYPE_SCHEMA)?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (5, ?1, ?2, ?3, ?4)",
            params![HTTP_CONTENT_TYPE_MIGRATION_NAME, checksum(HTTP_CONTENT_TYPE_SCHEMA), binary_version, now_us],
        )?;
        tx.commit()?;
    }
    verify_migration(connection, 5, HTTP_CONTENT_TYPE_SCHEMA)?;
    Ok(())
}

fn verify_migration(connection: &Connection, version: i64, sql: &str) -> StoreResult<()> {
    let recorded: Option<String> = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            [version],
            |row| row.get(0),
        )
        .optional()?;
    let expected = checksum(sql);
    match recorded {
        Some(value) if value == expected => Ok(()),
        Some(value) => Err(StoreError::MigrationChecksumMismatch {
            version,
            expected,
            found: value,
        }),
        None => Err(StoreError::MissingMigration(version)),
    }
}

fn checksum(sql: &str) -> String {
    let digest = crc32fast::hash(sql.as_bytes());
    format!("crc32:{digest:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_existing_v1_database_without_inventing_disabled_history() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(INITIAL_SCHEMA).unwrap();
        connection
            .pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations(version,name,checksum,binary_version,applied_at_us) VALUES(1,?1,?2,'old',1)",
                params![INITIAL_MIGRATION_NAME, checksum(INITIAL_SCHEMA)],
            )
            .unwrap();
        let tx = connection.transaction().unwrap();
        tx.pragma_update(None, "defer_foreign_keys", true).unwrap();
        tx.execute_batch(
            "INSERT INTO jobs(id,name,tags_json,enabled,created_at_us,updated_at_us,current_revision)
             VALUES('018f3f74-8d70-7cc0-98a2-eef43f17eab4','legacy','[]',1,1,1,1);
             INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by)
             VALUES('018f3f74-8d70-7cc0-98a2-eef43f17eab4',1,'{}',1,'add');
             INSERT INTO schedule_cursors(job_id,revision,cursor_us,updated_at_us)
             VALUES('018f3f74-8d70-7cc0-98a2-eef43f17eab4',1,42,1);",
        )
        .unwrap();
        tx.commit().unwrap();

        migrate(&mut connection, "new", 2).unwrap();

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);
        let disabled_since: Option<i64> = connection
            .query_row(
                "SELECT disabled_since_us FROM schedule_cursors WHERE job_id='018f3f74-8d70-7cc0-98a2-eef43f17eab4'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(disabled_since, None);
        let retention_age_us: Option<i64> = connection
            .query_row(
                "SELECT run_retention_age_us FROM settings WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retention_age_us, Some(7_776_000_000_000));
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM schema_migrations WHERE version=2 AND binary_version='new'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM schema_migrations WHERE version=3 AND binary_version='new'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn clean_database_receives_current_defaults() {
        let mut connection = Connection::open_in_memory().unwrap();

        migrate(&mut connection, "new", 2).unwrap();

        assert_eq!(
            connection
                .query_row(
                    "SELECT run_retention_age_us FROM settings WHERE singleton=1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            7_776_000_000_000
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='run_retention_pending'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT environment_json FROM settings WHERE singleton=1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "{}"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM pragma_table_info('attempts') WHERE name='http_content_type'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn upgrades_v3_settings_with_an_empty_global_environment() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection, "old", 1).unwrap();
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .execute("DELETE FROM schema_migrations WHERE version IN (4, 5)", [])
            .unwrap();
        connection
            .execute("ALTER TABLE settings DROP COLUMN environment_json", [])
            .unwrap();
        connection
            .execute("ALTER TABLE attempts DROP COLUMN http_content_type", [])
            .unwrap();

        migrate(&mut connection, "new", 2).unwrap();

        assert_eq!(
            connection
                .query_row(
                    "SELECT environment_json FROM settings WHERE singleton=1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "{}"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT binary_version FROM schema_migrations WHERE version=4",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "new"
        );
    }

    #[test]
    fn upgrades_v4_attempts_with_an_empty_http_content_type() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection, "old", 1).unwrap();
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute("DELETE FROM schema_migrations WHERE version=5", [])
            .unwrap();
        connection
            .execute("ALTER TABLE attempts DROP COLUMN http_content_type", [])
            .unwrap();

        migrate(&mut connection, "new", 2).unwrap();

        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM pragma_table_info('attempts') WHERE name='http_content_type'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT binary_version FROM schema_migrations WHERE version=5",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "new"
        );
    }
}
