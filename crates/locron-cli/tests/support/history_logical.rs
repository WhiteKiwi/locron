//! Private safe read-only oracle shared by two integration-test binaries.
//! Logical values and physical coordination records are deliberately separate.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use locron_core::filesystem::{DirectoryGuard, GuardedFile, open_private};
use locron_store::{StatePaths, Store};
use rusqlite::{Connection, OpenFlags, types::ValueRef};

pub fn checked<T, E>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|_| panic!("private qualification operation refused"))
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(u64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

impl Cell {
    pub fn text(value: &str) -> Self {
        Self::Text(value.as_bytes().to_vec())
    }
    pub fn integer(&self) -> i64 {
        match self {
            Self::Integer(value) => *value,
            _ => panic!("expected integer fact"),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    layout: Vec<Vec<Cell>>,
}

impl Table {
    pub fn column(&self, name: &str) -> usize {
        self.columns
            .iter()
            .position(|column| column == name)
            .expect("owned column exists")
    }
    pub fn single(&self, key: &str, value: &Cell) -> &Vec<Cell> {
        let column = self.column(key);
        let mut rows = self.rows.iter().filter(|row| &row[column] == value);
        let row = rows.next().expect("owned row exists");
        assert!(rows.next().is_none(), "owned identity was ambiguous");
        row
    }
    pub fn set(&mut self, key: &str, value: &Cell, field: &str, replacement: Cell) {
        let key_index = self.column(key);
        let field_index = self.column(field);
        let matching: Vec<_> = self
            .rows
            .iter_mut()
            .filter(|row| &row[key_index] == value)
            .collect();
        assert_eq!(matching.len(), 1, "owned update row missing or ambiguous");
        matching.into_iter().next().unwrap()[field_index] = replacement;
        self.rows.sort();
    }
    pub fn remove(&mut self, key: &str, value: &Cell) {
        let column = self.column(key);
        self.rows.retain(|row| &row[column] != value);
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Logical {
    schema: Vec<Vec<Cell>>,
    application_id: i64,
    user_version: i64,
    pub tables: BTreeMap<String, Table>,
}

const TABLES: &[(&str, &str)] = &[
    (
        "schema_migrations",
        "version name checksum binary_version applied_at_us",
    ),
    (
        "settings",
        "singleton global_concurrency execution_path run_retention_count run_retention_age_us output_limit_bytes per_run_output_limit_bytes updated_at_us environment_json",
    ),
    (
        "jobs",
        "id name description tags_json enabled created_at_us updated_at_us removed_at_us current_revision",
    ),
    (
        "job_revisions",
        "job_id revision definition_json created_at_us created_by",
    ),
    (
        "schedule_cursors",
        "job_id revision cursor_us interval_anchor_us one_time_resolved updated_at_us disabled_since_us",
    ),
    (
        "scheduler_lifetimes",
        "id pid binary_version started_at_us heartbeat_at_us ended_at_us exit_class",
    ),
    (
        "admission_state",
        "singleton last_admitted_job_id next_queue_sequence",
    ),
    (
        "runs",
        "id job_id revision trigger nominal_us requested_at_us eligible_at_us queue_sequence snapshot_json state reason catch_up_batch catch_up_position replacement_candidate cancellation_requested_at_us cancellation_reason finished_at_us",
    ),
    (
        "attempts",
        "run_id attempt_number lifetime_id state started_at_us running_at_us finished_at_us duration_us resolved_executable process_id process_group_id exit_code http_status result_class error_message http_content_type",
    ),
    (
        "retry_intents",
        "run_id prior_attempt_number not_before_us classification created_at_us",
    ),
    (
        "events",
        "id occurred_at_us kind job_id run_id details_json",
    ),
    (
        "output_artifacts",
        "run_id attempt_number relative_path state retained_payload_bytes physical_bytes discarded_bytes truncated truncated_at_us finalized_at_us prune_started_at_us pruned_at_us",
    ),
    ("run_retention_pending", "run_id selected_at_us"),
    ("sqlite_sequence", "name seq"),
];
const INDEXES: &[&str] = &[
    "jobs_live_name",
    "jobs_enabled_updated",
    "lifetimes_open",
    "runs_scheduled_occurrence",
    "runs_replacement_candidate",
    "runs_admission",
    "runs_history",
    "runs_retention",
    "attempts_active",
    "retries_due",
    "events_job",
    "events_run",
    "output_retention",
    "run_retention_pending_selected",
    "sqlite_autoindex_jobs_1",
    "sqlite_autoindex_job_revisions_1",
    "sqlite_autoindex_schedule_cursors_1",
    "sqlite_autoindex_scheduler_lifetimes_1",
    "sqlite_autoindex_runs_1",
    "sqlite_autoindex_runs_2",
    "sqlite_autoindex_attempts_1",
    "sqlite_autoindex_retry_intents_1",
    "sqlite_autoindex_output_artifacts_1",
    "sqlite_autoindex_output_artifacts_2",
    "sqlite_autoindex_run_retention_pending_1",
];

pub fn identity(file: &GuardedFile) -> (u64, u128) {
    #[cfg(windows)]
    {
        let actual = checked(locron_core::filesystem::file_identity(file));
        assert!(actual.file_id != 0, "empty full file identity");
        (actual.volume_serial_number, actual.file_id)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = checked(file.metadata());
        assert!(metadata.ino() != 0, "empty file identity");
        (metadata.dev(), u128::from(metadata.ino()))
    }
}

fn rows(connection: &Connection, sql: &str, budget: &mut usize) -> Vec<Vec<Cell>> {
    let mut statement = checked(connection.prepare(sql));
    let width = statement.column_count();
    let mut source = checked(statement.query([]));
    let mut result = Vec::new();
    while let Some(row) = checked(source.next()) {
        assert!(result.len() < 16384, "logical row cap exceeded");
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            let cell = match checked(row.get_ref(index)) {
                ValueRef::Null => Cell::Null,
                ValueRef::Integer(value) => Cell::Integer(value),
                ValueRef::Real(value) => Cell::Real(value.to_bits()),
                ValueRef::Text(value) => {
                    assert!(value.len() <= 65536, "logical value cap exceeded");
                    *budget = budget
                        .checked_sub(value.len())
                        .expect("logical byte cap exceeded");
                    Cell::Text(value.to_vec())
                }
                ValueRef::Blob(value) => {
                    assert!(value.len() <= 65536, "logical value cap exceeded");
                    *budget = budget
                        .checked_sub(value.len())
                        .expect("logical byte cap exceeded");
                    Cell::Blob(value.to_vec())
                }
            };
            cells.push(cell);
        }
        result.push(cells);
    }
    result.sort();
    result
}

#[cfg(windows)]
fn sqlite_sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}

#[cfg(windows)]
fn optional_sqlite_leaf(path: &Path) -> Option<GuardedFile> {
    checked(
        open_private(path, OpenOptions::new().read(true))
            .map(Some)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(error)
                }
            }),
    )
}

