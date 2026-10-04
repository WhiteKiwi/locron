// Private, closed-file protocol; no product parser or product expectation lives here.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use locron_core::filesystem::{DirectoryGuard, GuardedFile, create_private_new};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type Check<T> = Result<T, &'static str>;
const CONTROL_CAP: usize = 16 * 1024;
const SNAPSHOT_CAP: usize = 16 * 1024 * 1024;
const CELL_CAP: usize = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: PathBuf,
    identity: String,
    sha256: String,
    crc32: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ticket {
    schema: String,
    nonce: String,
    row: usize,
    started_us: u64,
    parent: PathBuf,
    state: PathBuf,
    parent_anchor: String,
    state_anchor: String,
    server: Artifact,
    store: Artifact,
    controller: Option<Artifact>,
}

struct Clock {
    origin: Instant,
}

impl Clock {
    fn admit(started_us: u64) -> Check<Self> {
        let elapsed = wall_us()?
            .checked_sub(started_us)
            .ok_or("wall clock reversed")?;
        let origin = Instant::now()
            .checked_sub(Duration::from_micros(elapsed))
            .ok_or("origin overflow")?;
        let clock = Self { origin };
        clock.check(90)?;
        Ok(clock)
    }

    fn check(&self, seconds: u64) -> Check<()> {
        require(
            Instant::now() < self.until(seconds)?,
            "original row horizon expired",
        )
    }

    fn until(&self, seconds: u64) -> Check<Instant> {
        self.origin
            .checked_add(Duration::from_secs(seconds))
            .ok_or("horizon overflow")
    }
}

fn wall_us() -> Check<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "wall time unavailable")?
        .as_micros()
        .try_into()
        .map_err(|_| "wall time overflow")
}

fn require(condition: bool, message: &'static str) -> Check<()> {
    if condition { Ok(()) } else { Err(message) }
}

