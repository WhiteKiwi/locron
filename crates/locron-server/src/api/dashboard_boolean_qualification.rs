// Private, closed-file protocol; no product parser or product expectation lives here.
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use locron_core::filesystem::{DirectoryGuard, GuardedFile, create_private_new};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type Check<T> = Result<T, &'static str>;
const CONTROL_CAP: usize = 16 * 1024;
const SNAPSHOT_CAP: usize = 16 * 1024 * 1024;
const RESPONSE_CAP: usize = 1024 * 1024;
const CELL_CAP: usize = 1024 * 1024;
const STORE_SELECTOR: &str =
    "store::dashboard_pr144_snapshot_fixture::owned_snapshot_fixture_child";
const SERVER_SELECTOR: &str = "api::dashboard_boolean_qualification::owned_http_fixture_child";

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
    let mut owned = tempfile::TempPath::try_from_path(stage).map_err(|_| "stage owner failed")?;
    owned.disable_cleanup(true);
    if owned.persist_noclobber(path).is_err() {
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

fn validate_receipt(
    value: &Value,
    ticket: &Ticket,
    pid: u32,
    sequence: u64,
    operation: &str,
) -> Check<()> {
    require(
        value["schema"] == "locron.private.pr144-receipt/v1"
            && value["nonce"] == ticket.nonce
            && value["pid"] == pid
            && value["sequence"] == sequence
            && value["operation"] == operation,
        "actual receipt binding mismatch",
    )
}

const KEYS: [&str; 195] = [
    "JSON/create/missing_false",
    "JSON/create/native_true",
    "JSON/create/native_false",
    "JSON/create/null_true",
    "JSON/create/empty_string_true",
    "JSON/create/true_string_true",
    "JSON/create/one_string_true",
    "JSON/create/false_string_false",
    "JSON/create/zero_string_false",
    "JSON/create/number_zero_reject",
    "JSON/create/number_one_reject",
    "JSON/create/negative_reject",
    "JSON/create/fraction_reject",
    "JSON/create/array_reject",
    "JSON/create/object_reject",
    "JSON/create/yes_reject",
    "JSON/create/uppercase_TRUE_reject",
    "JSON/create/padded_true_reject",
    "JSON/create/two_string_reject",
    "JSON/update/missing_false",
    "JSON/update/native_true",
    "JSON/update/native_false",
    "JSON/update/null_true",
    "JSON/update/empty_string_true",
    "JSON/update/true_string_true",
    "JSON/update/one_string_true",
    "JSON/update/false_string_false",
    "JSON/update/zero_string_false",
    "JSON/update/number_zero_reject",
    "JSON/update/number_one_reject",
    "JSON/update/negative_reject",
    "JSON/update/fraction_reject",
    "JSON/update/array_reject",
    "JSON/update/object_reject",
    "JSON/update/yes_reject",
    "JSON/update/uppercase_TRUE_reject",
    "JSON/update/padded_true_reject",
    "JSON/update/two_string_reject",
    "JSON/settings/missing_false",
    "JSON/settings/native_true",
    "JSON/settings/native_false",
    "JSON/settings/null_true",
    "JSON/settings/empty_string_true",
    "JSON/settings/true_string_true",
    "JSON/settings/one_string_true",
    "JSON/settings/false_string_false",
    "JSON/settings/zero_string_false",
    "JSON/settings/number_zero_reject",
    "JSON/settings/number_one_reject",
    "JSON/settings/negative_reject",
    "JSON/settings/fraction_reject",
    "JSON/settings/array_reject",
    "JSON/settings/object_reject",
    "JSON/settings/yes_reject",
    "JSON/settings/uppercase_TRUE_reject",
    "JSON/settings/padded_true_reject",
    "JSON/settings/two_string_reject",
    "QUERY/list.all/missing_false",
    "QUERY/list.all/bare_true",
    "QUERY/list.all/empty_true",
    "QUERY/list.all/true_true",
    "QUERY/list.all/one_true",
    "QUERY/list.all/false_false",
    "QUERY/list.all/zero_false",
    "QUERY/list.all/uppercase_TRUE_reject",
    "QUERY/list.all/two_reject",
    "QUERY/list.all/null_text_reject",
    "QUERY/list.all/padded_true_reject",
    "QUERY/run.wait/missing_false",
    "QUERY/run.wait/bare_true",
    "QUERY/run.wait/empty_true",
    "QUERY/run.wait/true_true",
    "QUERY/run.wait/one_true",
    "QUERY/run.wait/false_false",
    "QUERY/run.wait/zero_false",
    "QUERY/run.wait/uppercase_TRUE_reject",
    "QUERY/run.wait/two_reject",
    "QUERY/run.wait/null_text_reject",
    "QUERY/run.wait/padded_true_reject",
    "QUERY/run.dry-run/missing_false",
    "QUERY/run.dry-run/bare_true",
    "QUERY/run.dry-run/empty_true",
    "QUERY/run.dry-run/true_true",
    "QUERY/run.dry-run/one_true",
    "QUERY/run.dry-run/false_false",
    "QUERY/run.dry-run/zero_false",
    "QUERY/run.dry-run/uppercase_TRUE_reject",
    "QUERY/run.dry-run/two_reject",
    "QUERY/run.dry-run/null_text_reject",
    "QUERY/run.dry-run/padded_true_reject",
    "QUERY/cancel.acknowledge-unconfirmed/missing_false",
    "QUERY/cancel.acknowledge-unconfirmed/bare_true",
    "QUERY/cancel.acknowledge-unconfirmed/empty_true",
    "QUERY/cancel.acknowledge-unconfirmed/true_true",
    "QUERY/cancel.acknowledge-unconfirmed/one_true",
    "QUERY/cancel.acknowledge-unconfirmed/false_false",
    "QUERY/cancel.acknowledge-unconfirmed/zero_false",
    "QUERY/cancel.acknowledge-unconfirmed/uppercase_TRUE_reject",
    "QUERY/cancel.acknowledge-unconfirmed/two_reject",
    "QUERY/cancel.acknowledge-unconfirmed/null_text_reject",
    "QUERY/cancel.acknowledge-unconfirmed/padded_true_reject",
    "QUERY/export.include-values/missing_false",
    "QUERY/export.include-values/bare_true",
    "QUERY/export.include-values/empty_true",
    "QUERY/export.include-values/true_true",
    "QUERY/export.include-values/one_true",
    "QUERY/export.include-values/false_false",
    "QUERY/export.include-values/zero_false",
    "QUERY/export.include-values/uppercase_TRUE_reject",
    "QUERY/export.include-values/two_reject",
    "QUERY/export.include-values/null_text_reject",
    "QUERY/export.include-values/padded_true_reject",
    "QUERY/export.acknowledge-plaintext/missing_false",
    "QUERY/export.acknowledge-plaintext/bare_true",
    "QUERY/export.acknowledge-plaintext/empty_true",
    "QUERY/export.acknowledge-plaintext/true_true",
    "QUERY/export.acknowledge-plaintext/one_true",
    "QUERY/export.acknowledge-plaintext/false_false",
    "QUERY/export.acknowledge-plaintext/zero_false",
    "QUERY/export.acknowledge-plaintext/uppercase_TRUE_reject",
    "QUERY/export.acknowledge-plaintext/two_reject",
    "QUERY/export.acknowledge-plaintext/null_text_reject",
    "QUERY/export.acknowledge-plaintext/padded_true_reject",
    "QUERY/import.accept-plaintext-values/missing_false",
    "QUERY/import.accept-plaintext-values/bare_true",
    "QUERY/import.accept-plaintext-values/empty_true",
    "QUERY/import.accept-plaintext-values/true_true",
    "QUERY/import.accept-plaintext-values/one_true",
    "QUERY/import.accept-plaintext-values/false_false",
    "QUERY/import.accept-plaintext-values/zero_false",
    "QUERY/import.accept-plaintext-values/uppercase_TRUE_reject",
    "QUERY/import.accept-plaintext-values/two_reject",
    "QUERY/import.accept-plaintext-values/null_text_reject",
    "QUERY/import.accept-plaintext-values/padded_true_reject",
    "QUERY/import.dry-run/missing_false",
    "QUERY/import.dry-run/bare_true",
    "QUERY/import.dry-run/empty_true",
    "QUERY/import.dry-run/true_true",
    "QUERY/import.dry-run/one_true",
    "QUERY/import.dry-run/false_false",
    "QUERY/import.dry-run/zero_false",
    "QUERY/import.dry-run/uppercase_TRUE_reject",
    "QUERY/import.dry-run/two_reject",
    "QUERY/import.dry-run/null_text_reject",
    "QUERY/import.dry-run/padded_true_reject",
    "QUERY/prune.dry-run/missing_false",
    "QUERY/prune.dry-run/bare_true",
    "QUERY/prune.dry-run/empty_true",
    "QUERY/prune.dry-run/true_true",
    "QUERY/prune.dry-run/one_true",
    "QUERY/prune.dry-run/false_false",
    "QUERY/prune.dry-run/zero_false",
    "QUERY/prune.dry-run/uppercase_TRUE_reject",
    "QUERY/prune.dry-run/two_reject",
    "QUERY/prune.dry-run/null_text_reject",
    "QUERY/prune.dry-run/padded_true_reject",
    "PREVIEW/create_populated/native_true",
    "PREVIEW/create_populated/legacy_one",
    "PREVIEW/update_populated/native_true",
    "PREVIEW/update_populated/legacy_one",
    "PREVIEW/typed_setting_populated/native_true",
    "PREVIEW/typed_setting_populated/legacy_one",
    "PREVIEW/environment_created_populated/native_true",
    "PREVIEW/environment_created_populated/legacy_one",
    "PREVIEW/environment_replaced_populated/native_true",
    "PREVIEW/environment_replaced_populated/legacy_one",
    "PREVIEW/environment_absent/native_true",
    "PREVIEW/environment_absent/legacy_one",
    "PREVIEW/create_absent/native_true",
    "PREVIEW/create_absent/legacy_one",
    "LIVE/create/native_false",
    "LIVE/create/missing",
    "LIVE/update/native_false",
    "LIVE/update/missing",
    "LIVE/typed_setting/native_false",
    "LIVE/typed_setting/missing",
    "LIVE/environment/native_false",
    "LIVE/environment/missing",
    "ENV-REFUSE/preview/invalid_name",
    "ENV-REFUSE/preview/reserved_LOCRON_name",
    "ENV-REFUSE/preview/NUL_value",
    "ENV-REFUSE/live/invalid_name",
    "ENV-REFUSE/live/reserved_LOCRON_name",
    "ENV-REFUSE/live/NUL_value",
    "BODY-REFUSE/create/number",
    "BODY-REFUSE/create/array",
    "BODY-REFUSE/create/object",
    "BODY-REFUSE/update/number",
    "BODY-REFUSE/update/array",
    "BODY-REFUSE/update/object",
    "BODY-REFUSE/settings/number",
    "BODY-REFUSE/settings/array",
    "BODY-REFUSE/settings/object",
    "WIN-ENV/case_insensitive_replaced_preview",
    "WIN-ENV/case_insensitive_reserved_name_refusal",
];

use axum::extract::Query;
use axum::http::Uri;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
struct Observation {
    attempts: u64,
    send_ok: u64,
    active: u64,
    completed: u64,
    invalid: bool,
}

type Observers = Mutex<BTreeMap<PathBuf, Arc<Mutex<Observation>>>>;
static OBSERVERS: OnceLock<Observers> = OnceLock::new();

fn observation(root: &Path) -> Option<Arc<Mutex<Observation>>> {
    OBSERVERS
        .get_or_init(Mutex::default)
        .lock()
        .expect("PR144 observer registry poisoned")
        .get(root)
        .cloned()
}

pub(super) fn wake(root: &Path) -> std::io::Result<()> {
    let observer = observation(root);
    if let Some(observer) = &observer {
        let mut state = observer.lock().expect("PR144 observation poisoned");
        if let Some(next) = state.attempts.checked_add(1) {
            state.attempts = next;
        } else {
            state.invalid = true;
        }
    }
    // Every call, including an unregistered root, delegates the original real producer.
    let result = locron_core::notification::send_wake(root);
    if result.is_ok()
        && let Some(observer) = observer
    {
        let mut state = observer.lock().expect("PR144 observation poisoned");
        if let Some(next) = state.send_ok.checked_add(1) {
            state.send_ok = next;
        } else {
            state.invalid = true;
        }
    }
    result
}

pub(super) struct Worker(Option<Arc<Mutex<Observation>>>);

pub(super) fn worker(root: &Path) -> Worker {
    let observer = observation(root);
    if let Some(observer) = &observer {
        let mut state = observer.lock().expect("PR144 observation poisoned");
        if let Some(next) = state.active.checked_add(1) {
            state.active = next;
        } else {
            state.invalid = true;
        }
    }
    Worker(observer)
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(observer) = &self.0 {
            let mut state = observer.lock().expect("PR144 observation poisoned");
            if let Some(next) = state.active.checked_sub(1) {
                state.active = next;
            } else {
                state.invalid = true;
            }
            if let Some(next) = state.completed.checked_add(1) {
                state.completed = next;
            } else {
                state.invalid = true;
            }
        }
    }
}