#[cfg(windows)]
fn sqlite_pair(database: &Path) -> Option<(GuardedFile, GuardedFile)> {
    assert!(
        optional_sqlite_leaf(&sqlite_sidecar(database, "-journal")).is_none(),
        "oracle found a rollback journal"
    );
    let wal = optional_sqlite_leaf(&sqlite_sidecar(database, "-wal"));
    let shm = optional_sqlite_leaf(&sqlite_sidecar(database, "-shm"));
    match (wal, shm) {
        (Some(wal), Some(shm)) => Some((wal, shm)),
        (None, None) => None,
        _ => panic!("oracle found a partial coordination pair"),
    }
}

#[cfg(windows)]
fn immutable_sqlite_uri(path: &Path) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let path = path
        .to_str()
        .filter(|path| !path.contains('\0'))
        .expect("oracle path was not UTF-8 without NUL");
    let mut uri = String::from("file:");
    for byte in path.bytes() {
        uri.push('%');
        uri.push(char::from(HEX[usize::from(byte >> 4)]));
        uri.push(char::from(HEX[usize::from(byte & 15)]));
    }
    uri.push_str("?mode=ro&immutable=1");
    uri
}

pub fn logical(paths: &StatePaths) -> Logical {
    logical_boundary(paths, None)
}

