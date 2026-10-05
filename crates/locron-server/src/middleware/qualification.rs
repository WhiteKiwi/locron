//! Frozen cookie controls use real middleware, production session/asset routes, and owned roots.
//! The probe observes admission separately; it never replaces a production handler or issuer.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{FromRequest, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Version, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, middleware};
use futures_util::Stream;
use locron_core::filesystem::{DirectoryGuard, GuardedFile, create_private_new};
use locron_store::StatePaths;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use super::{AuthKind, CSRF_COOKIE, CSRF_HEADER, CookieState, SESSION_COOKIE};
use crate::AppState;

const T: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const C: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const GROUP_LIMIT: Duration = Duration::from_secs(60);
const REQUEST_LIMIT: Duration = Duration::from_secs(5);
const RESPONSE_LIMIT: usize = 1_048_576;
const PROBE_LIMIT: usize = 2_097_153;
const SENTINEL: &[u8] = b"PR145 owned sentinel: unchanged bytes and actual identity";
const ACCEPTED: &[u8] = b"PR145 accepted";

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
type Identity = locron_core::filesystem::FileIdentity;

fn admit(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "PR145 owned deadline elapsed",
        ))
    } else {
        Ok(())
    }
}

fn no_follow(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other("PR145 unknown symlink quarantined"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("PR145 unknown reparse object quarantined"));
        }
    }
    Ok(metadata)
}

#[cfg(unix)]
fn directory_identity(path: &Path, deadline: Instant) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    admit(deadline)?;
    let metadata = no_follow(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other(
            "PR145 directory identity is not a directory",
        ));
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn directory_plan(
    path: &Path,
    deadline: Instant,
) -> io::Result<locron_core::filesystem::PrivateDirectoryPlan> {
    admit(deadline)?;
    if !no_follow(path)?.is_dir() {
        return Err(io::Error::other(
            "PR145 directory identity is not a directory",
        ));
    }
    // Inspect an absent suffix so an ancestry observation does not demand a repair of a parent.
    let observation = path.join("pr145-never-created-identity-observation");
    match fs::symlink_metadata(&observation) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => {
            return Err(io::Error::other(
                "PR145 observation path is unexpectedly present",
            ));
        }
    }
    let plan =
        locron_core::filesystem::PrivateDirectoryPlan::inspect_until(&observation, deadline)?;
    if plan.root_identity().is_some() || plan.missing_components().len() != 1 {
        return Err(io::Error::other(
            "PR145 observation did not retain the actual directory",
        ));
    }
    Ok(plan)
}

#[cfg(windows)]
fn directory_identity(path: &Path, deadline: Instant) -> io::Result<Identity> {
    Ok(directory_plan(path, deadline)?.existing_identity())
}

#[cfg(unix)]
fn leaf_identity(file: &GuardedFile) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn leaf_identity(file: &GuardedFile) -> io::Result<Identity> {
    locron_core::filesystem::file_identity(file)
}

fn path_leaf_identity(path: &Path) -> io::Result<Identity> {
    if !no_follow(path)?.is_file() {
        return Err(io::Error::other(
            "PR145 cleanup leaf is not an owned no-follow file",
        ));
    }
    #[cfg(windows)]
    {
        let reader = locron_core::filesystem::open_private_read_stable(path)?;
        let identity = leaf_identity(&reader)?;
        if !no_follow(path)?.is_file() {
            return Err(io::Error::other(
                "PR145 cleanup leaf became a reparse object",
            ));
        }
        Ok(identity)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let reader = locron_core::filesystem::open_read_no_follow(path)?;
        let metadata = reader.metadata()?;
        let identity = Identity {
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        let named = no_follow(path)?;
        if !named.is_file() || named.dev() != identity.device || named.ino() != identity.inode {
            return Err(io::Error::other("PR145 cleanup leaf name/object changed"));
        }
        Ok(identity)
    }
}

struct OwnedRoot {
    path: PathBuf,
    parent: PathBuf,
    root_id: Identity,
    parent_id: Identity,
    leaf_id: Identity,
    root_guard: DirectoryGuard,
    parent_guard: DirectoryGuard,
    sentinel: GuardedFile,
    #[cfg(windows)]
    initial_root: locron_core::filesystem::PrivateDirectoryPlan,
}

impl OwnedRoot {
    fn create(deadline: Instant) -> io::Result<Self> {
        admit(deadline)?;
        // Disarm recursive cleanup immediately. Any later failure leaves a quarantined root.
        let raw = tempfile::Builder::new()
            .prefix("locron-pr145-cookie-")
            .tempdir()?
            .keep();
        let parent = raw
            .parent()
            .ok_or_else(|| io::Error::other("PR145 root lacks parent"))?
            .to_owned();
        #[cfg(windows)]
        let initial_root = directory_plan(&raw, deadline)?;
        let root_id = directory_identity(&raw, deadline)?;
        let parent_id = directory_identity(&parent, deadline)?;
        let parent_guard = DirectoryGuard::ancestors(&parent)?;
        admit(deadline)?;
        // Only the just-created, retained, test-owned object receives setup permissions.
        let root_guard = DirectoryGuard::private(&raw)?;
        let path = root_guard.normalized_path().to_owned();
        if directory_identity(&path, deadline)? != root_id {
            return Err(io::Error::other(
                "PR145 created root was replaced; quarantine",
            ));
        }
        let sentinel_path = path.join("owned-sentinel");
        let mut writer = create_private_new(&sentinel_path)?;
        writer.write_all(SENTINEL)?;
        writer.sync_all()?;
        let leaf_id = leaf_identity(&writer)?;
        // Stable readers deny concurrent write sharing; retain the same actual leaf, not its writer.
        drop(writer);
        admit(deadline)?;
        #[cfg(windows)]
        let sentinel = locron_core::filesystem::open_private_read_stable(&sentinel_path)?;
        #[cfg(unix)]
        let sentinel = locron_core::filesystem::open_read_no_follow(&sentinel_path)?;
        if leaf_identity(&sentinel)? != leaf_id {
            return Err(io::Error::other(
                "PR145 sentinel changed during the read-only handoff; quarantine",
            ));
        }
        #[cfg(windows)]
        {
            let sid = locron_core::windows::current_user_sid_until(deadline)?;
            if !sid.starts_with("S-1-") || std::mem::size_of::<usize>() != 8 {
                return Err(io::Error::other("PR145 actual native SID/ABI mismatch"));
            }
            let host = std::env::var("PROCESSOR_ARCHITECTURE")
                .map_err(|_| io::Error::other("PR145 missing actual native architecture"))?;
            let native = match std::env::consts::ARCH {
                "x86_64" => host == "AMD64",
                "aarch64" => host == "ARM64",
                _ => false,
            };
            if !native || std::env::var_os("PROCESSOR_ARCHITEW6432").is_some() {
                return Err(io::Error::other(
                    "PR145 actual native process architecture mismatch",
                ));
            }
            eprintln!(
                "PR145 native owner SID={sid} arch={} root={root_id:?} parent={parent_id:?} leaf={leaf_id:?}",
                std::env::consts::ARCH
            );
        }
        let owned = Self {
            path,
            parent,
            root_id,
            parent_id,
            leaf_id,
            root_guard,
            parent_guard,
            sentinel,
            #[cfg(windows)]
            initial_root,
        };
        owned.check(deadline)?;
        Ok(owned)
    }

    fn state(&self) -> AppState {
        AppState {
            paths: StatePaths::new(self.path.join("state-missing")),
            token: T.to_owned(),
            bound_port: 10824,
        }
    }

    fn check(&self, deadline: Instant) -> io::Result<()> {
        admit(deadline)?;
        if directory_identity(&self.path, deadline)? != self.root_id
            || directory_identity(&self.parent, deadline)? != self.parent_id
            || leaf_identity(&self.sentinel)? != self.leaf_id
        {
            return Err(io::Error::other("PR145 owned identity changed; quarantine"));
        }
        let names = fs::read_dir(&self.path)?
            .take(3)
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        if names.len() != 1 || names[0] != "owned-sentinel" {
            return Err(io::Error::other("PR145 unexpected inventory; quarantine"));
        }
        let path = self.path.join("owned-sentinel");
        if !no_follow(&path)?.is_file() {
            return Err(io::Error::other("PR145 sentinel is not a no-follow file"));
        }
        #[cfg(windows)]
        let mut reader = locron_core::filesystem::open_private_read_stable(&path)?;
        #[cfg(unix)]
        let mut reader = locron_core::filesystem::open_read_no_follow(&path)?;
        #[cfg(windows)]
        if leaf_identity(&reader)? != self.leaf_id {
            return Err(io::Error::other("PR145 path sentinel identity changed"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = reader.metadata()?;
            let id = Identity {
                device: metadata.dev(),
                inode: metadata.ino(),
            };
            if id != self.leaf_id {
                return Err(io::Error::other("PR145 path sentinel identity changed"));
            }
        }
        let mut bytes = Vec::new();
        #[cfg(windows)]
        (&mut *reader).take(4097).read_to_end(&mut bytes)?;
        #[cfg(unix)]
        (&mut *reader).take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 || Sha256::digest(&bytes) != Sha256::digest(SENTINEL) {
            return Err(io::Error::other("PR145 owned sentinel digest changed"));
        }
        if directory_identity(&self.path, deadline)? != self.root_id || !no_follow(&path)?.is_file()
        {
            return Err(io::Error::other("PR145 post-read identity changed"));
        }
        let paths = self.state().paths;
        for absent in [
            &paths.root,
            &paths.database,
            &paths.wake_socket,
            &paths.daemon_lock,
            &paths.dashboard_lock,
            &paths.outputs,
            &paths.temporary,
            &paths.root.join(crate::token::TOKEN_FILE_NAME),
            &paths.root.join("dashboard.token.lock"),
        ] {
            match fs::symlink_metadata(absent) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                _ => {
                    return Err(io::Error::other(
                        "PR145 private state was touched; quarantine",
                    ));
                }
            }
        }
        admit(deadline)
    }

    fn cleanup(self, deadline: Instant) -> io::Result<()> {
        self.check(deadline)?;
        let Self {
            path,
            parent,
            root_id,
            parent_id,
            leaf_id,
            root_guard,
            parent_guard,
            sentinel,
            #[cfg(windows)]
            initial_root,
        } = self;
        // Requests/native observations have terminated before releasing no-delete leaf handles.
        drop(sentinel);
        admit(deadline)?;
        if directory_identity(&path, deadline)? != root_id
            || directory_identity(&parent, deadline)? != parent_id
        {
            return Err(io::Error::other(
                "PR145 cleanup identity changed; quarantine",
            ));
        }
        if path_leaf_identity(&path.join("owned-sentinel"))? != leaf_id {
            return Err(io::Error::other(
                "PR145 cleanup leaf identity changed; quarantine",
            ));
        }
        fs::remove_file(path.join("owned-sentinel"))?;
        if fs::read_dir(&path)?.next().is_some() {
            return Err(io::Error::other("PR145 nonempty cleanup root; quarantine"));
        }
        admit(deadline)?;
        drop(root_guard);
        #[cfg(windows)]
        drop(initial_root);
        if directory_identity(&path, deadline)? != root_id
            || directory_identity(&parent, deadline)? != parent_id
        {
            return Err(io::Error::other(
                "PR145 cleanup root was replaced; quarantine",
            ));
        }
        fs::remove_dir(&path)?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => {
                return Err(io::Error::other(
                    "PR145 cleanup did not remove exactly the owned root",
                ));
            }
        }
        if directory_identity(&parent, deadline)? != parent_id {
            return Err(io::Error::other("PR145 cleanup parent identity changed"));
        }
        drop(parent_guard);
        admit(deadline)
    }
}

fn owner_call<T: Send + 'static>(
    deadline: Instant,
    operation: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> T {
    admit(deadline).expect("PR145 owner call within group deadline");
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let _ = tx.send(operation());
    });
    let result = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("PR145 owner deadline failed; unfinished worker/resources remain quarantined");
    let reap = deadline.min(Instant::now() + REQUEST_LIMIT);
    while !worker.is_finished() && Instant::now() < reap {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        worker.is_finished(),
        "PR145 owner did not terminate; resources quarantined"
    );
    worker
        .join()
        .expect("PR145 owner worker terminated without panic");
    admit(deadline).expect("PR145 owned work completed within its original deadline");
    result.expect("PR145 actual owned observation/cleanup succeeds")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Marker(&'static str);

struct CountedStream {
    chunks: std::collections::VecDeque<Result<Bytes, io::Error>>,
    polls: Arc<AtomicUsize>,
}

impl Stream for CountedStream {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.chunks.pop_front())
    }
}

