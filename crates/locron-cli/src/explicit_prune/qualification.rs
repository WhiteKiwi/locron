//! Instance-local qualification of the real explicit-prune caller.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
#[cfg(unix)]
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use locron_core::filesystem::{DirectoryGuard, GuardedFile};
#[cfg(windows)]
use locron_store::RetentionCandidate;
use locron_store::{StatePaths, Store};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
enum Case {
    Sel02,
    Sel05,
    Sel06,
    Sel10,
    Sel12,
    El07,
    El10,
    Id03,
    Id11,
    Id13,
    Id16,
    Id19,
    Id21,
    By07,
    Fs02,
    Fs07,
    Fs08,
    #[cfg(unix)]
    Fs12,
    #[cfg(unix)]
    Fs14,
    Fi01,
    Fi03,
    #[cfg(unix)]
    Fi04,
    Fi05,
    Fi07,
    Cli01,
    Cli04,
    Cli05,
    Cli09,
    #[cfg(windows)]
    N07,
    #[cfg(windows)]
    N08,
    Sel01,
    Sel03,
    Sel04,
    Id04,
    Id15,
    Id18,
    Id22,
    Fs03,
    Fs04,
    #[cfg(windows)]
    N05,
    #[cfg(windows)]
    N06,
    Sel16,
    By01,
    By04,
    Id02,
    By02,
}
impl Case {
    fn id(self) -> &'static str {
        match self {
            Self::Sel02 => "SEL02",
            Self::Sel05 => "SEL05",
            Self::Sel06 => "SEL06",
            Self::Sel10 => "SEL10",
            Self::Sel12 => "SEL12",
            Self::El07 => "EL07",
            Self::El10 => "EL10",
            Self::Id03 => "ID03",
            Self::Id11 => "ID11",
            Self::Id13 => "ID13",
            Self::Id16 => "ID16",
            Self::Id19 => "ID19",
            Self::Id21 => "ID21",
            Self::By07 => "BY07",
            Self::Fs02 => "FS02",
            Self::Fs07 => "FS07",
            Self::Fs08 => "FS08",
            #[cfg(unix)]
            Self::Fs12 => "FS12",
            #[cfg(unix)]
            Self::Fs14 => "FS14",
            Self::Fi01 => "FI01",
            Self::Fi03 => "FI03",
            #[cfg(unix)]
            Self::Fi04 => "FI04",
            Self::Fi05 => "FI05",
            Self::Fi07 => "FI07",
            Self::Cli01 => "CLI01",
            Self::Cli04 => "CLI04",
            Self::Cli05 => "CLI05",
            Self::Cli09 => "CLI09",
            #[cfg(windows)]
            Self::N07 => "N07",
            #[cfg(windows)]
            Self::N08 => "N08",
            Self::Sel01 => "SEL01",
            Self::Sel03 => "SEL03",
            Self::Sel04 => "SEL04",
            Self::Id04 => "ID04",
            Self::Id15 => "ID15",
            Self::Id18 => "ID18",
            Self::Id22 => "ID22",
            Self::Fs03 => "FS03",
            Self::Fs04 => "FS04",
            #[cfg(windows)]
            Self::N05 => "N05",
            #[cfg(windows)]
            Self::N06 => "N06",
            Self::Sel16 => "SEL16",
            Self::By01 => "BY01",
            Self::By04 => "BY04",
            Self::Id02 => "ID02",
            Self::By02 => "BY02",
        }
    }
}

const LIFETIME: &str = "00000000-0000-4000-8000-000000000001";
const JOB: &str = "00000000-0000-4000-8000-000000000002";
const FIXED_NOW: i64 = 5_184_000_000_000;
const CAP: usize = 65_536;
const TABLES: [&str; 14] = [
    "admission_state",
    "attempts",
    "events",
    "job_revisions",
    "jobs",
    "output_artifacts",
    "retry_intents",
    "run_retention_pending",
    "runs",
    "schedule_cursors",
    "scheduler_lifetimes",
    "schema_migrations",
    "settings",
    "sqlite_sequence",
];
const LEDGER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/planning/GROUP_B_PRUNE_QUALIFICATION_ROWS_2026-10-05.json"
));

fn need<T, E>(result: Result<T, E>, phase: &'static str) -> T {
    if let Ok(value) = result {
        value
    } else {
        panic!("prune qualification refused phase={phase}")
    }
}

#[track_caller]
fn check(deadline: Instant) {
    assert!(
        Instant::now() < deadline,
        "prune qualification original deadline elapsed"
    );
}

fn checked<T>(deadline: Instant, operation: impl FnOnce() -> T) -> T {
    check(deadline);
    let result = operation();
    check(deadline);
    result
}

fn path_text(path: &Path) -> String {
    let value = path
        .to_str()
        .expect("prune qualification path must be UTF8");
    assert!(
        path.is_absolute()
            && !value.is_empty()
            && value.len() <= 2048
            && value.encode_utf16().count() <= 2048
            && !value.chars().any(char::is_control),
        "prune qualification path admission refused"
    );
    value.to_owned()
}

fn hex(value: &str, count: usize) -> bool {
    value.len() == count
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "lowercase", deny_unknown_fields)]
enum Identity {
    Unix {
        dev: u64,
        ino: u64,
    },
    Windows {
        volume_serial_number: u64,
        file_id: String,
    },
}

fn file_id(file: &GuardedFile, deadline: Instant) -> Identity {
    check(deadline);
    let identity = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = need(file.metadata(), "file-metadata");
            Identity::Unix {
                dev: metadata.dev(),
                ino: metadata.ino(),
            }
        }
        #[cfg(windows)]
        {
            let identity = need(locron_core::filesystem::file_identity(file), "full-file-id");
            Identity::Windows {
                volume_serial_number: identity.volume_serial_number,
                file_id: format!("{:032x}", identity.file_id),
            }
        }
    };
    check(deadline);
    identity
}

fn root_id(path: &Path, deadline: Instant) -> Identity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let file = need(checked(deadline, || File::open(path)), "root-handle");
        let metadata = need(checked(deadline, || file.metadata()), "root-metadata");
        Identity::Unix {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
    #[cfg(windows)]
    {
        let plan = need(
            locron_core::filesystem::PrivateDirectoryPlan::inspect_until(path, deadline),
            "private-root-id",
        );
        let identity = plan
            .root_identity()
            .expect("existing private root required");
        Identity::Windows {
            volume_serial_number: identity.volume_serial_number,
            file_id: format!("{:032x}", identity.file_id),
        }
    }
}

fn open_guarded(path: &Path, deadline: Instant) -> GuardedFile {
    need(
        checked(deadline, || {
            locron_core::filesystem::open_read_no_follow(path)
        }),
        "guarded-open",
    )
}

fn read_guarded(file: &GuardedFile, cap: usize, deadline: Instant) -> Vec<u8> {
    let mut reader = need(checked(deadline, || file.try_clone()), "capture-clone");
    need(
        checked(deadline, || reader.seek(SeekFrom::Start(0))),
        "capture-seek",
    );
    let mut data = Vec::new();
    need(
        checked(deadline, || {
            reader
                .take(u64::try_from(cap + 1).unwrap())
                .read_to_end(&mut data)
        }),
        "bounded-read",
    );
    assert!(data.len() <= cap, "prune qualification capture cap refusal");
    data
}

