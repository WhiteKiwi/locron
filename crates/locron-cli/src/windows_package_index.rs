//! Read-only, test-only qualification of a bounded WinGet portable-index snapshot.
//!
//! These are metadata facts, not live package ownership. No registry, filesystem,
//! native image, canonical release, lifecycle or journal authority is created here.

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Result, bail, ensure};
use rusqlite::config::DbConfig;
use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB, Row};

use super::sha256_hex;
use super::windows_protocol::{maintenance_path, native_target};
use super::windows_receipt::{same_path, stable_version};

const SNAPSHOT_LIMIT: usize = 4 * 1024 * 1024;
const VALUE_LIMIT: usize = 16 * 1024;
// SQLite's length limit includes the complete encoded row, not only each value.
const ROW_LIMIT: i32 = 4 * 16 * 1024 + 128;
const METADATA_ROWS: usize = 8;
const PROGRESS_INTERVAL: i32 = 1_000;
const PROGRESS_CALLBACKS: u32 = 1_024;

/// Borrowed caller-selected registration metadata, never a live registry proof.
struct Registration<'a> {
    product_code: &'a str,
    package_id: &'a str,
    source_id: &'a str,
    version: &'a str,
    target: &'a str,
    install_location: &'a str,
    index_path: &'a str,
    console_alias: &'a str,
}

/// Exact row/path facts and snapshot digest; none can authenticate installed bytes.
#[derive(Debug, PartialEq, Eq)]
struct IndexFacts {
    directory: String,
    console_alias: String,
    console: String,
    snapshot_sha256: String,
}

fn before_deadline(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "original index deadline expired");
    Ok(())
}

impl Registration<'_> {
    fn validate(&self, root: &str) -> Result<()> {
        ensure!(
            self.package_id == "WhiteKiwi.locron",
            "foreign package identifier"
        );
        ensure!(
            !self.product_code.is_empty()
                && self.product_code.len() <= 255
                && !self.product_code.chars().any(char::is_control)
                && !self.product_code.contains(['\\', '/'])
                && !self.source_id.is_empty()
                && self.source_id.len() <= 255
                && !self.source_id.chars().any(char::is_control),
            "invalid caller-selected registration/source reference"
        );
        stable_version(self.version)?;
        native_target(self.target)?;
        let location = maintenance_path(self.install_location)?;
        maintenance_path(self.index_path)?;
        maintenance_path(self.console_alias)?;
        maintenance_path(root)?;
        ensure!(
            same_path(
                self.index_path,
                &format!("{location}\\{}.db", self.product_code)
            )?,
            "index filename differs from the selected registration"
        );
        ensure!(
            same_path(
                root,
                &format!("{location}\\locron-v{}-{}", self.version, self.target)
            )?,
            "indexed root differs from selected release/target/location"
        );
        ensure!(
            self.console_alias.rsplit(['\\', '/']).next() == Some("locron.exe")
                && !same_path(self.console_alias, &format!("{root}\\locron.exe"))?,
            "selected console alias is not the separate fixed command"
        );
        Ok(())
    }
}

fn bounded_reader(bytes: &[u8], original_deadline: Instant) -> Result<Connection> {
    before_deadline(original_deadline)?;
    ensure!(
        (100..=SNAPSHOT_LIMIT).contains(&bytes.len()) && &bytes[..16] == b"SQLite format 3\0",
        "invalid or oversized complete SQLite snapshot"
    );
    ensure!(
        bytes[18] == 1 && bytes[19] == 1,
        "WAL snapshot is not self-contained"
    );
    let mut database = Connection::open_in_memory()?;
    ensure!(
        database.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?,
        "index defensive mode was not enabled"
    );
    ensure!(
        !database.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?,
        "index schema remained trusted"
    );
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, ROW_LIMIT),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 16 * 1024),
        (Limit::SQLITE_LIMIT_COLUMN, 8),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
    ] {
        database.set_limit(limit, value)?;
        ensure!(
            database.limit(limit)? == value,
            "index limit was not applied"
        );
    }
    let mut callbacks = 0_u32;
    database.progress_handler(
        PROGRESS_INTERVAL,
        Some(move || {
            callbacks = callbacks.saturating_add(1);
            callbacks >= PROGRESS_CALLBACKS || Instant::now() >= original_deadline
        }),
    )?;
    database.execute_batch("PRAGMA temp_store=MEMORY; PRAGMA query_only=ON")?;
    before_deadline(original_deadline)?;
    // This exact complete slice cannot produce a short read in rusqlite's safe adapter.
    // SQLite never receives the original path/URI or permission to read a sidecar.
    database.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)?;
    before_deadline(original_deadline)?;
    Ok(database)
}