fn independent_definition(
    executable: &Path,
    cwd: &Path,
    nonce: &str,
    redacted: bool,
) -> Check<Value> {
    let input: Value = serde_json::from_str(include_str!(
        "../../../../docs/planning/DASHBOARD_BOOLEAN_FIXTURE_INPUTS.json"
    ))
    .map_err(|_| "selected independent inputs invalid")?;
    let mut definition = input["definition_literal"].clone();
    definition["target"]["executable"] = json!(executable);
    definition["cwd"] = json!(cwd);
    definition["environment"]["values"]["PR144_SECRET"] = json!(if redacted {
        "<redacted>".to_owned()
    } else {
        format!("new-{nonce}")
    });
    definition["environment"]["file"] = Value::Null;
    definition["environment"]["path"] = Value::Null;
    definition["completion_action"] = json!("retain");
    Ok(definition)
}

fn literal_definition(executable: &Path, cwd: &Path, nonce: &str) -> Check<Value> {
    let mut wire = independent_definition(executable, cwd, nonce, false)?;
    wire.as_object_mut()
        .ok_or("literal definition shape invalid")?
        .remove("completion_action");
    let environment = wire["environment"]
        .as_object_mut()
        .ok_or("literal environment shape invalid")?;
    environment.remove("file");
    environment.remove("path");
    Ok(wire)
}

fn create_body(definition: &Value) -> Value {
    json!({"name":"pr144-create","description":"pr144 fixture","tags":[],"enabled":true,"definition":definition})
}

fn parser_rows(executable: &Path, cwd: &Path, nonce: &str) -> Check<Vec<String>> {
    let origin = Instant::now();
    let deadline = origin
        .checked_add(Duration::from_secs(30))
        .ok_or("parser horizon overflow")?;
    let definition = independent_definition(executable, cwd, nonce, false)?;
    let wire_definition = literal_definition(executable, cwd, nonce)?;
    let wires: [Option<Value>; 19] = [
        None,
        Some(json!(true)),
        Some(json!(false)),
        Some(Value::Null),
        Some(json!("")),
        Some(json!("true")),
        Some(json!("1")),
        Some(json!("false")),
        Some(json!("0")),
        Some(json!(0)),
        Some(json!(1)),
        Some(json!(-1)),
        Some(json!(0.5)),
        Some(json!([])),
        Some(json!({})),
        Some(json!("yes")),
        Some(json!("TRUE")),
        Some(json!(" true ")),
        Some(json!("2")),
    ];
    let expected = [
        Some(false),
        Some(true),
        Some(false),
        Some(true),
        Some(true),
        Some(true),
        Some(true),
        Some(false),
        Some(false),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    ];
    let mut completed = Vec::new();
    for target in 0..3 {
        for (index, wire) in wires.iter().enumerate() {
            require(Instant::now() < deadline, "original parser horizon expired")?;
            let mut body = match target {
                0 => create_body(&wire_definition),
                1 => json!({"description":"would change"}),
                _ => json!({"value":"4"}),
            };
            if let Some(wire) = wire {
                body["dry_run"] = wire.clone();
            }
            let actual: Result<Check<bool>,serde_json::Error> = match target {
                0 => serde_json::from_value::<super::JobCreateRequest>(body).map(|request| {
                    require(request.name == "pr144-create" && request.description.as_deref() == Some("pr144 fixture")
                        && request.tags.is_empty() && request.enabled, "create companion fields changed")?;
                    require(serde_json::to_value(request.definition).map_err(|_| "parsed definition encode failed")? == definition, "complete definition defaults changed")?;
                    Ok(request.dry_run)
                }),
                1 => serde_json::from_value::<super::JobUpdateRequest>(body).map(|request| {
                    require(request.name.is_none() && request.tags.is_none() && request.enabled.is_none() && request.definition.is_none()
                        && matches!(request.description, super::DescriptionUpdate::Set(ref value) if value == "would change"), "update companion defaults changed")?;
                    Ok(request.dry_run)
                }),
                _ => serde_json::from_value::<super::SettingsPutRequest>(body).map(|request| {
                    require(request.value == "4", "setting companion changed")?;
                    Ok(request.dry_run)
                }),
            };
            match expected[index] {
                Some(expected) => require(
                    actual.map_err(|_| "valid actual request rejected")?? == expected,
                    "actual JSON flag mismatch",
                )?,
                None => require(actual.is_err(), "wrong JSON type or grammar accepted")?,
            }
            require(Instant::now() < deadline, "original parser horizon expired")?;
            completed.push(KEYS[target * 19 + index].to_owned());
        }
    }
    let fields = [
        "all",
        "wait",
        "dry-run",
        "acknowledge-unconfirmed",
        "include-values",
        "acknowledge-plaintext",
        "accept-plaintext-values",
        "dry-run",
        "dry-run",
    ];
    let suffixes = [
        None,
        Some(""),
        Some("="),
        Some("=true"),
        Some("=1"),
        Some("=false"),
        Some("=0"),
        Some("=TRUE"),
        Some("=2"),
        Some("=null"),
        Some("=%20true%20"),
    ];
    let expected = [
        Some(false),
        Some(true),
        Some(true),
        Some(true),
        Some(true),
        Some(false),
        Some(false),
        None,
        None,
        None,
        None,
    ];
    for (target, field) in fields.iter().enumerate() {
        for (index, suffix) in suffixes.iter().enumerate() {
            require(Instant::now() < deadline, "original parser horizon expired")?;
            let uri: Uri = match suffix {
                None => "/".to_owned(),
                Some(suffix) => format!("/?{field}{suffix}"),
            }
            .parse()
            .map_err(|_| "independent query URI invalid")?;
            let actual = query_flag(target, &uri);
            match expected[index] {
                Some(expected) => require(
                    actual.map_err(|_| "valid actual Query rejected")?? == expected,
                    "actual query flag mismatch",
                )?,
                None => require(actual.is_err(), "wrong query grammar accepted")?,
            }
            require(Instant::now() < deadline, "original parser horizon expired")?;
            completed.push(KEYS[57 + target * 11 + index].to_owned());
        }
    }
    require(completed.len() == 156, "parser row cardinality mismatch")?;
    Ok(completed)
}

fn query_flag(
    target: usize,
    uri: &Uri,
) -> Result<Check<bool>, axum::extract::rejection::QueryRejection> {
    match target {
        0 => Query::<super::ListJobsQuery>::try_from_uri(uri).map(|Query(value)| Ok(value.all)),
        1 | 2 => Query::<super::RunJobQuery>::try_from_uri(uri).map(|Query(value)| {
            require(
                if target == 1 {
                    !value.dry_run
                } else {
                    !value.wait
                },
                "run query companion changed",
            )?;
            Ok(if target == 1 {
                value.wait
            } else {
                value.dry_run
            })
        }),
        3 => Query::<super::CancelQuery>::try_from_uri(uri)
            .map(|Query(value)| Ok(value.acknowledge_unconfirmed)),
        4 | 5 => Query::<super::ExportQuery>::try_from_uri(uri).map(|Query(value)| {
            require(
                value.jobs.is_empty()
                    && value.tag.is_empty()
                    && if target == 4 {
                        !value.acknowledge_plaintext
                    } else {
                        !value.include_values
                    },
                "export companion defaults changed",
            )?;
            Ok(if target == 4 {
                value.include_values
            } else {
                value.acknowledge_plaintext
            })
        }),
        6 | 7 => Query::<super::ImportQuery>::try_from_uri(uri).map(|Query(value)| {
            require(
                if target == 6 {
                    !value.dry_run
                } else {
                    !value.accept_plaintext_values
                },
                "import query companion changed",
            )?;
            Ok(if target == 6 {
                value.accept_plaintext_values
            } else {
                value.dry_run
            })
        }),
        _ => Query::<super::PruneQuery>::try_from_uri(uri).map(|Query(value)| Ok(value.dry_run)),
    }
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

fn text(value: &str) -> Value {
    use std::fmt::Write as _;
    let mut bytes = String::new();
    for byte in value.bytes() {
        write!(bytes, "{byte:02x}").expect("String write cannot fail");
    }
    json!({"type":"text","value":bytes})
}

fn integer(value: i64) -> Value {
    json!({"type":"integer","value":value.to_string()})
}
fn null() -> Value {
    json!({"type":"null"})
}

fn integer_value(value: &Value) -> Check<i64> {
    require(value["type"] == "integer", "SQLite integer type mismatch")?;
    value["value"]
        .as_str()
        .ok_or("integer value missing")?
        .parse()
        .map_err(|_| "integer value invalid")
}

fn text_value(value: &Value) -> Check<String> {
    require(value["type"] == "text", "SQLite text type mismatch")?;
    let encoded = value["value"].as_str().ok_or("text bytes missing")?;
    require(
        encoded.len().is_multiple_of(2) && encoded.len() <= CELL_CAP * 2,
        "text encoding cap or length invalid",
    )?;
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let encoded = std::str::from_utf8(pair).map_err(|_| "hex encoding invalid")?;
            u8::from_str_radix(encoded, 16).map_err(|_| "hex byte invalid")
        })
        .collect::<Check<Vec<_>>>()?;
    String::from_utf8(bytes).map_err(|_| "fixture text UTF8 invalid")
}