fn write_private(path: &Path, bytes: &[u8], deadline: Instant) -> GuardedFile {
    let mut file = need(
        checked(deadline, || {
            locron_core::filesystem::create_private_new(path)
        }),
        "private-create",
    );
    need(checked(deadline, || file.write_all(bytes)), "private-write");
    need(checked(deadline, || file.flush()), "private-flush");
    file
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Cell {
    Null,
    Integer(i64),
    Real(u64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Oracle {
    tables: BTreeMap<String, Table>,
    schema: Table,
    application: i64,
    version: i64,
}

fn query(conn: &Connection, sql: &str, deadline: Instant) -> Table {
    use rusqlite::types::ValueRef;
    let mut statement = need(checked(deadline, || conn.prepare(sql)), "oracle-prepare");
    let columns = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let mut cursor = need(checked(deadline, || statement.query([])), "oracle-query");
    let mut rows = Vec::new();
    let mut bytes = 0_usize;
    while let Some(row) = need(checked(deadline, || cursor.next()), "oracle-row") {
        assert!(rows.len() < 512, "oracle finite row cap");
        let mut cells = Vec::new();
        for index in 0..columns.len() {
            let cell = match need(row.get_ref(index), "oracle-cell") {
                ValueRef::Null => Cell::Null,
                ValueRef::Integer(value) => Cell::Integer(value),
                ValueRef::Real(value) => Cell::Real(value.to_bits()),
                ValueRef::Text(value) => {
                    bytes += value.len();
                    Cell::Text(value.to_vec())
                }
                ValueRef::Blob(value) => {
                    bytes += value.len();
                    Cell::Blob(value.to_vec())
                }
            };
            assert!(bytes <= 2 * 1024 * 1024, "oracle finite byte cap");
            cells.push(cell);
        }
        rows.push(cells);
    }
    rows.sort();
    Table { columns, rows }
}

fn oracle(conn: &Connection, deadline: Instant) -> Oracle {
    let names = query(
        conn,
        "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
        deadline,
    );
    let expected = TABLES
        .iter()
        .map(|name| vec![Cell::Text(name.as_bytes().to_vec())])
        .collect::<Vec<_>>();
    let schema_known = names.rows == expected;
    assert!(schema_known, "unknown schema refused");
    let mut tables = BTreeMap::new();
    for name in TABLES {
        tables.insert(
            name.to_owned(),
            query(conn, &format!("SELECT * FROM {name}"), deadline),
        );
    }
    Oracle {
        tables,
        schema: query(
            conn,
            "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_schema ORDER BY type,name",
            deadline,
        ),
        application: need(
            checked(deadline, || {
                conn.query_row("PRAGMA application_id", [], |row| row.get(0))
            }),
            "application-id",
        ),
        version: need(
            checked(deadline, || {
                conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            }),
            "user-version",
        ),
    }
}

#[derive(Clone, Copy)]
enum Change {
    Pending,
    Pruned,
    Recovery,
}

fn expect_oracle(mut before: Oracle, after: &Oracle, changes: &[(String, i64, Change)]) {
    let table = before.tables.get_mut("output_artifacts").unwrap();
    let actual = &after.tables["output_artifacts"];
    for (run, attempt, change) in changes {
        let index = |name: &str| {
            table
                .columns
                .iter()
                .position(|column| column == name)
                .unwrap()
        };
        let state = index("state");
        let payload = index("retained_payload_bytes");
        let physical = index("physical_bytes");
        let started = index("prune_started_at_us");
        let pruned = index("pruned_at_us");
        let key = |row: &&Vec<Cell>| {
            row[0] == Cell::Text(run.as_bytes().to_vec()) && row[1] == Cell::Integer(*attempt)
        };
        let observed = actual
            .rows
            .iter()
            .find(key)
            .expect("actual output identity disappeared");
        let row = table
            .rows
            .iter_mut()
            .find(|row| {
                row[0] == Cell::Text(run.as_bytes().to_vec()) && row[1] == Cell::Integer(*attempt)
            })
            .unwrap();
        if !matches!(change, Change::Recovery) {
            assert!(
                matches!(observed[started], Cell::Integer(value) if value > 0),
                "actual pending timestamp missing"
            );
            row[started] = observed[started].clone();
        }
        row[state] = Cell::Text(if matches!(change, Change::Pending) {
            b"prune_pending".to_vec()
        } else {
            b"pruned".to_vec()
        });
        if !matches!(change, Change::Pending) {
            row[payload] = Cell::Integer(0);
            row[physical] = Cell::Integer(0);
            assert!(
                matches!(observed[pruned], Cell::Integer(value) if value > 0),
                "actual completion timestamp missing"
            );
            if matches!(change, Change::Recovery) {
                assert!(
                    observed[pruned] == Cell::Integer(FIXED_NOW),
                    "recovery fixed timestamp changed"
                );
            }
            row[pruned] = observed[pruned].clone();
        }
    }
    table.rows.sort();
    let protected = before == *after;
    assert!(
        protected,
        "complete typed durable oracle changed outside selected output fields"
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileFact {
    identity: Identity,
    bytes: Vec<u8>,
    readonly: bool,
    #[cfg(unix)]
    mode: u32,
}

fn fact(path: &Path, deadline: Instant) -> FileFact {
    let reader = open_guarded(path, deadline);
    let identity = checked(deadline, || file_id(&reader, deadline));
    let bytes = read_guarded(&reader, CAP, deadline);
    let metadata = need(checked(deadline, || reader.metadata()), "fact-metadata");
    FileFact {
        identity,
        bytes,
        readonly: metadata.permissions().readonly(),
        #[cfg(unix)]
        mode: {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        },
    }
}

struct Owner {
    container: Option<tempfile::TempDir>,
    paths: StatePaths,
    guards: Vec<DirectoryGuard>,
    files: Vec<GuardedFile>,
    connection: Option<Connection>,
    store: Option<Store>,
    captures: PathBuf,
    database_slot: Option<usize>,
    #[cfg(unix)]
    children: Vec<Child>,
    #[cfg(windows)]
    plan: Option<locron_core::filesystem::PrivateDirectoryPlan>,
    #[cfg(windows)]
    stock: Option<locron_core::windows::StockAdapterGuard>,
    done: bool,
}

impl Owner {
    fn new(deadline: Instant) -> Self {
        check(deadline);
        let mut created = tempfile::tempdir();
        if let Ok(container) = &mut created {
            container.disable_cleanup(true);
        }
        check(deadline);
        let container = need(created, "owned-container-create");
        #[cfg(windows)]
        let plan = need(
            locron_core::filesystem::PrivateDirectoryPlan::inspect_until(
                &container.path().join("state"),
                deadline,
            ),
            "absent-state-admission",
        );
        #[cfg(windows)]
        let paths = {
            assert!(
                plan.root_identity().is_none() && plan.missing_components() == ["state"],
                "fixed child absence required"
            );
            StatePaths::new(plan.normalized_path().to_owned())
        };
        let container_guard = need(
            checked(deadline, || DirectoryGuard::ancestors(container.path())),
            "owned-container-guard",
        );
        #[cfg(unix)]
        let paths = StatePaths::new(container_guard.normalized_path().join("state"));
        assert!(
            matches!(checked(deadline, || fs::symlink_metadata(&paths.root)), Err(error) if error.kind() == io::ErrorKind::NotFound),
            "state child is not absent"
        );
        let captures = container_guard.normalized_path().join("captures");
        Self {
            container: Some(container),
            paths,
            captures,
            guards: vec![container_guard],
            files: Vec::new(),
            database_slot: None,
            connection: None,
            store: None,
            #[cfg(unix)]
            children: Vec::new(),
            #[cfg(windows)]
            plan: Some(plan),
            #[cfg(windows)]
            stock: None,
            done: false,
        }
    }

    fn initialize(&mut self, deadline: Instant) {
        assert!(self.store.is_none(), "fixture initialized twice");
        check(deadline);
        let created = Store::open(self.paths.clone(), env!("CARGO_PKG_VERSION"), FIXED_NOW);
        let created = created.map(|store| self.store = Some(store));
        check(deadline);
        need(created, "actual-store-creator");
        let store = self.store.as_ref().unwrap();
        let guard = need(
            checked(deadline, || {
                DirectoryGuard::existing_private(&self.paths.root)
            }),
            "created-private-root",
        );
        let same_root = guard.normalized_path() == self.paths.root;
        assert!(
            same_root && store.paths() == &self.paths,
            "actual creator root spelling changed"
        );
        #[cfg(windows)]
        {
            let created = need(
                locron_core::filesystem::PrivateDirectoryPlan::inspect_until(
                    &self.paths.root,
                    deadline,
                ),
                "created-root-id",
            );
            let retained_prefix = self
                .plan
                .as_ref()
                .unwrap()
                .existing_guard()
                .normalized_path()
                == self.guards[0].normalized_path();
            assert!(
                retained_prefix
                    && created.root_identity().is_some()
                    && created.existing_identity()
                        != self.plan.as_ref().unwrap().existing_identity(),
                "creator/retained-prefix identity refused"
            );
        }
        self.guards.push(guard);
        let database = open_guarded(&self.paths.database, deadline);
        let normalized = database.normalized_path().to_owned();
        let db_id = file_id(&database, deadline);
        self.database_slot = Some(self.files.len());
        self.files.push(database);
        check(deadline);
        #[cfg(windows)]
        let raw = Connection::open_with_flags_and_vfs(
            &normalized,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            "win32-longpath",
        );
        #[cfg(unix)]
        let raw = Connection::open_with_flags(
            &normalized,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        );
        let raw = raw.map(|connection| self.connection = Some(connection));
        check(deadline);
        check(deadline);
        need(raw, "live-writer-raw-no-create");
        let conn = self.connection.as_ref().unwrap();
        need(
            checked(deadline, || conn.busy_timeout(Duration::from_secs(5))),
            "original-sqlite-busy-budget",
        );
        need(
            checked(deadline, || conn.pragma_update(None, "foreign_keys", true)),
            "fixture-foreign-keys",
        );
        let _initial = oracle(conn, deadline);
        let reported = conn.path().expect("raw DB filename missing");
        let unchanged = db_id == file_id(&open_guarded(Path::new(reported), deadline), deadline);
        assert!(unchanged, "raw connection opened another DB");
    }

    #[cfg(unix)]
    fn failed_children(&mut self) {
        // One diagnostic cleanup horizon; no killed status can become normal success.
        let cleanup = Instant::now() + Duration::from_secs(3);
        for child in &mut self.children {
            if Instant::now() >= cleanup {
                return;
            }
            if matches!(child.try_wait(), Ok(Some(_))) {
                continue;
            }
            if Instant::now() >= cleanup {
                return;
            }
            let _kill_result = child.kill();
            while Instant::now() < cleanup {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Err(_) => return,
                    Ok(None) => std::thread::yield_now(),
                }
            }
        }
    }

    fn close(mut self, deadline: Instant) {
        check(deadline);
        #[cfg(unix)]
        assert!(
            self.children.is_empty(),
            "unknown child ownership prevents cleanup"
        );
        drop(self.connection.take());
        drop(self.store.take());
        self.files.clear();
        self.guards.clear();
        #[cfg(windows)]
        {
            drop(self.stock.take());
            drop(self.plan.take());
        }
        if let Some(root) = self.container.take() {
            need(checked(deadline, || root.close()), "known-owned-root-close");
        }
        self.done = true;
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if !self.done
            && let Some(root) = self.container.take()
        {
            let _preserved = root.keep();
        }
    }
}

fn owned_row(key: &'static str, run: impl FnOnce(&mut Owner, Instant) + Send + 'static) {
    // Origin is before any preparation; a blocked native owner is never joined.
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(90);
    let operation = (entered + Duration::from_secs(60)).min(
        deadline
            .checked_sub(Duration::from_secs(10))
            .expect("overflow when subtracting duration from instant"),
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let mut owner = Owner::new(operation);
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut owner, operation)));
        if outcome.is_err() {
            #[cfg(unix)]
            owner.failed_children();
            let _ = sender.send(false);
            // Instance-local quarantine retains root, files, captures, children and guards.
            loop {
                std::thread::park();
            }
        }
        owner.close(deadline);
        let _ = sender.send(true);
    });
    let completed = need(
        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())),
        "original-row-result",
    );
    assert!(completed, "prune qualification row failed key={key}");
    while !worker.is_finished() {
        check(deadline);
        std::thread::yield_now();
    }
    need(worker.join(), "confirmed-owner-return");
    check(deadline);
    println!("prune-qualification PASS {key}");
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RunState {
    Succeeded,
    InterruptedUnknown,
    Running,
    Starting,
    Queued,
}
impl RunState {
    fn text(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::InterruptedUnknown => "interrupted_unknown",
            Self::Running => "running",
            Self::Starting => "starting",
            Self::Queued => "queued",
        }
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum OutputState {
    Finalized,
    Active,
    Pending,
}
impl OutputState {
    fn text(self) -> &'static str {
        match self {
            Self::Finalized => "finalized",
            Self::Active => "active",
            Self::Pending => "pending",
        }
    }
}
#[derive(Clone)]
struct Seed {
    run: String,
    attempt: i64,
    bytes: i64,
    finalized: i64,
    state: RunState,
    output: OutputState,
    relative: String,
}

fn run_id(number: usize) -> String {
    format!("00000000-0000-4000-8000-{number:012x}")
}

fn seed(number: usize, bytes: i64, expired: bool) -> Seed {
    let run = run_id(number + 10);
    Seed {
        relative: format!("{run}/1.log"),
        run,
        attempt: 1,
        bytes,
        finalized: if expired {
            1
        } else {
            i64::MAX / 2 + i64::try_from(number).unwrap()
        },
        state: RunState::Succeeded,
        output: OutputState::Finalized,
    }
}

struct Spec {
    seeds: Vec<Seed>,
    limit: i64,
    selected: Vec<usize>,
}

fn spec(case: Case) -> Spec {
    let case = case.id();
    let mut result = Spec {
        seeds: vec![seed(0, 10, true), seed(1, 10, true)],
        limit: 100,
        selected: vec![0, 1],
    };
    match case {
        "SEL01" => {
            result.seeds.clear();
            result.selected.clear();
        }
        "SEL02" => {
            result.seeds = vec![seed(0, 10, false), seed(1, 10, false), seed(2, 10, false)];
            result.selected = vec![0];
        }
        "SEL03" => {
            result.seeds = vec![seed(0, 100, false)];
            result.selected.clear();
        }
        "SEL04" => {
            result.seeds = vec![seed(0, 90, false)];
            result.selected.clear();
        }
        "SEL05" => {
            result.seeds = vec![seed(0, 10, true), seed(1, 10, false)];
            result.selected = vec![0];
        }
        "SEL06" => {
            result.seeds = vec![
                seed(0, 5, true),
                seed(1, 10, true),
                seed(2, 10, false),
                seed(3, 10, false),
            ];
            result.selected = vec![0, 1, 2];
        }
        "SEL10" => {
            result.seeds = vec![seed(0, 5, true), seed(0, 5, true), seed(1, 10, true)];
            result.seeds[1].attempt = 2;
            result.seeds[1].relative = format!("{}/2.log", result.seeds[1].run);
            result.selected = vec![0, 1, 2];
        }
        "SEL12" => {
            result.seeds = (0..101).map(|index| seed(index, 1, false)).collect();
            result.limit = 49;
            result.selected = (0..100).collect();
        }
        "SEL16" => {
            result.seeds = vec![seed(0, 0, false), seed(1, 10, false)];
            result.limit = 0;
        }
        "ID02" => {
            result.seeds[0].attempt = 65_535;
            result.seeds[0].relative = format!("{}/65535.log", result.seeds[0].run);
            result.seeds[1].run = result.seeds[0].run.clone();
            result.seeds[1].relative = format!("{}/1.log", result.seeds[1].run);
            result.seeds[0].finalized = 1;
            result.seeds[1].finalized = 1;
            result.selected = vec![1, 0];
        }
        "EL07" => {
            result.seeds.truncate(1);
            result.seeds[0].state = RunState::InterruptedUnknown;
            result.selected = vec![0];
        }
        "EL10" => {
            result.seeds.truncate(1);
            result.seeds[0].state = RunState::Running;
            result.selected.clear();
        }
        "BY01" => {
            result.seeds = vec![seed(0, -1, false)];
            result.selected.clear();
        }
        "BY02" => {
            result.seeds = vec![seed(0, 10, false)];
            result.limit = -1;
            result.selected.clear();
        }
        "BY04" => {
            result.seeds = vec![seed(0, -1, false)];
            result.selected.clear();
        }
        "BY07" => {
            result.seeds = vec![seed(0, i64::MAX, true), seed(1, 1, true)];
            result.selected.clear();
        }
        "FI05" => {
            result.seeds.push(seed(2, 10, true));
            result.selected = vec![0, 1, 2];
        }
        "FI07" => {
            result.seeds.truncate(1);
            result.selected = vec![0];
        }
        _ => {}
    }
    result
}

fn exec(conn: &Connection, sql: &str, deadline: Instant) {
    need(checked(deadline, || conn.execute_batch(sql)), "fixture-sql");
}