fn text(row: &Row<'_>, column: usize) -> Result<String> {
    let ValueRef::Text(bytes) = row.get_ref(column)? else {
        bail!("index value has unexpected SQLite storage type");
    };
    ensure!(
        bytes.len() <= VALUE_LIMIT && !bytes.contains(&0),
        "index value exceeds its bound or contains NUL"
    );
    Ok(std::str::from_utf8(bytes)?.to_owned())
}

fn ordinary_tables(database: &Connection, deadline: Instant) -> Result<()> {
    before_deadline(deadline)?;
    let mut statement = database
        .prepare("SELECT type,name,tbl_name,rootpage,sql FROM main.sqlite_schema LIMIT 4")?;
    let mut rows = statement.query([])?;
    let mut objects = BTreeMap::new();
    while let Some(row) = rows.next()? {
        before_deadline(deadline)?;
        ensure!(objects.len() < 3, "extra index schema object");
        let kind = text(row, 0)?;
        let name = text(row, 1)?;
        let table = text(row, 2)?;
        ensure!(row.get::<_, i64>(3)? > 0, "invalid schema root page");
        match (kind.as_str(), name.as_str(), table.as_str()) {
            ("table", "metadata", "metadata") | ("table", "portable", "portable") => {
                ensure!(!text(row, 4)?.is_empty(), "missing ordinary table SQL");
            }
            ("index", "sqlite_autoindex_portable_1", "portable") => {
                ensure!(
                    row.get_ref(4)? == ValueRef::Null,
                    "nonautomatic portable index"
                );
            }
            _ => bail!("index contains an unsupported schema object"),
        }
        ensure!(
            objects.insert(name, ()).is_none(),
            "duplicate index schema object"
        );
    }
    ensure!(
        objects.len() == 3,
        "missing ordinary portable schema object"
    );
    before_deadline(deadline)?;
    let mut statement = database.prepare(
        "SELECT name,type,ncol,wr,strict FROM pragma_table_list WHERE schema='main' AND name <> 'sqlite_schema' LIMIT 3",
    )?;
    let mut rows = statement.query([])?;
    let mut tables = BTreeMap::new();
    while let Some(row) = rows.next()? {
        before_deadline(deadline)?;
        let name = text(row, 0)?;
        let kind = text(row, 1)?;
        let shape = (
            kind,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        );
        ensure!(
            tables.insert(name, shape).is_none(),
            "duplicate index table"
        );
    }
    ensure!(
        tables.len() == 2
            && tables.get("metadata") == Some(&("table".into(), 2, 1, 0))
            && tables.get("portable") == Some(&("table".into(), 4, 0, 0)),
        "index does not have the ordinary portable schema 1.0 tables"
    );
    Ok(())
}

fn columns(
    database: &Connection,
    sql: &'static str,
    expected: &[(&str, &str, i64, i64)],
    deadline: Instant,
) -> Result<()> {
    before_deadline(deadline)?;
    let mut statement = database.prepare(sql)?;
    let mut rows = statement.query([])?;
    for (position, &(name, kind, not_null, primary)) in expected.iter().enumerate() {
        before_deadline(deadline)?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("missing index schema column"))?;
        ensure!(
            row.get::<_, i64>(0)? == i64::try_from(position)?
                && text(row, 1)? == name
                && text(row, 2)? == kind
                && row.get::<_, i64>(3)? == not_null
                && row.get_ref(4)? == ValueRef::Null
                && row.get::<_, i64>(5)? == primary
                && row.get::<_, i64>(6)? == 0,
            "index schema column differs from pinned schema 1.0"
        );
    }
    ensure!(rows.next()?.is_none(), "extra index schema column");
    Ok(())
}