fn rows<'a>(logical: &'a Value, table: &str) -> Check<&'a Vec<Value>> {
    logical["tables"][table]["rows"]
        .as_array()
        .ok_or("snapshot table rows missing")
}

fn column(logical: &Value, table: &str, name: &str) -> Check<usize> {
    let columns = logical["tables"][table]["columns"]
        .as_array()
        .ok_or("snapshot columns missing")?;
    let mut found = None;
    for row in columns {
        let cells = row.as_array().ok_or("column row invalid")?;
        require(cells.len() == 7, "table_xinfo column count changed")?;
        if text_value(&cells[1])? == name {
            require(found.is_none(), "duplicate column identity")?;
            found = Some(
                usize::try_from(integer_value(&cells[0])?).map_err(|_| "column ordinal invalid")?,
            );
        }
    }
    found.ok_or("required column absent")
}

fn field<'a>(logical: &Value, table: &str, row: &'a Value, name: &str) -> Check<&'a Value> {
    row.as_array()
        .ok_or("typed row invalid")?
        .get(column(logical, table, name)?)
        .ok_or("typed column missing")
}

fn row_with<'a>(logical: &'a Value, table: &str, name: &str, value: &Value) -> Check<&'a Value> {
    let mut found = None;
    for row in rows(logical, table)? {
        if field(logical, table, row, name)? == value {
            require(found.is_none(), "duplicate target row")?;
            found = Some(row);
        }
    }
    found.ok_or("required target row absent")
}

fn put(logical: &mut Value, table: &str, row_index: usize, name: &str, value: Value) -> Check<()> {
    let index = column(logical, table, name)?;
    logical["tables"][table]["rows"][row_index][index] = value;
    Ok(())
}

fn insert_row(logical: &mut Value, table: &str, fields: &[(&str, Value)]) -> Check<()> {
    let count = logical["tables"][table]["columns"]
        .as_array()
        .ok_or("columns absent")?
        .len();
    require(
        count == fields.len(),
        "independent expected row does not cover every column",
    )?;
    let mut cells = vec![Value::Null; count];
    let mut names = BTreeSet::new();
    for (name, value) in fields {
        let index = column(logical, table, name)?;
        require(
            index < count && names.insert(*name),
            "independent expected column duplicate",
        )?;
        cells[index] = value.clone();
    }
    require(
        cells.iter().all(|value| !value.is_null()),
        "independent row column missing",
    )?;
    logical["tables"][table]["rows"]
        .as_array_mut()
        .ok_or("rows absent")?
        .push(json!(cells));
    Ok(())
}

fn canonical_rows(logical: &mut Value) -> Check<()> {
    for table in logical["tables"]
        .as_object_mut()
        .ok_or("tables absent")?
        .values_mut()
    {
        let rows = table["rows"].as_array_mut().ok_or("rows absent")?;
        let mut encoded = rows
            .iter()
            .map(|row| {
                Ok((
                    serde_json::to_vec(row).map_err(|_| "typed row encode failed")?,
                    row.clone(),
                ))
            })
            .collect::<Check<Vec<_>>>()?;
        encoded.sort_by(|left, right| left.0.cmp(&right.0));
        *rows = encoded.into_iter().map(|(_, row)| row).collect();
    }
    Ok(())
}

fn baseline(ticket: &Ticket, logical: &Value) -> Check<()> {
    require(
        rows(logical, "schema_migrations")?.len() == 5
            && rows(logical, "jobs")?.len() == 2
            && rows(logical, "job_revisions")?.len() == 2
            && rows(logical, "schedule_cursors")?.len() == 2
            && rows(logical, "runs")?.len() == 1,
        "full populated fixture cardinality changed",
    )?;
    let setting = row_with(logical, "settings", "singleton", &integer(1))?;
    require(
        field(logical, "settings", setting, "global_concurrency")? == &integer(3)
            && field(logical, "settings", setting, "updated_at_us")? == &integer(100),
        "typed setting baseline changed",
    )?;
    let mut environment = json!({"PR144_OTHER":format!("other-{}",ticket.nonce)});
    if (164..166).contains(&ticket.row) || (176..178).contains(&ticket.row) {
        environment["PR144_NAME"] = json!(format!("old-{}", ticket.nonce));
    }
    if ticket.row == 193 {
        environment["Pr144_Name"] = json!(format!("old-{}", ticket.nonce));
    }
    require(
        field(logical, "settings", setting, "environment_json")?
            == &text(
                &serde_json::to_string(&environment)
                    .map_err(|_| "expected environment encode failed")?,
            ),
        "raw case or baseline environment changed",
    )?;
    let job = row_with(logical, "jobs", "id", &text(SEED_ID))?;
    for (name, value) in [
        ("name", text("pr144-seed")),
        ("description", text("seed description")),
        ("tags_json", text("[\"pr144\"]")),
        ("enabled", integer(1)),
        ("created_at_us", integer(100)),
        ("updated_at_us", integer(100)),
        ("removed_at_us", null()),
        ("current_revision", integer(1)),
    ] {
        require(
            field(logical, "jobs", job, name)? == &value,
            "independent job seed changed",
        )?;
    }
    let revision = row_with(logical, "job_revisions", "job_id", &text(SEED_ID))?;
    require(
        field(logical, "job_revisions", revision, "definition_json")?
            == &text(&durable_definition(&independent_definition(
                &ticket.server.path,
                &ticket.state,
                &ticket.nonce,
                false,
            )?)?),
        "independent seed definition changed",
    )?;
    let run = &rows(logical, "runs")?[0];
    require(
        field(logical, "runs", run, "job_id")? == &text("01900000-0000-7000-8000-000000000002")
            && field(logical, "runs", run, "state")? == &text("queued"),
        "unrelated queued seed changed",
    )?;
    let admission = row_with(logical, "admission_state", "singleton", &integer(1))?;
    require(
        field(logical, "admission_state", admission, "next_queue_sequence")? == &integer(2),
        "admission sequence seed changed",
    )
}

const SEED_ID: &str = "01900000-0000-7000-8000-000000000001";

fn measured_time(value: &Value, before: u64, after: u64) -> Check<i64> {
    let time = integer_value(value)?;
    let time_u64 = u64::try_from(time).map_err(|_| "production time negative")?;
    require(
        (before..=after).contains(&time_u64),
        "production timestamp outside actual request bounds",
    )?;
    Ok(time)
}

fn live_expected(
    ticket: &Ticket,
    before: &Value,
    after: &Value,
    bounds: (u64, u64),
) -> Check<Value> {
    let mut expected = before.clone();
    match ticket.row {
        170..=173 => {
            let create = ticket.row <= 171;
            let job = if create {
                row_with(after, "jobs", "name", &text("pr144-create"))?
            } else {
                row_with(after, "jobs", "id", &text(SEED_ID))?
            };
            let id = text_value(field(after, "jobs", job, "id")?)?;
            if create {
                let uuid = uuid::Uuid::parse_str(&id).map_err(|_| "generated UUID invalid")?;
                require(
                    uuid.get_version_num() == 7 && uuid.to_string() == id && id != SEED_ID,
                    "generated UUID canonical v7 mismatch",
                )?;
            } else {
                require(id == SEED_ID, "update changed job identity")?;
            }
            let time = measured_time(
                field(after, "jobs", job, "updated_at_us")?,
                bounds.0,
                bounds.1,
            )?;
            let revision = if create { 1 } else { 2 };
            if create {
                insert_row(
                    &mut expected,
                    "jobs",
                    &[
                        ("id", text(&id)),
                        ("name", text("pr144-create")),
                        ("description", text("pr144 fixture")),
                        ("tags_json", text("[]")),
                        ("enabled", integer(1)),
                        ("created_at_us", integer(time)),
                        ("updated_at_us", integer(time)),
                        ("removed_at_us", null()),
                        ("current_revision", integer(1)),
                    ],
                )?;
            } else {
                let index = rows(&expected, "jobs")?
                    .iter()
                    .position(|row| {
                        field(&expected, "jobs", row, "id")
                            .is_ok_and(|value| value == &text(SEED_ID))
                    })
                    .ok_or("update target absent")?;
                put(
                    &mut expected,
                    "jobs",
                    index,
                    "description",
                    text("would change"),
                )?;
                put(&mut expected, "jobs", index, "updated_at_us", integer(time))?;
                put(&mut expected, "jobs", index, "current_revision", integer(2))?;
            }
            let definition = durable_definition(&independent_definition(
                &ticket.server.path,
                &ticket.state,
                &ticket.nonce,
                false,
            )?)?;
            insert_row(
                &mut expected,
                "job_revisions",
                &[
                    ("job_id", text(&id)),
                    ("revision", integer(revision)),
                    ("definition_json", text(&definition)),
                    ("created_at_us", integer(time)),
                    ("created_by", text(if create { "add" } else { "update" })),
                ],
            )?;
            insert_row(
                &mut expected,
                "schedule_cursors",
                &[
                    ("job_id", text(&id)),
                    ("revision", integer(revision)),
                    ("cursor_us", integer(if create { time } else { 100 })),
                    ("interval_anchor_us", null()),
                    ("one_time_resolved", integer(0)),
                    ("updated_at_us", integer(time)),
                    ("disabled_since_us", null()),
                ],
            )?;
            let sequence = row_with(before, "sqlite_sequence", "name", &text("events"))?;
            let next = integer_value(field(before, "sqlite_sequence", sequence, "seq")?)?
                .checked_add(1)
                .ok_or("event sequence overflow")?;
            insert_row(
                &mut expected,
                "events",
                &[
                    ("id", integer(next)),
                    ("occurred_at_us", integer(time)),
                    (
                        "kind",
                        text(if create { "job_added" } else { "job_updated" }),
                    ),
                    ("job_id", text(&id)),
                    ("run_id", null()),
                    (
                        "details_json",
                        text(if create { "{}" } else { "{\"revision\":2}" }),
                    ),
                ],
            )?;
            let index = rows(&expected, "sqlite_sequence")?
                .iter()
                .position(|row| {
                    field(&expected, "sqlite_sequence", row, "name")
                        .is_ok_and(|value| value == &text("events"))
                })
                .ok_or("event sequence absent")?;
            put(
                &mut expected,
                "sqlite_sequence",
                index,
                "seq",
                integer(next),
            )?;
        }
        174..=177 => {
            let setting = row_with(after, "settings", "singleton", &integer(1))?;
            let time = measured_time(
                field(after, "settings", setting, "updated_at_us")?,
                bounds.0,
                bounds.1,
            )?;
            put(&mut expected, "settings", 0, "updated_at_us", integer(time))?;
            if ticket.row <= 175 {
                put(
                    &mut expected,
                    "settings",
                    0,
                    "global_concurrency",
                    integer(4),
                )?;
            } else {
                let old = row_with(before, "settings", "singleton", &integer(1))?;
                let mut environment: Value = serde_json::from_str(&text_value(field(
                    before,
                    "settings",
                    old,
                    "environment_json",
                )?)?)
                .map_err(|_| "baseline environment invalid")?;
                environment["PR144_NAME"] = json!(format!("new-{}", ticket.nonce));
                put(
                    &mut expected,
                    "settings",
                    0,
                    "environment_json",
                    text(
                        &serde_json::to_string(&environment)
                            .map_err(|_| "expected environment encode failed")?,
                    ),
                )?;
            }
        }
        _ => return Err("live row outside selected set"),
    }
    canonical_rows(&mut expected)?;
    require(
        expected == *after,
        "full independent durable delta or unrelated logical state mismatch",
    )?;
    Ok(expected)
}