#[derive(Clone)]
struct Probe {
    calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    capture: Arc<Mutex<Option<Capture>>>,
    consume: bool,
}

struct Capture {
    auth: AuthKind,
    method: Method,
    uri: axum::http::Uri,
    version: Version,
    headers: HeaderMap,
    marker: Marker,
    unrelated: u64,
    before_polls: usize,
    body: Vec<u8>,
}

async fn receiver(State(probe): State<Probe>, request: Request) -> Response {
    probe.calls.fetch_add(1, Ordering::SeqCst);
    let (parts, body) = request.into_parts();
    let before_polls = probe.polls.load(Ordering::SeqCst);
    let bytes = if probe.consume {
        to_bytes(body, PROBE_LIMIT)
            .await
            .unwrap_or_else(|_| panic!("PR145 actual probe capture failed; input is never printed"))
    } else {
        Bytes::new()
    };
    let capture = Capture {
        auth: *parts
            .extensions
            .get::<AuthKind>()
            .expect("PR145 actual AuthKind extension"),
        method: parts.method,
        uri: parts.uri,
        version: parts.version,
        headers: parts.headers,
        marker: parts
            .extensions
            .get::<Marker>()
            .expect("PR145 actual marker retained")
            .clone(),
        unrelated: *parts
            .extensions
            .get::<u64>()
            .expect("PR145 unrelated typed extension retained"),
        before_polls,
        body: bytes.to_vec(),
    };
    assert!(
        probe
            .capture
            .lock()
            .expect("PR145 capture lock")
            .replace(capture)
            .is_none(),
        "PR145 Next called more than once"
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(ACCEPTED))
        .expect("PR145 fixed nonreflecting response")
}

fn probe_router(state: AppState, probe: Probe) -> Router {
    Router::new()
        .fallback(receiver)
        .layer(middleware::from_fn_with_state(state.clone(), super::csrf))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            super::authenticate,
        ))
        .layer(middleware::from_fn_with_state(state.clone(), super::origin))
        .layer(middleware::from_fn_with_state(state.clone(), super::host))
        .layer(middleware::from_fn_with_state(
            state,
            super::referrer_policy,
        ))
        .with_state(probe)
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert!(
        value.len().is_multiple_of(2),
        "PR145 opaque fixture hex has complete bytes"
    );
    (0..value.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&value[i..i + 2], 16).expect("PR145 frozen opaque fixture byte")
        })
        .collect()
}

fn embedded_script() -> String {
    let index = crate::assets::Assets::get("index.html").expect("PR145 actual embedded index");
    let index = std::str::from_utf8(index.data.as_ref()).expect("PR145 actual index UTF8");
    let scripts = index
        .split(['\"', '\''])
        .filter(|path| {
            path.starts_with("/assets/")
                && path
                    .rsplit_once('.')
                    .is_some_and(|(_, extension)| extension == "js")
        })
        .collect::<Vec<_>>();
    assert!(
        scripts.len() == 1,
        "PR145 one actual referenced lowercase-js script"
    );
    assert!(
        crate::assets::Assets::get(scripts[0].trim_start_matches('/')).is_some(),
        "PR145 actual script embedded"
    );
    scripts[0].to_owned()
}

fn expand(value: &str) -> String {
    let values = [
        ("T", T.to_owned()),
        ("C", C.to_owned()),
        ("S", "b".repeat(64)),
        ("D", "d".repeat(64)),
        ("UPPER_T", T.to_ascii_uppercase()),
        ("UPPER_C", C.to_ascii_uppercase()),
        ("T63", T[..63].to_owned()),
        ("C63", C[..63].to_owned()),
        ("T65", format!("{T}a")),
        ("C65", format!("{C}a")),
        ("NONHEX64", "g".repeat(64)),
        ("T_TAIL", T[1..].to_owned()),
        ("C_TAIL", C[1..].to_owned()),
        ("ENTRY_REFERENCED_SCRIPT", embedded_script()),
    ];
    values
        .into_iter()
        .fold(value.to_owned(), |value, (key, replacement)| {
            value.replace(&format!("{{{key}}}"), &replacement)
        })
}

fn field(value: &Value) -> HeaderValue {
    if let Some(value) = value.as_str() {
        HeaderValue::from_str(&expand(value)).expect("PR145 readable actual header")
    } else {
        HeaderValue::from_bytes(&hex_bytes(
            value["hex"]
                .as_str()
                .expect("PR145 opaque actual header hex"),
        ))
        .expect("PR145 opaque actual HeaderValue")
    }
}

