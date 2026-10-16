use crate::store::contention::Operation;
#[cfg(test)]
use rusqlite::TransactionBehavior;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{StoreError, StoreResult};

/// SQLite `application_id` marker identifying a locron database.
pub const APPLICATION_ID: i32 = 0x4c4f_4352; // "LOCR"
/// Highest schema `user_version` this binary knows how to migrate to.
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

pub(crate) fn migrate_admitted(
    connection: &mut Connection,
    binary_version: &str,
    now_us: i64,
    operation: &Operation,
) -> StoreResult<()> {
    #[cfg(windows)]
    let initial_execution_path = Some(locron_core::execution::default_execution_path());
    #[cfg(not(windows))]
    let initial_execution_path: Option<String> = None;
    migrate_with_initial_execution_path_admitted(
        connection,
        binary_version,
        now_us,
        initial_execution_path.as_deref(),
        operation,
    )
}

fn migrate_with_initial_execution_path_admitted(
    connection: &mut Connection,
    binary_version: &str,
    now_us: i64,
    initial_execution_path: Option<&str>,
    operation: &Operation,
) -> StoreResult<()> {
    let version = schema_version(connection)?;
    if version == LATEST_SCHEMA_VERSION {
        return verify_applied_migrations(connection, version);
    }
    migrate_pending_admitted(
        connection,
        binary_version,
        now_us,
        initial_execution_path,
        version,
        operation,
    )
}

fn migrate_pending_admitted(
    connection: &mut Connection,
    binary_version: &str,
    now_us: i64,
    initial_execution_path: Option<&str>,
    observed_version: i64,
    operation: &Operation,
) -> StoreResult<()> {
    let tx = operation.begin(connection)?;
    // Preliminary reads are not authoritative after waiting for another opener.
    // Validate its entire committed ledger before selecting any pending work.
    migration_pending(&tx, observed_version)?;
    if schema_version(&tx)? == LATEST_SCHEMA_VERSION {
        tx.rollback()?;
        return Ok(());
    }
    initialize_schema(&tx, binary_version, now_us, initial_execution_path)?;
    verify_migration(&tx, 1, INITIAL_SCHEMA)?;
    if migration_pending(&tx, 1)? {
        tx.execute_batch(DISABLED_CURSOR_SCHEMA)?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (2, ?1, ?2, ?3, ?4)",
            params![DISABLED_CURSOR_MIGRATION_NAME, checksum(DISABLED_CURSOR_SCHEMA), binary_version, now_us],
        )?;
    }
    verify_migration(&tx, 2, DISABLED_CURSOR_SCHEMA)?;
    if migration_pending(&tx, 2)? {
        tx.execute_batch(RETENTION_RECOVERY_SCHEMA)?;
        tx.pragma_update(None, "user_version", 3)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (3, ?1, ?2, ?3, ?4)",
            params![RETENTION_RECOVERY_MIGRATION_NAME, checksum(RETENTION_RECOVERY_SCHEMA), binary_version, now_us],
        )?;
    }
    verify_migration(&tx, 3, RETENTION_RECOVERY_SCHEMA)?;
    if migration_pending(&tx, 3)? {
        tx.execute_batch(GLOBAL_ENVIRONMENT_SCHEMA)?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (4, ?1, ?2, ?3, ?4)",
            params![GLOBAL_ENVIRONMENT_MIGRATION_NAME, checksum(GLOBAL_ENVIRONMENT_SCHEMA), binary_version, now_us],
        )?;
    }
    verify_migration(&tx, 4, GLOBAL_ENVIRONMENT_SCHEMA)?;
    if migration_pending(&tx, 4)? {
        tx.execute_batch(HTTP_CONTENT_TYPE_SCHEMA)?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (5, ?1, ?2, ?3, ?4)",
            params![HTTP_CONTENT_TYPE_MIGRATION_NAME, checksum(HTTP_CONTENT_TYPE_SCHEMA), binary_version, now_us],
        )?;
    }
    verify_applied_migrations(&tx, LATEST_SCHEMA_VERSION)?;
    operation.commit(tx)
}