fn request(ticket: &Ticket) -> Check<(reqwest::Method, String, Value)> {
    let row = ticket.row;
    let key = KEYS[row];
    let definition = literal_definition(&ticket.server.path, &ticket.state, &ticket.nonce)?;
    let mut body = if key.contains("/create") {
        create_body(&definition)
    } else if key.contains("/update") {
        json!({"description":"would change"})
    } else if key.contains("typed_setting") || key.starts_with("BODY-REFUSE/settings/") {
        json!({"value":"4"})
    } else {
        json!({"value":format!("new-{}",ticket.nonce)})
    };
    if key.ends_with("legacy_one") {
        body["dry_run"] = json!("1");
    } else if key.ends_with("native_false") || key.starts_with("ENV-REFUSE/live/") {
        body["dry_run"] = json!(false);
    } else if !key.ends_with("/missing") {
        body["dry_run"] = json!(true);
    }
    let mut environment = "PR144_NAME";
    if key.ends_with("invalid_name") {
        environment = "BAD-NAME";
    }
    if key.ends_with("reserved_LOCRON_name") {
        environment = "LOCRON_QUALIFICATION";
    }
    if row == 194 {
        environment = "lOcRoN_QUALIFICATION";
    }
    if key.ends_with("NUL_value") {
        body["value"] = json!("\u{0000}");
    }
    if key.starts_with("BODY-REFUSE/") {
        body["dry_run"] = match row % 3 {
            1 => json!(0),
            2 => json!([]),
            _ => json!({}),
        };
    }
    let (method, path) = if key.contains("/create") {
        (reqwest::Method::POST, "/api/v1/jobs".to_owned())
    } else if key.contains("/update") {
        (
            reqwest::Method::PUT,
            format!(
                "/api/v1/jobs/{}",
                if populated(row) {
                    SEED_ID
                } else {
                    "never-stored-reference"
                }
            ),
        )
    } else if key.contains("typed_setting") || key.starts_with("BODY-REFUSE/settings/") {
        (
            reqwest::Method::PUT,
            "/api/v1/settings/global_concurrency".to_owned(),
        )
    } else {
        (
            reqwest::Method::PUT,
            format!("/api/v1/settings/environment.{environment}"),
        )
    };
    Ok((method, path, body))
}

fn response_expected(ticket: &Ticket, data: &Value, logical: Option<&Value>) -> Check<()> {
    let key = KEYS[ticket.row];
    let definition =
        independent_definition(&ticket.server.path, &ticket.state, &ticket.nonce, true)?;
    let expected = if key.contains("/create") && key.starts_with("PREVIEW/") {
        json!({"dry_run":true,"id":"<non-durable>","name":"pr144-create","description":"pr144 fixture","tags":[],"enabled":true,"definition":definition})
    } else if key.contains("/update") && key.starts_with("PREVIEW/") {
        let before = json!({"name":"pr144-seed","description":"seed description","tags":["pr144"],"enabled":true,"definition":definition});
        let mut after = before.clone();
        after["description"] = json!("would change");
        json!({"dry_run":true,"id":SEED_ID,"revision":2,"schedule_changed":false,"changed_fields":["description"],"before":before,"after":after,"cursor_us":100})
    } else if key.contains("typed_setting") && key.starts_with("PREVIEW/") {
        json!({"key":"global_concurrency","value":"4","dry_run":true})
    } else if (170..174).contains(&ticket.row) {
        let logical = logical.ok_or("live snapshot absent")?;
        let create = ticket.row <= 171;
        let job = if create {
            row_with(logical, "jobs", "name", &text("pr144-create"))?
        } else {
            row_with(logical, "jobs", "id", &text(SEED_ID))?
        };
        let time = integer_value(field(logical, "jobs", job, "updated_at_us")?)?;
        json!({"id":text_value(field(logical,"jobs",job,"id")?)?,"name":if create {"pr144-create"} else {"pr144-seed"},
            "description":if create {"pr144 fixture"} else {"would change"},"tags_json":if create {"[]"} else {"[\"pr144\"]"},"enabled":true,
            "removed_at_us":Value::Null,"current_revision":if create {1} else {2},"definition_json":serde_json::to_string(&definition).map_err(|_| "redacted expected definition encode failed")?,
            "cursor_us":if create {time} else {100},"updated_at_us":time,"cursor_updated_at_us":time,"disabled_since_us":Value::Null})
    } else if (174..176).contains(&ticket.row) {
        let logical = logical.ok_or("live settings snapshot absent")?;
        let setting = row_with(logical, "settings", "singleton", &integer(1))?;
        let environment: Value = serde_json::from_str(&text_value(field(
            logical,
            "settings",
            setting,
            "environment_json",
        )?)?)
        .map_err(|_| "settings environment invalid")?;
        let mut redacted = json!({});
        for name in environment
            .as_object()
            .ok_or("environment map invalid")?
            .keys()
        {
            redacted[name] = json!({"configured":true,"value_redacted":true});
        }
        json!({"global_concurrency":4,"execution_path":text_value(field(logical,"settings",setting,"execution_path")?)?,
            "run_retention_count":integer_value(field(logical,"settings",setting,"run_retention_count")?)?,
            "run_retention_age_us":integer_value(field(logical,"settings",setting,"run_retention_age_us")?)?,
            "output_limit_bytes":integer_value(field(logical,"settings",setting,"output_limit_bytes")?)?,
            "per_run_output_limit_bytes":integer_value(field(logical,"settings",setting,"per_run_output_limit_bytes")?)?,"environment":redacted})
    } else {
        json!({"key":"environment.PR144_NAME","action":if (164..166).contains(&ticket.row) || (176..178).contains(&ticket.row) || ticket.row == 193 {"replaced"} else {"created"},
            "configured":true,"value_redacted":true,"dry_run": !(170..178).contains(&ticket.row)})
    };
    // No assertion formatter ever prints expected/actual secret-bearing data.
    require(
        *data == expected,
        "independent whole HTTP data or redaction mismatch",
    )
}

async fn wait_json(path: &Path, cap: usize, clock: &Clock, horizon: u64) -> Check<Value> {
    loop {
        clock.check(horizon)?;
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::time::sleep(Duration::from_millis(5)).await
            }
            Err(_) => return Err("receipt inspection failed"),
            Ok(meta) => {
                require(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "receipt type refused",
                )?;
                let value = read_json(path, cap)?;
                clock.check(horizon)?;
                return Ok(value);
            }
        }
    }
}

async fn readonly_snapshot(
    ticket: &Ticket,
    pid: u32,
    sequence: u64,
    clock: &Clock,
) -> Check<Value> {
    clock.check(87)?;
    publish(
        &ticket.parent.join(format!("store-command-{sequence}.json")),
        &json!({"nonce":ticket.nonce,"pid":pid,"sequence":sequence,"operation":"snapshot"}),
        CONTROL_CAP,
    )?;
    let snapshot = wait_json(
        &ticket.parent.join(format!("snapshot-{sequence}.json")),
        SNAPSHOT_CAP,
        clock,
        87,
    )
    .await?;
    validate_receipt(&snapshot["receipt"], ticket, pid, sequence, "snapshot")?;
    require(
        snapshot["readonly_transaction_completed"] == true
            && snapshot["readonly_connection_closed"] == true,
        "actual readonly completion missing",
    )?;
    Ok(snapshot)
}

fn absence(state: &Path) -> Check<()> {
    for name in [
        "state.db",
        "state.db-wal",
        "state.db-shm",
        "state.db-journal",
    ] {
        match fs::symlink_metadata(state.join(name)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err("absent database or sidecar created"),
        }
    }
    Ok(())
}

fn sha256(file: &GuardedFile) -> Check<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = file.try_clone().map_err(|_| "artifact clone failed")?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "artifact seek failed")?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .map_err(|_| "artifact hashing failed")?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn non_sqlite(ticket: &Ticket) -> Check<Value> {
    let mut inventory = json!({});
    for name in [
        "sentinel",
        "dashboard.token",
        "dashboard.token.lock",
        "outputs/owned-output",
    ] {
        let path = ticket.state.join(name);
        let file = private_read(&path)?;
        inventory[name] = json!({"identity":identity(&file)?,"sha256":sha256(&file)?,"size":file.metadata().map_err(|_| "sentinel metadata failed")?.len()});
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let meta = fs::symlink_metadata(ticket.state.join("wake.sock"))
            .map_err(|_| "socket metadata failed")?;
        require(meta.file_type().is_socket(), "wake socket type changed")?;
        inventory["wake.sock"] =
            json!({"type":"socket","dev":meta.dev().to_string(),"ino":meta.ino().to_string()});
    }
    Ok(inventory)
}