fn identity(file: &GuardedFile) -> Check<String> {
    #[cfg(windows)]
    {
        let id = locron_core::filesystem::file_identity(file)
            .map_err(|_| "native file identity failed")?;
        Ok(format!(
            "{:016x}:{:032x}",
            id.volume_serial_number, id.file_id
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().map_err(|_| "file metadata failed")?;
        Ok(format!("{:016x}:{:016x}", meta.dev(), meta.ino()))
    }
}

fn private_read(path: &Path) -> Check<GuardedFile> {
    let parent = path.parent().ok_or("private leaf parent missing")?;
    let _guard =
        DirectoryGuard::existing_private(parent).map_err(|_| "existing private parent refused")?;
    #[cfg(windows)]
    let file = locron_core::filesystem::open_private(path, fs::OpenOptions::new().read(true))
        .map_err(|_| "private leaf admission failed")?;
    #[cfg(unix)]
    let file = {
        require(
            locron_core::filesystem::is_private(path, false)
                .map_err(|_| "leaf privacy inspection failed")?,
            "broad private leaf refused",
        )?;
        locron_core::filesystem::open_read_no_follow(path)
            .map_err(|_| "private leaf admission failed")?
    };
    Ok(file)
}

fn read_private(path: &Path, cap: usize) -> Check<Vec<u8>> {
    let mut file = private_read(path).map_err(|_| "private read admission failed")?;
    let size = usize::try_from(
        file.metadata()
            .map_err(|_| "private metadata failed")?
            .len(),
    )
    .map_err(|_| "private size overflow")?;
    require(size <= cap, "private read cap exceeded")?;
    let mut bytes = Vec::with_capacity(size);
    Read::by_ref(&mut *file)
        .take(
            u64::try_from(cap)
                .map_err(|_| "cap overflow")?
                .checked_add(1)
                .ok_or("cap overflow")?,
        )
        .read_to_end(&mut bytes)
        .map_err(|_| "private read failed")?;
    require(
        bytes.len() == size && bytes.len() <= cap,
        "private read changed or exceeded cap",
    )?;
    Ok(bytes)
}

fn read_json(path: &Path, cap: usize) -> Check<Value> {
    serde_json::from_slice(&read_private(path, cap)?).map_err(|_| "private JSON invalid")
}

static OWNED_LEAVES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<PathBuf, String>>,
> = std::sync::OnceLock::new();

fn record_owned(path: &Path, id: &str) -> Check<()> {
    let mut owned = OWNED_LEAVES
        .get_or_init(std::sync::Mutex::default)
        .lock()
        .map_err(|_| "owned leaf ledger poisoned")?;
    require(
        !owned.contains_key(path),
        "duplicate owned publication identity",
    )?;
    owned.insert(path.to_path_buf(), id.to_owned());
    Ok(())
}

fn owned_manifest(parent: &Path) -> Check<Value> {
    let owned = OWNED_LEAVES
        .get_or_init(std::sync::Mutex::default)
        .lock()
        .map_err(|_| "owned leaf ledger poisoned")?;
    let mut result = json!({});
    for (path, id) in owned.iter().filter(|(path, _)| path.starts_with(parent)) {
        let relative = path
            .strip_prefix(parent)
            .map_err(|_| "owned leaf scope invalid")?;
        let name = relative
            .to_str()
            .ok_or("owned leaf UTF8 name invalid")?
            .replace('\\', "/");
        require(!name.is_empty(), "owned leaf name empty")?;
        result[&name] = json!(id);
    }
    Ok(result)
}

fn publish(path: &Path, value: &impl Serialize, cap: usize) -> Check<()> {
    publish_closed(
        path,
        || serde_json::to_vec(value).map_err(|_| "private JSON encode failed"),
        cap,
    )
}

fn publish_done(path: &Path, mut value: Value, cap: usize) -> Check<()> {
    publish_closed(
        path,
        move || {
            value["ownership"] = owned_manifest(
                path.parent()
                    .ok_or("completed publication parent missing")?,
            )?;
            serde_json::to_vec(&value).map_err(|_| "completed receipt encode failed")
        },
        cap,
    )
}

fn publish_closed(path: &Path, body: impl FnOnce() -> Check<Vec<u8>>, cap: usize) -> Check<()> {
    DirectoryGuard::existing_private(path.parent().ok_or("publication parent missing")?)
        .map_err(|_| "existing publication parent refused")?;
    let stage = path.with_extension("closed-stage");
    let mut file = create_private_new(&stage).map_err(|_| "exclusive stage admission failed")?;
    let expected = identity(&file)?;
    record_owned(path, &expected)?;
    let bytes = body()?;
    require(bytes.len() <= cap, "private publication cap exceeded")?;
    file.write_all(&bytes)
        .map_err(|_| "private stage write failed")?;
    file.sync_all().map_err(|_| "private stage sync failed")?;
    drop(file);
    let owned = tempfile::TempPath::try_from_path(stage).map_err(|_| "stage owner failed")?;
    if let Err(error) = owned.persist_noclobber(path) {
        let _ = error.path.keep();
        return Err("closed stage no-clobber publication failed");
    }
    let final_file = private_read(path)?;
    require(
        identity(&final_file)? == expected,
        "publication identity changed",
    )
}

struct Admission {
    ticket: Ticket,
    clock: Clock,
    _parent: DirectoryGuard,
    _state: DirectoryGuard,
    _anchors: Vec<GuardedFile>,
    artifact: GuardedFile,
}

fn admit(role: &str) -> Check<Admission> {
    let path = PathBuf::from(std::env::var_os("LOCRON_PR144_TICKET").ok_or("ticket missing")?);
    let ticket: Ticket = serde_json::from_slice(&read_private(&path, CONTROL_CAP)?)
        .map_err(|_| "ticket shape invalid")?;
    require(
        ticket.schema == "locron.private.pr144/v1",
        "ticket schema mismatch",
    )?;
    require(
        ticket.nonce == std::env::var("LOCRON_PR144_NONCE").map_err(|_| "nonce missing")?,
        "nonce mismatch",
    )?;
    require(
        (156..195).contains(&ticket.row) && (cfg!(windows) || ticket.row < 193),
        "row admission invalid",
    )?;
    let clock = Clock::admit(ticket.started_us)?;
    let parent = DirectoryGuard::existing_private(&ticket.parent)
        .map_err(|_| "existing parent admission failed")?;
    let state = DirectoryGuard::existing_private(&ticket.state)
        .map_err(|_| "existing root admission failed")?;
    require(
        parent.normalized_path() == ticket.parent && state.normalized_path() == ticket.state,
        "guarded canonical root mismatch",
    )?;
    let mut anchors = Vec::new();
    for (path, expected, label) in [
        (
            ticket.parent.join("anchor"),
            &ticket.parent_anchor,
            "parent",
        ),
        (ticket.state.join("sentinel"), &ticket.state_anchor, "state"),
    ] {
        let file = private_read(&path).map_err(|_| "anchor admission failed")?;
        require(identity(&file)? == *expected, "anchor identity mismatch")?;
        require(
            read_private(&path, CONTROL_CAP)? == format!("{}:{label}", ticket.nonce).as_bytes(),
            "anchor content mismatch",
        )?;
        anchors.push(file);
    }
    let expected = match role {
        "server" => &ticket.server,
        "store" => &ticket.store,
        "controller" => ticket
            .controller
            .as_ref()
            .ok_or("controller binding missing")?,
        _ => return Err("artifact role invalid"),
    };
    require(
        fs::canonicalize(std::env::current_exe().map_err(|_| "current artifact missing")?)
            .map_err(|_| "artifact canonicalization failed")?
            == expected.path,
        "actual current artifact mismatch",
    )?;
    #[cfg(windows)]
    let artifact = locron_core::filesystem::read_owned_executable(&expected.path)
        .map_err(|_| "owned executable admission failed")?;
    #[cfg(unix)]
    let artifact = locron_core::filesystem::open_read_no_follow(&expected.path)
        .map_err(|_| "executable admission failed")?;
    require(
        identity(&artifact)? == expected.identity,
        "artifact identity mismatch",
    )?;
    clock.check(30)?;
    Ok(Admission {
        ticket,
        clock,
        _parent: parent,
        _state: state,
        _anchors: anchors,
        artifact,
    })
}

fn populated(row: usize) -> bool {
    !(166..170).contains(&row) && !(184..193).contains(&row)
}

fn receipt(ticket: &Ticket, sequence: u64, operation: &str) -> Value {
    json!({"schema":"locron.private.pr144-receipt/v1", "nonce":ticket.nonce,
        "pid":std::process::id(), "sequence":sequence, "operation":operation})
}

use super::{CreateJob, StatePaths, Store};

fn close_store(store: Store) -> Check<()> {
    let Store {
        connection,
        paths: _,
        #[cfg(windows)]
            _read_guards: read_guards,
        #[cfg(windows)]
            _state_guard: state_guard,
    } = store;
    let connection = connection
        .into_inner()
        .map_err(|_| "connection mutex poisoned")?;
    connection
        .close()
        .map_err(|_| "actual SQLite close failed")?;
    #[cfg(windows)]
    {
        drop(read_guards);
        drop(state_guard);
    }
    Ok(())
}

fn definition(ticket: &Ticket) -> Check<Value> {
    let inputs: Value = serde_json::from_str(include_str!(
        "../../../../docs/planning/DASHBOARD_BOOLEAN_FIXTURE_INPUTS.json"
    ))
    .map_err(|_| "independent fixture JSON invalid")?;
    let mut definition = inputs["definition_literal"].clone();
    definition["target"]["executable"] = json!(ticket.server.path);
    definition["cwd"] = json!(ticket.state);
    definition["environment"]["values"]["PR144_SECRET"] = json!(format!("new-{}", ticket.nonce));
    // Independent normalized defaults, not a serialization of the product parser.
    definition["environment"]["file"] = Value::Null;
    definition["environment"]["path"] = Value::Null;
    definition["completion_action"] = json!("retain");
    Ok(definition)
}

fn ordered_object(value: &Value, names: &[&str]) -> Check<String> {
    let object = value
        .as_object()
        .ok_or("independent golden object absent")?;
    require(
        object.len() == names.len(),
        "independent golden field set changed",
    )?;
    let mut fields = Vec::new();
    for name in names {
        let value = object
            .get(*name)
            .ok_or("independent golden field missing")?;
        fields.push(format!(
            "{}:{}",
            serde_json::to_string(name).map_err(|_| "golden key encode failed")?,
            serde_json::to_string(value).map_err(|_| "golden value encode failed")?
        ));
    }
    Ok(format!("{{{}}}", fields.join(",")))
}

fn durable_definition(definition: &Value) -> Check<String> {
    let schedule = ordered_object(&definition["schedule"], &["kind", "expression", "timezone"])?;
    let target = ordered_object(&definition["target"], &["kind", "executable", "args"])?;
    let environment = ordered_object(&definition["environment"], &["file", "values", "path"])?;
    let policy = ordered_object(
        &definition["policy"],
        &[
            "overlap",
            "missed_run",
            "start_deadline",
            "catch_up_limit",
            "retries",
            "retry_delay",
            "retry_cap",
            "backoff",
            "retry_timeout",
            "timeout",
            "termination_grace",
            "per_job_concurrency",
        ],
    )?;
    let cwd = serde_json::to_string(&definition["cwd"]).map_err(|_| "golden cwd encode failed")?;
    Ok(format!(
        "{{\"schedule\":{schedule},\"target\":{target},\"cwd\":{cwd},\"environment\":{environment},\"policy\":{policy},\"completion_action\":\"retain\"}}"
    ))
}

fn seed(ticket: &Ticket) -> Check<Store> {
    let store = Store::open(
        StatePaths::new(ticket.state.clone()),
        env!("CARGO_PKG_VERSION"),
        100,
    )
    .map_err(|_| "owned Store setup failed")?;
    let definition = durable_definition(&definition(ticket)?)?;
    for (id, name, description) in [
        (
            "01900000-0000-7000-8000-000000000001",
            "pr144-seed",
            "seed description",
        ),
        (
            "01900000-0000-7000-8000-000000000002",
            "pr144-unrelated",
            "unrelated metadata",
        ),
    ] {
        store
            .create_job(&CreateJob {
                id: id.to_owned(),
                name: name.to_owned(),
                description: Some(description.to_owned()),
                tags_json: "[\"pr144\"]".to_owned(),
                enabled: true,
                definition_json: definition.clone(),
                now_us: 100,
                cursor_us: 100,
            })
            .map_err(|_| "actual job seed failed")?;
    }
    store
        .enqueue_manual(
            "pr144-unrelated",
            "01900000-0000-7000-8000-000000000003",
            101,
        )
        .map_err(|_| "unrelated queued run seed failed")?;
    store
        .set_environment("PR144_OTHER", Some(&format!("other-{}", ticket.nonce)), 100)
        .map_err(|_| "unrelated environment seed failed")?;
    if (164..166).contains(&ticket.row) || (176..178).contains(&ticket.row) {
        store
            .set_environment("PR144_NAME", Some(&format!("old-{}", ticket.nonce)), 100)
            .map_err(|_| "replaced environment seed failed")?;
    }
    #[cfg(windows)]
    if ticket.row == 193 {
        // Fixed setup-only legacy operation. No caller-supplied SQL or raw environment.
        let raw = json!({"Pr144_Name":format!("old-{}", ticket.nonce), "PR144_OTHER":format!("other-{}", ticket.nonce)});
        store
            .conn()
            .map_err(|_| "legacy setup connection failed")?
            .execute(
                "UPDATE settings SET environment_json=?1 WHERE singleton=1",
                [serde_json::to_string(&raw).map_err(|_| "legacy setup encode failed")?],
            )
            .map_err(|_| "fixed legacy setup failed")?;
    }
    store
        .set_setting("global_concurrency", "3", 100)
        .map_err(|_| "typed setting seed failed")?;
    Ok(store)
}

fn charge(total: &mut usize, size: usize) -> Check<()> {
    *total = total
        .checked_add(size)
        .ok_or("snapshot arithmetic overflow")?;
    require(*total <= SNAPSHOT_CAP, "encoded snapshot budget exceeded")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut result = String::new();
    for byte in bytes {
        write!(result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

fn cell(value: rusqlite::types::ValueRef<'_>, budget: &mut usize) -> Check<Value> {
    use rusqlite::types::ValueRef;
    let result = match value {
        ValueRef::Null => json!({"type":"null"}),
        ValueRef::Integer(value) => json!({"type":"integer", "value":value.to_string()}),
        ValueRef::Real(value) => json!({"type":"real", "bits":format!("{:016x}", value.to_bits())}),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            require(bytes.len() <= CELL_CAP, "cell cap exceeded")?;
            let encoded = bytes
                .len()
                .checked_mul(2)
                .and_then(|size| size.checked_add(32))
                .ok_or("cell encoding overflow")?;
            require(
                budget
                    .checked_add(encoded)
                    .is_some_and(|size| size <= SNAPSHOT_CAP),
                "cell budget exceeded",
            )?;
            json!({"type":if matches!(value, ValueRef::Text(_)) {"text"} else {"blob"}, "value":hex(bytes)})
        }
    };
    charge(
        budget,
        serde_json::to_vec(&result)
            .map_err(|_| "cell encode failed")?
            .len(),
    )?;
    Ok(result)
}

fn typed_rows(
    tx: &rusqlite::Transaction<'_>,
    sql: &str,
    cap: usize,
    budget: &mut usize,
) -> Check<Value> {
    let mut statement = tx.prepare(sql).map_err(|_| "snapshot statement failed")?;
    let columns = statement.column_count();
    require(columns <= 128, "snapshot column cap exceeded")?;
    let mut rows = statement.query([]).map_err(|_| "snapshot query failed")?;
    let mut output = Vec::new();
    while let Some(row) = rows.next().map_err(|_| "snapshot row failed")? {
        require(output.len() < cap, "snapshot row cap exceeded")?;
        let mut values = Vec::with_capacity(columns);
        for column in 0..columns {
            let value = row
                .get_ref(column)
                .map_err(|_| "snapshot typed cell failed")?;
            let cell = cell(value, budget)?;
            values.push(cell);
        }
        let values = Value::Array(values);
        let encoded = serde_json::to_vec(&values).map_err(|_| "row encoding failed")?;
        charge(
            budget,
            values
                .as_array()
                .ok_or("typed row is not array")?
                .len()
                .checked_add(2)
                .ok_or("row encoding overflow")?,
        )?;
        output.push((encoded, values));
    }
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(Value::Array(
        output.into_iter().map(|(_, values)| values).collect(),
    ))
}

fn snapshot(database: &Path) -> Check<Value> {
    let store = Store::open_read_only(database).map_err(|_| "actual readonly Store open failed")?;
    let mut logical = json!({});
    let mut diagnostics = json!({});
    {
        let mut connection = store.conn().map_err(|_| "readonly connection failed")?;
        let tx = connection
            .transaction()
            .map_err(|_| "readonly transaction failed")?;
        let mut budget = 0;
        diagnostics["schema_rootpages"] = typed_rows(
            &tx,
            "SELECT name,rootpage FROM main.sqlite_schema",
            128,
            &mut budget,
        )?;
        logical["schema"] = typed_rows(
            &tx,
            "SELECT type,name,tbl_name,sql FROM main.sqlite_schema",
            128,
            &mut budget,
        )?;
        for name in ["application_id", "user_version", "schema_version"] {
            let value: i64 = tx
                .query_row(&format!("PRAGMA main.{name}"), [], |row| row.get(0))
                .map_err(|_| "logical pragma failed")?;
            logical["pragmas"][name] = json!(value.to_string());
        }
        for name in [
            "page_count",
            "freelist_count",
            "data_version",
            "journal_mode",
        ] {
            diagnostics[name] = typed_rows(&tx, &format!("PRAGMA main.{name}"), 1, &mut budget)?;
        }
        let names: Vec<String> = {
            let mut query = tx
                .prepare("SELECT name FROM main.sqlite_schema WHERE type='table' ORDER BY name")
                .map_err(|_| "table list failed")?;
            query
                .query_map([], |row| row.get(0))
                .map_err(|_| "table list query failed")?
                .collect::<Result<_, _>>()
                .map_err(|_| "table list row failed")?
        };
        require(names.len() <= 32, "main table cap exceeded")?;
        for name in names {
            let quoted = name.replace('"', "\"\"");
            let columns = typed_rows(
                &tx,
                &format!("PRAGMA main.table_xinfo(\"{quoted}\")"),
                128,
                &mut budget,
            )?;
            let rows = typed_rows(
                &tx,
                &format!("SELECT * FROM main.\"{quoted}\""),
                4096,
                &mut budget,
            )?;
            logical["tables"][&name] = json!({"columns":columns,"rows":rows});
        }
        tx.commit()
            .map_err(|_| "readonly transaction completion failed")?;
    }
    close_store(store)?;
    Ok(
        json!({"logical":logical,"diagnostics":diagnostics,"readonly_transaction_completed":true,"readonly_connection_closed":true}),
    )
}

fn child() -> Check<()> {
    let admission = admit("store")?;
    let ticket = &admission.ticket;
    require(
        populated(ticket.row),
        "absent row cannot open Store fixture",
    )?;
    let mut artifact = admission
        .artifact
        .try_clone()
        .map_err(|_| "artifact clone failed")?;
    let mut hash = crc32fast::Hasher::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        let size = artifact
            .read(&mut buffer)
            .map_err(|_| "actual artifact read failed")?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    require(
        format!("{:08x}", hash.finalize()) == ticket.store.crc32,
        "actual Store artifact CRC mismatch",
    )?;
    let keeper = seed(ticket)?;
    let database = private_read(&ticket.state.join("state.db"))?;
    let database_identity = identity(&database)?;
    record_owned(&ticket.state.join("state.db"), &database_identity)?;
    drop(database);
    admission.clock.check(30)?;
    let mut ready = receipt(ticket, 0, "ready");
    ready["keeper_open"] = json!(true);
    publish(&ticket.parent.join("store-ready.json"), &ready, CONTROL_CAP)?;
    let mut sequence: u64 = 1;
    loop {
        admission.clock.check(90)?;
        let path = ticket.parent.join(format!("store-command-{sequence}.json"));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => return Err("store command inspection failed"),
            Ok(meta) => require(
                meta.is_file() && !meta.file_type().is_symlink(),
                "store command is not regular",
            )?,
        }
        let command = read_json(&path, CONTROL_CAP)?;
        require(
            command["nonce"] == ticket.nonce
                && command["sequence"] == sequence
                && command["pid"] == std::process::id(),
            "store command binding mismatch",
        )?;
        match command["operation"].as_str() {
            Some("snapshot") if sequence <= 2 => {
                admission.clock.check(87)?;
                let mut value = snapshot(&ticket.state.join("state.db"))?;
                admission.clock.check(87)?;
                value["receipt"] = receipt(ticket, sequence, "snapshot");
                publish(
                    &ticket.parent.join(format!("snapshot-{sequence}.json")),
                    &value,
                    SNAPSHOT_CAP,
                )?;
            }
            Some("stop") if sequence == 3 => {
                close_store(keeper)?;
                let database = private_read(&ticket.state.join("state.db"))?;
                require(
                    identity(&database)? == database_identity,
                    "owned database replaced; retained",
                )?;
                drop(database);
                for name in ["state.db-wal", "state.db-shm", "state.db-journal"] {
                    let path = ticket.state.join(name);
                    match fs::symlink_metadata(&path) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Err(_) => return Err("closed sidecar inventory failed"),
                        Ok(_) => {
                            let file = private_read(&path)?;
                            record_owned(&path, &identity(&file)?)?;
                        }
                    }
                }
                admission.clock.check(90)?;
                let mut done = receipt(ticket, sequence, "closed");
                done["keeper_connection_closed"] = json!(true);
                publish_done(&ticket.parent.join("store-done.json"), done, CONTROL_CAP)?;
                return Ok(());
            }
            _ => return Err("store command sequence or operation invalid"),
        }
        sequence = sequence.checked_add(1).ok_or("store sequence overflow")?;
    }
}

#[test]
fn owned_snapshot_fixture_child() {
    if std::env::var_os("LOCRON_PR144_TICKET").is_none() {
        // Support selector is not a qualification case and does not publish a key.
        return;
    }
    if let Err(message) = child() {
        panic!("PR144 owned Store child failed: {message}; private root retained");
    }
}