fn seed_store(owner: &mut Owner, selected: Case, deadline: Instant) -> Spec {
    let case = selected.id();
    owner.initialize(deadline);
    let mut spec = spec(selected);
    // Invalid persisted identities remain real FK/PK-valid corruption fixtures.
    if case.starts_with("ID") && case != "ID02" {
        let invalid = spec.seeds.last_mut().unwrap();
        match case {
            "ID03" => {
                invalid.run = "not-a-uuid".to_owned();
                invalid.relative = "not-a-uuid/1.log".to_owned();
            }
            "ID04" => {
                invalid.run = invalid.run.to_ascii_uppercase();
                invalid.relative = format!("{}/1.log", invalid.run);
            }
            "ID11" => {
                invalid.attempt = 0;
                invalid.relative = format!("{}/0.log", invalid.run);
            }
            "ID13" => {
                invalid.attempt = 65_536;
                invalid.relative = format!("{}/65536.log", invalid.run);
            }
            "ID15" => invalid.relative = path_text(&owner.paths.root.join("external-canary")),
            "ID16" => invalid.relative = "../external-canary".to_owned(),
            "ID18" => invalid.relative = format!("{}\\1.log", invalid.run),
            "ID19" => invalid.relative = format!("{}/1.partial", invalid.run),
            "ID21" => invalid.relative = format!("{}/2.log", invalid.run),
            "ID22" => invalid.relative = format!("{}/01.log", invalid.run),
            _ => panic!("unknown selected identity control"),
        }
        spec.selected.clear();
    }
    let conn = owner.connection.as_ref().unwrap();
    let corruption = matches!(case, "BY01" | "BY02" | "BY04" | "ID11");
    if corruption {
        exec(conn, "PRAGMA ignore_check_constraints=ON", deadline);
    }
    exec(
        conn,
        "BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON",
        deadline,
    );
    need(
        checked(deadline, || {
            conn.execute("UPDATE settings SET output_limit_bytes=?1,run_retention_count=10000,run_retention_age_us=7776000000000 WHERE singleton=1",[spec.limit])
        }),
        "seed-settings",
    );
    need(
        checked(deadline, || {
            conn.execute("INSERT INTO jobs(id,name,tags_json,enabled,created_at_us,updated_at_us,current_revision) VALUES(?1,'prune-qualification','[]',0,1,1,1)",[JOB])
        }),
        "seed-job",
    );
    need(
        checked(deadline, || {
            conn.execute("INSERT INTO job_revisions VALUES(?1,1,'{}',1,'add')", [JOB])
        }),
        "seed-revision",
    );
    need(
        checked(deadline, || {
            conn.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,updated_at_us) VALUES(?1,1,1,1)",[JOB])
        }),
        "seed-cursor",
    );
    need(
        checked(deadline, || {
            conn.execute(
                "INSERT INTO scheduler_lifetimes VALUES(?1,1,'fixture',1,1,NULL,NULL)",
                [LIFETIME],
            )
        }),
        "seed-lifetime",
    );
    need(
        checked(deadline, || {
            conn.execute("INSERT INTO events(occurred_at_us,kind,job_id,run_id,details_json) VALUES(1,'fixture',?1,NULL,'{}')",[JOB])
        }),
        "seed-event",
    );
    let protected = match case {
        "SEL02" => 80,
        "SEL05" => 70,
        "SEL06" => 85,
        "SEL12" => 49,
        "BY04" => 11,
        _ => 0,
    };
    let mut all = spec.seeds.clone();
    if protected > 0 {
        let mut canary = seed(200, protected, false);
        canary.state = RunState::Running;
        all.push(canary);
    }
    let mut active = seed(201, 7, false);
    active.state = RunState::Running;
    active.output = OutputState::Active;
    active.relative = format!("{}/1.partial", active.run);
    all.push(active);
    let mut pending = seed(202, 3, false);
    pending.state = RunState::Starting;
    pending.output = OutputState::Pending;
    pending.relative = format!("{}/1.partial", pending.run);
    all.push(pending);
    if case == "FI07" {
        let mut later = seed(203, 11, false);
        later.state = RunState::Running;
        all.push(later);
        let mut foreign = seed(204, 13, false);
        foreign.state = RunState::Queued;
        all.push(foreign);
    }
    let mut runs = BTreeSet::new();
    for (index, item) in all.iter().enumerate() {
        if runs.insert(item.run.clone()) {
            need(
                checked(deadline, || {
                    conn.execute("INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,finished_at_us) VALUES(?1,?2,1,'manual',1,1,?3,'{}',?4,?5)",params![item.run,JOB,i64::try_from(index+1).unwrap(),item.state.text(),if item.state==RunState::Running || item.state==RunState::Starting || item.state==RunState::Queued {None}else{Some(10_i64)}])
                }),
                "seed-run",
            );
        }
        let attempt_state = if item.output == OutputState::Active {
            "running"
        } else if item.output == OutputState::Pending {
            "starting"
        } else {
            "succeeded"
        };
        need(
            checked(deadline, || {
                conn.execute("INSERT INTO attempts(run_id,attempt_number,lifetime_id,state,started_at_us,finished_at_us) VALUES(?1,?2,?3,?4,1,?5)",params![item.run,item.attempt,LIFETIME,attempt_state,if item.output==OutputState::Finalized {Some(10_i64)}else{None}])
            }),
            "seed-attempt",
        );
        need(
            checked(deadline, || {
                conn.execute("INSERT INTO output_artifacts(run_id,attempt_number,relative_path,state,retained_payload_bytes,physical_bytes,finalized_at_us) VALUES(?1,?2,?3,?4,?5,?5,?6)",params![item.run,item.attempt,item.relative,item.output.text(),item.bytes,if item.output==OutputState::Finalized {Some(item.finalized)}else{None}])
            }),
            "seed-output",
        );
    }
    exec(conn, "COMMIT", deadline);
    if corruption {
        exec(conn, "PRAGMA ignore_check_constraints=OFF", deadline);
    }
    let checks: i64 = need(
        checked(deadline, || {
            conn.query_row("PRAGMA ignore_check_constraints", [], |row| row.get(0))
        }),
        "checks-restored",
    );
    assert_eq!(checks, 0);
    let foreign = query(conn, "PRAGMA foreign_key_check", deadline);
    assert!(foreign.rows.is_empty(), "fixture relationships invalid");
    for item in &all {
        let directory = owner.paths.outputs.join(&item.run);
        owner.guards.push(need(
            checked(deadline, || DirectoryGuard::private(&directory)),
            "seed-directory",
        ));
        let extension = if item.output == OutputState::Active || item.output == OutputState::Pending
        {
            "partial"
        } else {
            "log"
        };
        let path = directory.join(format!("{}.{extension}", item.attempt));
        let length = usize::try_from(item.bytes.clamp(0, 16)).unwrap();
        drop(write_private(&path, &vec![b'q'; length], deadline));
    }
    drop(write_private(
        &owner.paths.root.join("external-canary"),
        b"preserved external fixture",
        deadline,
    ));
    if case == "FI07" {
        let directory = owner.paths.outputs.join(run_id(900));
        owner.guards.push(need(
            checked(deadline, || DirectoryGuard::private(&directory)),
            "orphan-parent",
        ));
        let path = directory.join("1.log");
        drop(write_private(&path, b"fresh owned orphan", deadline));
        let modified = need(
            checked(deadline, || {
                fs::metadata(&path).and_then(|meta| meta.modified())
            }),
            "orphan-mtime",
        );
        let micros = need(modified.duration_since(UNIX_EPOCH), "orphan-mtime-origin").as_micros();
        assert!(
            micros > u128::try_from(FIXED_NOW).unwrap(),
            "orphan eligibility unobserved"
        );
    }
    check(deadline);
    spec
}

fn owned_files(owner: &Owner, deadline: Instant) -> BTreeMap<PathBuf, FileFact> {
    let conn = owner.connection.as_ref().unwrap();
    let rows = query(
        conn,
        "SELECT run_id,attempt_number,state FROM output_artifacts ORDER BY run_id,attempt_number",
        deadline,
    );
    let mut paths = Vec::new();
    for row in rows.rows {
        let (Cell::Text(run), Cell::Integer(attempt), Cell::Text(state)) =
            (&row[0], &row[1], &row[2])
        else {
            panic!("fixture output types changed")
        };
        let run = std::str::from_utf8(run).unwrap();
        let extension = if state == b"active" || state == b"pending" {
            "partial"
        } else {
            "log"
        };
        paths.push(
            owner
                .paths
                .outputs
                .join(run)
                .join(format!("{attempt}.{extension}")),
        );
    }
    paths.push(owner.paths.root.join("external-canary"));
    let orphan = owner.paths.outputs.join(run_id(900)).join("1.log");
    match checked(deadline, || fs::symlink_metadata(&orphan)) {
        Ok(metadata) => {
            assert!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "orphan type refused"
            );
            paths.push(orphan);
        }
        Err(error) => {
            assert!(
                error.kind() == io::ErrorKind::NotFound,
                "orphan observation refused"
            );
        }
    }
    let mut result = BTreeMap::new();
    for path in paths {
        match checked(deadline, || fs::symlink_metadata(&path)) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                result.insert(path.clone(), fact(&path, deadline));
            }
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                assert!(
                    owner
                        .guards
                        .iter()
                        .any(|guard| guard.normalized_path() == path),
                    "unknown nonfile output refused"
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => panic!("output observation type refused"),
        }
    }
    result
}

fn expect_files(before: &BTreeMap<PathBuf, FileFact>, removed: &[PathBuf], deadline: Instant) {
    for (path, expected) in before {
        if removed.contains(path) {
            let absent = matches!(checked(deadline,||fs::symlink_metadata(path)),Err(error) if error.kind()==io::ErrorKind::NotFound);
            assert!(absent, "selected leaf was not actually removed");
        } else {
            let unchanged = fact(path, deadline) == *expected;
            assert!(
                unchanged,
                "protected leaf full identity/bytes/permissions changed"
            );
        }
    }
}

#[cfg(windows)]
fn candidate(item: &Seed) -> RetentionCandidate {
    RetentionCandidate {
        run_id: item.run.clone(),
        attempt_number: item.attempt,
        relative_path: item.relative.clone(),
        physical_bytes: item.bytes,
        finalized_at_us: item.finalized,
    }
}