fn sqlite_diagnostics(state: &Path, clock: &Clock) -> Check<Value> {
    let mut result = json!({});
    for name in [
        "state.db",
        "state.db-wal",
        "state.db-shm",
        "state.db-journal",
    ] {
        clock.check(87)?;
        let path = state.join(name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                result[name] = json!({"present":false});
            }
            Err(_) => return Err("SQLite diagnostics inspection failed"),
            Ok(meta) => {
                require(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "SQLite diagnostic leaf type refused",
                )?;
                let file = private_read(&path)?;
                result[name] = json!({"present":true,"identity":identity(&file)?,"size":file.metadata().map_err(|_| "SQLite diagnostic size failed")?.len(),"sha256":sha256(&file)?});
            }
        }
        clock.check(87)?;
    }
    Ok(result)
}

async fn http_child() -> Check<()> {
    let admission = admit("server")?;
    let ticket = &admission.ticket;
    let clock = &admission.clock;
    require(
        sha256(&admission.artifact)? == ticket.server.sha256,
        "actual Server artifact SHA mismatch",
    )?;
    let paths = locron_store::StatePaths::new(ticket.state.clone());
    let lock_path = ticket.state.join("dashboard.token.lock");
    let mut lock_file = private_read(&lock_path)?;
    let lock_id = identity(&lock_file)?;
    require(
        lock_file
            .metadata()
            .map_err(|_| "prepared permanent lock metadata failed")?
            .len()
            == 0
            && lock_file
                .read(&mut [0; 1])
                .map_err(|_| "prepared permanent lock read failed")?
                == 0,
        "prepared permanent lock was not empty",
    )?;
    drop(lock_file);
    let token = crate::token::ensure(&paths).map_err(|_| "owned token issuer failed")?;
    let mut lock_file = private_read(&lock_path)?;
    require(
        identity(&lock_file)? == lock_id,
        "prepared permanent lock identity changed",
    )?;
    require(
        lock_file
            .metadata()
            .map_err(|_| "reused permanent lock metadata failed")?
            .len()
            == 0
            && lock_file
                .read(&mut [0; 1])
                .map_err(|_| "reused permanent lock read failed")?
                == 0,
        "reused permanent lock was not empty",
    )?;
    drop(lock_file);
    let token_file = private_read(&crate::token::token_path(&paths))?;
    record_owned(&crate::token::token_path(&paths), &identity(&token_file)?)?;
    drop(token_file);
    let store_pid = if populated(ticket.row) {
        let start = wait_json(
            &ticket.parent.join("store-start.json"),
            CONTROL_CAP,
            clock,
            30,
        )
        .await?;
        require(
            start["nonce"] == ticket.nonce,
            "actual Store spawn nonce mismatch",
        )?;
        Some(
            u32::try_from(start["pid"].as_u64().ok_or("actual Store pid absent")?)
                .map_err(|_| "actual Store pid overflow")?,
        )
    } else {
        absence(&ticket.state)?;
        None
    };
    let before = match store_pid {
        Some(pid) => Some(readonly_snapshot(ticket, pid, 1, clock).await?),
        None => None,
    };
    if let Some(before) = &before {
        baseline(ticket, &before["logical"])?;
    }
    let files = non_sqlite(ticket)?;
    let physical_before = sqlite_diagnostics(&ticket.state, clock)?;
    let observed = Arc::new(Mutex::new(Observation::default()));
    require(
        OBSERVERS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "observer registry poisoned")?
            .insert(ticket.state.clone(), Arc::clone(&observed))
            .is_none(),
        "duplicate root observer",
    )?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "real loopback bind failed")?;
    let address = listener
        .local_addr()
        .map_err(|_| "actual bound address failed")?;
    let state = crate::AppState {
        paths,
        token: token.clone(),
        bound_port: address.port(),
    };
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, crate::router(state))
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|_| "real client build failed")?;
    clock.check(30)?;
    publish(
        &ticket.parent.join("server-ready.json"),
        &receipt(ticket, 0, "ready"),
        CONTROL_CAP,
    )?;
    clock.check(87)?;
    let (method, path, body) = request(ticket)?;
    let time_before = wall_us()?;
    let mut response = client
        .request(method, format!("http://{address}{path}"))
        .header("Authorization", format!("token {token}"))
        .header("Connection", "close")
        .json(&body)
        .timeout(
            clock
                .until(87)?
                .checked_duration_since(Instant::now())
                .ok_or("HTTP horizon expired")?,
        )
        .send()
        .await
        .map_err(|_| "actual HTTP request failed")?;
    let status = response.status().as_u16();
    require(
        response
            .headers()
            .get("referrer-policy")
            .is_some_and(|value| value == "no-referrer")
            && !response.headers().contains_key("set-cookie"),
        "actual response header contract changed",
    )?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "actual HTTP response read failed")?
    {
        require(
            bytes
                .len()
                .checked_add(chunk.len())
                .is_some_and(|size| size <= RESPONSE_CAP),
            "HTTP response cap exceeded",
        )?;
        bytes.extend_from_slice(&chunk);
    }
    let time_after = wall_us()?;
    clock.check(87)?;
    let new_canary = format!("new-{}", ticket.nonce);
    let old_canary = format!("old-{}", ticket.nonce);
    require(
        !bytes
            .windows(new_canary.len())
            .any(|value| value == new_canary.as_bytes())
            && !bytes
                .windows(old_canary.len())
                .any(|value| value == old_canary.as_bytes())
            && !bytes
                .windows(token.len())
                .any(|value| value == token.as_bytes()),
        "actual HTTP response leaked canary or token",
    )?;
    if !populated(ticket.row) {
        absence(&ticket.state)?;
    }
    let after = match store_pid {
        Some(pid) => Some(readonly_snapshot(ticket, pid, 2, clock).await?),
        None => None,
    };
    let physical_after = sqlite_diagnostics(&ticket.state, clock)?;
    require(
        files == non_sqlite(ticket)?,
        "non-SQLite leaf identity or bytes changed",
    )?;
    if let (Some(before), Some(after)) = (&before, &after) {
        if (170..178).contains(&ticket.row) {
            live_expected(
                ticket,
                &before["logical"],
                &after["logical"],
                (time_before, time_after),
            )?;
        } else {
            require(
                before["logical"] == after["logical"],
                "preview/refusal changed complete logical state",
            )?;
        }
    }
    let rejection = (178..184).contains(&ticket.row) || ticket.row == 194;
    let body_rejection = (184..193).contains(&ticket.row);
    if body_rejection {
        require(
            status == 422 && !bytes.is_empty(),
            "actual Axum JsonDataError422 missing",
        )?;
        require(
            std::str::from_utf8(&bytes).is_ok_and(|body| {
                body.starts_with("Failed to deserialize the JSON body into the target type:")
            }),
            "actual framework rejection body changed",
        )?;
    } else {
        let envelope: Value =
            serde_json::from_slice(&bytes).map_err(|_| "actual response JSON invalid")?;
        if rejection {
            let message = if KEYS[ticket.row].ends_with("NUL_value") {
                "environment value for PR144_NAME contains NUL"
            } else if KEYS[ticket.row].ends_with("invalid_name") {
                "invalid or reserved environment name BAD-NAME"
            } else if ticket.row == 194 {
                "invalid or reserved environment name lOcRoN_QUALIFICATION"
            } else {
                "invalid or reserved environment name LOCRON_QUALIFICATION"
            };
            require(
                status == 400
                    && envelope
                        == json!({"schema":"locron.api/v1","ok":false,"error":{"code":"invalid_request","message":message}}),
                "actual refusal status or whole envelope mismatch",
            )?;
        } else {
            require(
                status == 200
                    && envelope.as_object().is_some_and(|object| object.len() == 4)
                    && envelope["schema"] == "locron.api/v1"
                    && envelope["ok"] == true
                    && envelope["warnings"] == json!([]),
                "actual success status or envelope mismatch",
            )?;
            response_expected(
                ticket,
                &envelope["data"],
                after.as_ref().map(|value| &value["logical"]),
            )?;
        }
    }
    drop(response);
    drop(client);
    stop.send(())
        .map_err(|()| "server graceful stop receiver missing")?;
    tokio::time::timeout_at(tokio::time::Instant::from_std(clock.until(90)?), server)
        .await
        .map_err(|_| "actual server join horizon expired")?
        .map_err(|_| "actual server task failed")?
        .map_err(|_| "actual server close failed")?;
    let observation = observed.lock().map_err(|_| "observer poisoned")?;
    let live = (170..178).contains(&ticket.row);
    require(
        !observation.invalid
            && observation.active == 0
            && observation.completed == u64::from(!body_rejection)
            && observation.attempts == u64::from(live)
            && observation.send_ok == u64::from(live),
        "actual wake attempt/delegate or blocking worker completion mismatch",
    )?;
    let mut done = receipt(ticket, 1, "completed");
    done["http_status"] = json!(status);
    done["attempts"] = json!(observation.attempts);
    done["send_ok"] = json!(observation.send_ok);
    done["blocking_active"] = json!(0);
    done["blocking_completed"] = json!(observation.completed);
    done["server_joined"] = json!(true);
    done["sqlite_physical_diagnostics"] =
        json!({"before":physical_before,"after":physical_after,"equality_oracle":false});
    done["response_sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    if let (Some(before), Some(after)) = (&before, &after) {
        done["logical_before_sha256"] = json!(format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&before["logical"]).map_err(|_| "logical encode failed")?
            )
        ));
        done["logical_after_sha256"] = json!(format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&after["logical"]).map_err(|_| "logical encode failed")?
            )
        ));
    }
    drop(observation);
    require(
        OBSERVERS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "observer registry poisoned")?
            .remove(&ticket.state)
            .is_some(),
        "root observer completion missing",
    )?;
    clock.check(90)?;
    publish_done(&ticket.parent.join("server-done.json"), done, CONTROL_CAP)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_http_fixture_child() {
    if std::env::var_os("LOCRON_PR144_TICKET").is_none() {
        return;
    }
    if let Err(message) = http_child().await {
        panic!("PR144 owned HTTP child failed: {message}; private root retained");
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltArtifact {
    path: PathBuf,
    sha256: String,
    crc32: String,
    package_id: String,
    target_kind: Vec<String>,
    profile_test: bool,
    features: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildFacts {
    schema: String,
    checkout: String,
    run: String,
    attempt: String,
    job: String,
    compiler: String,
    host: String,
    target: String,
    store: BuiltArtifact,
    controller: Option<BuiltArtifact>,
}

fn open_artifact(path: &Path) -> Check<GuardedFile> {
    #[cfg(windows)]
    let file = locron_core::filesystem::read_owned_executable(path)
        .map_err(|_| "owned candidate executable refused")?;
    #[cfg(unix)]
    let file = locron_core::filesystem::open_read_no_follow(path)
        .map_err(|_| "candidate executable refused")?;
    Ok(file)
}

fn binding(fact: &BuiltArtifact) -> Check<(Artifact, GuardedFile)> {
    // Admit the passed compiler path before canonicalizing its native spelling.
    let file = open_artifact(&fact.path)?;
    let path = fs::canonicalize(&fact.path).map_err(|_| "actual compiler artifact missing")?;
    require(
        fact.path.is_absolute() && !fact.package_id.is_empty() && !fact.target_kind.is_empty(),
        "compiler artifact binding incomplete",
    )?;
    require(
        sha256(&file)? == fact.sha256,
        "actual compiled artifact SHA mismatch",
    )?;
    Ok((
        Artifact {
            path,
            identity: identity(&file)?,
            sha256: fact.sha256.clone(),
            crc32: fact.crc32.clone(),
        },
        file,
    ))
}

fn owned_file(path: &Path, bytes: &[u8]) -> Check<String> {
    let mut file = create_private_new(path).map_err(|_| "exclusive owned file creation failed")?;
    file.write_all(bytes)
        .map_err(|_| "owned file write failed")?;
    file.sync_all().map_err(|_| "owned file sync failed")?;
    let id = identity(&file)?;
    record_owned(path, &id)?;
    Ok(id)
}

struct FreshRoot {
    parent: PathBuf,
    state: PathBuf,
    parent_guard: DirectoryGuard,
    state_guard: DirectoryGuard,
    subdirectories: Vec<DirectoryGuard>,
    #[cfg(unix)]
    directory_identities: BTreeMap<PathBuf, (u64, u64)>,
    parent_anchor: String,
    state_anchor: String,
}

fn fresh_root(nonce: &str) -> Check<FreshRoot> {
    let base = fs::canonicalize(
        std::env::var_os("RUNNER_TEMP").ok_or("required hosted fixture base missing")?,
    )
    .map_err(|_| "hosted fixture base canonicalization failed")?;
    let base_guard =
        DirectoryGuard::ancestors(&base).map_err(|_| "fixture parent chain refused")?;
    let temporary = tempfile::Builder::new()
        .prefix("locron-pr144-")
        .tempdir_in(base_guard.normalized_path())
        .map_err(|_| "fresh mkdtemp failed")?;
    // Immediate keep: a failed/foreign/in-flight root must never be recursively removed by Drop.
    let parent = temporary.keep();
    let parent_guard =
        DirectoryGuard::private(&parent).map_err(|_| "initial parent privacy failed")?;
    let parent = parent_guard.normalized_path().to_path_buf();
    let state_guard = DirectoryGuard::private(&parent.join("state"))
        .map_err(|_| "initial state privacy failed")?;
    let state = state_guard.normalized_path().to_path_buf();
    let mut subdirectories = Vec::new();
    for name in ["outputs", "tmp"] {
        subdirectories.push(
            DirectoryGuard::private(&state.join(name))
                .map_err(|_| "initial owned subdirectory failed")?,
        );
    }
    #[cfg(unix)]
    let directory_identities = {
        let mut identities = BTreeMap::new();
        for path in [&parent, &state, &state.join("outputs"), &state.join("tmp")] {
            let meta =
                fs::symlink_metadata(path).map_err(|_| "created directory identity failed")?;
            require(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "created directory type refused",
            )?;
            identities.insert(path.clone(), (meta.dev(), meta.ino()));
        }
        identities
    };
    let parent_anchor = owned_file(&parent.join("anchor"), format!("{nonce}:parent").as_bytes())?;
    let state_anchor = owned_file(&state.join("sentinel"), format!("{nonce}:state").as_bytes())?;
    owned_file(
        &state.join("outputs/owned-output"),
        format!("{nonce}:output").as_bytes(),
    )?;
    Ok(FreshRoot {
        parent,
        state,
        parent_guard,
        state_guard,
        subdirectories,
        #[cfg(unix)]
        directory_identities,
        parent_anchor,
        state_anchor,
    })
}

fn exact_entries(path: &Path, allowed: &BTreeSet<String>) -> Check<Vec<(PathBuf, String)>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(path).map_err(|_| "cleanup inventory failed")? {
        let entry = entry.map_err(|_| "cleanup entry failed")?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "cleanup leaf name invalid")?;
        require(
            allowed.contains(&name),
            "foreign or unexpected leaf retained",
        )?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| "cleanup metadata failed")?;
        require(
            meta.is_file() && !meta.file_type().is_symlink(),
            "cleanup leaf type changed",
        )?;
        let file = private_read(&entry.path())?;
        result.push((entry.path(), identity(&file)?));
    }
    Ok(result)
}