fn chunks(row: &Value) -> Vec<Result<Bytes, io::Error>> {
    let wire = &row["wire"];
    if let Some(stream) = wire.get("body_stream") {
        if let Some(chunks) = stream["chunks"].as_array() {
            return chunks
                .iter()
                .map(|chunk| {
                    if let Some(value) = chunk.as_str() {
                        Ok(Bytes::from(expand(value)))
                    } else if let Some(hex) = chunk["hex"].as_str() {
                        Ok(Bytes::from(hex_bytes(hex)))
                    } else {
                        Err(io::Error::other(expand(
                            chunk["controlled_stream_error"]
                                .as_str()
                                .expect("PR145 frozen actual stream fault"),
                        )))
                    }
                })
                .collect();
        }
        let count = usize::try_from(
            stream["exact_bytes"]
                .as_u64()
                .expect("PR145 selected form cap"),
        )
        .expect("PR145 finite form cap");
        assert!(
            (super::CSRF_FORM_LIMIT..=super::CSRF_FORM_LIMIT + 1).contains(&count),
            "PR145 exact selected cap boundary"
        );
        let mut bytes = format!("csrf_token={C}&padding=").into_bytes();
        bytes.resize(count, b'a');
        return vec![Ok(Bytes::from(bytes))];
    }
    let body = &wire["body"];
    let bytes = if let Some(body) = body.as_str() {
        expand(body).into_bytes()
    } else {
        let count = usize::try_from(
            body["exact_bytes"]
                .as_u64()
                .expect("PR145 exact selected JSON cap"),
        )
        .expect("PR145 finite JSON cap");
        assert!(
            count == PROBE_LIMIT,
            "PR145 exact default JSON overflow control"
        );
        let mut bytes = format!("{{\"token\":\"{T}\",\"padding\":\"").into_bytes();
        bytes.resize(count - 2, b'a');
        bytes.extend_from_slice(b"\"}");
        bytes
    };
    assert!(
        bytes.len() <= PROBE_LIMIT,
        "PR145 request bytes stay bounded"
    );
    vec![Ok(Bytes::from(bytes))]
}

fn request(row: &Value, polls: Arc<AtomicUsize>) -> Request {
    let wire = &row["wire"];
    let mut request = Request::builder()
        .method(wire["method"].as_str().expect("PR145 frozen method"))
        .uri(expand(wire["uri"].as_str().expect("PR145 frozen URI")))
        .version(Version::HTTP_11)
        .body(Body::from_stream(CountedStream {
            chunks: chunks(row).into(),
            polls,
        }))
        .expect("PR145 actual request");
    request
        .headers_mut()
        .append(header::HOST, HeaderValue::from_static("localhost:10824"));
    for (key, name) in [
        ("cookie_fields", header::COOKIE),
        ("authorization", header::AUTHORIZATION),
        (
            "csrf_echo",
            axum::http::HeaderName::from_static(CSRF_HEADER),
        ),
        ("content_type", header::CONTENT_TYPE),
    ] {
        for value in wire[key].as_array().expect("PR145 frozen header fields") {
            request.headers_mut().append(name.clone(), field(value));
        }
    }
    request
        .headers_mut()
        .append("x-pr145-canary", HeaderValue::from_static("HEADERCANARY"));
    request
        .extensions_mut()
        .insert(Marker("original typed extension"));
    request.extensions_mut().insert(145_u64);
    request
}

async fn bounded<T>(deadline: Instant, future: impl std::future::Future<Output = T>) -> T {
    admit(deadline).expect("PR145 request starts before group deadline");
    let deadline = deadline.min(Instant::now() + REQUEST_LIMIT);
    let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
        .await
        .expect("PR145 actual request deadline; no completion inferred");
    admit(deadline).expect("PR145 actual request completed within its original deadline");
    result
}