pub fn logical_with_writer(paths: &StatePaths, writer: &Store) -> Logical {
    logical_boundary(paths, Some(writer))
}

fn logical_boundary(paths: &StatePaths, writer: Option<&Store>) -> Logical {
    if let Some(writer) = writer {
        assert!(
            writer.paths().database == paths.database,
            "oracle writer named another database"
        );
    }
    let parent = checked(DirectoryGuard::existing_private(&paths.root));
    let guard = checked(open_private(&paths.database, OpenOptions::new().read(true)));
    let original_id = identity(&guard);
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    #[cfg(windows)]
    let stable = if writer.is_none() {
        let stable = checked(locron_core::filesystem::open_private_read_stable(
            guard.normalized_path(),
        ));
        assert!(
            identity(&stable) == original_id,
            "oracle stable gate selected another database"
        );
        Some(stable)
    } else {
        None
    };
    #[cfg(windows)]
    let coordination = sqlite_pair(guard.normalized_path());
    #[cfg(windows)]
    assert!(
        writer.is_none() || coordination.is_some(),
        "live oracle required an existing coordination pair"
    );
    #[cfg(windows)]
    let coordination_id = coordination
        .as_ref()
        .map(|(wal, shm)| (identity(wal), identity(shm)));
    #[cfg(windows)]
    let mut connection = if coordination.is_some() {
        checked(Connection::open_with_flags_and_vfs(
            guard.normalized_path(),
            flags,
            "win32-longpath",
        ))
    } else {
        assert!(
            stable.is_some() && sqlite_pair(guard.normalized_path()).is_none(),
            "closed oracle coordination absence changed"
        );
        checked(Connection::open_with_flags_and_vfs(
            immutable_sqlite_uri(guard.normalized_path()),
            flags | OpenFlags::SQLITE_OPEN_URI,
            "win32-longpath",
        ))
    };
    #[cfg(unix)]
    let mut connection = checked(Connection::open_with_flags(guard.normalized_path(), flags));
    assert!(
        checked(connection.is_readonly(rusqlite::MAIN_DB)),
        "oracle was not read-only"
    );
    checked(connection.busy_timeout(Duration::from_secs(5)));
    checked(connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;"));
    let reported = connection.path().expect("SQLite reported a file");
    let reported_guard = checked(open_private(
        Path::new(reported),
        OpenOptions::new().read(true),
    ));
    assert!(
        identity(&reported_guard) == original_id,
        "SQLite selected another file identity"
    );
    let transaction = checked(connection.transaction());
    assert_eq!(
        checked(transaction.pragma_query_value(None, "query_only", |row| row.get::<_, i64>(0))),
        1
    );
    let application_id =
        checked(transaction.pragma_query_value(None, "application_id", |row| row.get(0)));
    let user_version =
        checked(transaction.pragma_query_value(None, "user_version", |row| row.get(0)));
    let mut budget = 32 * 1024 * 1024;
    let schema = rows(
        &transaction,
        "SELECT type,name,tbl_name,sql FROM sqlite_schema",
        &mut budget,
    );
    let expected_tables: BTreeSet<_> = TABLES.iter().map(|(name, _)| Cell::text(name)).collect();
    let observed_tables: BTreeSet<_> = schema
        .iter()
        .filter(|row| row[0] == Cell::text("table"))
        .map(|row| row[1].clone())
        .collect();
    assert!(
        observed_tables == expected_tables,
        "unknown or missing logical table"
    );
    for row in &schema {
        assert!(
            row[0] == Cell::text("table")
                || (row[0] == Cell::text("index")
                    && INDEXES.iter().any(|name| row[1] == Cell::text(name))),
            "unknown logical schema object"
        );
    }
    let mut tables = BTreeMap::new();
    for (name, expected_columns) in TABLES {
        let sql = format!("SELECT * FROM \"{name}\"");
        let columns = {
            let statement = checked(transaction.prepare(&sql));
            statement
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert!(
            columns
                .iter()
                .map(String::as_str)
                .eq(expected_columns.split_whitespace()),
            "logical column layout changed"
        );
        let layout = rows(
            &transaction,
            &format!("PRAGMA table_xinfo(\"{name}\")"),
            &mut budget,
        );
        let contents = rows(&transaction, &sql, &mut budget);
        tables.insert(
            (*name).to_owned(),
            Table {
                columns,
                rows: contents,
                layout,
            },
        );
    }
    checked(transaction.commit());
    assert!(
        identity(&guard) == original_id && identity(&reported_guard) == original_id,
        "oracle file identity changed"
    );
    drop(connection);
    #[cfg(windows)]
    {
        let readback = sqlite_pair(guard.normalized_path());
        assert!(
            readback
                .as_ref()
                .map(|(wal, shm)| (identity(wal), identity(shm)))
                == coordination_id,
            "oracle coordination identity or absence changed"
        );
        assert!(
            identity(&guard) == original_id
                && stable
                    .as_ref()
                    .is_none_or(|stable| identity(stable) == original_id),
            "oracle retained database identity changed"
        );
        drop(readback);
    }
    drop(reported_guard);
    #[cfg(windows)]
    {
        drop(coordination);
        drop(stable);
    }
    drop(guard);
    drop(parent);
    Logical {
        schema,
        application_id,
        user_version,
        tables,
    }
}

#[derive(Eq, PartialEq)]
pub struct PhysicalFile {
    identity: (u64, u128),
    bytes: Vec<u8>,
}

pub struct Physical {
    stable: BTreeMap<PathBuf, PhysicalFile>,
    pub coordination: BTreeMap<PathBuf, PhysicalFile>,
}

pub fn physical(paths: &StatePaths) -> Physical {
    fn collect(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, PhysicalFile>) {
        let parent = checked(DirectoryGuard::existing_private(directory));
        for entry in checked(std::fs::read_dir(parent.normalized_path())) {
            let path = checked(entry).path();
            let metadata = checked(std::fs::symlink_metadata(&path));
            assert!(
                !metadata.file_type().is_symlink(),
                "physical oracle encountered a link"
            );
            if metadata.is_dir() {
                collect(root, &path, result);
            } else {
                assert!(
                    metadata.is_file(),
                    "physical oracle encountered another object"
                );
                assert!(result.len() < 512, "physical file cap exceeded");
                let mut guard = checked(open_private(&path, OpenOptions::new().read(true)));
                let file_id = identity(&guard);
                let mut bytes = Vec::new();
                checked(
                    (&mut *guard)
                        .take(16 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes),
                );
                assert!(
                    bytes.len() <= 16 * 1024 * 1024,
                    "physical byte cap exceeded"
                );
                assert!(
                    identity(&guard) == file_id,
                    "physical identity changed during read"
                );
                result.insert(
                    checked(path.strip_prefix(root)).to_owned(),
                    PhysicalFile {
                        identity: file_id,
                        bytes,
                    },
                );
            }
        }
    }
    let parent = checked(DirectoryGuard::existing_private(&paths.root));
    let mut stable = BTreeMap::new();
    collect(
        parent.normalized_path(),
        parent.normalized_path(),
        &mut stable,
    );
    let mut coordination = BTreeMap::new();
    for name in ["state.db-wal", "state.db-shm", "state.db-journal"] {
        if let Some(file) = stable.remove(Path::new(name)) {
            coordination.insert(PathBuf::from(name), file);
        }
    }
    assert!(
        stable.contains_key(Path::new("state.db")),
        "physical database was absent"
    );
    Physical {
        stable,
        coordination,
    }
}

pub fn unchanged_physical(before: &Physical, immediately_after: &Physical) {
    assert!(
        before.stable == immediately_after.stable,
        "preview changed database or foreign/output bytes or identity"
    );
    // Each coordination record was independently admitted private/no-follow and read at full identity.
    // Its actual presence, identity and bytes remain separate observations, never a permission to mutate SQL.
    eprintln!(
        "history_coordination/v1 changed={}",
        before.coordination != immediately_after.coordination
    );
}