fn portable_unique_index(database: &Connection, deadline: Instant) -> Result<()> {
    before_deadline(deadline)?;
    let mut statement = database.prepare("PRAGMA main.index_list('portable')")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("missing portable unique index"))?;
    ensure!(
        row.get::<_, i64>(0)? == 0
            && text(row, 1)? == "sqlite_autoindex_portable_1"
            && row.get::<_, i64>(2)? == 1
            && text(row, 3)? == "u"
            && row.get::<_, i64>(4)? == 0,
        "portable unique index differs from pinned schema"
    );
    ensure!(rows.next()?.is_none(), "extra portable index");
    before_deadline(deadline)?;
    let mut statement =
        database.prepare("PRAGMA main.index_xinfo('sqlite_autoindex_portable_1')")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("missing unique path key"))?;
    ensure!(
        row.get::<_, i64>(0)? == 0
            && row.get::<_, i64>(1)? == 0
            && text(row, 2)? == "filepath"
            && row.get::<_, i64>(3)? == 0
            && text(row, 4)? == "NOCASE"
            && row.get::<_, i64>(5)? == 1,
        "portable path key lost its unique NOCASE binding"
    );
    let row = rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("missing unique-index rowid"))?;
    ensure!(
        row.get::<_, i64>(0)? == 1
            && row.get::<_, i64>(1)? == -1
            && row.get_ref(2)? == ValueRef::Null
            && row.get::<_, i64>(3)? == 0
            && text(row, 4)? == "BINARY"
            && row.get::<_, i64>(5)? == 0,
        "portable unique-index auxiliary entry differs"
    );
    ensure!(rows.next()?.is_none(), "extra portable unique-index entry");
    Ok(())
}

fn metadata(database: &Connection, deadline: Instant) -> Result<()> {
    before_deadline(deadline)?;
    let mut statement = database.prepare("SELECT name,value FROM metadata LIMIT 9")?;
    let mut rows = statement.query([])?;
    let mut values = BTreeMap::new();
    while let Some(row) = rows.next()? {
        before_deadline(deadline)?;
        ensure!(
            values.len() < METADATA_ROWS,
            "index metadata exceeds its row bound"
        );
        ensure!(
            values.insert(text(row, 0)?, text(row, 1)?).is_none(),
            "duplicate index metadata key"
        );
    }
    ensure!(
        values.get("majorVersion").map(String::as_str) == Some("1")
            && values.get("minorVersion").map(String::as_str) == Some("0"),
        "unsupported portable index schema version"
    );
    Ok(())
}