// Recheck the logical initial-schema winner under the caller's BEGIN IMMEDIATE.
fn initialize_schema(
    tx: &Transaction<'_>,
    binary_version: &str,
    now_us: i64,
    initial_execution_path: Option<&str>,
) -> StoreResult<()> {
    if !migration_pending(tx, 0)? {
        return Ok(());
    }
    tx.execute_batch(INITIAL_SCHEMA)?;
    if let Some(path) = initial_execution_path {
        tx.execute(
            "UPDATE settings SET execution_path=?1 WHERE singleton=1",
            [path],
        )?;
    }
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", 1)?;
    tx.execute(
        "INSERT INTO schema_migrations(version, name, checksum, binary_version, applied_at_us) VALUES (1, ?1, ?2, ?3, ?4)",
        params![INITIAL_MIGRATION_NAME, checksum(INITIAL_SCHEMA), binary_version, now_us],
    )?;
    Ok(())
}

fn schema_version(connection: &Connection) -> StoreResult<i64> {
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
    Ok(version)
}

// Called only while the pending chain's BEGIN IMMEDIATE owns write admission.
fn migration_pending(connection: &Connection, predecessor: i64) -> StoreResult<bool> {
    let version = schema_version(connection)?;
    if version < predecessor {
        return Err(StoreError::MigrationConflict);
    }
    verify_applied_migrations(connection, version)?;
    Ok(version == predecessor)
}