fn remove_leaves(leaves: &[(PathBuf, String)], clock: Option<&Clock>) -> Check<()> {
    for (path, expected) in leaves {
        if let Some(clock) = clock {
            clock.check(90)?;
        }
        let file = private_read(path)?;
        require(
            identity(&file)? == *expected,
            "cleanup leaf replaced; retained",
        )?;
        drop(file);
        fs::remove_file(path).map_err(|_| "identity-checked leaf cleanup failed")?;
        if let Some(clock) = clock {
            clock.check(90)?;
        }
    }
    Ok(())
}

fn cleanup(
    root: FreshRoot,
    row: Option<usize>,
    clock: Option<&Clock>,
    owners: &Value,
) -> Check<()> {
    if let Some(clock) = clock {
        clock.check(90)?;
    }
    require(
        root.parent_guard.normalized_path() == root.parent
            && root.state_guard.normalized_path() == root.state,
        "retained root guard mismatch",
    )?;
    #[cfg(unix)]
    for (path, (device, inode)) in &root.directory_identities {
        let current = fs::symlink_metadata(path).map_err(|_| "owned directory identity lost")?;
        require(
            current.is_dir()
                && !current.file_type().is_symlink()
                && current.dev() == *device
                && current.ino() == *inode,
            "owned directory replaced; retained",
        )?;
    }
    let state_anchor = private_read(&root.state.join("sentinel"))?;
    let parent_anchor = private_read(&root.parent.join("anchor"))?;
    require(
        identity(&state_anchor)? == root.state_anchor
            && identity(&parent_anchor)? == root.parent_anchor,
        "cleanup root anchors changed",
    )?;
    drop(state_anchor);
    drop(parent_anchor);
    let mut state_files: BTreeSet<String> = ["sentinel"].map(str::to_owned).into_iter().collect();
    if row.is_some() {
        state_files.insert("dashboard.token".to_owned());
        state_files.insert("dashboard.token.lock".to_owned());
    }
    if row.is_some_and(populated) {
        state_files.extend(
            [
                "state.db",
                "state.db-wal",
                "state.db-shm",
                "state.db-journal",
            ]
            .map(str::to_owned),
        );
    }
    // Directories are checked separately; no recursive cleanup or discovery-based deletion.
    let actual_state: BTreeSet<_> = fs::read_dir(&root.state)
        .map_err(|_| "root inventory failed")?
        .map(|entry| {
            entry
                .map_err(|_| "root entry failed")?
                .file_name()
                .into_string()
                .map_err(|_| "root entry name invalid")
        })
        .collect::<Check<_>>()?;
    require(
        actual_state
            .iter()
            .all(|name| state_files.contains(name) || name == "outputs" || name == "tmp"),
        "foreign state object retained",
    )?;
    let mut leaves = Vec::new();
    for name in &state_files {
        let path = root.state.join(name);
        match fs::symlink_metadata(&path) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && name.starts_with("state.db-") => {}
            Err(_) => return Err("required cleanup leaf missing"),
            Ok(meta) => {
                require(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "state cleanup type refused",
                )?;
                let mut file = private_read(&path)?;
                let expected = expected_leaf(&path, &root.parent, owners)?;
                require(
                    identity(&file)? == expected,
                    "owned state leaf replaced; retained",
                )?;
                if name == "dashboard.token.lock" {
                    require(
                        file.metadata()
                            .map_err(|_| "cleanup permanent lock metadata failed")?
                            .len()
                            == 0
                            && file
                                .read(&mut [0; 1])
                                .map_err(|_| "cleanup permanent lock read failed")?
                                == 0,
                        "nonempty permanent lock retained",
                    )?;
                }
                leaves.push((path, expected));
            }
        }
    }
    let output_allowed = ["owned-output".to_owned()].into_iter().collect();
    for (path, id) in exact_entries(&root.state.join("outputs"), &output_allowed)? {
        let expected = expected_leaf(&path, &root.parent, owners)?;
        require(id == expected, "owned output replaced; retained")?;
        leaves.push((path, expected));
    }
    require(
        fs::read_dir(root.state.join("tmp"))
            .map_err(|_| "tmp inventory failed")?
            .next()
            .is_none(),
        "foreign tmp object retained",
    )?;
    let mut parent_allowed: BTreeSet<_> = ["anchor"].map(str::to_owned).into_iter().collect();
    if let Some(row) = row {
        parent_allowed.extend(
            [
                "ticket.json",
                "server.stdout",
                "server.stderr",
                "server-ready.json",
                "server-done.json",
            ]
            .map(str::to_owned),
        );
        if populated(row) {
            parent_allowed.extend(
                [
                    "store.stdout",
                    "store.stderr",
                    "store-ready.json",
                    "store-start.json",
                    "store-done.json",
                    "snapshot-1.json",
                    "snapshot-2.json",
                    "store-command-1.json",
                    "store-command-2.json",
                    "store-command-3.json",
                ]
                .map(str::to_owned),
            );
        }
        #[cfg(windows)]
        parent_allowed.extend(
            [
                "controller.stdout",
                "controller.stderr",
                "controller-done.json",
            ]
            .map(str::to_owned),
        );
    }
    let actual_parent: BTreeSet<_> = fs::read_dir(&root.parent)
        .map_err(|_| "parent inventory failed")?
        .map(|entry| {
            entry
                .map_err(|_| "parent entry failed")?
                .file_name()
                .into_string()
                .map_err(|_| "parent entry name invalid")
        })
        .collect::<Check<_>>()?;
    require(
        actual_parent
            == parent_allowed
                .iter()
                .cloned()
                .chain(["state".to_owned()])
                .collect(),
        "missing/foreign control or capture retained",
    )?;
    let mut parent_leaves = Vec::new();
    for name in parent_allowed {
        let path = root.parent.join(&name);
        let file = private_read(&path)?;
        require(
            file.metadata()
                .map_err(|_| "capture metadata failed")?
                .len()
                <= u64::try_from(if name.starts_with("snapshot-") {
                    SNAPSHOT_CAP
                } else {
                    CONTROL_CAP
                })
                .map_err(|_| "capture cap overflow")?,
            "private capture or receipt exceeded cap",
        )?;
        let expected = expected_leaf(&path, &root.parent, owners)?;
        require(
            identity(&file)? == expected,
            "owned control/capture replaced; retained",
        )?;
        parent_leaves.push((path, expected));
    }
    remove_leaves(&leaves, clock)?;
    remove_leaves(&parent_leaves, clock)?;
    require(
        root.subdirectories.len() == 2,
        "owned subdirectory guards missing",
    )?;
    for (guard, name) in root.subdirectories.into_iter().zip(["outputs", "tmp"]) {
        if let Some(clock) = clock {
            clock.check(90)?;
        }
        require(
            guard.normalized_path() == root.state.join(name),
            "retained subdirectory guard changed",
        )?;
        let admitted = DirectoryGuard::existing_private(guard.normalized_path())
            .map_err(|_| "owned subdirectory admission failed")?;
        drop(admitted);
        if let Some(clock) = clock {
            clock.check(90)?;
        }
        drop(guard);
        fs::remove_dir(root.state.join(name))
            .map_err(|_| "empty owned directory cleanup failed")?;
    }
    if let Some(clock) = clock {
        clock.check(90)?;
    }
    drop(root.state_guard);
    fs::remove_dir(&root.state).map_err(|_| "empty state root cleanup failed")?;
    if let Some(clock) = clock {
        clock.check(90)?;
    }
    drop(root.parent_guard);
    fs::remove_dir(&root.parent).map_err(|_| "empty parent cleanup failed")?;
    if let Some(clock) = clock {
        clock.check(90)?;
    }
    Ok(())
}