fn verify_index_snapshot(
    bytes: &[u8],
    registration: &Registration<'_>,
    root: &str,
    original_deadline: Instant,
) -> Result<IndexFacts> {
    before_deadline(original_deadline)?;
    registration.validate(root)?;
    let database = bounded_reader(bytes, original_deadline)?;
    ordinary_tables(&database, original_deadline)?;
    columns(
        &database,
        "PRAGMA main.table_xinfo('metadata')",
        &[("name", "TEXT", 1, 1), ("value", "TEXT", 1, 0)],
        original_deadline,
    )?;
    columns(
        &database,
        "PRAGMA main.table_xinfo('portable')",
        &[
            ("filepath", "TEXT", 1, 0),
            ("filetype", "INT64", 1, 0),
            ("sha256", "BLOB", 0, 0),
            ("symlinktarget", "TEXT", 0, 0),
        ],
        original_deadline,
    )?;
    portable_unique_index(&database, original_deadline)?;
    metadata(&database, original_deadline)?;
    before_deadline(original_deadline)?;
    let mut statement =
        database.prepare("SELECT filepath,filetype,sha256,symlinktarget FROM portable LIMIT 3")?;
    let mut rows = statement.query([])?;
    let mut directory = None;
    let mut alias = None;
    let console = format!("{root}\\locron.exe");
    for _ in 0..2 {
        before_deadline(original_deadline)?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("missing portable ownership row"))?;
        let path = text(row, 0)?;
        maintenance_path(&path)?;
        ensure!(
            text(row, 2)?.is_empty(),
            "directory/alias contains an unexpected hash"
        );
        let target = text(row, 3)?;
        match row.get_ref(1)? {
            ValueRef::Integer(2) => {
                ensure!(
                    directory.is_none() && same_path(&path, root)? && target.is_empty(),
                    "duplicate, foreign or stale directory row"
                );
                directory = Some(path);
            }
            ValueRef::Integer(3) => {
                ensure!(
                    alias.is_none()
                        && same_path(&path, registration.console_alias)?
                        && same_path(&target, &console)?,
                    "duplicate, foreign or stale console alias row"
                );
                alias = Some(path);
            }
            _ => bail!("portable row is not the exact Directory/console Symlink type"),
        }
    }
    ensure!(rows.next()?.is_none(), "extra portable ownership row");
    before_deadline(original_deadline)?;
    let snapshot_sha256 = sha256_hex(bytes);
    before_deadline(original_deadline)?;
    Ok(IndexFacts {
        directory: directory.ok_or_else(|| anyhow::anyhow!("missing Directory row"))?,
        console_alias: alias.ok_or_else(|| anyhow::anyhow!("missing console Symlink row"))?,
        console,
        snapshot_sha256,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rusqlite::params;

    use super::*;

    const ROOT: &str = r"C:\Users\Fixture\AppData\Local\Microsoft\WinGet\Packages\WhiteKiwi.locron_source\locron-v0.10.0-x86_64-pc-windows-msvc";
    const LOCATION: &str =
        r"C:\Users\Fixture\AppData\Local\Microsoft\WinGet\Packages\WhiteKiwi.locron_source";
    const INDEX: &str = r"C:\Users\Fixture\AppData\Local\Microsoft\WinGet\Packages\WhiteKiwi.locron_source\WhiteKiwi.locron_source.db";
    const ALIAS: &str = r"C:\Users\Fixture\AppData\Local\Microsoft\WinGet\Links\locron.exe";

    fn registration() -> Registration<'static> {
        Registration {
            product_code: "WhiteKiwi.locron_source",
            package_id: "WhiteKiwi.locron",
            source_id: "Microsoft.Winget.Source_8wekyb3d8bbwe",
            version: "0.10.0",
            target: "x86_64-pc-windows-msvc",
            install_location: LOCATION,
            index_path: INDEX,
            console_alias: ALIAS,
        }
    }

    fn fixture() -> Connection {
        let database = Connection::open_in_memory().unwrap();
        database.execute_batch(
            "CREATE TABLE metadata(name TEXT PRIMARY KEY NOT NULL,value TEXT NOT NULL) WITHOUT ROWID;
             CREATE TABLE portable(filepath TEXT NOT NULL UNIQUE COLLATE NOCASE,filetype INT64 NOT NULL,sha256 BLOB,symlinktarget TEXT);
             INSERT INTO metadata VALUES('majorVersion','1'),('minorVersion','0'),('lastwritetime','1791000000'),('databaseIdentifier','fixture');",
        ).unwrap();
        database
            .execute("INSERT INTO portable VALUES(?1,2,'','')", [ROOT])
            .unwrap();
        database
            .execute(
                "INSERT INTO portable VALUES(?1,3,'',?2)",
                params![ALIAS, format!("{ROOT}\\locron.exe")],
            )
            .unwrap();
        database
    }

    fn bytes(database: &Connection) -> Vec<u8> {
        database.serialize(MAIN_DB).unwrap().to_vec()
    }

    fn verify(database: &Connection) -> Result<IndexFacts> {
        verify_index_snapshot(
            &bytes(database),
            &registration(),
            ROOT,
            Instant::now() + Duration::from_secs(30),
        )
    }

    #[test]
    fn pinned_schema_accepts_directory_and_console_alias_without_claiming_child_hashes() {
        let database = fixture();
        let original = bytes(&database);
        let facts = verify_index_snapshot(
            &original,
            &registration(),
            ROOT,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(facts.directory, ROOT);
        assert_eq!(facts.console_alias, ALIAS);
        assert_eq!(facts.console, format!("{ROOT}\\locron.exe"));
        assert_eq!(facts.snapshot_sha256, sha256_hex(&original));
        assert_eq!(bytes(&database), original);
        // This proof remains accepted if a fictitious installed child is absent;
        // live five-leaf/canonical/image verification must independently refuse it.
    }

    #[test]
    fn individually_bounded_long_paths_share_a_row_above_the_value_cap() {
        let original_deadline = Instant::now() + Duration::from_secs(30);
        let components = vec!["漢".repeat(200); 14].join("\\");
        let location = format!(r"C:\fixture\{components}\packages");
        let root = format!(r"{location}\locron-v0.10.0-x86_64-pc-windows-msvc");
        let index = format!(r"{location}\WhiteKiwi.locron_source.db");
        let alias = format!(r"C:\links\{components}\locron.exe");
        let console = format!(r"{root}\locron.exe");
        assert!(alias.len() + console.len() > VALUE_LIMIT);
        for path in [&location, &root, &index, &alias, &console] {
            assert!(path.len() <= VALUE_LIMIT);
            maintenance_path(path).unwrap();
        }
        let database = fixture();
        database
            .execute("UPDATE portable SET filepath=?1 WHERE filetype=2", [&root])
            .unwrap();
        database
            .execute(
                "UPDATE portable SET filepath=?1,symlinktarget=?2 WHERE filetype=3",
                params![&alias, &console],
            )
            .unwrap();
        let selected = Registration {
            install_location: &location,
            index_path: &index,
            console_alias: &alias,
            ..registration()
        };
        let original = bytes(&database);
        let facts = verify_index_snapshot(&original, &selected, &root, original_deadline).unwrap();
        assert_eq!(facts.directory, root);
        assert_eq!(facts.console_alias, alias);
        assert_eq!(facts.console, console);
        assert_eq!(bytes(&database), original);
    }

    #[test]
    fn wrong_row_storage_types_hashes_and_targets_refuse() {
        for sql in [
            "UPDATE portable SET sha256=X'' WHERE filetype=2",
            "UPDATE portable SET sha256='00' WHERE filetype=2",
            "UPDATE portable SET symlinktarget=NULL WHERE filetype=2",
            "UPDATE portable SET filetype=1 WHERE filetype=2",
            "UPDATE portable SET filetype=2.5 WHERE filetype=2",
            "UPDATE portable SET symlinktarget='C:\\foreign\\locron.exe' WHERE filetype=3",
            "UPDATE portable SET filepath=filepath||'.stale' WHERE filetype=2",
            "UPDATE portable SET filepath=filepath||'.extra' WHERE filetype=3",
        ] {
            let database = fixture();
            database.execute_batch(sql).unwrap();
            assert!(verify(&database).is_err(), "accepted mutation {sql}");
        }
    }

    #[test]
    fn missing_duplicate_extra_and_gui_alias_rows_refuse() {
        for sql in [
            "DELETE FROM portable WHERE filetype=2",
            "UPDATE portable SET filetype=2 WHERE filetype=3",
            "INSERT INTO portable VALUES('C:\\extra',2,'','')",
            "INSERT INTO portable VALUES('C:\\links\\locron-service-launcher.exe',3,'','C:\\launcher.exe')",
        ] {
            let database = fixture();
            database.execute_batch(sql).unwrap();
            assert!(verify(&database).is_err(), "accepted mutation {sql}");
        }
    }

    #[test]
    fn ordinary_table_version_column_and_unique_collation_shapes_are_required() {
        for sql in [
            "UPDATE metadata SET value='2' WHERE name='majorVersion'",
            "UPDATE metadata SET value='1' WHERE name='minorVersion'",
            "DELETE FROM metadata WHERE name='minorVersion'",
            "ALTER TABLE portable ADD COLUMN extra TEXT",
            "CREATE TABLE extra(value TEXT)",
            "CREATE TABLE sqlitexhidden(value TEXT)",
            "CREATE VIEW hidden AS SELECT * FROM portable",
            "CREATE TRIGGER hidden AFTER INSERT ON portable BEGIN SELECT 1; END",
            "CREATE INDEX extra_index ON portable(filetype)",
            "CREATE INDEX extra_metadata_index ON metadata(value)",
            "ALTER TABLE portable RENAME TO old;
             CREATE TABLE portable(filepath TEXT NOT NULL UNIQUE,filetype INT64 NOT NULL,sha256 BLOB,symlinktarget TEXT);
             INSERT INTO portable SELECT * FROM old; DROP TABLE old;",
        ] {
            let database = fixture();
            database.execute_batch(sql).unwrap();
            assert!(verify(&database).is_err(), "accepted mutation {sql}");
        }
    }

    #[test]
    fn registration_version_source_product_code_and_index_path_are_not_guessed() {
        let original = bytes(&fixture());
        for changed in [
            Registration {
                package_id: "foreign.locron",
                ..registration()
            },
            Registration {
                source_id: "",
                ..registration()
            },
            Registration {
                product_code: "../foreign",
                ..registration()
            },
            Registration {
                index_path: r"C:\foreign.db",
                ..registration()
            },
            Registration {
                version: "0.10.1",
                ..registration()
            },
            Registration {
                target: "aarch64-pc-windows-msvc",
                ..registration()
            },
            Registration {
                console_alias: r"C:\links\locron-service-launcher.exe",
                ..registration()
            },
        ] {
            assert!(
                verify_index_snapshot(
                    &original,
                    &changed,
                    ROOT,
                    Instant::now() + Duration::from_secs(30)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn snapshot_wal_size_metadata_and_value_caps_refuse_without_repair() {
        let original = bytes(&fixture());
        for position in [18, 19] {
            let mut wal = original.clone();
            wal[position] = 2;
            assert!(
                verify_index_snapshot(
                    &wal,
                    &registration(),
                    ROOT,
                    Instant::now() + Duration::from_secs(30)
                )
                .is_err()
            );
            assert_eq!(wal[position], 2);
        }
        assert!(bounded_reader(&original[..99], Instant::now() + Duration::from_secs(30)).is_err());
        assert!(
            bounded_reader(
                &vec![0; SNAPSHOT_LIMIT + 1],
                Instant::now() + Duration::from_secs(30)
            )
            .is_err()
        );
        let database = fixture();
        for number in 0..5 {
            database
                .execute(
                    "INSERT INTO metadata VALUES(?1,'bounded')",
                    [format!("extra{number}")],
                )
                .unwrap();
        }
        assert!(verify(&database).is_err());
        let database = fixture();
        database
            .execute(
                "UPDATE portable SET sha256=?1 WHERE filetype=3",
                ["x".repeat(VALUE_LIMIT + 1)],
            )
            .unwrap();
        assert!(verify(&database).is_err());
        let database = fixture();
        database
            .execute(
                "UPDATE metadata SET value=?1 WHERE name='databaseIdentifier'",
                ["x".repeat(VALUE_LIMIT + 1)],
            )
            .unwrap();
        assert!(verify(&database).is_err());
        let database = fixture();
        database
            .execute(
                "UPDATE metadata SET value=?1 WHERE name='databaseIdentifier'",
                ["before\0after"],
            )
            .unwrap();
        assert!(verify(&database).is_err());
    }

    #[test]
    fn deserialized_snapshot_is_readonly_and_uses_no_attached_database() {
        let original = bytes(&fixture());
        let database = bounded_reader(&original, Instant::now() + Duration::from_secs(30)).unwrap();
        assert!(database.execute("DELETE FROM portable", []).is_err());
        assert!(
            database
                .execute_batch("ATTACH ':memory:' AS foreign")
                .is_err()
        );
        assert_eq!(database.limit(Limit::SQLITE_LIMIT_ATTACHED).unwrap(), 0);
        assert_eq!(
            database.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
            ROW_LIMIT
        );
        assert!(
            database
                .db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
                .unwrap()
        );
        assert!(
            !database
                .db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA)
                .unwrap()
        );
    }

    #[test]
    fn original_expiry_and_finite_progress_work_refuse() {
        let original = bytes(&fixture());
        assert!(verify_index_snapshot(&original, &registration(), ROOT, Instant::now()).is_err());
        let database = bounded_reader(&original, Instant::now() + Duration::from_secs(30)).unwrap();
        let error = database.query_row::<i64, _, _>(
            "WITH RECURSIVE counter(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM counter WHERE x<10000000) SELECT max(x) FROM counter",
            [], |row| row.get(0),
        ).unwrap_err();
        assert!(
            matches!(error, rusqlite::Error::SqliteFailure(failure, _) if failure.code == rusqlite::ErrorCode::OperationInterrupted)
        );
    }
}