struct Outcome {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

async fn outcome(response: Response, deadline: Instant) -> Outcome {
    let (parts, body) = response.into_parts();
    let body = bounded(deadline, to_bytes(body, RESPONSE_LIMIT))
        .await
        .expect("PR145 bounded actual response bytes");
    Outcome {
        status: parts.status,
        headers: parts.headers,
        body,
    }
}

fn secret_free(outcome: &Outcome) {
    let body = String::from_utf8_lossy(&outcome.body);
    for canary in [
        T,
        C,
        &T.to_ascii_uppercase(),
        &C.to_ascii_uppercase(),
        &"b".repeat(64),
        &"d".repeat(64),
        "PATHCANARY",
        "QUERYCANARY",
        "HEADERCANARY",
        "BODYCANARY",
        "OPAQUECANARY",
        "MEDIACANARY",
    ] {
        assert!(
            !body.contains(canary),
            "PR145 response body must not reflect synthetic secret/canary"
        );
        for (name, value) in &outcome.headers {
            if name != header::SET_COOKIE {
                assert!(
                    !value
                        .as_bytes()
                        .windows(canary.len())
                        .any(|window| window == canary.as_bytes()),
                    "PR145 non-cookie response header must not reflect canary"
                );
            }
        }
    }
}

fn issued(outcome: &Outcome, count: usize) {
    let cookies = (&outcome.headers.get_all(header::SET_COOKIE))
        .into_iter()
        .collect::<Vec<_>>();
    assert!(cookies.len() == count, "PR145 actual Set-Cookie count");
    let mut names = BTreeSet::new();
    for value in cookies {
        let cookie = cookie::Cookie::parse(value.to_str().expect("PR145 issued readable cookie"))
            .expect("PR145 actual issuer cookie syntax");
        assert!(
            names.insert(cookie.name().to_owned()),
            "PR145 no duplicate issuance"
        );
        assert!(
            cookie.path() == Some("/")
                && cookie.same_site() == Some(cookie::SameSite::Lax)
                && cookie.max_age() == Some(cookie::time::Duration::seconds(7_776_000))
                && cookie.secure() != Some(true),
            "PR145 exact actual issuer attributes"
        );
        match cookie.name() {
            SESSION_COOKIE => assert!(
                cookie.value() == T && cookie.http_only() == Some(true),
                "PR145 exact authorized session issuance"
            ),
            CSRF_COOKIE => assert!(
                cookie.value().len() == 64
                    && cookie.value() != C
                    && cookie
                        .value()
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    && cookie.http_only() != Some(true),
                "PR145 actual fresh lowercase-hex CSRF issuance"
            ),
            _ => panic!("PR145 issuer added a foreign cookie"),
        }
    }
    match count {
        0 => assert!(names.is_empty(), "PR145 refusal has no cookie delta"),
        1 => assert!(
            names == BTreeSet::from([CSRF_COOKIE.to_owned()]),
            "PR145 missing CSRF status issues only CSRF"
        ),
        2 => assert!(
            names == BTreeSet::from([SESSION_COOKIE.to_owned(), CSRF_COOKIE.to_owned()]),
            "PR145 real successful paste alone issues the pair"
        ),
        _ => panic!("PR145 unknown issuance count"),
    }
}

fn envelope_for(profile: &str) -> Value {
    let error = match profile {
        "generic_auth" => Some((
            "unauthenticated",
            "a valid access token or session cookie is required",
        )),
        "csrf_cookie_refusal" => Some((
            "refused",
            "cookie-authenticated mutations require a CSRF token",
        )),
        "csrf_echo_refusal" => Some((
            "refused",
            "X-CSRF-Token does not match the csrf_token cookie",
        )),
        "secret_rejected" => Some(("unauthenticated", "access token rejected")),
        "session_success" => None,
        _ => panic!("PR145 unknown frozen envelope profile"),
    };
    if let Some((code, message)) = error {
        json!({"schema":"locron.api/v1","ok":false,"error":{"code":code,"message":message}})
    } else {
        json!({"schema":"locron.api/v1","ok":true,"data":{"authenticated":true},"warnings":[]})
    }
}

fn metadata(request: &Request) -> (Method, axum::http::Uri, Version, HeaderMap, Marker, u64) {
    (
        request.method().clone(),
        request.uri().clone(),
        request.version(),
        request.headers().clone(),
        request
            .extensions()
            .get::<Marker>()
            .expect("PR145 marker before helper")
            .clone(),
        *request
            .extensions()
            .get::<u64>()
            .expect("PR145 typed extension before helper"),
    )
}

async fn direct_form(row: &Value, deadline: Instant) {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut actual = request(row, polls.clone());
    actual.extensions_mut().insert(AuthKind::Session);
    let before = metadata(&actual);
    let field = bounded(deadline, super::csrf_from_form_field(&mut actual)).await;
    assert!(
        metadata(&actual) == before
            && actual.extensions().get::<AuthKind>() == Some(&AuthKind::Session),
        "PR145 real helper preserves all request metadata/extensions"
    );
    let disposition = row["expected"]
        .get("body_after_helper")
        .and_then(Value::as_str);
    let read_failed = disposition.is_some_and(|value| value.starts_with("EMPTY_AFTER_"));
    let restored = bounded(deadline, to_bytes(actual.into_body(), PROBE_LIMIT))
        .await
        .expect("PR145 restored helper body remains bounded");
    let original = chunks(row)
        .into_iter()
        .filter_map(Result::ok)
        .flat_map(|bytes| bytes.to_vec())
        .collect::<Vec<_>>();
    assert!(
        if read_failed {
            restored.is_empty()
        } else {
            restored.as_ref() == original
        },
        "PR145 exact actual helper restored/empty disposition"
    );
    if disposition.is_some() {
        assert!(
            polls.load(Ordering::SeqCst) > 0,
            "PR145 helper counts actual polls including EOF/error"
        );
    }
    if read_failed
        || matches!(
            row["wire"]["label"].as_str(),
            Some("invalid_utf8" | "empty_stream")
        )
    {
        assert!(
            field.is_none(),
            "PR145 actual failed/invalid/empty form helper has no field"
        );
    }
    if row["expected"]["paired_probe_next_calls"] == 1
        && row["expected"]["request_stream_polls"] == "POSITIVE_BEFORE_NEXT"
    {
        assert!(
            field.as_deref() == Some(C)
                || field.as_deref() == Some(C.to_ascii_uppercase().as_str()),
            "PR145 actual accepted form field exact"
        );
    }
}

fn state_name(state: CookieState<'_>) -> &'static str {
    match state {
        CookieState::Missing => "Missing",
        CookieState::UniqueValidHex(_) => "UniqueValidHex",
        CookieState::UniqueReadableMalformed => "UniqueReadableMalformed",
        CookieState::DuplicateOrUnreadable => "DuplicateOrUnreadable",
    }
}

async fn observe_row(row: &Value, state: &AppState, deadline: Instant) {
    let expected = &row["expected"];
    let polls = Arc::new(AtomicUsize::new(0));
    let actual = request(row, polls.clone());
    for (key, cookie) in [
        ("session_state", SESSION_COOKIE),
        ("csrf_state", CSRF_COOKIE),
    ] {
        if let Some(shape) = row["wire"][key].as_str() {
            let observed = super::cookie_value(actual.headers(), cookie);
            let shape_name = shape
                .split('/')
                .next()
                .expect("PR145 nonempty frozen state");
            assert!(
                state_name(observed) == shape_name,
                "PR145 actual typed state matches the frozen row"
            );
            if let CookieState::UniqueValidHex(value) = observed {
                match shape {
                    "UniqueValidHex/current" => assert!(
                        super::constant_time_eq(value, T),
                        "PR145 current typed session matches exact synthetic secret"
                    ),
                    "UniqueValidHex/stale" => assert!(
                        !super::constant_time_eq(value, T),
                        "PR145 stale typed session is a real independent negative"
                    ),
                    "UniqueValidHex" => {}
                    _ => panic!("PR145 unknown frozen valid state"),
                }
            }
        }
    }
    let original = metadata(&actual);
    let direct = row["adapter"].as_str().expect("PR145 frozen adapter") == "production_router";
    let flow = expected["request_stream_polls"]
        .as_str()
        .expect("PR145 frozen poll oracle");
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(None));
    let probe = Probe {
        calls: calls.clone(),
        polls: polls.clone(),
        capture: captured.clone(),
        consume: !direct || flow != "ZERO",
    };
    let response = bounded(deadline, probe_router(state.clone(), probe).oneshot(actual))
        .await
        .expect("PR145 actual probe service");
    let probe_out = outcome(response, deadline).await;
    let next = usize::try_from(
        expected["paired_probe_next_calls"]
            .as_u64()
            .expect("PR145 exact frozen Next count"),
    )
    .expect("PR145 finite Next count");
    assert!(
        calls.load(Ordering::SeqCst) == next,
        "PR145 actual Next count"
    );
    let capture = captured.lock().expect("PR145 capture lock").take();
    if next == 1 {
        let capture = capture.expect("PR145 real accepted Next capture");
        assert!(
            capture.method == original.0
                && capture.uri == original.1
                && capture.version == original.2
                && capture.headers == original.3
                && capture.marker == original.4
                && capture.unrelated == original.5,
            "PR145 actual accepted request metadata exact"
        );
        let auth = row["observed_auth"]
            .as_str()
            .expect("PR145 frozen accepted AuthKind");
        assert!(
            match auth {
                "Session" => capture.auth == AuthKind::Session,
                "Bearer" => capture.auth == AuthKind::Bearer,
                "Unauthenticated" => capture.auth == AuthKind::Unauthenticated,
                _ => false,
            },
            "PR145 actual independent AuthKind positive"
        );
        if flow == "POSITIVE_BEFORE_NEXT" {
            assert!(
                capture.before_polls > 0,
                "PR145 real form polls precede Next"
            );
        } else {
            assert!(
                capture.before_polls == 0,
                "PR145 no poll before accepted Next"
            );
        }
        if !direct || flow != "ZERO" {
            let bytes = chunks(row)
                .into_iter()
                .map(|part| part.expect("PR145 accepted body has no read error"))
                .flat_map(|part| part.to_vec())
                .collect::<Vec<_>>();
            assert!(capture.body == bytes, "PR145 real accepted full body exact");
            assert!(
                polls.load(Ordering::SeqCst) > 0,
                "PR145 positive real Next body polls"
            );
        } else {
            assert!(
                capture.body.is_empty() && polls.load(Ordering::SeqCst) == 0,
                "PR145 safe production/public body remains unpolled"
            );
        }
        assert!(
            probe_out.status == StatusCode::OK,
            "PR145 actual successful probe status"
        );
    } else {
        assert!(capture.is_none(), "PR145 refused Next was never entered");
        if flow == "ZERO" {
            assert!(
                polls.load(Ordering::SeqCst) == 0,
                "PR145 refusal actual body polls are zero"
            );
        } else {
            assert!(
                polls.load(Ordering::SeqCst) > 0,
                "PR145 actual late form refusal polls"
            );
        }
    }
    let measured = if direct {
        let actual_polls = Arc::new(AtomicUsize::new(0));
        let response = bounded(
            deadline,
            crate::router(state.clone()).oneshot(request(row, actual_polls.clone())),
        )
        .await
        .expect("PR145 real production service");
        let measured = outcome(response, deadline).await;
        if flow == "ZERO" {
            assert!(
                actual_polls.load(Ordering::SeqCst) == 0,
                "PR145 production pre-body refusal/public/status has zero polls"
            );
        } else {
            assert!(
                actual_polls.load(Ordering::SeqCst) > 0,
                "PR145 production body validation actually polls"
            );
        }
        measured
    } else {
        probe_out
    };
    assert!(
        measured.status.as_u16()
            == expected["status"].as_u64().expect("PR145 frozen status") as u16,
        "PR145 actual frozen response status"
    );
    assert!(
        measured.headers.get("referrer-policy") == Some(&HeaderValue::from_static("no-referrer")),
        "PR145 actual exact outer referrer policy"
    );
    secret_free(&measured);
    issued(
        &measured,
        usize::try_from(
            expected["set_cookie_fields"]
                .as_u64()
                .expect("PR145 frozen cookie count"),
        )
        .expect("PR145 finite cookie count"),
    );
    match expected["profile"].as_str().expect("PR145 frozen profile") {
        "probe_success" => assert!(
            measured.body.as_ref() == ACCEPTED
                || (original.0 == Method::HEAD && measured.body.is_empty()),
            "PR145 fixed nonreflecting probe response"
        ),
        "embedded_asset_exact" => {
            let path = original.1.path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            let asset =
                crate::assets::Assets::get(path).expect("PR145 actual expected embedded asset");
            assert!(
                measured.body.as_ref() == asset.data.as_ref(),
                "PR145 complete actual embedded bytes"
            );
            assert!(
                measured
                    .headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    == Some(asset.metadata.mimetype()),
                "PR145 actual asset content type"
            );
            assert!(
                measured.headers.get(header::CACHE_CONTROL)
                    == Some(&HeaderValue::from_static("no-cache")),
                "PR145 exact actual public cache policy"
            );
        }
        "json_extractor_rejection" => {
            // Observe the real unchanged extractor separately, including its plain disposition/text.
            let input = request(row, Arc::new(AtomicUsize::new(0)));
            let rejection = match bounded(
                deadline,
                Json::<crate::api::SessionRequest>::from_request(input, state),
            )
            .await
            {
                Err(rejection) => rejection.into_response(),
                Ok(_) => panic!(
                    "PR145 selected input unexpectedly accepted by the existing Json extractor; input is never printed"
                ),
            };
            let canonical = outcome(rejection, deadline).await;
            assert!(
                measured.status == canonical.status
                    && measured.body == canonical.body
                    && measured.headers.get(header::CONTENT_TYPE)
                        == canonical.headers.get(header::CONTENT_TYPE),
                "PR145 exact unchanged real Axum rejection status/body/content type"
            );
            assert!(
                measured
                    .headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.starts_with("text/plain")),
                "PR145 no fabricated Locron rejection envelope"
            );
        }
        profile => {
            let actual: Value =
                serde_json::from_slice(&measured.body).expect("PR145 real envelope syntax");
            assert!(
                actual == envelope_for(profile),
                "PR145 exact actual nonreflecting envelope"
            );
        }
    }
    if matches!(
        row["group"].as_str(),
        Some("body" | "form_field" | "media_type" | "csrf_header")
    ) || flow == "POSITIVE_BEFORE_NEXT"
    {
        direct_form(row, deadline).await;
    }
}