fn install_fault(owner: &Owner, spec: &Spec, pending: bool, deadline: Instant) {
    let run = &spec.seeds[0].run;
    assert!(
        hex(&run.replace('-', ""), 32),
        "fault identity is not fixed canonical data"
    );
    let (old, new) = if pending {
        ("finalized", "prune_pending")
    } else {
        ("prune_pending", "pruned")
    };
    exec(
        owner.connection.as_ref().unwrap(),
        &format!(
            "CREATE TRIGGER prune_fixture_fault BEFORE UPDATE OF state ON output_artifacts WHEN OLD.run_id='{run}' AND OLD.state='{old}' AND NEW.state='{new}' BEGIN SELECT RAISE(ABORT,'prune fixture injected refusal'); END"
        ),
        deadline,
    );
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    head: String,
    tree: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExecutableBinding {
    path: String,
    sha256: String,
    size: u64,
    profile_test: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildBinding {
    version: u8,
    revision: Revision,
    production: ExecutableBinding,
    recovery: ExecutableBinding,
}

struct Binding {
    record: BuildBinding,
    slot: usize,
    identity: Identity,
    bytes: Vec<u8>,
}

fn binding_check(owner: &Owner, binding: &Binding, deadline: Instant) {
    let same = file_id(&owner.files[binding.slot], deadline) == binding.identity
        && read_guarded(&owner.files[binding.slot], 32768, deadline) == binding.bytes;
    assert!(same, "retained artifact binding changed");
}

struct Artifact {
    path: PathBuf,
    identity: Identity,
    sha256: String,
    bytes: u64,
    slot: usize,
}

fn digest_file(file: &GuardedFile, deadline: Instant) -> (String, u64) {
    let before = need(checked(deadline, || file.metadata()), "artifact-metadata");
    assert!(
        before.is_file() && before.len() > 0 && before.len() <= 512 * 1024 * 1024,
        "artifact size refused"
    );
    let mut reader = need(checked(deadline, || file.try_clone()), "artifact-clone");
    need(
        checked(deadline, || reader.seek(SeekFrom::Start(0))),
        "artifact-seek",
    );
    let mut buffer = [0_u8; 8192];
    let mut total = 0_u64;
    let mut hash = Sha256::new();
    loop {
        let remaining = 512_u64 * 1024 * 1024 - total;
        let limit =
            usize::try_from((remaining + 1).min(u64::try_from(buffer.len()).unwrap())).unwrap();
        let count = need(
            checked(deadline, || reader.read(&mut buffer[..limit])),
            "artifact-read",
        );
        if count == 0 {
            break;
        }
        total += u64::try_from(count).unwrap();
        assert!(total <= 512 * 1024 * 1024, "artifact hash cap");
        hash.update(&buffer[..count]);
    }
    let after = need(
        checked(deadline, || file.metadata()),
        "artifact-post-metadata",
    );
    let first_modified = need(
        checked(deadline, || before.modified()),
        "artifact-before-mtime",
    );
    let last_modified = need(
        checked(deadline, || after.modified()),
        "artifact-after-mtime",
    );
    let stable = before.len() == total && after.len() == total && first_modified == last_modified;
    assert!(stable, "artifact changed while hashing");
    (format!("{:x}", hash.finalize()), total)
}

fn native_abi(file: &GuardedFile, deadline: Instant) {
    let mut reader = need(checked(deadline, || file.try_clone()), "ABI-clone");
    need(
        checked(deadline, || reader.seek(SeekFrom::Start(0))),
        "ABI-seek",
    );
    let mut header = [0_u8; 64];
    need(
        checked(deadline, || reader.read_exact(&mut header)),
        "ABI-header",
    );
    #[cfg(windows)]
    {
        assert_eq!(&header[..2], b"MZ");
        let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
        assert!(offset <= 1024 * 1024, "PE offset refused");
        need(
            checked(deadline, || reader.seek(SeekFrom::Start(u64::from(offset)))),
            "PE-seek",
        );
        let mut pe = [0_u8; 6];
        need(
            checked(deadline, || reader.read_exact(&mut pe)),
            "PE-header",
        );
        assert_eq!(&pe[..4], b"PE\0\0");
        let machine = u16::from_le_bytes(pe[4..6].try_into().unwrap());
        assert_eq!(
            machine,
            if cfg!(target_arch = "aarch64") {
                0xaa64
            } else {
                0x8664
            }
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(&header[..4], b"\xcf\xfa\xed\xfe");
        assert_eq!(
            u32::from_le_bytes(header[4..8].try_into().unwrap()),
            if cfg!(target_arch = "aarch64") {
                0x0100_000c
            } else {
                0x0100_0007
            }
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(&header[..6], b"\x7fELF\x02\x01");
        assert_eq!(
            u16::from_le_bytes(header[18..20].try_into().unwrap()),
            if cfg!(target_arch = "aarch64") {
                183
            } else {
                62
            }
        );
    }
    check(deadline);
}

fn artifact(owner: &mut Owner, name: &'static str, deadline: Instant) -> Artifact {
    let path = PathBuf::from(need(std::env::var(name), "required-prune-artifact-env"));
    path_text(&path);
    let file = open_guarded(&path, deadline);
    let identity = file_id(&file, deadline);
    native_abi(&file, deadline);
    let (sha256, bytes) = digest_file(&file, deadline);
    let slot = owner.files.len();
    owner.files.push(file);
    Artifact {
        path,
        identity,
        sha256,
        bytes,
        slot,
    }
}

fn recheck(owner: &Owner, artifact: &Artifact, deadline: Instant) {
    let file = &owner.files[artifact.slot];
    let same = file_id(file, deadline) == artifact.identity;
    assert!(same, "artifact full identity changed");
    let digest = digest_file(file, deadline);
    let same_digest = digest == (artifact.sha256.clone(), artifact.bytes);
    assert!(same_digest, "artifact SHA changed");
}

fn bindings(owner: &mut Owner, deadline: Instant) -> (Artifact, Artifact, Binding) {
    let production = artifact(owner, "LOCRON_PRUNE_TEST_BINARY", deadline);
    let recovery = artifact(owner, "LOCRON_PRUNE_RECOVERY_TEST_BINARY", deadline);
    let distinct = production.identity != recovery.identity && production.path != recovery.path;
    assert!(distinct, "production/recovery artifacts must differ");
    // Fixed adjacent record is emitted by the admitted CargoJSON producer, not an env guess.
    let path = production
        .path
        .parent()
        .filter(|path| path.file_name() == Some(std::ffi::OsStr::new("debug")))
        .expect("default debug artifact required")
        .parent()
        .unwrap()
        .join("prune-artifact-binding.json");
    let file = open_guarded(&path, deadline);
    let identity = file_id(&file, deadline);
    let bytes = read_guarded(&file, 32768, deadline);
    let slot = owner.files.len();
    owner.files.push(file);
    let binding: BuildBinding = need(serde_json::from_slice(&bytes), "closed-build-binding");
    assert!(
        binding.version == 1 && hex(&binding.revision.head, 40) && hex(&binding.revision.tree, 40),
        "build revision refused"
    );
    for (actual, bound, test) in [
        (&production, &binding.production, false),
        (&recovery, &binding.recovery, true),
    ] {
        let same = path_text(&actual.path) == bound.path
            && actual.sha256 == bound.sha256
            && actual.bytes == bound.size
            && bound.profile_test == test;
        assert!(
            same && hex(&bound.sha256, 64),
            "actual CargoJSON artifact binding changed"
        );
    }
    (
        production,
        recovery,
        Binding {
            record: binding,
            slot,
            identity,
            bytes,
        },
    )
}

fn capture(owner: &mut Owner, suffix: &str, deadline: Instant) -> usize {
    let absent = matches!(checked(deadline,||fs::symlink_metadata(&owner.captures)),Err(error) if error.kind()==io::ErrorKind::NotFound);
    if absent {
        owner.guards.push(need(
            checked(deadline, || DirectoryGuard::private(&owner.captures)),
            "actual-capture-creator",
        ));
    }
    let path = owner.captures.join(suffix);
    let slot = owner.files.len();
    owner.files.push(write_private(&path, b"", deadline));
    slot
}

#[cfg(unix)]
fn invoke_child(
    owner: &mut Owner,
    executable: &Artifact,
    arguments: &[String],
    environment: &[(&str, String)],
    suffix: &str,
    deadline: Instant,
) -> (i32, Vec<u8>, Vec<u8>) {
    recheck(owner, executable, deadline);
    let stdout = capture(owner, &format!("{suffix}-stdout"), deadline);
    let stderr = capture(owner, &format!("{suffix}-stderr"), deadline);
    let out_id = file_id(&owner.files[stdout], deadline);
    let err_id = file_id(&owner.files[stderr], deadline);
    let out = need(
        checked(deadline, || owner.files[stdout].try_clone()),
        "child-stdout-duplicate",
    );
    let err = need(
        checked(deadline, || owner.files[stderr].try_clone()),
        "child-stderr-duplicate",
    );
    let mut command = Command::new(&executable.path);
    command.current_dir(
        if environment
            .iter()
            .any(|(key, _)| *key == "LOCRON_PRUNE_RECOVERY_MODE")
        {
            &owner.paths.root
        } else {
            owner.guards[0].normalized_path()
        },
    );
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    command
        .env_remove("LOCRON_STATE_DIR")
        .env_remove("LOCRON_PRUNE_RECOVERY_MODE")
        .env_remove("LOCRON_PRUNE_RECOVERY_DESCRIPTOR");
    for (key, value) in environment {
        command.env(key, value);
    }
    check(deadline);
    let result = command.spawn();
    // Anchor the actual child before any post-spawn clock check.
    owner.children.push(need(result, "actual-child-spawn"));
    drop(command);
    check(deadline);
    let slot = owner.children.len() - 1;
    assert!(owner.children[slot].id() > 0, "actual child PID missing");
    let status = loop {
        if let Some(status) = need(
            checked(deadline, || owner.children[slot].try_wait()),
            "actual-child-try-wait",
        ) {
            break status;
        }
        std::thread::yield_now();
    };
    need(
        owner.children.remove(slot).wait(),
        "actual-child-cached-wait",
    );
    let same = out_id == file_id(&owner.files[stdout], deadline)
        && err_id == file_id(&owner.files[stderr], deadline);
    assert!(same, "retained captures changed");
    let out = read_guarded(&owner.files[stdout], CAP, deadline);
    let err = read_guarded(&owner.files[stderr], CAP, deadline);
    recheck(owner, executable, deadline);
    (
        status.code().expect("actual child exit code unobserved"),
        out,
        err,
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identities {
    root: Identity,
    database: Identity,
    production: Identity,
    recovery: Identity,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hashes {
    production: String,
    recovery: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    version: u8,
    surface: String,
    root: String,
    database: String,
    production: String,
    recovery: String,
    revision: Revision,
    identities: Identities,
    hashes: Hashes,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    surface: String,
    root: String,
    identity: Identity,
    sha256: String,
}

fn recovery_envelope(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    surface: &str,
    deadline: Instant,
) -> String {
    let descriptor = Descriptor {
        version: 1,
        surface: surface.to_owned(),
        root: path_text(&owner.paths.root),
        database: path_text(&owner.paths.database),
        production: path_text(&production.path),
        recovery: path_text(&recovery.path),
        revision: binding.record.revision.clone(),
        identities: Identities {
            root: root_id(&owner.paths.root, deadline),
            database: file_id(&owner.files[owner.database_slot.unwrap()], deadline),
            production: production.identity.clone(),
            recovery: recovery.identity.clone(),
        },
        hashes: Hashes {
            production: production.sha256.clone(),
            recovery: recovery.sha256.clone(),
        },
    };
    let bytes = need(serde_json::to_vec(&descriptor), "descriptor-serialize");
    assert!(bytes.len() <= 32768, "descriptor cap refused");
    let path = owner.paths.temporary.join("prune-fi07-recovery-v1.json");
    let file = write_private(&path, &bytes, deadline);
    let envelope = Envelope {
        version: 1,
        surface: surface.to_owned(),
        root: descriptor.root,
        identity: file_id(&file, deadline),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    owner.files.push(file);
    let text = need(serde_json::to_string(&envelope), "envelope-serialize");
    assert!(text.len() <= 8192, "envelope cap refused");
    text
}

fn verify_support(out: &[u8], err: &[u8], code: i32, surface: &str) {
    assert_eq!(code, 0);
    assert!(err.is_empty(), "recovery stderr not empty");
    let text = std::str::from_utf8(out).expect("recovery capture UTF8 refused");
    assert_eq!(
        text.matches(&format!(
            "prune-qualification FI07-SUPPORT {surface} actions=1 outputs_pruned=1 cleanup=1"
        ))
        .count(),
        1
    );
    assert_eq!(
        text.matches("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;")
            .count(),
        1
    );
    assert!(
        !text.contains("prune-qualification PASS "),
        "support cannot fabricate ordinary row PASS"
    );
    // This receipt follows validation of the actual owned child's closed capture and status.
    println!(
        "prune-qualification FI07-RECEIPT {surface} passed=1 failed=0 ignored=0 measured=0 actions=1 outputs_pruned=1 cleanup=1"
    );
}

#[cfg(windows)]
const RECOVERY_SCRIPT: &str = r#"
$child = $null
$files = @($null, $null)
$streams = @($null, $null)
try {
    $names = @($request.PSObject.Properties.Name | Sort-Object)
    if (($names -join ',') -cne 'descriptor,operation,production,recovery,root,stderr,stdout,surface,version,working_directory') { throw 'prune recovery input refused' }
    if ($request.version -ne 1 -or $request.surface -cnotin @('api','cli')) { throw 'prune recovery input refused' }
    foreach ($name in @('root','recovery','production','stdout','stderr','working_directory')) {
        $value = $request.$name
        if ($value -isnot [string] -or $value.Length -lt 1 -or $value.Length -gt 2048 -or $value.IndexOf([char]0) -ge 0) { throw 'prune recovery input refused' }
    }
    if ($request.descriptor -isnot [string] -or $request.descriptor.Length -gt 8192 -or $request.descriptor.IndexOf([char]0) -ge 0) { throw 'prune recovery envelope refused' }
    $op = [string]$request.operation
    $recover = $op -ceq ('recovery-' + $request.surface)
    $words = switch -CaseSensitive ($op) {
        'production-json-live' {'--json prune'}
        'production-json-preview' {'--json prune --dry-run'}
        'production-human-live' {'prune'}
        'production-human-preview' {'prune --dry-run'}
        'production-id-refusal' {'--json prune --id 00000000-0000-4000-8000-000000000002'}
        default {if (-not $recover) {throw 'operation refused'}}
    }
    if (-not $recover -and ($request.surface -cne 'cli' -or $request.descriptor -cne '' -or $request.root.Contains('"') -or $request.root.EndsWith('\') -or $request.root.EndsWith('/'))) {throw 'production root refused'}
    $files[0] = [IO.FileStream]::new($request.stdout, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
    $files[1] = [IO.FileStream]::new($request.stderr, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
    foreach ($file in $files) {
        if ($file.Length -ne 0 -or $file.Position -ne 0) { throw 'prune recovery capture refused' }
    }
    $info = [Diagnostics.ProcessStartInfo]::new()
    if ($recover) {
        $info.FileName = $request.recovery
        $info.Arguments = '--exact prune_qualification::contracts --nocapture'
        if ($request.working_directory -cne $request.root) {throw 'working directory refused'}
    } else {
        $info.FileName = $request.production
        $info.Arguments = '--state-dir "' + $request.root + '" ' + $words
    }
    $info.WorkingDirectory = $request.working_directory
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables.Remove('LOCRON_PRUNE_RECOVERY_MODE')
    $info.EnvironmentVariables.Remove('LOCRON_PRUNE_RECOVERY_DESCRIPTOR')
    $info.EnvironmentVariables.Remove('LOCRON_STATE_DIR')
    $info.EnvironmentVariables['LOCRON_PRUNE_TEST_BINARY'] = $request.production
    $info.EnvironmentVariables['LOCRON_PRUNE_RECOVERY_TEST_BINARY'] = $request.recovery
    if ($recover) {
        $info.EnvironmentVariables['LOCRON_PRUNE_RECOVERY_MODE'] = 'fi07-' + $request.surface + '-v1'
        $info.EnvironmentVariables['LOCRON_PRUNE_RECOVERY_DESCRIPTOR'] = $request.descriptor
    }
    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $info
    if (-not $child.Start()) { throw 'prune recovery spawn refused' }
    $pidValue = $child.Id
    if ($pidValue -le 0) { throw 'prune recovery pid refused' }
    $child.StandardInput.Close()
    $streams = @($child.StandardOutput.BaseStream, $child.StandardError.BaseStream)
    $buffers = @([byte[]]::new(4096), [byte[]]::new(4096))
    $tasks = @($null, $null)
    $totals = @(0, 0)
    $eof = @($false, $false)
    for ($index = 0; $index -lt 2; $index++) {
        $tasks[$index] = $streams[$index].ReadAsync($buffers[$index], 0, 4096)
    }
    $exited = $false
    while (-not ($exited -and $eof[0] -and $eof[1])) {
        for ($index = 0; $index -lt 2; $index++) {
            if (-not $eof[$index] -and $tasks[$index].IsCompleted) {
                $count = $tasks[$index].GetAwaiter().GetResult()
                if ($count -lt 0 -or $count -gt $buffers[$index].Length) { throw 'prune recovery read refused' }
                if ($count -eq 0) {
                    $eof[$index] = $true
                    $tasks[$index] = $null
                } else {
                    $totals[$index] += $count
                    if ($totals[$index] -gt 65537) { throw 'prune recovery capture refused' }
                    $files[$index].Write($buffers[$index], 0, $count)
                    if ($totals[$index] -gt 65536) { throw 'prune recovery capture refused' }
                    $next = [Math]::Min(4096, 65537 - $totals[$index])
                    $tasks[$index] = $streams[$index].ReadAsync($buffers[$index], 0, $next)
                }
            }
        }
        $exited = $child.WaitForExit(1)
    }
    $code = $child.ExitCode
    for ($index = 0; $index -lt 2; $index++) {
        $files[$index].Flush($true)
        $files[$index].Dispose()
        $files[$index] = $null
        $streams[$index].Dispose()
        $streams[$index] = $null
    }
    [ordered]@{ version = 1; surface = $request.surface; child_pid = $pidValue; child_reaped = $true; child_exit_code = $code; stdout_bytes = $totals[0]; stderr_bytes = $totals[1]; io_complete = $true } | & $locronToJson -Compress
} catch {
    throw [InvalidOperationException]::new('prune recovery adapter refused')
} finally {
    foreach ($stream in $streams) { if ($null -ne $stream) { $stream.Dispose() } }
    foreach ($file in $files) { if ($null -ne $file) { $file.Dispose() } }
    if ($null -ne $child) { $child.Dispose() }
}
"#;

#[cfg(windows)]
fn native_call(owner: &mut Owner, script: &'static str, input: &Value, deadline: Instant) -> Value {
    use std::os::windows::ffi::OsStrExt as _;

    check(deadline);
    if owner.stock.is_none() {
        owner.stock = Some(need(
            locron_core::windows::StockAdapterGuard::acquire_until(
                locron_core::windows::StockModuleSet::UtilityAndManagement,
                deadline,
            ),
            "owned-stock-admission",
        ));
    }
    assert!(
        owner
            .stock
            .as_ref()
            .unwrap()
            .executable()
            .as_os_str()
            .encode_wide()
            .count()
            <= 4096,
        "stock command budget refused"
    );
    let result = locron_core::windows::run_script_json_until(script, input, deadline);
    check(deadline);
    need(result, "native-Job-root-I/O-completion")
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct NativeInvocation<'a> {
    operation: &'static str,
    envelope: &'a str,
    suffix: &'a str,
}

#[cfg(windows)]
fn native_invoke(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    invocation: NativeInvocation<'_>,
    deadline: Instant,
) -> (i32, Vec<u8>, Vec<u8>) {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Completion {
        version: u8,
        surface: String,
        child_pid: u32,
        child_reaped: bool,
        child_exit_code: i32,
        stdout_bytes: usize,
        stderr_bytes: usize,
        io_complete: bool,
    }
    let NativeInvocation {
        operation,
        envelope,
        suffix,
    } = invocation;
    let recovery_mode = matches!(operation, "recovery-api" | "recovery-cli");
    let surface = if operation == "recovery-api" {
        "api"
    } else {
        "cli"
    };
    assert!(
        matches!(
            operation,
            "recovery-api"
                | "recovery-cli"
                | "production-json-live"
                | "production-json-preview"
                | "production-human-live"
                | "production-human-preview"
                | "production-id-refusal"
        ),
        "closed operation refused"
    );
    let root = path_text(&owner.paths.root);
    if !recovery_mode {
        assert!(
            envelope.is_empty() && !root.contains('"') && !root.ends_with(['\\', '/']),
            "production quoted root refused"
        );
    }
    let stdout = capture(owner, &format!("{suffix}-stdout"), deadline);
    let stderr = capture(owner, &format!("{suffix}-stderr"), deadline);
    let stdout_id = file_id(&owner.files[stdout], deadline);
    let stderr_id = file_id(&owner.files[stderr], deadline);
    let working = if recovery_mode {
        root.clone()
    } else {
        path_text(owner.guards[0].normalized_path())
    };
    let input = json!({"version":1,"surface":surface,"operation":operation,"working_directory":working,"root":root,"descriptor":envelope,
        "production":path_text(&production.path),"recovery":path_text(&recovery.path),
        "stdout":path_text(owner.files[stdout].normalized_path()),"stderr":path_text(owner.files[stderr].normalized_path())});
    assert!(
        need(serde_json::to_vec(&input), "request-serialize").len() <= 65536,
        "Core input cap refused"
    );
    binding_check(owner, binding, deadline);
    recheck(owner, production, deadline);
    recheck(owner, recovery, deadline);
    let value = native_call(owner, RECOVERY_SCRIPT, &input, deadline);
    let completed: Completion = need(serde_json::from_value(value), "closed-native-completion");
    assert!(
        completed.version == 1
            && completed.surface == surface
            && completed.child_pid > 0
            && completed.child_reaped
            && completed.io_complete
            && completed.stdout_bytes <= CAP
            && completed.stderr_bytes <= CAP,
        "native child/I/O facts refused"
    );
    // Core Ok, not child root reap alone, proves its enrolled Job and outer I/O completed.
    check(deadline);
    let same = stdout_id == file_id(&owner.files[stdout], deadline)
        && stderr_id == file_id(&owner.files[stderr], deadline);
    assert!(same, "native captures replaced");
    let out = read_guarded(&owner.files[stdout], CAP, deadline);
    let err = read_guarded(&owner.files[stderr], CAP, deadline);
    assert_eq!(
        (out.len(), err.len()),
        (completed.stdout_bytes, completed.stderr_bytes)
    );
    binding_check(owner, binding, deadline);
    recheck(owner, production, deadline);
    recheck(owner, recovery, deadline);
    (completed.child_exit_code, out, err)
}

fn recover(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    surface: &str,
    deadline: Instant,
) {
    binding_check(owner, binding, deadline);
    let envelope = recovery_envelope(owner, production, recovery, binding, surface, deadline);
    #[cfg(unix)]
    {
        let environment = [
            ("LOCRON_PRUNE_TEST_BINARY", path_text(&production.path)),
            (
                "LOCRON_PRUNE_RECOVERY_TEST_BINARY",
                path_text(&recovery.path),
            ),
            ("LOCRON_PRUNE_RECOVERY_MODE", format!("fi07-{surface}-v1")),
            ("LOCRON_PRUNE_RECOVERY_DESCRIPTOR", envelope),
        ];
        let (code, out, err) = invoke_child(
            owner,
            recovery,
            &[
                "--exact".to_owned(),
                "prune_qualification::contracts".to_owned(),
                "--nocapture".to_owned(),
            ],
            &environment,
            "recovery",
            deadline,
        );
        verify_support(&out, &err, code, surface);
    }
    #[cfg(windows)]
    {
        let operation = if surface == "api" {
            "recovery-api"
        } else {
            "recovery-cli"
        };
        let (code, out, err) = native_invoke(
            owner,
            production,
            recovery,
            binding,
            NativeInvocation {
                operation,
                envelope: &envelope,
                suffix: "recovery",
            },
            deadline,
        );
        verify_support(&out, &err, code, surface);
    }
    binding_check(owner, binding, deadline);
    recheck(owner, production, deadline);
    recheck(owner, recovery, deadline);
}

fn public_cli(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    operation: &'static str,
    suffix: &str,
    deadline: Instant,
) -> (i32, Vec<u8>, Vec<u8>) {
    let words: &[&str] = match operation {
        "production-json-live" => &["--json", "prune"],
        "production-json-preview" => &["--json", "prune", "--dry-run"],
        "production-human-live" => &["prune"],
        "production-human-preview" => &["prune", "--dry-run"],
        "production-id-refusal" => &["--json", "prune", "--id", JOB],
        _ => panic!("unknown fixed production operation"),
    };
    #[cfg(unix)]
    {
        binding_check(owner, binding, deadline);
        let mut args = vec!["--state-dir".to_owned(), path_text(&owner.paths.root)];
        args.extend(words.iter().map(|word| (*word).to_owned()));
        let result = invoke_child(owner, production, &args, &[], suffix, deadline);
        binding_check(owner, binding, deadline);
        recheck(owner, recovery, deadline);
        result
    }
    #[cfg(windows)]
    {
        let _closed_words = words;
        native_invoke(
            owner,
            production,
            recovery,
            binding,
            NativeInvocation {
                operation,
                envelope: "",
                suffix,
            },
            deadline,
        )
    }
}

fn trigger_drop(owner: &Owner, deadline: Instant) {
    let conn = owner.connection.as_ref().unwrap();
    let rows = query(
        conn,
        "SELECT name FROM sqlite_schema WHERE type='trigger' ORDER BY name",
        deadline,
    );
    let known = rows.rows == vec![vec![Cell::Text(b"prune_fixture_fault".to_vec())]];
    assert!(known, "unknown trigger prevents recovery");
    exec(conn, "DROP TRIGGER prune_fixture_fault", deadline);
}

fn selected_paths(owner: &Owner, spec: &Spec) -> Vec<PathBuf> {
    spec.selected
        .iter()
        .map(|index| {
            owner
                .paths
                .outputs
                .join(&spec.seeds[*index].run)
                .join(format!("{}.log", spec.seeds[*index].attempt))
        })
        .collect()
}

fn changes(spec: &Spec, count: usize, change: Change) -> Vec<(String, i64, Change)> {
    spec.selected
        .iter()
        .take(count)
        .map(|index| {
            (
                spec.seeds[*index].run.clone(),
                spec.seeds[*index].attempt,
                change,
            )
        })
        .collect()
}

fn output_data(spec: &Spec, dry: bool) -> Value {
    let bytes: i64 = spec
        .selected
        .iter()
        .map(|index| spec.seeds[*index].bytes)
        .sum();
    json!({"dry_run":dry,"candidate_count":spec.selected.len(),"bytes":bytes})
}

// All effects below call the real caller/remover. Fixture SQL and expected deltas
// are finite inputs/oracles, not another prune selection/effect implementation.
fn same_oracle(owner: &Owner, before: &Oracle, deadline: Instant) {
    let unchanged = *before == oracle(owner.connection.as_ref().unwrap(), deadline);
    assert!(
        unchanged,
        "complete typed oracle changed on refusal/preview"
    );
}

fn root_stable(owner: &Owner, identity: &Identity, deadline: Instant) {
    assert!(
        *identity == root_id(&owner.paths.root, deadline),
        "owned root full identity changed"
    );
}

fn known_leaf_remove(path: &Path, deadline: Instant) {
    need(
        checked(deadline, || {
            locron_core::filesystem::remove_private_file(path)
        }),
        "known-fixture-leaf-remove",
    );
}

fn drop_run_guards(owner: &mut Owner) {
    owner.guards.truncate(2);
}

fn absent(path: &Path, deadline: Instant) {
    assert!(
        matches!(checked(deadline,||fs::symlink_metadata(path)),Err(error) if error.kind()==io::ErrorKind::NotFound),
        "required path absence not observed"
    );
}

fn run_success(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    spec: &Spec,
    deadline: Instant,
) {
    let before = oracle(owner.connection.as_ref().unwrap(), deadline);
    let files = owned_files(owner, deadline);
    let root = root_id(&owner.paths.root, deadline);
    let preview = actual_call(
        owner, production, recovery, binding, true, "preview", deadline,
    );
    successful(&preview, &output_data(spec, true));
    same_oracle(owner, &before, deadline);
    expect_files(&files, &[], deadline);
    let live = actual_call(
        owner, production, recovery, binding, false, "live", deadline,
    );
    successful(&live, &output_data(spec, false));
    expect_oracle(
        before,
        &oracle(owner.connection.as_ref().unwrap(), deadline),
        &changes(spec, spec.selected.len(), Change::Pruned),
    );
    expect_files(&files, &selected_paths(owner, spec), deadline);
    root_stable(owner, &root, deadline);
}

fn corrupted(
    owner: &mut Owner,
    case: Case,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    deadline: Instant,
) {
    let before = oracle(owner.connection.as_ref().unwrap(), deadline);
    let files = owned_files(owner, deadline);
    for (dry, suffix) in [(true, "preview"), (false, "live")] {
        let result = actual_call(owner, production, recovery, binding, dry, suffix, deadline);
        rejected(&result, case.id());
        same_oracle(owner, &before, deadline);
        expect_files(&files, &[], deadline);
    }
}

fn fault_case(
    owner: &mut Owner,
    case: Case,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    spec: &Spec,
    deadline: Instant,
) {
    let name = case.id();
    if name == "FI01" || name == "FI03" || name == "FI07" {
        install_fault(owner, spec, name == "FI01", deadline);
    }
    let paths = selected_paths(owner, spec);
    let mut extra = None;
    if name == "FI05" {
        drop_run_guards(owner);
        known_leaf_remove(&paths[1], deadline);
        owner.guards.push(need(
            checked(deadline, || DirectoryGuard::private(&paths[1])),
            "owned-nonfile-leaf",
        ));
        let canary = paths[1].join("preserve");
        drop(write_private(&canary, b"nonfile canary", deadline));
        extra = Some((canary.clone(), fact(&canary, deadline)));
    }
    let before = oracle(owner.connection.as_ref().unwrap(), deadline);
    let files = owned_files(owner, deadline);
    if name == "FI04" {
        #[cfg(unix)]
        sync_refusal(owner, deadline);
        #[cfg(not(unix))]
        panic!("Unix sync row cannot execute on this platform");
    } else {
        let result = actual_call(
            owner, production, recovery, binding, false, "failed", deadline,
        );
        rejected(&result, name);
    }
    let (effects, removed) = match name {
        "FI01" => (Vec::new(), Vec::new()),
        "FI05" => {
            let mut delta = changes(spec, 1, Change::Pruned);
            let second = &spec.seeds[spec.selected[1]];
            delta.push((second.run.clone(), second.attempt, Change::Pending));
            (delta, vec![paths[0].clone()])
        }
        _ => (changes(spec, 1, Change::Pending), vec![paths[0].clone()]),
    };
    let pending = oracle(owner.connection.as_ref().unwrap(), deadline);
    expect_oracle(before, &pending, &effects);
    expect_files(&files, &removed, deadline);
    if let Some((path, value)) = extra {
        assert!(fact(&path, deadline) == value, "nonfile canary changed");
    }
    if name == "FI07" {
        trigger_drop(owner, deadline);
        let no_fault = oracle(owner.connection.as_ref().unwrap(), deadline);
        recover(owner, production, recovery, binding, SURFACE, deadline);
        expect_oracle(
            no_fault,
            &oracle(owner.connection.as_ref().unwrap(), deadline),
            &changes(spec, 1, Change::Recovery),
        );
        absent(&paths[0], deadline);
        expect_files(&files, &removed, deadline);
    }
}

fn filesystem_case(
    owner: &mut Owner,
    case: Case,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    spec: &Spec,
    deadline: Instant,
) {
    let name = case.id();
    let path = selected_paths(owner, spec)[0].clone();
    let directory = path.parent().unwrap().to_owned();
    drop_run_guards(owner);
    let before = oracle(owner.connection.as_ref().unwrap(), deadline);
    let files_before = owned_files(owner, deadline);
    let external = owner.paths.root.join("external-canary");
    let canary = fact(&external, deadline);
    #[cfg(unix)]
    let outputs_id = root_id(&owner.paths.outputs, deadline);
    #[cfg(not(unix))]
    let _outputs_id = root_id(&owner.paths.outputs, deadline);
    match name {
        "FS02" => {
            let rows = query(
                owner.connection.as_ref().unwrap(),
                "SELECT run_id,attempt_number,state FROM output_artifacts",
                deadline,
            );
            let mut parents = BTreeSet::new();
            for row in rows.rows {
                let (Cell::Text(run), Cell::Integer(attempt), Cell::Text(state)) =
                    (&row[0], &row[1], &row[2])
                else {
                    panic!("fixture SQL types changed")
                };
                let parent = owner.paths.outputs.join(std::str::from_utf8(run).unwrap());
                let extension = if state == b"active" || state == b"pending" {
                    "partial"
                } else {
                    "log"
                };
                known_leaf_remove(&parent.join(format!("{attempt}.{extension}")), deadline);
                parents.insert(parent);
            }
            for parent in parents {
                need(
                    checked(deadline, || fs::remove_dir(parent)),
                    "known-empty-run-remove",
                );
            }
            need(
                checked(deadline, || fs::remove_dir(&owner.paths.outputs)),
                "known-empty-outputs-remove",
            );
            need(
                helper_remove(&owner.paths, &path, deadline),
                "missing-outputs-helper",
            );
            absent(&owner.paths.outputs, deadline);
        }
        "FS03" => {
            known_leaf_remove(&path, deadline);
            need(
                checked(deadline, || fs::remove_dir(&directory)),
                "known-empty-run-remove",
            );
            need(
                helper_remove(&owner.paths, &path, deadline),
                "missing-run-helper",
            );
            absent(&directory, deadline);
        }
        "FS04" => {
            known_leaf_remove(&path, deadline);
            need(
                helper_remove(&owner.paths, &path, deadline),
                "missing-leaf-helper",
            );
            absent(&path, deadline);
        }
        "FS07" => {
            known_leaf_remove(&path, deadline);
            need(
                checked(deadline, || fs::remove_dir(&directory)),
                "owned-run-remove",
            );
            drop(write_private(&directory, b"owned regular parent", deadline));
            let saved = fact(&directory, deadline);
            assert!(
                helper_remove(&owner.paths, &path, deadline).is_err(),
                "regular parent not refused"
            );
            assert!(
                fact(&directory, deadline) == saved,
                "regular parent changed"
            );
        }
        "FS08" => {
            known_leaf_remove(&path, deadline);
            owner.guards.push(need(
                checked(deadline, || DirectoryGuard::private(&path)),
                "directory-leaf-create",
            ));
            let child = path.join("preserve");
            drop(write_private(&child, b"owned directory canary", deadline));
            let saved = fact(&child, deadline);
            let id = root_id(&path, deadline);
            assert!(
                helper_remove(&owner.paths, &path, deadline).is_err(),
                "directory leaf not refused"
            );
            assert!(
                fact(&child, deadline) == saved && root_id(&path, deadline) == id,
                "directory/canary changed"
            );
        }
        #[cfg(unix)]
        "FS12" => {
            use std::os::unix::fs::{MetadataExt as _, symlink};
            known_leaf_remove(&path, deadline);
            need(
                checked(deadline, || symlink(&external, &path)),
                "owned-symlink-create",
            );
            let saved = need(
                checked(deadline, || fs::symlink_metadata(&path)),
                "link-metadata",
            );
            let target = need(checked(deadline, || fs::read_link(&path)), "link-target");
            assert!(
                helper_remove(&owner.paths, &path, deadline).is_err(),
                "symlink leaf not refused"
            );
            let after = need(
                checked(deadline, || fs::symlink_metadata(&path)),
                "link-after",
            );
            assert!(
                saved.dev() == after.dev()
                    && saved.ino() == after.ino()
                    && after.file_type().is_symlink()
                    && need(
                        checked(deadline, || fs::read_link(&path)),
                        "link-target-after"
                    ) == target,
                "owned link changed"
            );
        }
        #[cfg(unix)]
        "FS14" => {
            use std::os::unix::fs::PermissionsExt as _;
            let saved = need(
                checked(deadline, || fs::metadata(&owner.paths.outputs)),
                "outputs-mode",
            )
            .permissions();
            need(
                checked(deadline, || {
                    fs::set_permissions(&owner.paths.outputs, fs::Permissions::from_mode(0o755))
                }),
                "owned-mode-fixture",
            );
            assert!(
                helper_remove(&owner.paths, &path, deadline).is_err(),
                "broad outputs not refused"
            );
            assert_eq!(
                need(
                    checked(deadline, || fs::metadata(&owner.paths.outputs)),
                    "mode-after"
                )
                .permissions()
                .mode()
                    & 0o777,
                0o755
            );
            assert!(
                root_id(&owner.paths.outputs, deadline) == outputs_id,
                "outputs identity changed"
            );
            need(
                checked(deadline, || {
                    fs::set_permissions(&owner.paths.outputs, saved)
                }),
                "exact-owned-mode-restore",
            );
        }
        _ => panic!("unknown filesystem row"),
    }
    same_oracle(owner, &before, deadline);
    assert!(
        fact(&external, deadline) == canary,
        "external canary changed"
    );
    let mut preserved = files_before;
    if name == "FS02" {
        preserved.retain(|path, _| !path.starts_with(&owner.paths.outputs));
    } else if name != "FS14" {
        preserved.remove(&path);
    }
    expect_files(&preserved, &[], deadline);
    if name == "FS03" || name == "FS04" {
        let files = owned_files(owner, deadline);
        let result = actual_call(
            owner, production, recovery, binding, false, "live", deadline,
        );
        successful(&result, &output_data(spec, false));
        expect_oracle(
            before,
            &oracle(owner.connection.as_ref().unwrap(), deadline),
            &changes(spec, spec.selected.len(), Change::Pruned),
        );
        expect_files(&files, &selected_paths(owner, spec), deadline);
        if name == "FS03" {
            absent(&directory, deadline);
        } else {
            absent(&path, deadline);
        }
    }
}

#[cfg(windows)]
const ACL_SCRIPT: &str = r"$p=[string]$request.path; $a=[IO.File]::GetAccessControl($p); if ($request.action -ceq 'grant') { $s=$a.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All); $a.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'),'Read','Allow')); [IO.File]::SetAccessControl($p,$a); @{version=1;sddl=$s}|& $locronToJson -Compress } elseif ($request.action -ceq 'restore') { $a.SetSecurityDescriptorSddlForm([string]$request.sddl,[Security.AccessControl.AccessControlSections]::All); [IO.File]::SetAccessControl($p,$a); @{version=1;sddl=$a.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)}|& $locronToJson -Compress } else {throw 'fixed ACL action refused'}";

#[cfg(windows)]
const ATTRIBUTE_SCRIPT: &str = r"$p=[string]$request.path; if ($request.action -ceq 'readonly') { $s=[int][IO.File]::GetAttributes($p); [IO.File]::SetAttributes($p,[IO.FileAttributes]($s -bor 1)); @{version=1;attributes=$s}|& $locronToJson -Compress } elseif ($request.action -ceq 'restore') { [IO.File]::SetAttributes($p,[IO.FileAttributes][int]$request.attributes); @{version=1;attributes=[int][IO.File]::GetAttributes($p)}|& $locronToJson -Compress } else {throw 'fixed attribute action refused'}";

#[cfg(windows)]
const JUNCTION_SCRIPT: &str = "New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null; @{created=$true} | & $locronToJson -Compress";

#[cfg(windows)]
fn native_case(
    owner: &mut Owner,
    case: Case,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    spec: &Spec,
    deadline: Instant,
) {
    use std::os::windows::fs::MetadataExt as _;
    let name = case.id();
    let path = selected_paths(owner, spec)[0].clone();
    let before = oracle(owner.connection.as_ref().unwrap(), deadline);
    let files = owned_files(owner, deadline);
    let original = fact(&path, deadline);
    let mut holder = None;
    let mut saved_acl = None;
    let mut attributes = None;
    let mut junction = None;
    match name {
        "N05" => {
            let result = native_call(
                owner,
                ACL_SCRIPT,
                &json!({"path":path_text(&path),"action":"grant"}),
                deadline,
            );
            assert_eq!(result["version"], 1);
            let sddl = result["sddl"]
                .as_str()
                .expect("fixed ACL receipt missing")
                .to_owned();
            assert!(sddl.len() <= 32768, "ACL receipt cap refused");
            saved_acl = Some(sddl);
            assert!(
                !need(
                    checked(deadline, || locron_core::filesystem::is_private(
                        &path, false
                    )),
                    "actual-untrusted-ACL"
                ),
                "untrusted grant did not apply"
            );
        }
        "N07" => {
            let file = open_guarded(&path, deadline);
            assert!(
                file_id(&file, deadline) == original.identity,
                "held leaf identity changed"
            );
            let Err(error) = helper_remove(&owner.paths, &path, deadline) else {
                panic!("actual held-helper refusal absent");
            };
            assert!(
                error.kind == Some(io::ErrorKind::PermissionDenied)
                    && matches!(error.raw, Some(32 | 33)),
                "actual no-delete sharing refusal missing"
            );
            holder = Some(file);
        }
        "N08" => {
            let result = native_call(
                owner,
                ATTRIBUTE_SCRIPT,
                &json!({"path":path_text(&path),"action":"readonly"}),
                deadline,
            );
            assert_eq!(result["version"], 1);
            let saved = result["attributes"]
                .as_u64()
                .expect("actual attributes missing");
            assert!(u32::try_from(saved).is_ok(), "attributes range refused");
            attributes = Some(saved);
            assert_eq!(
                need(
                    checked(deadline, || fs::metadata(&path)),
                    "actual-readonly-metadata"
                )
                .file_attributes(),
                u32::try_from(saved).unwrap() | 1
            );
        }
        "N06" => {
            drop_run_guards(owner);
            known_leaf_remove(&path, deadline);
            let link = path.parent().unwrap().to_owned();
            need(
                checked(deadline, || fs::remove_dir(&link)),
                "known-run-parent-remove",
            );
            let target = owner.guards[0].normalized_path().join("external-target");
            owner.guards.push(need(
                checked(deadline, || DirectoryGuard::private(&target)),
                "known-target-create",
            ));
            let canary = target.join("1.log");
            drop(write_private(
                &canary,
                b"preserved junction target",
                deadline,
            ));
            let target_id = root_id(&target, deadline);
            let canary_fact = fact(&canary, deadline);
            let parent_id = root_id(&owner.paths.outputs, deadline);
            let created = native_call(
                owner,
                JUNCTION_SCRIPT,
                &json!({"link":path_text(&link),"target":path_text(&target)}),
                deadline,
            );
            assert_eq!(created["created"], true);
            let metadata = need(
                checked(deadline, || fs::symlink_metadata(&link)),
                "actual-junction-type",
            );
            assert!(
                metadata.file_attributes() & 0x410 == 0x410,
                "actual junction type refused"
            );
            let actual = need(
                checked(deadline, || fs::read_link(&link)),
                "actual-junction-target",
            );
            assert!(actual == target, "actual known junction target changed");
            junction = Some((
                link,
                target,
                canary,
                target_id,
                canary_fact,
                parent_id,
                actual,
            ));
        }
        _ => panic!("unknown native row"),
    }
    same_oracle(owner, &before, deadline);
    let failure = actual_call(
        owner,
        production,
        recovery,
        binding,
        false,
        "native-refusal",
        deadline,
    );
    rejected(&failure, name);
    let pending = oracle(owner.connection.as_ref().unwrap(), deadline);
    expect_oracle(before, &pending, &changes(spec, 1, Change::Pending));
    if name != "N06" {
        let after = fact(&path, deadline);
        assert!(
            after.identity == original.identity && after.bytes == original.bytes,
            "native refused leaf changed"
        );
        let mut later = files.clone();
        later.remove(&path);
        expect_files(&later, &[], deadline);
    }
    if let Some(file) = holder {
        assert!(
            file_id(&file, deadline) == original.identity,
            "exact held object changed"
        );
        drop(file);
        need(
            checked(deadline, || helper_remove(&owner.paths, &path, deadline)),
            "same-pending-remove-positive",
        );
        let first = candidate(&spec.seeds[spec.selected[0]]);
        need(
            checked(deadline, || {
                owner
                    .store
                    .as_ref()
                    .unwrap()
                    .finish_output_prune(&first, FIXED_NOW)
            }),
            "same-pending-finish-positive",
        );
        expect_oracle(
            pending,
            &oracle(owner.connection.as_ref().unwrap(), deadline),
            &changes(spec, 1, Change::Recovery),
        );
        expect_files(&files, std::slice::from_ref(&path), deadline);
        println!("prune-qualification CONTROL {SURFACE} N07 helper-finish-positive");
    }
    if let Some(sddl) = saved_acl {
        let restored = native_call(
            owner,
            ACL_SCRIPT,
            &json!({"path":path_text(&path),"action":"restore","sddl":sddl}),
            deadline,
        );
        assert!(
            restored["sddl"].as_str() == Some(sddl.as_str()),
            "exact ACL restoration missing"
        );
        assert!(
            fact(&path, deadline) == original,
            "restored ACL object facts changed"
        );
    }
    if let Some(saved) = attributes {
        let restored = native_call(
            owner,
            ATTRIBUTE_SCRIPT,
            &json!({"path":path_text(&path),"action":"restore","attributes":saved}),
            deadline,
        );
        assert_eq!(restored["attributes"], saved);
        assert!(
            fact(&path, deadline) == original,
            "exact attribute restoration changed object"
        );
    }
    if let Some((link, target, canary, target_id, canary_fact, parent_id, actual)) = junction {
        assert!(
            need(
                checked(deadline, || fs::read_link(&link)),
                "known-junction-target-after"
            ) == actual
                && root_id(&target, deadline) == target_id
                && fact(&canary, deadline) == canary_fact
                && root_id(&owner.paths.outputs, deadline) == parent_id,
            "known junction/target boundary changed"
        );
        // No junction-self128-bit identity or hostile same-account replacement proof.
        let metadata = need(
            checked(deadline, || fs::symlink_metadata(&link)),
            "known-junction-type-after",
        );
        assert!(
            metadata.file_attributes() & 0x410 == 0x410,
            "junction type unobserved"
        );
        need(
            checked(deadline, || fs::remove_dir(&link)),
            "known-junction-nonrecursive-remove",
        );
        absent(&link, deadline);
        assert!(
            root_id(&target, deadline) == target_id && fact(&canary, deadline) == canary_fact,
            "external target changed during link removal"
        );
        let mut later = files;
        later.remove(&path);
        expect_files(&later, &[], deadline);
    }
}

fn run_row(owner: &mut Owner, case: Case, deadline: Instant) {
    let (production, recovery, binding) = bindings(owner, deadline);
    if special(owner, case, &production, &recovery, &binding, deadline) {
        binding_check(owner, &binding, deadline);
        recheck(owner, &production, deadline);
        recheck(owner, &recovery, deadline);
        return;
    }
    let spec = seed_store(owner, case, deadline);
    let name = case.id();
    if name.starts_with("ID") && name != "ID02" || matches!(name, "BY01" | "BY02" | "BY04" | "BY07")
    {
        corrupted(owner, case, &production, &recovery, &binding, deadline);
    } else if name.starts_with("FI") {
        fault_case(
            owner,
            case,
            &production,
            &recovery,
            &binding,
            &spec,
            deadline,
        );
    } else if name.starts_with("FS") {
        filesystem_case(
            owner,
            case,
            &production,
            &recovery,
            &binding,
            &spec,
            deadline,
        );
    } else if name.starts_with('N') {
        #[cfg(windows)]
        native_case(
            owner,
            case,
            &production,
            &recovery,
            &binding,
            &spec,
            deadline,
        );
        #[cfg(not(windows))]
        panic!("native row cannot execute on this platform");
    } else {
        run_success(owner, &production, &recovery, &binding, &spec, deadline);
    }
    binding_check(owner, &binding, deadline);
    recheck(owner, &production, deadline);
    recheck(owner, &recovery, deadline);
}

fn ordinary_contracts() {
    let ledger: Value = need(serde_json::from_str(LEDGER), "frozen-ledger");
    let expected = ledger["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["surface"] == SURFACE
                && (row["platforms"] == "all"
                    || cfg!(unix) && row["platforms"] == "unix"
                    || cfg!(windows) && row["platforms"] == "windows")
        })
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let actual = ROWS
        .iter()
        .map(|(key, _)| (*key).to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        expected == actual && actual.len() == ROWS.len(),
        "exact frozen applicable row union changed"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(LEDGER.as_bytes())),
        "2f3e1e88f4cb1bc954993287d726a3b442d3fad922f60ce0462f88211557d3a5"
    );
    for &(key, case) in ROWS {
        owned_row(key, move |owner, deadline| run_row(owner, case, deadline));
    }
}

const ROWS: &[(&str, Case)] = &[
    ("SEL02-cli", Case::Sel02),
    ("SEL05-cli", Case::Sel05),
    ("SEL06-cli", Case::Sel06),
    ("SEL10-cli", Case::Sel10),
    ("SEL12-cli", Case::Sel12),
    ("EL07-cli", Case::El07),
    ("EL10-cli", Case::El10),
    ("ID03-cli", Case::Id03),
    ("ID11-cli", Case::Id11),
    ("ID13-cli", Case::Id13),
    ("ID16-cli", Case::Id16),
    ("ID19-cli", Case::Id19),
    ("ID21-cli", Case::Id21),
    ("BY07-cli", Case::By07),
    ("FS02-cli", Case::Fs02),
    ("FS07-cli", Case::Fs07),
    ("FS08-cli", Case::Fs08),
    #[cfg(unix)]
    ("FS12-cli", Case::Fs12),
    #[cfg(unix)]
    ("FS14-cli", Case::Fs14),
    ("FI01-cli", Case::Fi01),
    ("FI03-cli", Case::Fi03),
    #[cfg(unix)]
    ("FI04-cli", Case::Fi04),
    ("FI05-cli", Case::Fi05),
    ("FI07-cli", Case::Fi07),
    ("CLI01", Case::Cli01),
    ("CLI04", Case::Cli04),
    ("CLI05", Case::Cli05),
    ("CLI09", Case::Cli09),
    #[cfg(windows)]
    ("N07-cli", Case::N07),
    #[cfg(windows)]
    ("N08-cli", Case::N08),
    ("SEL01-cli", Case::Sel01),
    ("SEL03-cli", Case::Sel03),
    ("SEL04-cli", Case::Sel04),
    ("ID04-cli", Case::Id04),
    ("ID15-cli", Case::Id15),
    ("ID18-cli", Case::Id18),
    ("ID22-cli", Case::Id22),
    ("FS03-cli", Case::Fs03),
    ("FS04-cli", Case::Fs04),
    #[cfg(windows)]
    ("N05-cli", Case::N05),
    #[cfg(windows)]
    ("N06-cli", Case::N06),
    ("SEL16-cli", Case::Sel16),
    ("BY01-cli", Case::By01),
    ("BY04-cli", Case::By04),
    ("ID02-cli", Case::Id02),
    ("BY02-cli", Case::By02),
];

const SURFACE: &str = "cli";

struct Outcome {
    code: i32,
    body: Value,
}
struct Refusal {
    #[cfg(windows)]
    kind: Option<io::ErrorKind>,
    #[cfg(windows)]
    raw: Option<i32>,
}

fn helper_remove(paths: &StatePaths, path: &Path, deadline: Instant) -> Result<(), Refusal> {
    checked(deadline, || {
        super::explicit_prune::remove_output(paths, path)
    })
    .map_err(|error| {
        #[cfg(windows)]
        {
            let original = error.downcast_ref::<io::Error>();
            Refusal {
                kind: original.map(io::Error::kind),
                raw: original.and_then(io::Error::raw_os_error),
            }
        }
        #[cfg(not(windows))]
        {
            let _original = error;
            Refusal {}
        }
    })
}

fn actual_call(
    owner: &mut Owner,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    dry: bool,
    suffix: &str,
    deadline: Instant,
) -> Outcome {
    let operation = if dry {
        "production-json-preview"
    } else {
        "production-json-live"
    };
    let (code, out, err) = public_cli(
        owner, production, recovery, binding, operation, suffix, deadline,
    );
    assert!(err.is_empty(), "actual CLI JSON stderr changed");
    let body = need(serde_json::from_slice(&out), "actual CLI complete envelope");
    Outcome { code, body }
}

fn successful(outcome: &Outcome, data: &Value) {
    let expected =
        json!({"schema":"locron.cli/v1","ok":true,"command":"prune","data":data,"warnings":[]});
    let exact = outcome.code == 0 && outcome.body == expected;
    assert!(exact, "actual complete CLI success envelope changed");
}

fn rejected(outcome: &Outcome, case: &str) {
    let (status, category) = match case {
        "BY01" | "BY02" | "BY04" => (3, "durable_conflict"),
        "ID03" | "ID04" | "ID11" | "BY07" | "FI01" | "FI03" | "FI07" => (5, "state_error"),
        _ => (2, "invalid_request"),
    };
    let object = outcome
        .body
        .as_object()
        .expect("actual CLI error envelope missing");
    let error = outcome.body["error"]
        .as_object()
        .expect("actual CLI error object missing");
    let exact = outcome.code == status
        && object.len() == 5
        && outcome.body["schema"] == "locron.cli/v1"
        && outcome.body["ok"] == false
        && outcome.body["command"] == "prune"
        && outcome.body["warnings"] == json!([])
        && error.len() == 2
        && outcome.body["error"]["code"] == category
        && outcome.body["error"]["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty());
    assert!(
        exact,
        "actual CLI primary exit/category/complete envelope changed"
    );
}

#[cfg(unix)]
fn sync_refusal(owner: &mut Owner, deadline: Instant) {
    let mut calls = 0_usize;
    let Err(error) = checked(deadline, || {
        super::prune_with_remover(&owner.paths, false, super::Format::Json, |paths, path| {
            super::explicit_prune::remove_output_with_sync(paths, path, |directory| {
                check(deadline);
                assert!(
                    directory == path.parent().unwrap(),
                    "sync-stage directory changed"
                );
                calls += 1;
                Err(io::Error::other("owned post-unlink sync-stage refusal"))
            })
        })
    }) else {
        panic!("actual sync-stage refusal missing");
    };
    check(deadline);
    assert_eq!(calls, 1, "actual sync stage must be evaluated once");
    assert!(
        error
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::Other),
        "actual primary sync error changed"
    );
}

fn special(
    owner: &mut Owner,
    case: Case,
    production: &Artifact,
    recovery: &Artifact,
    binding: &Binding,
    deadline: Instant,
) -> bool {
    if !case.id().starts_with("CLI") {
        return false;
    }
    if matches!(case.id(), "CLI05" | "CLI09") {
        let operation = if case.id() == "CLI05" {
            "production-human-preview"
        } else {
            "production-id-refusal"
        };
        let (code, out, err) = public_cli(
            owner, production, recovery, binding, operation, "public", deadline,
        );
        if case.id() == "CLI05" {
            let exact = code == 0
                && err.is_empty()
                && out == b"dry run: would prune 0 runs, 0 outputs (0 bytes)\n";
            assert!(exact, "actual absent-state preview output changed");
        } else {
            let text = std::str::from_utf8(&err).expect("Clap refusal must be UTF8");
            assert!(
                code == 2 && out.is_empty() && text.contains("unexpected argument '--id'"),
                "actual Clap --id refusal changed"
            );
        }
        for path in [
            &owner.paths.root,
            &owner.paths.database,
            &owner.paths.outputs,
            &owner.paths.temporary,
            &owner.paths.daemon_lock,
        ] {
            absent(path, deadline);
        }
    } else {
        let spec = seed_store(owner, case, deadline);
        let before = oracle(owner.connection.as_ref().unwrap(), deadline);
        let files = owned_files(owner, deadline);
        if case.id() == "CLI01" {
            let (code, out, err) = public_cli(
                owner,
                production,
                recovery,
                binding,
                "production-human-preview",
                "human",
                deadline,
            );
            let runs = spec
                .selected
                .iter()
                .map(|index| spec.seeds[*index].run.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            let expected = format!(
                "dry run: would prune {runs} runs, {} outputs ({} bytes)\n",
                spec.selected.len(),
                output_data(&spec, true)["bytes"]
            );
            let exact = code == 0 && err.is_empty() && out == expected.as_bytes();
            assert!(exact, "actual human preview bytes changed");
            same_oracle(owner, &before, deadline);
            expect_files(&files, &[], deadline);
        } else {
            let outcome = actual_call(
                owner, production, recovery, binding, false, "public", deadline,
            );
            successful(&outcome, &output_data(&spec, false));
            expect_oracle(
                before,
                &oracle(owner.connection.as_ref().unwrap(), deadline),
                &changes(&spec, spec.selected.len(), Change::Pruned),
            );
            expect_files(&files, &selected_paths(owner, &spec), deadline);
        }
    }
    binding_check(owner, binding, deadline);
    recheck(owner, production, deadline);
    recheck(owner, recovery, deadline);
    true
}

fn support_input() -> Option<(String, String)> {
    let mode = std::env::var_os("LOCRON_PRUNE_RECOVERY_MODE");
    let envelope = std::env::var_os("LOCRON_PRUNE_RECOVERY_DESCRIPTOR");
    match (mode, envelope) {
        (None, None) => None,
        (Some(mode), Some(envelope)) => {
            let mode = need(mode.into_string(), "support-mode-UTF8");
            let envelope = need(envelope.into_string(), "support-envelope-UTF8");
            assert!(
                matches!(mode.as_str(), "fi07-api-v1" | "fi07-cli-v1") && envelope.len() <= 8192,
                "support input refused"
            );
            // Parse and reject unknown/partial input before any Store access.
            let value: Envelope = need(serde_json::from_str(&envelope), "closed support envelope");
            let surface = if mode == "fi07-api-v1" { "api" } else { "cli" };
            assert!(
                value.version == 1 && value.surface == surface && hex(&value.sha256, 64),
                "support envelope fields refused"
            );
            path_text(Path::new(&value.root));
            Some((surface.to_owned(), envelope))
        }
        _ => panic!("partial support input refused"),
    }
}

fn validate_pending_fixture(
    owner: &Owner,
    deadline: Instant,
) -> (Oracle, Spec, BTreeMap<PathBuf, FileFact>) {
    let conn = owner.connection.as_ref().unwrap();
    let before = oracle(conn, deadline);
    for (name, count) in [
        ("jobs", 1),
        ("job_revisions", 1),
        ("schedule_cursors", 1),
        ("scheduler_lifetimes", 1),
        ("events", 1),
        ("runs", 5),
        ("attempts", 5),
        ("output_artifacts", 5),
        ("retry_intents", 0),
        ("run_retention_pending", 0),
        ("admission_state", 1),
        ("settings", 1),
        ("schema_migrations", 5),
        ("sqlite_sequence", 1),
    ] {
        assert_eq!(
            before.tables[name].rows.len(),
            count,
            "support fixed fixture table count changed"
        );
    }
    let relationships = query(conn, "PRAGMA foreign_key_check", deadline);
    assert!(
        relationships.rows.is_empty(),
        "support foreign keys refused"
    );
    assert!(
        query(
            conn,
            "SELECT name FROM sqlite_schema WHERE type='trigger'",
            deadline
        )
        .rows
        .is_empty(),
        "support fault trigger still present"
    );
    let settings = query(
        conn,
        "SELECT output_limit_bytes,run_retention_count,run_retention_age_us FROM settings WHERE singleton=1",
        deadline,
    );
    assert!(
        settings.rows
            == vec![vec![
                Cell::Integer(100),
                Cell::Integer(10000),
                Cell::Integer(7_776_000_000_000)
            ]],
        "support settings refused"
    );
    let lifetime = query(
        conn,
        "SELECT id,pid,binary_version,started_at_us,heartbeat_at_us,ended_at_us,exit_class FROM scheduler_lifetimes",
        deadline,
    );
    assert!(
        lifetime.rows
            == vec![vec![
                Cell::Text(LIFETIME.as_bytes().to_vec()),
                Cell::Integer(1),
                Cell::Text(b"fixture".to_vec()),
                Cell::Integer(1),
                Cell::Integer(1),
                Cell::Null,
                Cell::Null
            ]],
        "support lifetime refused"
    );
    let outputs = query(
        conn,
        "SELECT run_id,attempt_number,relative_path,state,retained_payload_bytes,physical_bytes,finalized_at_us,prune_started_at_us,pruned_at_us FROM output_artifacts ORDER BY run_id",
        deadline,
    );
    let mut expected = Vec::new();
    for (number, bytes, state, extension, finalized) in [
        (0, 10, "prune_pending", "log", Some(1)),
        (201, 7, "active", "partial", None),
        (202, 3, "pending", "partial", None),
        (203, 11, "finalized", "log", Some(i64::MAX / 2 + 203)),
        (204, 13, "finalized", "log", Some(i64::MAX / 2 + 204)),
    ] {
        let run = run_id(number + 10);
        expected.push(vec![
            Cell::Text(run.as_bytes().to_vec()),
            Cell::Integer(1),
            Cell::Text(format!("{run}/1.{extension}").into_bytes()),
            Cell::Text(state.as_bytes().to_vec()),
            Cell::Integer(bytes),
            Cell::Integer(bytes),
            finalized.map_or(Cell::Null, Cell::Integer),
            if number == 0 {
                outputs.rows[0][7].clone()
            } else {
                Cell::Null
            },
            Cell::Null,
        ]);
    }
    assert!(
        outputs.rows == expected && matches!(outputs.rows[0][7],Cell::Integer(value) if value>0),
        "support same pending/canary records refused"
    );
    let states = query(conn, "SELECT id,state FROM runs ORDER BY id", deadline);
    let expected = [
        (0, "succeeded"),
        (201, "running"),
        (202, "starting"),
        (203, "running"),
        (204, "queued"),
    ]
    .map(|(number, state)| {
        vec![
            Cell::Text(run_id(number + 10).into_bytes()),
            Cell::Text(state.as_bytes().to_vec()),
        ]
    });
    assert!(
        states.rows == expected,
        "support terminal/current/noneligible states refused"
    );
    let mut spec = spec(Case::Fi07);
    spec.seeds.truncate(1);
    spec.selected = vec![0];
    let first = selected_paths(owner, &spec)[0].clone();
    absent(&first, deadline);
    let orphan = owner.paths.outputs.join(run_id(900)).join("1.log");
    let modified = need(
        checked(deadline, || {
            fs::metadata(&orphan).and_then(|value| value.modified())
        }),
        "support orphan mtime",
    );
    assert!(
        need(modified.duration_since(UNIX_EPOCH), "support orphan epoch").as_micros()
            > u128::try_from(FIXED_NOW).unwrap(),
        "support orphan eligibility refused"
    );
    let files = owned_files(owner, deadline);
    (before, spec, files)
}

fn support(surface: &str, text: &str) {
    // Fixed finite support owner; all native work stays off the result-driver thread.
    owned_row_support(surface, text);
}

fn owned_row_support(surface: &str, text: &str) {
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(90);
    let operation = (entered + Duration::from_secs(60)).min(
        deadline
            .checked_sub(Duration::from_secs(10))
            .expect("overflow when subtracting duration from instant"),
    );
    let marker_surface = surface.to_owned();
    let surface = surface.to_owned();
    let text = text.to_owned();
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let envelope: Envelope = need(serde_json::from_str(&text), "support envelope");
        let root = PathBuf::from(&envelope.root);
        path_text(&root);
        let root_guard = need(
            checked(operation, || DirectoryGuard::existing_private(&root)),
            "support private root",
        );
        assert!(
            root_guard.normalized_path() == root,
            "support canonical root refused"
        );
        let container_guard = need(
            checked(operation, || {
                DirectoryGuard::ancestors(root.parent().unwrap())
            }),
            "support owned container",
        );
        let paths = StatePaths::new(root.clone());
        let descriptor_path = paths.temporary.join("prune-fi07-recovery-v1.json");
        let descriptor_file = open_guarded(&descriptor_path, operation);
        let identity = file_id(&descriptor_file, operation);
        let bytes = read_guarded(&descriptor_file, 32768, operation);
        assert!(
            identity == envelope.identity
                && format!("{:x}", Sha256::digest(&bytes)) == envelope.sha256,
            "support descriptor creator identity/hash refused"
        );
        let descriptor: Descriptor = need(serde_json::from_slice(&bytes), "closed descriptor");
        assert!(
            descriptor.version == 1
                && descriptor.surface == surface
                && descriptor.root == envelope.root
                && descriptor.database == path_text(&paths.database)
                && root_id(&root, operation) == descriptor.identities.root
                && hex(&descriptor.revision.head, 40)
                && hex(&descriptor.revision.tree, 40)
                && hex(&descriptor.hashes.production, 64)
                && hex(&descriptor.hashes.recovery, 64),
            "support descriptor facts refused"
        );
        let database = open_guarded(&paths.database, operation);
        assert!(
            file_id(&database, operation) == descriptor.identities.database,
            "support DB creator identity refused"
        );
        #[cfg(windows)]
        let plan = Some(need(
            locron_core::filesystem::PrivateDirectoryPlan::inspect_until(&root, operation),
            "support retained private plan",
        ));
        let mut owner = Owner {
            container: None,
            paths,
            captures: container_guard.normalized_path().join("captures"),
            guards: vec![container_guard, root_guard],
            files: vec![database, descriptor_file],
            database_slot: Some(0),
            connection: None,
            store: None,
            #[cfg(unix)]
            children: Vec::new(),
            #[cfg(windows)]
            plan,
            #[cfg(windows)]
            stock: None,
            done: false,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (production, recovery, binding) = bindings(&mut owner, operation);
            assert!(
                path_text(&production.path) == descriptor.production
                    && path_text(&recovery.path) == descriptor.recovery
                    && production.identity == descriptor.identities.production
                    && recovery.identity == descriptor.identities.recovery
                    && production.sha256 == descriptor.hashes.production
                    && recovery.sha256 == descriptor.hashes.recovery
                    && binding.record.revision.head == descriptor.revision.head
                    && binding.record.revision.tree == descriptor.revision.tree,
                "support independent artifact binding refused"
            );
            let current = open_guarded(
                &need(
                    checked(operation, std::env::current_exe),
                    "actual support executable",
                ),
                operation,
            );
            let (hash, size) = digest_file(&current, operation);
            assert!(
                file_id(&current, operation) == recovery.identity
                    && hash == recovery.sha256
                    && size == recovery.bytes,
                "actual support binary/ABI refused"
            );
            native_abi(&current, operation);
            owner.files.push(current);
            // Validate same existing DB/owners before actual Store access, never create input state.
            check(operation);
            let store = Store::open(owner.paths.clone(), env!("CARGO_PKG_VERSION"), FIXED_NOW);
            let store = store.map(|store| owner.store = Some(store));
            check(operation);
            need(store, "support actual existing Store");
            let normalized = owner.files[0].normalized_path();
            check(operation);
            #[cfg(windows)]
            let raw = Connection::open_with_flags_and_vfs(
                normalized,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                "win32-longpath",
            );
            #[cfg(unix)]
            let raw = Connection::open_with_flags(
                normalized,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            );
            let raw = raw.map(|connection| owner.connection = Some(connection));
            check(operation);
            need(raw, "support retained writer raw oracle");
            need(
                checked(operation, || {
                    owner
                        .connection
                        .as_ref()
                        .unwrap()
                        .busy_timeout(Duration::from_secs(5))
                }),
                "support original SQLite budget",
            );
            let (before, spec, files) = validate_pending_fixture(&owner, operation);
            let report = need(
                checked(operation, || {
                    super::maintenance::maintain(
                        owner.store.as_ref().unwrap(),
                        &owner.paths,
                        LIFETIME,
                        FIXED_NOW,
                    )
                }),
                "actual same-pending maintain",
            );
            let exact = report
                == super::maintenance::MaintenanceReport {
                    actions: 1,
                    outputs_pruned: 1,
                    ..super::maintenance::MaintenanceReport::default()
                };
            assert!(
                exact,
                "actual maintenance effects must be exactly one pending completion"
            );
            expect_oracle(
                before,
                &oracle(owner.connection.as_ref().unwrap(), operation),
                &changes(&spec, 1, Change::Recovery),
            );
            expect_files(&files, &[], operation);
            absent(&selected_paths(&owner, &spec)[0], operation);
            binding_check(&owner, &binding, operation);
            recheck(&owner, &production, operation);
            recheck(&owner, &recovery, operation);
            assert!(
                file_id(&owner.files[1], operation) == identity
                    && read_guarded(&owner.files[1], 32768, operation) == bytes,
                "support retained descriptor changed"
            );
        }));
        if result.is_err() {
            let _ = sender.send(false);
            loop {
                std::thread::park();
            }
        }
        owner.close(deadline);
        let _ = sender.send(true);
    });
    let completed = need(
        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())),
        "support original owner result",
    );
    assert!(completed, "support owner failed");
    while !worker.is_finished() {
        check(deadline);
        std::thread::yield_now();
    }
    need(worker.join(), "support actual owner returned");
    check(deadline);
    println!(
        "prune-qualification FI07-SUPPORT {marker_surface} actions=1 outputs_pruned=1 cleanup=1"
    );
}

#[test]
fn contracts() {
    if let Some((surface, envelope)) = support_input() {
        support(&surface, &envelope);
    } else {
        ordinary_contracts();
    }
}