struct ChildOwner {
    child: Option<std::process::Child>,
    deadline: Instant,
    _capture_guards: Vec<DirectoryGuard>,
}

impl ChildOwner {
    fn id(&self) -> Check<u32> {
        self.child
            .as_ref()
            .map(std::process::Child::id)
            .ok_or("owned child already reaped")
    }

    fn poll(&mut self) -> Check<Option<std::process::ExitStatus>> {
        let child = self.child.as_mut().ok_or("child double reap")?;
        let status = child.try_wait().map_err(|_| "actual child reap failed")?;
        if status.is_some() {
            self.child.take();
        }
        Ok(status)
    }
}

impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let until = Instant::now()
                .checked_add(Duration::from_secs(3))
                .map_or(self.deadline, |until| until.min(self.deadline));
            while Instant::now() < until {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Err(_) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
            // Failure containment never creates a completed receipt and never enables cleanup.
        }
    }
}

fn spawn_child(
    ticket: &Ticket,
    artifact: &Artifact,
    selector: Option<&str>,
    role: &str,
    clock: &Clock,
) -> Check<ChildOwner> {
    clock.check(87)?;
    let mut command = std::process::Command::new(&artifact.path);
    if let Some(selector) = selector {
        command.args([selector, "--exact", "--test-threads=1", "--nocapture"]);
    }
    command
        .env("LOCRON_PR144_TICKET", ticket.parent.join("ticket.json"))
        .env("LOCRON_PR144_NONCE", &ticket.nonce)
        .stdin(std::process::Stdio::null());
    let mut guards = Vec::new();
    for channel in ["stdout", "stderr"] {
        let path = ticket.parent.join(format!("{role}.{channel}"));
        clock.check(87)?;
        let file = create_private_new(&path).map_err(|_| "exclusive capture creation failed")?;
        clock.check(87)?;
        record_owned(&path, &identity(&file)?)?;
        let (file, guard) = file.into_parts();
        guards.push(guard);
        let stdio = std::process::Stdio::from(file);
        if channel == "stdout" {
            command.stdout(stdio);
        } else {
            command.stderr(stdio);
        }
    }
    let deadline = clock.until(90)?;
    clock.check(87)?;
    // Ownership starts in this return expression; nothing fallible follows spawn inside it.
    let owner = ChildOwner {
        child: Some(command.spawn().map_err(|_| "actual child spawn failed")?),
        deadline,
        _capture_guards: guards,
    };
    clock.check(87)?;
    Ok(owner)
}

fn merge_ownership(target: &mut Value, source: &Value) -> Check<()> {
    for (name, id) in source
        .as_object()
        .ok_or("completed owned manifest missing")?
    {
        require(
            id.as_str().is_some() && target.get(name).is_none(),
            "duplicate/invalid completed owned identity",
        )?;
        target[name] = id.clone();
    }
    Ok(())
}

fn expected_leaf(path: &Path, parent: &Path, owners: &Value) -> Check<String> {
    let name = path
        .strip_prefix(parent)
        .map_err(|_| "cleanup path escaped root")?
        .to_str()
        .ok_or("cleanup name UTF8 invalid")?
        .replace('\\', "/");
    owners[&name]
        .as_str()
        .map(str::to_owned)
        .ok_or("foreign/unrecognized cleanup leaf retained")
}