fn verify_applied_migrations(connection: &Connection, version: i64) -> StoreResult<()> {
    for (applied_version, schema) in [
        (1, INITIAL_SCHEMA),
        (2, DISABLED_CURSOR_SCHEMA),
        (3, RETENTION_RECOVERY_SCHEMA),
        (4, GLOBAL_ENVIRONMENT_SCHEMA),
        (5, HTTP_CONTENT_TYPE_SCHEMA),
    ] {
        if applied_version <= version {
            verify_migration(connection, applied_version, schema)?;
        }
    }
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
fn migrate(connection: &mut Connection, binary_version: &str, now_us: i64) -> StoreResult<()> {
    connection.busy_timeout(std::time::Duration::ZERO)?;
    migrate_admitted(
        connection,
        binary_version,
        now_us,
        &Operation::new("migration", None),
    )
}

#[cfg(test)]
fn migrate_with_initial_execution_path(
    connection: &mut Connection,
    binary_version: &str,
    now_us: i64,
    initial_execution_path: Option<&str>,
) -> StoreResult<()> {
    connection.busy_timeout(std::time::Duration::ZERO)?;
    migrate_with_initial_execution_path_admitted(
        connection,
        binary_version,
        now_us,
        initial_execution_path,
        &Operation::new("migration", None),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    type MigrationRow = (i64, String, String, String, i64);

    fn migration_rows(connection: &Connection) -> Vec<MigrationRow> {
        connection
            .prepare(
                "SELECT version,name,checksum,binary_version,applied_at_us FROM schema_migrations ORDER BY version",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    struct InitialDatabase {
        path: std::path::PathBuf,
        _guard: locron_core::filesystem::DirectoryGuard,
        _temporary: tempfile::TempDir,
    }

    impl InitialDatabase {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("private");
            let guard = locron_core::filesystem::DirectoryGuard::private(&root).unwrap();
            let path = root.join("initial.db");
            drop(locron_core::filesystem::create_private_new(&path).unwrap());
            Self {
                path,
                _guard: guard,
                _temporary: temporary,
            }
        }

        fn open(&self) -> Connection {
            let connection = Connection::open(&self.path).unwrap();
            let mode: String = connection
                .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
            connection
        }
    }

    fn stored_execution_path(connection: &Connection) -> String {
        connection
            .query_row(
                "SELECT execution_path FROM settings WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn settings_tables(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn historical_database(version: usize) -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        if version == 0 {
            return connection;
        }
        let tx = connection.transaction().unwrap();
        for (index, (name, sql)) in [
            (INITIAL_MIGRATION_NAME, INITIAL_SCHEMA),
            (DISABLED_CURSOR_MIGRATION_NAME, DISABLED_CURSOR_SCHEMA),
            (RETENTION_RECOVERY_MIGRATION_NAME, RETENTION_RECOVERY_SCHEMA),
            (GLOBAL_ENVIRONMENT_MIGRATION_NAME, GLOBAL_ENVIRONMENT_SCHEMA),
            (HTTP_CONTENT_TYPE_MIGRATION_NAME, HTTP_CONTENT_TYPE_SCHEMA),
        ]
        .into_iter()
        .take(version)
        .enumerate()
        {
            tx.execute_batch(sql).unwrap();
            tx.execute(
                "INSERT INTO schema_migrations VALUES (?1,?2,?3,'historical',123)",
                params![i64::try_from(index + 1).unwrap(), name, checksum(sql)],
            )
            .unwrap();
        }
        tx.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        tx.pragma_update(None, "user_version", i64::try_from(version).unwrap())
            .unwrap();
        tx.execute(
            "UPDATE settings SET execution_path='preserved',global_concurrency=7,updated_at_us=77 WHERE singleton=1",
            [],
        )
        .unwrap();
        tx.commit().unwrap();
        connection
    }

    #[derive(Debug, PartialEq)]
    struct DatabaseSnapshot {
        schema: Vec<(String, String, String, Option<String>)>,
        settings: Vec<rusqlite::types::Value>,
        migrations: Vec<MigrationRow>,
        application_id: i32,
        version: i64,
    }

    fn database_snapshot(connection: &Connection) -> DatabaseSnapshot {
        let schema = connection
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let settings = connection
            .query_row("SELECT * FROM settings WHERE singleton=1", [], |row| {
                (0..row.as_ref().column_count())
                    .map(|index| row.get(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        DatabaseSnapshot {
            schema,
            settings,
            migrations: migration_rows(connection),
            application_id: connection
                .pragma_query_value(None, "application_id", |row| row.get(0))
                .unwrap(),
            version: connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap(),
        }
    }

    #[test]
    fn every_pending_chain_uses_one_commit_and_preserves_existing_records() {
        for version in 0..5 {
            let mut connection = historical_database(version);
            let previous = if version == 0 {
                Vec::new()
            } else {
                migration_rows(&connection)
            };
            let operation = Operation::new("open", None);
            migrate_with_initial_execution_path_admitted(
                &mut connection,
                "upgrade",
                456,
                Some(r"C:\initial 路径"),
                &operation,
            )
            .unwrap();
            assert_eq!(operation.committed_steps(), 1, "starting version {version}");
            let snapshot = database_snapshot(&connection);
            assert_eq!(snapshot.version, LATEST_SCHEMA_VERSION);
            assert_eq!(snapshot.application_id, APPLICATION_ID);
            assert_eq!(snapshot.migrations.len(), 5);
            assert_eq!(snapshot.migrations[..version], previous);
            assert!(
                snapshot.migrations[version..]
                    .iter()
                    .all(|row| row.3 == "upgrade" && row.4 == 456)
            );
            assert_eq!(
                stored_execution_path(&connection),
                if version == 0 {
                    r"C:\initial 路径"
                } else {
                    "preserved"
                }
            );
            verify_applied_migrations(&connection, LATEST_SCHEMA_VERSION).unwrap();

            let verification = Operation::new("open", None);
            migrate_admitted(&mut connection, "later", 789, &verification).unwrap();
            assert_eq!(verification.committed_steps(), 0);
            assert_eq!(database_snapshot(&connection), snapshot);
        }
    }

    #[test]
    fn current_schema_verification_never_requests_write_admission() {
        let database = InitialDatabase::new();
        let mut writer = database.open();
        migrate(&mut writer, "winner", 123).unwrap();
        let mut verifier = database.open();
        verifier.busy_timeout(std::time::Duration::ZERO).unwrap();
        let before = database_snapshot(&verifier);
        let tx = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        // Any attempted BEGIN would fail immediately while this real writer is held.
        let operation = Operation::new("open", None);
        migrate_admitted(&mut verifier, "reader", 456, &operation).unwrap();
        assert_eq!(operation.committed_steps(), 0);
        assert!(verifier.is_autocommit());
        assert_eq!(database_snapshot(&verifier), before);
        tx.rollback().unwrap();
    }

    fn assert_failed_chain_preserves_database(connection: &mut Connection) -> StoreError {
        let before = database_snapshot(connection);
        let operation = Operation::new("open", None);
        let result = migrate_admitted(connection, "failed upgrade", 456, &operation);
        assert!(connection.is_autocommit());
        assert_eq!(operation.committed_steps(), 0);
        assert_eq!(database_snapshot(connection), before);
        let observation = operation.finish(&result, connection.is_autocommit());
        assert_eq!(observation.durable_outcome, "not_committed");
        assert!(!observation.retryable);
        result.unwrap_err()
    }

    #[test]
    fn final_ledger_refusal_rolls_back_all_pending_ddl_settings_and_markers() {
        let mut connection = historical_database(1);
        connection
            .execute_batch(
                "CREATE TRIGGER refuse_fifth BEFORE INSERT ON schema_migrations
                 WHEN NEW.version=5 BEGIN SELECT RAISE(ABORT,'fixed-refusal'); END;",
            )
            .unwrap();
        let error = assert_failed_chain_preserves_database(&mut connection);
        assert!(matches!(error, StoreError::Sqlite(_)));
    }

    #[test]
    fn final_ddl_conflict_rolls_back_earlier_pending_migrations() {
        let mut connection = historical_database(1);
        connection.execute_batch(HTTP_CONTENT_TYPE_SCHEMA).unwrap();
        let error = assert_failed_chain_preserves_database(&mut connection);
        assert!(matches!(error, StoreError::Sqlite(_)));
    }

    #[test]
    fn final_checksum_verification_precedes_the_only_commit() {
        let mut connection = historical_database(1);
        connection
            .execute_batch(
                "CREATE TRIGGER corrupt_old_checksum AFTER INSERT ON schema_migrations
                 WHEN NEW.version=5 BEGIN
                   UPDATE schema_migrations SET checksum='tampered' WHERE version=1;
                 END;",
            )
            .unwrap();
        let error = assert_failed_chain_preserves_database(&mut connection);
        assert!(matches!(
            error,
            StoreError::MigrationChecksumMismatch { version: 1, .. }
        ));
    }

    #[test]
    fn stale_admission_releases_a_verified_winner_without_committing() {
        let mut connection = historical_database(5);
        let before = database_snapshot(&connection);
        let operation = Operation::new("open", None);
        migrate_pending_admitted(
            &mut connection,
            "stale loser",
            456,
            Some(r"C:\replacement"),
            0,
            &operation,
        )
        .unwrap();
        assert!(connection.is_autocommit());
        assert_eq!(operation.committed_steps(), 0);
        assert_eq!(database_snapshot(&connection), before);
    }

    #[test]
    fn stale_admission_revalidates_markers_complete_ledger_and_forward_progress() {
        for case in 0..5 {
            let mut connection = historical_database(if case == 4 { 1 } else { 5 });
            match case {
                0 => connection
                    .pragma_update(None, "application_id", 42)
                    .unwrap(),
                1 => connection.pragma_update(None, "user_version", 6).unwrap(),
                2 => {
                    connection
                        .execute("DELETE FROM schema_migrations WHERE version=3", [])
                        .unwrap();
                }
                3 => {
                    connection
                        .execute(
                            "UPDATE schema_migrations SET checksum='tampered' WHERE version=4",
                            [],
                        )
                        .unwrap();
                }
                _ => {}
            }
            let before = database_snapshot(&connection);
            let operation = Operation::new("open", None);
            let error = migrate_pending_admitted(
                &mut connection,
                "stale loser",
                456,
                None,
                if case == 4 { 2 } else { 0 },
                &operation,
            )
            .unwrap_err();
            assert!(matches!(
                (case, error),
                (0, StoreError::NotLocronDatabase(42))
                    | (1, StoreError::SchemaTooNew { found: 6, .. })
                    | (2, StoreError::MissingMigration(3))
                    | (3, StoreError::MigrationChecksumMismatch { version: 4, .. })
                    | (4, StoreError::MigrationConflict)
            ));
            assert!(connection.is_autocommit());
            assert_eq!(operation.committed_steps(), 0);
            assert_eq!(database_snapshot(&connection), before);
        }
    }

    #[test]
    fn expired_operation_refuses_before_initial_schema_mutation() {
        let mut connection = historical_database(0);
        let operation = Operation::new("open", Some(std::time::Instant::now()));
        assert!(matches!(
            migrate_admitted(&mut connection, "expired", 456, &operation),
            Err(StoreError::MaintenanceDeferred)
        ));
        assert!(connection.is_autocommit());
        assert_eq!(operation.committed_steps(), 0);
        assert_eq!(settings_tables(&connection), 0);
        assert_eq!(schema_version(&connection).unwrap(), 0);
    }

    #[test]
    fn initial_path_commits_atomically_and_stale_initializer_preserves_the_winner() {
        let database = InitialDatabase::new();
        let mut creator = database.open();
        let mut observer = database.open();
        observer.busy_timeout(std::time::Duration::ZERO).unwrap();
        let winner_path = r"C:\initial 路径;C:\Windows\System32";
        let loser_path = r"C:\different opener";
        assert_eq!(settings_tables(&observer), 0);

        let tx = creator
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        initialize_schema(&tx, "winner", 123, Some(winner_path)).unwrap();
        assert_eq!(stored_execution_path(&tx), winner_path);
        assert_eq!(settings_tables(&observer), 0);
        let Err(error) = observer.transaction_with_behavior(TransactionBehavior::Immediate) else {
            panic!("another writer cannot enter the initial transaction");
        };
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(code, _)
                if code.code == rusqlite::ErrorCode::DatabaseBusy
        ));
        tx.commit().unwrap();
        assert_eq!(stored_execution_path(&observer), winner_path);
        let initial_rows = migration_rows(&observer);
        assert_eq!(initial_rows.len(), 1);
        assert_eq!(initial_rows[0].2, checksum(INITIAL_SCHEMA));

        // This is the same admitted initializer a version-zero stale opener reaches.
        let tx = observer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        initialize_schema(&tx, "stale loser", 456, Some(loser_path)).unwrap();
        tx.commit().unwrap();
        assert_eq!(migration_rows(&observer), initial_rows);
        assert_eq!(stored_execution_path(&observer), winner_path);
        migrate_with_initial_execution_path(&mut observer, "catch-up", 789, Some(loser_path))
            .unwrap();
        assert_eq!(stored_execution_path(&observer), winner_path);
        assert_eq!(migration_rows(&observer)[0], initial_rows[0]);
    }

    #[test]
    fn rolled_back_initialization_publishes_no_settings_and_next_opener_chooses_its_default() {
        let database = InitialDatabase::new();
        let mut creator = database.open();
        let mut recovery = database.open();
        let tx = creator
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        initialize_schema(&tx, "abandoned", 123, Some(r"C:\abandoned")).unwrap();
        assert_eq!(settings_tables(&recovery), 0);
        tx.rollback().unwrap();
        drop(creator);
        assert_eq!(settings_tables(&recovery), 0);
        let version: i64 = recovery
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);

        let recovery_path = r"C:\recovery 路径";
        migrate_with_initial_execution_path(&mut recovery, "recovery", 456, Some(recovery_path))
            .unwrap();
        assert_eq!(stored_execution_path(&recovery), recovery_path);
        let rows = migration_rows(&recovery);
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|row| row.3 == "recovery" && row.4 == 456));
        assert_eq!(rows[0].2, checksum(INITIAL_SCHEMA));
    }

    #[test]
    fn creator_close_after_initial_commit_keeps_the_path_through_later_migration_recovery() {
        let database = InitialDatabase::new();
        let mut creator = database.open();
        let mut recovery = database.open();
        let winner_path = r"C:\committed 路径";
        let tx = creator
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        initialize_schema(&tx, "creator", 123, Some(winner_path)).unwrap();
        tx.commit().unwrap();
        drop(creator);
        assert_eq!(stored_execution_path(&recovery), winner_path);
        let initial_row = migration_rows(&recovery).remove(0);
        assert_eq!(initial_row.2, checksum(INITIAL_SCHEMA));

        migrate_with_initial_execution_path(
            &mut recovery,
            "recovery",
            456,
            Some(r"C:\replacement"),
        )
        .unwrap();
        assert_eq!(stored_execution_path(&recovery), winner_path);
        let rows = migration_rows(&recovery);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], initial_row);
        assert!(
            rows[1..]
                .iter()
                .all(|row| row.3 == "recovery" && row.4 == 456)
        );
    }

    #[test]
    fn versioned_historical_and_custom_paths_are_preserved_even_when_untouched() {
        for existing_path in ["/usr/local/bin:/usr/bin:/bin", r"C:\custom 路径"] {
            let mut connection = Connection::open_in_memory().unwrap();
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            initialize_schema(&tx, "old", 123, None).unwrap();
            tx.execute(
                "UPDATE settings SET execution_path=?1 WHERE singleton=1",
                [existing_path],
            )
            .unwrap();
            tx.commit().unwrap();
            let initial_row = migration_rows(&connection).remove(0);

            migrate_with_initial_execution_path(
                &mut connection,
                "new",
                456,
                Some(r"C:\new Windows default"),
            )
            .unwrap();
            assert_eq!(stored_execution_path(&connection), existing_path);
            assert_eq!(migration_rows(&connection)[0], initial_row);
            assert_eq!(
                connection
                    .query_row(
                        "SELECT updated_at_us FROM settings WHERE singleton=1",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn stale_pending_step_accepts_only_verified_concurrent_advance() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = locron_core::filesystem::DirectoryGuard::private(&root).unwrap();
        let database = root.join("migration.db");
        drop(locron_core::filesystem::create_private_new(&database).unwrap());
        let mut first = Connection::open(&database).unwrap();
        let mut second = Connection::open(&database).unwrap();
        let stale_version: i64 = first
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(stale_version, 0);

        migrate(&mut second, "winner", 123).unwrap();
        second
            .execute(
                "UPDATE settings SET global_concurrency=7,execution_path='kept' WHERE singleton=1",
                [],
            )
            .unwrap();
        let winner_rows = migration_rows(&second);
        assert_eq!(winner_rows.len(), 5);
        assert!(
            winner_rows
                .iter()
                .all(|row| row.3 == "winner" && row.4 == 123)
        );

        let tx = first
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(!migration_pending(&tx, stale_version).unwrap());
        tx.commit().unwrap();
        migrate(&mut first, "catch-up", 456).unwrap();

        assert_eq!(migration_rows(&first), winner_rows);
        assert_eq!(
            first
                .query_row(
                    "SELECT global_concurrency,execution_path FROM settings WHERE singleton=1",
                    [],
                    |row| { Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)) }
                )
                .unwrap(),
            (7, "kept".into())
        );
    }

    #[test]
    fn pending_step_refuses_changed_markers_and_unverified_advance() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection, "winner", 1).unwrap();
        let original = migration_rows(&connection);

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.pragma_update(None, "application_id", 42).unwrap();
        assert!(matches!(
            migration_pending(&tx, 0),
            Err(StoreError::NotLocronDatabase(42))
        ));
        drop(tx);

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.pragma_update(None, "user_version", 6).unwrap();
        assert!(matches!(
            migration_pending(&tx, 0),
            Err(StoreError::SchemaTooNew {
                found: 6,
                supported: 5
            })
        ));
        drop(tx);

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("DELETE FROM schema_migrations WHERE version=3", [])
            .unwrap();
        assert!(matches!(
            migration_pending(&tx, 0),
            Err(StoreError::MissingMigration(3))
        ));
        drop(tx);

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute(
            "UPDATE schema_migrations SET checksum='tampered' WHERE version=4",
            [],
        )
        .unwrap();
        assert!(matches!(
            migration_pending(&tx, 0),
            Err(StoreError::MigrationChecksumMismatch { version: 4, .. })
        ));
        drop(tx);

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.pragma_update(None, "user_version", 2).unwrap();
        assert!(matches!(
            migration_pending(&tx, 3),
            Err(StoreError::MigrationConflict)
        ));
        drop(tx);

        assert_eq!(migration_rows(&connection), original);
        assert_eq!(
            connection
                .pragma_query_value(None, "application_id", |row| row.get::<_, i32>(0))
                .unwrap(),
            APPLICATION_ID
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            LATEST_SCHEMA_VERSION
        );
    }

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