async fn group(name: &'static str, keys: &'static [&'static str]) {
    // This one deadline covers setup, every actual call, and checked nonrecursive teardown.
    let started = Instant::now();
    let deadline = started + GROUP_LIMIT;
    assert!(!keys.is_empty(), "PR145 zero-selected group forbidden");
    let all: Vec<Value> =
        serde_json::from_str(ROWS).expect("PR145 frozen190 complete row descriptors");
    assert!(all.len() == 190, "PR145 exact full backend row count");
    let all_keys = all
        .iter()
        .map(|row| row["id"].as_str().expect("PR145 row key"))
        .collect::<BTreeSet<_>>();
    assert!(
        all_keys.len() == 190,
        "PR145 no duplicate full backend keys"
    );
    let selected = all
        .iter()
        .filter(|row| row["group"] == name)
        .collect::<Vec<_>>();
    let expected = keys.iter().copied().collect::<BTreeSet<_>>();
    assert!(
        expected.len() == keys.len() && selected.len() == keys.len(),
        "PR145 exact selected literal key count"
    );
    let selected_keys = selected
        .iter()
        .map(|row| row["id"].as_str().expect("PR145 selected key"))
        .collect::<BTreeSet<_>>();
    assert!(
        selected_keys == expected,
        "PR145 no missing/unknown selected key"
    );
    let root = owner_call(deadline, move || OwnedRoot::create(deadline));
    let state = root.state();
    let mut pending = BTreeSet::new();
    for row in selected {
        observe_row(row, &state, deadline).await;
        assert!(
            pending.insert(row["id"].as_str().expect("PR145 checked pending key")),
            "PR145 duplicate pending control"
        );
    }
    assert!(
        pending == expected,
        "PR145 every selected row asserted before cleanup"
    );
    let cleanup_deadline = deadline.min(Instant::now() + REQUEST_LIMIT);
    owner_call(cleanup_deadline, move || root.cleanup(cleanup_deadline));
    // Publish completion only after actual termination, identity/inventory verification and teardown.
    let completed = pending;
    assert!(
        completed == expected,
        "PR145 every exact key completed after checked cleanup"
    );
    eprintln!(
        "PR145_COOKIE_LEDGER {}",
        json!({"group":name,"selected":keys,"completed":completed,
        "count":completed.len(),"arch":std::env::consts::ARCH,"elapsed_ms":started.elapsed().as_millis(),"cleanup":"checked"})
    );
}

// Whole frozen wires and expected oracles. New public6 supplements every original184 backend row.
const ROWS: &str = r###"[
{"id":"SC01-status","group":"session_cookie","adapter":"production_router","wire":{"label":"missing","method":"GET","uri":"/api/v1/session","cookie_fields":[],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"Missing","csrf_state":"Missing"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC01-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"missing","method":"POST","uri":"/api/v1/session","cookie_fields":[],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"Missing","csrf_state":"Missing"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"SC02-status","group":"session_cookie","adapter":"production_router","wire":{"label":"unrelated_only","method":"GET","uri":"/api/v1/session","cookie_fields":["other=OTHER"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"Missing","csrf_state":"Missing"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC02-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"unrelated_only","method":"POST","uri":"/api/v1/session","cookie_fields":["other=OTHER"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"Missing","csrf_state":"Missing"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"SC03-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_only","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"Missing"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":1,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC03-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_only","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"Missing"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC04-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_combined_with_csrf","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC04-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_combined_with_csrf","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC05-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_split_with_csrf","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC05-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_split_with_csrf","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC06-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_last_after_unrelated_fields","method":"GET","uri":"/api/v1/session","cookie_fields":["other=OTHER","csrf_token={C}","last=LAST; locron_session={T}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC06-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_last_after_unrelated_fields","method":"POST","uri":"/api/v1/session","cookie_fields":["other=OTHER","csrf_token={C}","last=LAST; locron_session={T}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/current","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"SC07-status","group":"session_cookie","adapter":"production_router","wire":{"label":"stale_wellformed_hex","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={S}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/stale","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC07-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"stale_wellformed_hex","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={S}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/stale","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"SC08-status","group":"session_cookie","adapter":"production_router","wire":{"label":"uppercase_secret_stale","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={UPPER_T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/stale","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC08-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"uppercase_secret_stale","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={UPPER_T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueValidHex/stale","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"SC09-status","group":"session_cookie","adapter":"production_router","wire":{"label":"empty_target","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session=; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC09-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"empty_target","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session=; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC10-status","group":"session_cookie","adapter":"production_router","wire":{"label":"bare_target","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC10-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"bare_target","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC11-status","group":"session_cookie","adapter":"production_router","wire":{"label":"short63","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T63}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC11-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"short63","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T63}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC12-status","group":"session_cookie","adapter":"production_router","wire":{"label":"long65","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T65}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC12-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"long65","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T65}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC13-status","group":"session_cookie","adapter":"production_router","wire":{"label":"nonhex64","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={NONHEX64}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC13-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"nonhex64","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={NONHEX64}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC14-status","group":"session_cookie","adapter":"production_router","wire":{"label":"quoted64","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session=\"{T}\"; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC14-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"quoted64","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session=\"{T}\"; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC15-status","group":"session_cookie","adapter":"production_router","wire":{"label":"space_after_equals","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session= {T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC15-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"space_after_equals","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session= {T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC16-status","group":"session_cookie","adapter":"production_router","wire":{"label":"space_before_equals","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session ={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC16-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"space_before_equals","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session ={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC17-status","group":"session_cookie","adapter":"production_router","wire":{"label":"percent_encoded_value","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session=%30{T_TAIL}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC17-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"percent_encoded_value","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session=%30{T_TAIL}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"UniqueReadableMalformed","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC18-status","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_single","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC18-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC19-status","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_different_single","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session={S}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC19-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_different_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session={S}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC20-status","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_split","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}","locron_session={T}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC20-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}","locron_session={T}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC21-status","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_different_split","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}","locron_session={S}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC21-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"duplicate_different_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}","locron_session={S}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC22-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_then_malformed_single","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session=%recoverable; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC22-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_then_malformed_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; locron_session=%recoverable; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC23-status","group":"session_cookie","adapter":"production_router","wire":{"label":"malformed_then_current_single","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session=%recoverable; locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC23-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"malformed_then_current_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session=%recoverable; locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"UniqueValidHex"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC24-status","group":"session_cookie","adapter":"production_router","wire":{"label":"current_then_unreadable_later_field","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}",{"hex":"4f504151554543414e415259ff"}],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC24-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"current_then_unreadable_later_field","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}",{"hex":"4f504151554543414e415259ff"}],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC25-status","group":"session_cookie","adapter":"production_router","wire":{"label":"unreadable_earlier_then_current","method":"GET","uri":"/api/v1/session","cookie_fields":[{"hex":"4f504151554543414e415259ff"},"locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"SC25-paste","group":"session_cookie","adapter":"production_router","wire":{"label":"unreadable_earlier_then_current","method":"POST","uri":"/api/v1/session","cookie_fields":[{"hex":"4f504151554543414e415259ff"},"locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","session_state":"DuplicateOrUnreadable","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC01","group":"csrf_cookie","adapter":"production_router","wire":{"label":"missing","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"Missing"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC02","group":"csrf_cookie","adapter":"production_router","wire":{"label":"current_lowercase_hex","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC03","group":"csrf_cookie","adapter":"production_router","wire":{"label":"uppercase_hex","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={UPPER_C}"],"authorization":[],"csrf_echo":["{UPPER_C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC04","group":"csrf_cookie","adapter":"production_router","wire":{"label":"combined_with_session","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC05","group":"csrf_cookie","adapter":"production_router","wire":{"label":"split_with_session","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}","other=OTHER"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC06","group":"csrf_cookie","adapter":"production_router","wire":{"label":"valid_last_after_unrelated","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","first=FIRST","last=LAST; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueValidHex"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC07","group":"csrf_cookie","adapter":"production_router","wire":{"label":"empty","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token="],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC08","group":"csrf_cookie","adapter":"production_router","wire":{"label":"bare","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC09","group":"csrf_cookie","adapter":"production_router","wire":{"label":"short63","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C63}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC10","group":"csrf_cookie","adapter":"production_router","wire":{"label":"long65","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C65}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC11","group":"csrf_cookie","adapter":"production_router","wire":{"label":"nonhex64","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={NONHEX64}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC12","group":"csrf_cookie","adapter":"production_router","wire":{"label":"quoted","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=\"{C}\""],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC13","group":"csrf_cookie","adapter":"production_router","wire":{"label":"leading_value_space","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token= {C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC14","group":"csrf_cookie","adapter":"production_router","wire":{"label":"key_space_before_equals","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token ={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC15","group":"csrf_cookie","adapter":"production_router","wire":{"label":"percent_encoded_hex","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=%61{C_TAIL}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC16","group":"csrf_cookie","adapter":"production_router","wire":{"label":"percent_recoverable","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"UniqueReadableMalformed"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"CC17","group":"csrf_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC18","group":"csrf_cookie","adapter":"production_router","wire":{"label":"duplicate_different_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}; csrf_token={D}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC19","group":"csrf_cookie","adapter":"production_router","wire":{"label":"duplicate_equal_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}","csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC20","group":"csrf_cookie","adapter":"production_router","wire":{"label":"duplicate_different_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}","csrf_token={D}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC21","group":"csrf_cookie","adapter":"production_router","wire":{"label":"malformed_then_valid_single","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=%recoverable; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC22","group":"csrf_cookie","adapter":"production_router","wire":{"label":"valid_then_malformed_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token={C}","csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC23","group":"csrf_cookie","adapter":"production_router","wire":{"label":"malformed_then_bare_split","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=%recoverable","csrf_token"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC24","group":"csrf_cookie","adapter":"production_router","wire":{"label":"malformed_then_unreadable_field","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}","csrf_token=%recoverable",{"hex":"4f504151554543414e415259ff"}],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"CC25","group":"csrf_cookie","adapter":"production_router","wire":{"label":"unreadable_then_malformed","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}",{"hex":"4f504151554543414e415259ff"},"csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","csrf_state":"DuplicateOrUnreadable"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"HD01","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"missing_form_fallback","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"HD02","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"valid_lowercase","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"HD03","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"valid_uppercase_matched_cookie","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={UPPER_C}"],"authorization":[],"csrf_echo":["{UPPER_C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={UPPER_C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"HD04","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"wellformed_uppercase_mismatch","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{UPPER_C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD05","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"empty","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[""],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD06","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"short63","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C63}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD07","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"long65","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C65}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD08","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"nonhex64","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{NONHEX64}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD09","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"quoted64","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["\"{C}\""],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD10","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"leading_whitespace","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[" {C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD11","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"duplicate_equal","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}","{C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD12","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"duplicate_different","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}","{D}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD13","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"unreadable","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[{"hex":"48454144455243414e415259ff"}],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"HD14","group":"csrf_header","adapter":"production_middleware_probe","wire":{"label":"comma_combined","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{C}, {C}"],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT01","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"missing","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT02","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"exact_form","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MT03","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"uppercase_form","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["APPLICATION/X-WWW-FORM-URLENCODED"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MT04","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"charset_parameter","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded; charset=utf-8"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MT05","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"trimmed_essence_ows","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":[" application/x-www-form-urlencoded "],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MT06","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"suffix_extra","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded+extra"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT07","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"prefixed_garbage","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["bad-application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT08","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"json","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT09","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"multipart","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["multipart/form-data; boundary=BODYCANARY"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT10","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"duplicate_equal","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded","application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT11","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"duplicate_different","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded","application/json"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT12","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"unreadable","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":[{"hex":"4d4544494143414e415259ff"}],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT13","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"empty","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":[""],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MT14","group":"media_type","adapter":"production_middleware_probe","wire":{"label":"ordinary_parameter","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded; ignored=PARAMCANARY"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF01","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"correct_single","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF02","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"unrelated_before","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"other=BODYCANARY&csrf_token={C}"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF03","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"unrelated_after","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&other=BODYCANARY"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF04","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"unrelated_bare","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"other&csrf_token={C}"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF05","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"other_empty","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"other=&csrf_token={C}"},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"FF06","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"case_different_target","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"CSRF_TOKEN={C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF07","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"duplicate_equal","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&csrf_token={C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF08","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"duplicate_different","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&csrf_token={D}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF09","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"bare_target","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF10","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"empty_target","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token="},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF11","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"short63","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C63}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF12","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"long65","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C65}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF13","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"nonhex","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={NONHEX64}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF14","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"quoted","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token=\"{C}\""},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF15","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"percent_encoded_key","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"%63srf_token={C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF16","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"percent_encoded_value","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token=%61{C_TAIL}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF17","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"plus_value","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token=+{C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF18","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"space_key","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token ={C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF19","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"space_value","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token= {C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF20","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"bare_then_valid","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token&csrf_token={C}"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"FF21","group":"form_field","adapter":"production_middleware_probe","wire":{"label":"valid_then_bare","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&csrf_token"},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"BD01","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"chunked_valid","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"chunks":["csrf_","token={C}","&other=BODYCANARY"]},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EXACT_RESTORED"},"observed_auth":"Session"},
{"id":"BD02","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"read_error_first","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"chunks":[{"controlled_stream_error":"first-read-BODYCANARY"}]},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EMPTY_AFTER_READ_ERROR_METADATA_RETAINED"},"observed_auth":"NOT_ENTERED"},
{"id":"BD03","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"read_error_after_prefix","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"chunks":["csrf_token=",{"controlled_stream_error":"after-prefix-BODYCANARY"}]},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EMPTY_AFTER_READ_ERROR_METADATA_RETAINED"},"observed_auth":"NOT_ENTERED"},
{"id":"BD04","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"exact_1mib","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"construction":"csrf_token={C}&padding= plus ASCII 'a' to exact 1_048_576 bytes","exact_bytes":1048576},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EXACT_RESTORED"},"observed_auth":"Session"},
{"id":"BD05","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"over_1mib","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"construction":"same valid prefix, exact 1_048_577 bytes","exact_bytes":1048577},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EMPTY_AFTER_LIMIT_ERROR_METADATA_RETAINED"},"observed_auth":"NOT_ENTERED"},
{"id":"BD06","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"invalid_utf8","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"chunks":["csrf_token={C}&other=",{"hex":"ff"}]},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EXACT_RESTORED"},"observed_auth":"NOT_ENTERED"},
{"id":"BD07","group":"body","adapter":"production_middleware_probe_and_direct_actual_form_helper","wire":{"label":"empty_stream","method":"POST","uri":"/api/v1/pr145-probe?query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body_stream":{"chunks":[],"count_eof_poll":true},"metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY","http_version":"HTTP/1.1"}},"expected":{"status":403,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","body_after_helper":"EXACT_RESTORED_EMPTY"},"observed_auth":"NOT_ENTERED"},
{"id":"MD01","group":"method","adapter":"production_middleware_probe","wire":{"label":"GET_malformed_csrf_boundary","method":"GET","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MD02","group":"method","adapter":"production_middleware_probe","wire":{"label":"HEAD_malformed_csrf_boundary","method":"HEAD","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MD03","group":"method","adapter":"production_middleware_probe","wire":{"label":"OPTIONS_malformed_csrf_boundary","method":"OPTIONS","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"MD04","group":"method","adapter":"production_middleware_probe","wire":{"label":"POST_malformed_csrf_boundary","method":"POST","uri":"/api/v1/pr145-probe","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MD05","group":"method","adapter":"production_middleware_probe","wire":{"label":"PUT_malformed_csrf_boundary","method":"PUT","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MD06","group":"method","adapter":"production_middleware_probe","wire":{"label":"PATCH_malformed_csrf_boundary","method":"PATCH","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"MD07","group":"method","adapter":"production_middleware_probe","wire":{"label":"DELETE_malformed_csrf_boundary","method":"DELETE","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"NOT_ENTERED"},
{"id":"PS01","group":"positive","adapter":"production_middleware_probe","wire":{"label":"bearer_lower_no_cookie","method":"POST","uri":"/api/v1/pr145-probe","authorization":["token {T}"],"cookie_fields":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Bearer"},"observed_auth":"Bearer"},
{"id":"PS02","group":"positive","adapter":"production_middleware_probe","wire":{"label":"bearer_title_malformed_cookie","method":"POST","uri":"/api/v1/pr145-probe","authorization":["Token {T}"],"cookie_fields":["locron_session=%bad; csrf_token=%recoverable"],"csrf_echo":[""],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Bearer"},"observed_auth":"Bearer"},
{"id":"PS03","group":"positive","adapter":"production_middleware_probe","wire":{"label":"bearer_duplicate_targets_and_echo","method":"POST","uri":"/api/v1/pr145-probe","authorization":["token {T}"],"cookie_fields":["locron_session={T}; locron_session={S}; csrf_token={C}; csrf_token={D}"],"csrf_echo":["{C}","{D}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Bearer"},"observed_auth":"Bearer"},
{"id":"PS04","group":"positive","adapter":"production_middleware_probe","wire":{"label":"bearer_unreadable_cookie_and_echo","method":"POST","uri":"/api/v1/pr145-probe","authorization":["token {T}"],"cookie_fields":[{"hex":"4f504151554543414e415259ff"}],"csrf_echo":[{"hex":"48454144455243414e415259ff"}],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Bearer"},"observed_auth":"Bearer"},
{"id":"PS05","group":"positive","adapter":"production_middleware_probe","wire":{"label":"session_correct_header_metadata","method":"POST","uri":"/api/v1/pr145-probe","authorization":[],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"probe\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"PS06","group":"positive","adapter":"production_middleware_probe","wire":{"label":"session_correct_form_exact_restore","method":"POST","uri":"/api/v1/pr145-probe","authorization":[],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&probe=BODYCANARY","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"PS07","group":"positive","adapter":"production_middleware_probe","wire":{"label":"session_uppercase_csrf_exact_match","method":"POST","uri":"/api/v1/pr145-probe","authorization":[],"cookie_fields":["locron_session={T}; csrf_token={UPPER_C}"],"csrf_echo":["{UPPER_C}"],"content_type":["application/json"],"body":"{\"probe\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"probe_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session"},"observed_auth":"Session"},
{"id":"PS08","group":"positive","adapter":"production_router","wire":{"label":"initial_paste_genuine_missing_session","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["csrf_token=%recoverable"],"csrf_echo":[""],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":200,"request_stream_polls":"ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"PA01","group":"paste_auth","adapter":"production_router","wire":{"label":"wrong_token_with_valid_session","method":"POST","uri":"/api/v1/session","authorization":["token {S}"],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA02","group":"paste_auth","adapter":"production_router","wire":{"label":"wrong_scheme_with_valid_session","method":"POST","uri":"/api/v1/session","authorization":["Bearer {T}"],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA03","group":"paste_auth","adapter":"production_router","wire":{"label":"empty_authorization","method":"POST","uri":"/api/v1/session","authorization":[""],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA04","group":"paste_auth","adapter":"production_router","wire":{"label":"duplicate_equal_valid_authorization","method":"POST","uri":"/api/v1/session","authorization":["token {T}","token {T}"],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA05","group":"paste_auth","adapter":"production_router","wire":{"label":"duplicate_valid_invalid_authorization","method":"POST","uri":"/api/v1/session","authorization":["token {T}","token {S}"],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA06","group":"paste_auth","adapter":"production_router","wire":{"label":"unreadable_authorization","method":"POST","uri":"/api/v1/session","authorization":[{"hex":"4155544843414e415259ff"}],"cookie_fields":["locron_session={T}; csrf_token={C}"],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA07","group":"paste_auth","adapter":"production_router","wire":{"label":"malformed_session_absent_authorization","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session=%bad; csrf_token={C}"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA08","group":"paste_auth","adapter":"production_router","wire":{"label":"duplicate_session_absent_authorization","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; locron_session={T}; csrf_token={C}"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA09","group":"paste_auth","adapter":"production_router","wire":{"label":"unreadable_cookie_absent_authorization","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":[{"hex":"4f504151554543414e415259ff"}],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PA10","group":"paste_auth","adapter":"production_router","wire":{"label":"stale_wellformed_session_repaste","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={S}; csrf_token={C}; csrf_token=%recoverable"],"csrf_echo":[""],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Unauthenticated"},"observed_auth":"Unauthenticated"},
{"id":"PA11","group":"paste_auth","adapter":"production_router","wire":{"label":"valid_bearer_independent_paste","method":"POST","uri":"/api/v1/session","authorization":["token {T}"],"cookie_fields":["locron_session=%bad; locron_session={S}; csrf_token=%recoverable"],"csrf_echo":[""],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Bearer"},"observed_auth":"Bearer"},
{"id":"RB01","group":"recovery_body","adapter":"production_router","wire":{"label":"exact_secret","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"POSITIVE","set_cookie_fields":2,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB02","group":"recovery_body","adapter":"production_router","wire":{"label":"wrong_wellformed_secret","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{S}\"}"},"expected":{"status":401,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"secret_rejected","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB03","group":"recovery_body","adapter":"production_router","wire":{"label":"uppercase_secret_wrong_case","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{UPPER_T}\"}"},"expected":{"status":401,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"secret_rejected","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB04","group":"recovery_body","adapter":"production_router","wire":{"label":"missing_token","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"other\":\"BODYCANARY\"}"},"expected":{"status":422,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB05","group":"recovery_body","adapter":"production_router","wire":{"label":"token_integer","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":17}"},"expected":{"status":422,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB06","group":"recovery_body","adapter":"production_router","wire":{"label":"token_array","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":[\"BODYCANARY\"]}"},"expected":{"status":422,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB07","group":"recovery_body","adapter":"production_router","wire":{"label":"token_object","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":{\"key\":\"BODYCANARY\"}}"},"expected":{"status":422,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB08","group":"recovery_body","adapter":"production_router","wire":{"label":"token_null","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":null}"},"expected":{"status":422,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB09","group":"recovery_body","adapter":"production_router","wire":{"label":"malformed_json","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"BODYCANARY\""},"expected":{"status":400,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB10","group":"recovery_body","adapter":"production_router","wire":{"label":"correct_secret_form_media","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"token={T}&csrf_token={C}"},"expected":{"status":415,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB11","group":"recovery_body","adapter":"production_router","wire":{"label":"correct_secret_missing_media","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":[],"body":"{\"token\":\"{T}\"}"},"expected":{"status":415,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"RB12","group":"recovery_body","adapter":"production_router","wire":{"label":"json_over_default_2mib","method":"POST","uri":"/api/v1/session","authorization":[],"cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"csrf_echo":[],"content_type":["application/json"],"body":{"construction":"{\"token\":\"{T}\",\"padding\":\"ASCII a padding\"}","exact_bytes":2097153}},"expected":{"status":413,"request_stream_polls":"POSITIVE","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer","auth_extension":"Session","probe_status":200,"api_handler_body_validation":"exact existing Json/constant_time_eq path; no replacement validator"},"observed_auth":"Session"},
{"id":"BN01","group":"boundary","adapter":"production_router","wire":{"label":"status_genuine_missing_csrf","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":1,"paired_probe_next_calls":1,"profile":"session_success","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Session"},
{"id":"BN02","group":"boundary","adapter":"production_router","wire":{"label":"status_readable_malformed_csrf","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"BN03","group":"boundary","adapter":"production_router","wire":{"label":"status_duplicate_csrf","method":"GET","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"BN04","group":"boundary","adapter":"production_router","wire":{"label":"public_entry_session_malformed_csrf_safe","method":"GET","uri":"/","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Session"},
{"id":"BN05","group":"boundary","adapter":"production_router","wire":{"label":"public_referenced_script_missing_session","method":"GET","uri":"{ENTRY_REFERENCED_SCRIPT}","cookie_fields":[],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"BN06","group":"boundary","adapter":"production_router","wire":{"label":"valid_csrf_paste_missing_echo_json","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"BN07","group":"boundary","adapter":"production_router","wire":{"label":"valid_csrf_paste_wrong_echo","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":["{D}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\"}"},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_echo_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"BN08","group":"boundary","adapter":"production_router","wire":{"label":"valid_csrf_paste_good_form_json_handler_415","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token={C}"],"authorization":[],"csrf_echo":[],"content_type":["application/x-www-form-urlencoded"],"body":"csrf_token={C}&token={T}"},"expected":{"status":415,"request_stream_polls":"POSITIVE_BEFORE_NEXT","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"json_extractor_rejection","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Session"},
{"id":"GR01","group":"guard","adapter":"production_router","wire":{"label":"recovery_present_valid_echo","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":["{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR02","group":"guard","adapter":"production_router","wire":{"label":"recovery_present_empty_malformed_echo","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[""],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR03","group":"guard","adapter":"production_router","wire":{"label":"recovery_duplicate_equal_echo","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":["{C}","{C}"],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR04","group":"guard","adapter":"production_router","wire":{"label":"recovery_unreadable_echo","method":"POST","uri":"/api/v1/session","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[{"hex":"48454144455243414e415259ff"}],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR05","group":"guard","adapter":"production_middleware_probe","wire":{"label":"trailing_slash_is_not_recovery","method":"POST","uri":"/api/v1/session/","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR06","group":"guard","adapter":"production_middleware_probe","wire":{"label":"encoded_path_is_not_recovery","method":"POST","uri":"/api/v1/%73ession","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":403,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"csrf_cookie_refusal","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"GR07","group":"guard","adapter":"production_router","wire":{"label":"generic_status_canary_body_suppression","method":"GET","uri":"/api/v1/session?token={T}&path=PATHCANARY&query=QUERYCANARY","cookie_fields":["locron_session={T}; csrf_token=%recoverable"],"authorization":[],"csrf_echo":[],"content_type":["application/json"],"body":"{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}","metadata":{"marker":"original typed extension","extra_header":"HEADERCANARY"}},"expected":{"status":401,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":0,"profile":"generic_auth","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"NOT_ENTERED"},
{"id":"PUBLIC-MALFORMED-ENTRY","group":"public","adapter":"production_router","wire":{"label":"public_malformed_entry","method":"GET","uri":"/","cookie_fields":["locron_session=not-hex"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"PUBLIC-MALFORMED-SCRIPT","group":"public","adapter":"production_router","wire":{"label":"public_malformed_script","method":"GET","uri":"{ENTRY_REFERENCED_SCRIPT}","cookie_fields":["locron_session=not-hex"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"PUBLIC-DUPLICATE-ENTRY","group":"public","adapter":"production_router","wire":{"label":"public_duplicate_entry","method":"GET","uri":"/","cookie_fields":["locron_session={T}; locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"PUBLIC-DUPLICATE-SCRIPT","group":"public","adapter":"production_router","wire":{"label":"public_duplicate_script","method":"GET","uri":"{ENTRY_REFERENCED_SCRIPT}","cookie_fields":["locron_session={T}; locron_session={T}"],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"PUBLIC-UNREADABLE-ENTRY","group":"public","adapter":"production_router","wire":{"label":"public_unreadable_entry","method":"GET","uri":"/","cookie_fields":[{"hex":"6c6f63726f6e5f73657373696f6e3dff"}],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"},
{"id":"PUBLIC-UNREADABLE-SCRIPT","group":"public","adapter":"production_router","wire":{"label":"public_unreadable_script","method":"GET","uri":"{ENTRY_REFERENCED_SCRIPT}","cookie_fields":[{"hex":"6c6f63726f6e5f73657373696f6e3dff"}],"authorization":[],"csrf_echo":[],"content_type":[],"body":"BODYCANARY"},"expected":{"status":200,"request_stream_polls":"ZERO","set_cookie_fields":0,"paired_probe_next_calls":1,"profile":"embedded_asset_exact","auth_extension":"Unauthenticated","state_effects":"ZERO","referrer_policy":"no-referrer"},"observed_auth":"Unauthenticated"}
]"###;

#[tokio::test]
async fn dashboard_cookie_session_cookie_completes_all_50_rows() {
    const KEYS: &[&str] = &[
        "SC01-status",
        "SC01-paste",
        "SC02-status",
        "SC02-paste",
        "SC03-status",
        "SC03-paste",
        "SC04-status",
        "SC04-paste",
        "SC05-status",
        "SC05-paste",
        "SC06-status",
        "SC06-paste",
        "SC07-status",
        "SC07-paste",
        "SC08-status",
        "SC08-paste",
        "SC09-status",
        "SC09-paste",
        "SC10-status",
        "SC10-paste",
        "SC11-status",
        "SC11-paste",
        "SC12-status",
        "SC12-paste",
        "SC13-status",
        "SC13-paste",
        "SC14-status",
        "SC14-paste",
        "SC15-status",
        "SC15-paste",
        "SC16-status",
        "SC16-paste",
        "SC17-status",
        "SC17-paste",
        "SC18-status",
        "SC18-paste",
        "SC19-status",
        "SC19-paste",
        "SC20-status",
        "SC20-paste",
        "SC21-status",
        "SC21-paste",
        "SC22-status",
        "SC22-paste",
        "SC23-status",
        "SC23-paste",
        "SC24-status",
        "SC24-paste",
        "SC25-status",
        "SC25-paste",
    ];
    group("session_cookie", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_csrf_cookie_completes_all_25_rows() {
    const KEYS: &[&str] = &[
        "CC01", "CC02", "CC03", "CC04", "CC05", "CC06", "CC07", "CC08", "CC09", "CC10", "CC11",
        "CC12", "CC13", "CC14", "CC15", "CC16", "CC17", "CC18", "CC19", "CC20", "CC21", "CC22",
        "CC23", "CC24", "CC25",
    ];
    group("csrf_cookie", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_csrf_header_completes_all_14_rows() {
    const KEYS: &[&str] = &[
        "HD01", "HD02", "HD03", "HD04", "HD05", "HD06", "HD07", "HD08", "HD09", "HD10", "HD11",
        "HD12", "HD13", "HD14",
    ];
    group("csrf_header", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_media_type_completes_all_14_rows() {
    const KEYS: &[&str] = &[
        "MT01", "MT02", "MT03", "MT04", "MT05", "MT06", "MT07", "MT08", "MT09", "MT10", "MT11",
        "MT12", "MT13", "MT14",
    ];
    group("media_type", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_form_field_completes_all_21_rows() {
    const KEYS: &[&str] = &[
        "FF01", "FF02", "FF03", "FF04", "FF05", "FF06", "FF07", "FF08", "FF09", "FF10", "FF11",
        "FF12", "FF13", "FF14", "FF15", "FF16", "FF17", "FF18", "FF19", "FF20", "FF21",
    ];
    group("form_field", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_body_completes_all_7_rows() {
    const KEYS: &[&str] = &["BD01", "BD02", "BD03", "BD04", "BD05", "BD06", "BD07"];
    group("body", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_method_completes_all_7_rows() {
    const KEYS: &[&str] = &["MD01", "MD02", "MD03", "MD04", "MD05", "MD06", "MD07"];
    group("method", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_positive_completes_all_8_rows() {
    const KEYS: &[&str] = &[
        "PS01", "PS02", "PS03", "PS04", "PS05", "PS06", "PS07", "PS08",
    ];
    group("positive", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_paste_auth_completes_all_11_rows() {
    const KEYS: &[&str] = &[
        "PA01", "PA02", "PA03", "PA04", "PA05", "PA06", "PA07", "PA08", "PA09", "PA10", "PA11",
    ];
    group("paste_auth", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_recovery_body_completes_all_12_rows() {
    const KEYS: &[&str] = &[
        "RB01", "RB02", "RB03", "RB04", "RB05", "RB06", "RB07", "RB08", "RB09", "RB10", "RB11",
        "RB12",
    ];
    group("recovery_body", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_boundary_completes_all_8_rows() {
    const KEYS: &[&str] = &[
        "BN01", "BN02", "BN03", "BN04", "BN05", "BN06", "BN07", "BN08",
    ];
    group("boundary", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_guard_completes_all_7_rows() {
    const KEYS: &[&str] = &["GR01", "GR02", "GR03", "GR04", "GR05", "GR06", "GR07"];
    group("guard", KEYS).await;
}

#[tokio::test]
async fn dashboard_cookie_public_completes_all_6_rows() {
    const KEYS: &[&str] = &[
        "PUBLIC-MALFORMED-ENTRY",
        "PUBLIC-MALFORMED-SCRIPT",
        "PUBLIC-DUPLICATE-ENTRY",
        "PUBLIC-DUPLICATE-SCRIPT",
        "PUBLIC-UNREADABLE-ENTRY",
        "PUBLIC-UNREADABLE-SCRIPT",
    ];
    group("public", KEYS).await;
}