#[cfg(unix)]
async fn run_owned(ticket: &Ticket, clock: &Clock) -> Check<Value> {
    let socket = tokio::net::UnixDatagram::bind(ticket.state.join("wake.sock"))
        .map_err(|_| "real Unix receiver bind failed")?;
    locron_core::notification::send_wake(&ticket.state)
        .map_err(|_| "real receiver setup positive send failed")?;
    let mut buffer = [0; 128];
    let size = tokio::time::timeout_at(
        tokio::time::Instant::from_std(clock.until(30)?),
        socket.recv(&mut buffer),
    )
    .await
    .map_err(|_| "receiver setup positive horizon expired")?
    .map_err(|_| "receiver setup positive read failed")?;
    require(
        &buffer[..size] == b"locron-wake/v1",
        "real setup positive frame mismatch",
    )?;
    match socket.try_recv(&mut buffer) {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
        _ => return Err("unexpected frame before receiver arm"),
    }
    let socket_identity = fs::symlink_metadata(ticket.state.join("wake.sock"))
        .map_err(|_| "created socket identity failed")?;
    let mut store = if populated(ticket.row) {
        Some(spawn_child(
            ticket,
            &ticket.store,
            Some(STORE_SELECTOR),
            "store",
            clock,
        )?)
    } else {
        None
    };
    let store_pid = store.as_ref().map(ChildOwner::id).transpose()?;
    if let Some(pid) = store_pid {
        let ready = wait_json(
            &ticket.parent.join("store-ready.json"),
            CONTROL_CAP,
            clock,
            30,
        )
        .await?;
        validate_receipt(&ready, ticket, pid, 0, "ready")?;
        require(ready["keeper_open"] == true, "actual keeper ready missing")?;
        publish(
            &ticket.parent.join("store-start.json"),
            &json!({"nonce":ticket.nonce,"pid":pid}),
            CONTROL_CAP,
        )?;
    }
    let mut server = spawn_child(
        ticket,
        &ticket.server,
        Some(SERVER_SELECTOR),
        "server",
        clock,
    )?;
    let server_pid = server.id()?;
    let mut deliveries: u64 = 0;
    loop {
        clock.check(90)?;
        loop {
            match socket.try_recv(&mut buffer) {
                Ok(size) => {
                    require(
                        &buffer[..size] == b"locron-wake/v1",
                        "actual delivery frame mismatch",
                    )?;
                    deliveries = deliveries
                        .checked_add(1)
                        .ok_or("delivery counter overflow")?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => return Err("actual wake receiver failed"),
            }
        }
        if let Some(status) = server.poll()? {
            require(
                status.success(),
                "actual Server child nonzero; captures retained",
            )?;
            break;
        }
        if let Some(store) = &mut store {
            require(
                store.poll()?.is_none(),
                "keeper exited before stop; captures retained",
            )?;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let done = read_json(&ticket.parent.join("server-done.json"), CONTROL_CAP)?;
    let mut owners = json!({});
    merge_ownership(&mut owners, &done["ownership"])?;
    validate_receipt(&done, ticket, server_pid, 1, "completed")?;
    require(
        done["server_joined"] == true && done["blocking_active"] == 0,
        "actual worker completion missing",
    )?;
    if let Some(pid) = store_pid {
        publish(
            &ticket.parent.join("store-command-3.json"),
            &json!({"nonce":ticket.nonce,"pid":pid,"sequence":3,"operation":"stop"}),
            CONTROL_CAP,
        )?;
        let store = store.as_mut().ok_or("owned Store missing")?;
        loop {
            clock.check(90)?;
            if let Some(status) = store.poll()? {
                require(
                    status.success(),
                    "actual Store child nonzero; captures retained",
                )?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let closed = read_json(&ticket.parent.join("store-done.json"), CONTROL_CAP)?;
        validate_receipt(&closed, ticket, pid, 3, "closed")?;
        merge_ownership(&mut owners, &closed["ownership"])?;
        require(
            closed["keeper_connection_closed"] == true,
            "actual keeper close receipt missing",
        )?;
    }
    // Server is joined and both real children are reaped before the listener is released.
    loop {
        match socket.try_recv(&mut buffer) {
            Ok(size) => {
                require(
                    &buffer[..size] == b"locron-wake/v1",
                    "final delivery invalid",
                )?;
                deliveries = deliveries
                    .checked_add(1)
                    .ok_or("delivery counter overflow")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => return Err("final listener drain failed"),
        }
    }
    require(
        deliveries == u64::from((170..178).contains(&ticket.row)),
        "actual measured wake delivery mismatch",
    )?;
    let path = ticket.state.join("wake.sock");
    let original = socket_identity;
    drop(socket);
    let current = fs::symlink_metadata(&path).map_err(|_| "owned socket replaced")?;
    require(
        current.file_type().is_socket()
            && current.dev() == original.dev()
            && current.ino() == original.ino(),
        "socket identity changed; retained",
    )?;
    fs::remove_file(path).map_err(|_| "owned listener leaf cleanup failed")?;
    drop(server);
    drop(store);
    merge_ownership(&mut owners, &owned_manifest(&ticket.parent)?)?;
    let mut proof = done;
    proof["cleanup_ownership"] = owners;
    proof["deliveries"] = json!(deliveries);
    proof["actual_root_reaps"] = json!(if populated(ticket.row) { 2 } else { 1 });
    proof["receiver_closed"] = json!(true);
    // Nonces/PIDs stay private. Public ledger selects scalar/hash fields explicitly below.
    Ok(proof)
}

#[cfg(windows)]
async fn run_owned(ticket: &Ticket, clock: &Clock) -> Check<Value> {
    let artifact = ticket
        .controller
        .as_ref()
        .ok_or("native controller missing")?;
    let mut controller = spawn_child(ticket, artifact, None, "controller", clock)?;
    let pid = controller.id()?;
    loop {
        clock.check(90)?;
        if let Some(status) = controller.poll()? {
            require(
                status.success(),
                "actual native controller nonzero; all state retained",
            )?;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let proof = read_json(&ticket.parent.join("controller-done.json"), CONTROL_CAP)?;
    validate_receipt(&proof, ticket, pid, 1, "owned-completed")?;
    require(
        proof["server_root_reaped"] == true
            && proof["server_job_empty"] == true
            && proof["store_root_reaped"] == populated(ticket.row)
            && proof["store_job_empty"] == populated(ticket.row)
            && proof["listener_joined"] == true,
        "authoritative native ownership completion missing",
    )?;
    require(
        proof["server"]["attempts"] == u64::from((170..178).contains(&ticket.row))
            && proof["deliveries"] == u64::from((170..178).contains(&ticket.row)),
        "native wake attempt/delivery mismatch",
    )?;
    let mut result = proof["server"].clone();
    let mut owners = owned_manifest(&ticket.parent)?;
    merge_ownership(&mut owners, &proof["ownership"])?;
    merge_ownership(&mut owners, &proof["workers_ownership"])?;
    result["cleanup_ownership"] = owners;
    result["deliveries"] = proof["deliveries"].clone();
    result["actual_root_reaps"] = json!(if populated(ticket.row) { 3 } else { 2 });
    result["listener_joined"] = json!(true);
    result["native_empty_jobs"] = json!(if populated(ticket.row) { 2 } else { 1 });
    drop(controller);
    Ok(result)
}

async fn qualification() -> Check<()> {
    let facts_path = PathBuf::from(
        std::env::var_os("LOCRON_PR144_BUILD_FACTS")
            .ok_or("required hosted compiler facts missing")?,
    );
    // This is an explicitly passed CI input, not a user-selected token/state path.
    let mut facts_file = locron_core::filesystem::open_read_no_follow(&facts_path)
        .map_err(|_| "hosted facts no-follow admission failed")?;
    let mut facts_bytes = Vec::new();
    Read::by_ref(&mut *facts_file)
        .take(
            u64::try_from(CONTROL_CAP)
                .map_err(|_| "facts cap overflow")?
                .checked_add(1)
                .ok_or("facts cap overflow")?,
        )
        .read_to_end(&mut facts_bytes)
        .map_err(|_| "hosted facts read failed")?;
    require(
        facts_bytes.len() <= CONTROL_CAP,
        "hosted facts cap exceeded",
    )?;
    let facts: BuildFacts =
        serde_json::from_slice(&facts_bytes).map_err(|_| "hosted build facts invalid")?;
    require(
        facts.schema == "locron.pr144-build/v1"
            && facts.checkout.len() == 40
            && !facts.run.is_empty()
            && !facts.attempt.is_empty()
            && !facts.job.is_empty()
            && facts.compiler.contains("host: ")
            && facts.host == facts.target,
        "actual hosted native/toolchain provenance missing",
    )?;
    require(
        facts.store.profile_test
            && facts.store.target_kind == ["lib"]
            && facts.store.package_id.contains("locron-store"),
        "Store compiler artifact selector mismatch",
    )?;
    let (store, store_guard) = binding(&facts.store)?;
    let (controller, controller_guard) = if cfg!(windows) {
        let fact = facts
            .controller
            .as_ref()
            .ok_or("native compiler example artifact missing")?;
        require(
            !fact.profile_test
                && fact.target_kind == ["example"]
                && fact.package_id.contains("locron"),
            "native example compiler selector mismatch",
        )?;
        let (binding, guard) = binding(fact)?;
        (Some(binding), Some(guard))
    } else {
        require(
            facts.controller.is_none(),
            "foreign native artifact in Unix facts",
        )?;
        (None, None)
    };
    let executable = fs::canonicalize(
        std::env::current_exe().map_err(|_| "actual Server test artifact missing")?,
    )
    .map_err(|_| "Server artifact canonicalization failed")?;
    let server_guard = open_artifact(&executable)?;
    let server = Artifact {
        path: executable,
        identity: identity(&server_guard)?,
        sha256: sha256(&server_guard)?,
        crc32: String::new(),
    };
    let evidence = PathBuf::from(
        std::env::var_os("LOCRON_PR144_EVIDENCE_DIR")
            .ok_or("required redacted ledger directory missing")?,
    );
    require(
        fs::symlink_metadata(&evidence)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "evidence root already exists; refused without privacy repair",
    )?;
    let evidence_guard =
        DirectoryGuard::private(&evidence).map_err(|_| "initial fresh evidence privacy failed")?;
    let evidence = evidence_guard.normalized_path().to_path_buf();
    let nonce = uuid::Uuid::now_v7().to_string();
    let parser_root = fresh_root(&nonce)?;
    let mut completed = parser_rows(&server.path, &parser_root.state, &nonce)?;
    let parser_ownership = owned_manifest(&parser_root.parent)?;
    cleanup(parser_root, None, None, &parser_ownership)?;
    let mut ledger = Vec::new();
    for key in &completed {
        ledger.push(json!({"key":key,"completed":true,"kind":"actual-parser"}));
    }
    publish(
        &evidence.join("parser156.json"),
        &json!({"schema":"locron.pr144-parser/v1","checkout":facts.checkout,"run":facts.run,"attempt":facts.attempt,"job":facts.job,"rows":ledger}),
        RESPONSE_CAP,
    )?;
    let phase = Instant::now();
    let phase_deadline = phase
        .checked_add(Duration::from_secs(600))
        .ok_or("runtime phase overflow")?;
    let count = if cfg!(windows) { 195 } else { 193 };
    for row in 156..count {
        require(
            Instant::now() < phase_deadline,
            "original runtime phase horizon expired",
        )?;
        let clock = Clock {
            origin: Instant::now(),
        };
        let started_us = wall_us()?;
        publish(
            &evidence.join(format!("begin-{row}.json")),
            &json!({"key":KEYS[row],"completed":false,"phase":"started","checkout":facts.checkout,"run":facts.run,"attempt":facts.attempt,"job":facts.job}),
            CONTROL_CAP,
        )?;
        let nonce = uuid::Uuid::now_v7().to_string();
        let root = fresh_root(&nonce)?;
        owned_file(&root.state.join("dashboard.token.lock"), b"")?;
        let ticket = Ticket {
            schema: "locron.private.pr144/v1".to_owned(),
            nonce,
            row,
            started_us,
            parent: root.parent.clone(),
            state: root.state.clone(),
            parent_anchor: root.parent_anchor.clone(),
            state_anchor: root.state_anchor.clone(),
            server: server.clone(),
            store: store.clone(),
            controller: controller.clone(),
        };
        clock.check(30)?;
        publish(&root.parent.join("ticket.json"), &ticket, CONTROL_CAP)?;
        let proof = run_owned(&ticket, &clock).await?;
        clock.check(90)?;
        // Cleanup owns no live child/listener/connection or pending API blocking worker.
        cleanup(root, Some(row), Some(&clock), &proof["cleanup_ownership"])?;
        require(
            Instant::now() < phase_deadline,
            "original runtime phase horizon expired",
        )?;
        completed.push(KEYS[row].to_owned());
        let mut entry =
            json!({"key":KEYS[row],"completed":true,"kind":"actual-http","cleanup_verified":true});
        for field in [
            "http_status",
            "attempts",
            "send_ok",
            "blocking_active",
            "blocking_completed",
            "server_joined",
            "deliveries",
            "actual_root_reaps",
            "listener_joined",
            "receiver_closed",
            "native_empty_jobs",
            "response_sha256",
            "logical_before_sha256",
            "logical_after_sha256",
        ] {
            if let Some(value) = proof.get(field) {
                entry[field] = value.clone();
            }
        }
        publish(
            &evidence.join(format!("row-{row}.json")),
            &entry,
            CONTROL_CAP,
        )?;
        ledger.push(entry);
        clock.check(90)?;
        require(
            Instant::now() < phase_deadline,
            "original runtime phase horizon expired",
        )?;
    }
    let expected: BTreeSet<_> = KEYS[..count].iter().map(|key| (*key).to_owned()).collect();
    let actual: BTreeSet<_> = completed.iter().cloned().collect();
    require(
        completed.len() == count && actual.len() == count && expected == actual,
        "missing/duplicate/foreign completion key",
    )?;
    require(
        sha256(&store_guard)? == store.sha256 && sha256(&server_guard)? == server.sha256,
        "candidate artifact changed during qualification",
    )?;
    if let (Some(artifact), Some(guard)) = (&controller, &controller_guard) {
        require(
            sha256(guard)? == artifact.sha256,
            "native controller artifact changed",
        )?;
    }
    let keyset = KEYS[..count].join("\n") + "\n";
    let result = json!({"schema":"locron.pr144-redacted-ledger/v1","checkout":facts.checkout,"run":facts.run,"attempt":facts.attempt,"job":facts.job,
        "compiler":facts.compiler,"host":facts.host,"target":facts.target,"rows":ledger,"expected":count,"completed":completed.len(),
        "keyset_sha256":format!("{:x}",Sha256::digest(keyset.as_bytes())),"selected195_keyset_sha256":"e2318e39838f2a1ce6ad1e372bc4a8a6a6ccab74f8d9a5488489ba3bcc89de78",
        "store_artifact":{"identity":store.identity,"sha256":store.sha256,"package_id":facts.store.package_id,"features":facts.store.features},
        "server_artifact":{"identity":server.identity,"sha256":server.sha256},"controller_artifact":controller.as_ref().map(|artifact|json!({"identity":artifact.identity,"sha256":artifact.sha256}))});
    publish(&evidence.join("completed.json"), &result, RESPONSE_CAP)?;
    println!("PR144 actual qualification completed {count}/{count}; redacted ledger saved");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dashboard_boolean_qualification_195_owned() {
    if let Err(message) = qualification().await {
        panic!("PR144 qualification failed: {message}; private uncompleted roots retained");
    }
}
