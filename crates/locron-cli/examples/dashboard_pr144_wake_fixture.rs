//! Owned native controller for the private PR144 qualification; no product CLI entry.

#[cfg(windows)]
mod native {
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
        let _guard = DirectoryGuard::existing_private(parent)
            .map_err(|_| "existing private parent refused")?;
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
        let mut file =
            create_private_new(&stage).map_err(|_| "exclusive stage admission failed")?;
        let expected = identity(&file)?;
        record_owned(path, &expected)?;
        let bytes = body()?;
        require(bytes.len() <= cap, "private publication cap exceeded")?;
        file.write_all(&bytes)
            .map_err(|_| "private stage write failed")?;
        file.sync_all().map_err(|_| "private stage sync failed")?;
        drop(file);
        let mut owned =
            tempfile::TempPath::try_from_path(stage).map_err(|_| "stage owner failed")?;
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

    use futures_util::FutureExt;
    use locron_engine::windows_child::{ChildWindow, OwnedChild};
    use sha2::{Digest, Sha256};
    use std::sync::Arc;

    fn hash(file: &GuardedFile) -> Check<String> {
        use std::io::{Seek, SeekFrom};
        let mut file = file.try_clone().map_err(|_| "artifact clone failed")?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| "artifact seek failed")?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 16 * 1024];
        loop {
            let size = file
                .read(&mut buffer)
                .map_err(|_| "artifact SHA read failed")?;
            if size == 0 {
                break;
            }
            hash.update(&buffer[..size]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    struct NativeOwner {
        child: OwnedChild,
        _captures: Vec<DirectoryGuard>,
    }

    fn spawn(
        ticket: &Ticket,
        artifact: &Artifact,
        selector: &str,
        role: &str,
        clock: &Clock,
    ) -> Check<NativeOwner> {
        clock.check(87)?;
        let mut command = tokio::process::Command::new(&artifact.path);
        command
            .args([selector, "--exact", "--test-threads=1", "--nocapture"])
            .env("LOCRON_PR144_TICKET", ticket.parent.join("ticket.json"))
            .env("LOCRON_PR144_NONCE", &ticket.nonce)
            .stdin(std::process::Stdio::null());
        let mut captures = Vec::new();
        for channel in ["stdout", "stderr"] {
            clock.check(87)?;
            let file = create_private_new(&ticket.parent.join(format!("{role}.{channel}")))
                .map_err(|_| "private native capture creation failed")?;
            clock.check(87)?;
            record_owned(
                &ticket.parent.join(format!("{role}.{channel}")),
                &identity(&file)?,
            )?;
            let (file, guard) = file.into_parts();
            captures.push(guard);
            if channel == "stdout" {
                command.stdout(std::process::Stdio::from(file));
            } else {
                command.stderr(std::process::Stdio::from(file));
            }
        }
        // The existing suspended/enrolled Engine path owns the worker before it runs.
        // ExecutionMayHaveStarted is a failure with retained state, never completion.
        clock.check(87)?;
        let owner = NativeOwner {
            child: OwnedChild::spawn(command, ChildWindow::Hidden)
                .map_err(|_| "native owned spawn refused or unconfirmed")?,
            _captures: captures,
        };
        clock.check(87)?;
        Ok(owner)
    }

    async fn wait_json(path: &Path, clock: &Clock) -> Check<Value> {
        loop {
            clock.check(30)?;
            match fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tokio::time::sleep(Duration::from_millis(5)).await
                }
                Err(_) => return Err("native ready inspection failed"),
                Ok(meta) => {
                    require(
                        meta.is_file() && !meta.file_type().is_symlink(),
                        "native ready type invalid",
                    )?;
                    let result = read_json(path, CONTROL_CAP)?;
                    clock.check(30)?;
                    return Ok(result);
                }
            }
        }
    }

    async fn confirm(owner: &mut NativeOwner, clock: &Clock) -> Check<()> {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(3))
            .ok_or("native ownership horizon overflow")?
            .min(clock.until(90)?);
        let status = owner
            .child
            .confirm_exit_until(deadline)
            .await
            .map_err(|_| "actual root reap or Job empty unconfirmed")?;
        require(
            status.success(),
            "actual native worker nonzero; private captures retained",
        )?;
        require(
            owner
                .child
                .tree_empty()
                .map_err(|_| "authoritative Job query failed")?,
            "native Job not empty",
        )?;
        clock.check(90)
    }

    pub(super) async fn run() -> Check<()> {
        let admission = admit("controller")?;
        let ticket = &admission.ticket;
        let clock = &admission.clock;
        require(
            hash(&admission.artifact)?
                == ticket
                    .controller
                    .as_ref()
                    .ok_or("controller artifact missing")?
                    .sha256,
            "actual controller SHA mismatch",
        )?;
        let mut artifacts = Vec::new();
        for artifact in [&ticket.server, &ticket.store] {
            let file = locron_core::filesystem::read_owned_executable(&artifact.path)
                .map_err(|_| "native worker artifact refused")?;
            require(
                identity(&file)? == artifact.identity && hash(&file)? == artifact.sha256,
                "native worker actual identity/SHA mismatch",
            )?;
            artifacts.push(file);
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        let listener = locron_engine::ipc::bind_wake(&ticket.state, Arc::clone(&notify))
            .map_err(|_| "actual secured engine wake bind failed")?;
        locron_core::notification::send_wake(&ticket.state)
            .map_err(|_| "actual native setup positive send failed")?;
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(clock.until(30)?),
            notify.notified(),
        )
        .await
        .map_err(|_| "actual native setup positive notification absent")?;
        require(
            notify.notified().now_or_never().is_none(),
            "unexpected native frame before arm",
        )?;
        let mut store = if populated(ticket.row) {
            Some(spawn(
                ticket,
                &ticket.store,
                STORE_SELECTOR,
                "store",
                clock,
            )?)
        } else {
            None
        };
        let store_pid = store
            .as_ref()
            .map(|owner| owner.child.id().ok_or("actual native Store PID missing"))
            .transpose()?;
        if let Some(pid) = store_pid {
            let ready = wait_json(&ticket.parent.join("store-ready.json"), clock).await?;
            validate_receipt(&ready, ticket, pid, 0, "ready")?;
            require(
                ready["keeper_open"] == true,
                "actual native keeper ready missing",
            )?;
            publish(
                &ticket.parent.join("store-start.json"),
                &json!({"nonce":ticket.nonce,"pid":pid}),
                CONTROL_CAP,
            )?;
        }
        let mut server = spawn(ticket, &ticket.server, SERVER_SELECTOR, "server", clock)?;
        let server_pid = server
            .child
            .id()
            .ok_or("actual native Server PID missing")?;
        let mut deliveries: u64 = 0;
        loop {
            clock.check(90)?;
            require(
                !listener.is_finished(),
                "actual native listener exited before completion",
            )?;
            if notify.notified().now_or_never().is_some() {
                deliveries = deliveries
                    .checked_add(1)
                    .ok_or("native delivery overflow")?;
            }
            if server
                .child
                .try_wait()
                .map_err(|_| "actual native Server wait failed")?
                .is_some()
            {
                break;
            }
            if let Some(store) = &mut store {
                require(
                    store
                        .child
                        .try_wait()
                        .map_err(|_| "actual keeper wait failed")?
                        .is_none(),
                    "native keeper exited before stop",
                )?;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        confirm(&mut server, clock).await?;
        let server_done = read_json(&ticket.parent.join("server-done.json"), CONTROL_CAP)?;
        validate_receipt(&server_done, ticket, server_pid, 1, "completed")?;
        let mut workers_ownership = server_done["ownership"].clone();
        let live = (170..178).contains(&ticket.row);
        require(
            server_done["server_joined"] == true
                && server_done["blocking_active"] == 0
                && server_done["attempts"] == u64::from(live)
                && server_done["send_ok"] == u64::from(live),
            "native actual HTTP/blocking/wake receipt invalid",
        )?;
        if live && deliveries == 0 {
            // Wait for the already-delegated real frame, under the original row horizon.
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(clock.until(90)?),
                notify.notified(),
            )
            .await
            .map_err(|_| "actual native delegated delivery missing")?;
            deliveries = 1;
        }
        if let Some(pid) = store_pid {
            publish(
                &ticket.parent.join("store-command-3.json"),
                &json!({"nonce":ticket.nonce,"pid":pid,"sequence":3,"operation":"stop"}),
                CONTROL_CAP,
            )?;
            let owner = store.as_mut().ok_or("native Store owner missing")?;
            loop {
                clock.check(90)?;
                if owner
                    .child
                    .try_wait()
                    .map_err(|_| "actual native Store wait failed")?
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            confirm(owner, clock).await?;
            let closed = read_json(&ticket.parent.join("store-done.json"), CONTROL_CAP)?;
            validate_receipt(&closed, ticket, pid, 3, "closed")?;
            for (name, id) in closed["ownership"]
                .as_object()
                .ok_or("native Store owned manifest missing")?
            {
                require(
                    workers_ownership.get(name).is_none(),
                    "duplicate native worker owned identity",
                )?;
                workers_ownership[name] = id.clone();
            }
            require(
                closed["keeper_connection_closed"] == true,
                "actual native keeper close missing",
            )?;
        }
        if notify.notified().now_or_never().is_some() {
            deliveries = deliveries
                .checked_add(1)
                .ok_or("native delivery overflow")?;
        }
        require(
            deliveries == u64::from(live),
            "native measured delivery count mismatch",
        )?;
        // Awaited cancellation joins the existing listener Rust task and drops its handles.
        listener.abort();
        let stopped =
            tokio::time::timeout_at(tokio::time::Instant::from_std(clock.until(90)?), listener)
                .await
                .map_err(|_| "native listener join horizon expired")?;
        require(
            stopped.is_err_and(|error| error.is_cancelled()),
            "actual native listener cancellation join missing",
        )?;
        drop(server);
        drop(store);
        drop(artifacts);
        clock.check(90)?;
        let mut done = receipt(ticket, 1, "owned-completed");
        done["workers_ownership"] = workers_ownership;
        done["server"] = server_done;
        done["deliveries"] = json!(deliveries);
        done["server_root_reaped"] = json!(true);
        done["server_job_empty"] = json!(true);
        done["store_root_reaped"] = json!(populated(ticket.row));
        done["store_job_empty"] = json!(populated(ticket.row));
        done["listener_joined"] = json!(true);
        publish_done(
            &ticket.parent.join("controller-done.json"),
            done,
            CONTROL_CAP,
        )
    }
}

#[cfg(windows)]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    if let Err(message) = native::run().await {
        panic!("PR144 native fixture failed: {message}; private root retained");
    }
}

#[cfg(not(windows))]
fn main() {}
