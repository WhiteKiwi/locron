//! Bounded local hints and lifetime-scoped role control, without public async types.

use std::io;
use std::path::Path;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
const CLIENT_TIMEOUT: Duration = Duration::from_millis(200);

/// Wake framing shared by the secured Windows client and listener.
pub const WAKE_MESSAGE: &[u8] = b"locron-wake/v1\n";
/// Role control is accepted only on the separate lifetime endpoint.
pub const SHUTDOWN_MESSAGE: &[u8] = b"locron-stop/v1\n";
/// Acknowledges consumption, never actual role exit or target completion.
pub const ACK_MESSAGE: &[u8] = b"locron-ack/v1\n";

/// Passive transport facts shared by CLI, API and MCP diagnostics.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WakeFacts {
    /// Platform transport, independent of endpoint availability.
    pub transport: &'static str,
    /// Legacy Unix filesystem-presence fact; named pipes have no socket file.
    pub socket_present: Option<bool>,
    /// Diagnostics never send a hint to claim endpoint availability.
    pub availability: &'static str,
}

/// Reports facts without creating state, connecting to a peer or delivering a hint.
#[must_use]
pub fn wake_facts(root: &Path) -> WakeFacts {
    WakeFacts {
        transport: if cfg!(windows) {
            "named_pipe"
        } else if cfg!(unix) {
            "unix_datagram"
        } else {
            "unsupported"
        },
        socket_present: if cfg!(unix) {
            Some(root.join("wake.sock").exists())
        } else {
            None
        },
        availability: "unprobed",
    }
}

/// Sends an already-durable command's best-effort wake hint.
pub fn send_wake(root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let socket = std::os::unix::net::UnixDatagram::unbound()?;
        socket.connect(root.join("wake.sock"))?;
        socket.send(b"locron-wake/v1")?;
        Ok(())
    }
    #[cfg(windows)]
    {
        send_message(&endpoint_name(root, "wake", None)?, WAKE_MESSAGE)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = root;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "local wake is unavailable",
        ))
    }
}

/// Closed stage of an actually returned Windows debug wake error.
#[cfg(all(windows, debug_assertions))]
#[derive(Clone, Copy)]
pub enum WakePipeFailureStage {
    /// Guarded endpoint derivation returned an error.
    Name,
    /// Native client open returned a terminal error.
    Open,
    /// The frame exchange returned an error.
    Exchange,
    /// Existing worker, runtime or deadline infrastructure returned an error.
    Infrastructure,
}

/// Observes one best-effort wake call without repeating any operation.
///
/// The original owned result is returned with a stage only when it failed.
#[cfg(all(windows, debug_assertions))]
pub fn send_wake_with_stage(root: &Path) -> (io::Result<()>, Option<WakePipeFailureStage>) {
    let name = match endpoint_name(root, "wake", None) {
        Ok(name) => name,
        Err(error) => return (Err(error), Some(WakePipeFailureStage::Name)),
    };
    match send_message_reply(&name, WAKE_MESSAGE, None, client_runtime) {
        Ok(()) => (Ok(()), None),
        Err(error) => (Err(error.error), Some(error.stage)),
    }
}

#[cfg(all(windows, debug_assertions))]
struct MessageError {
    error: io::Error,
    stage: WakePipeFailureStage,
}

#[cfg(all(windows, debug_assertions))]
impl From<io::Error> for MessageError {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            stage: WakePipeFailureStage::Infrastructure,
        }
    }
}

#[cfg(all(windows, not(debug_assertions)))]
type MessageError = io::Error;

#[cfg(windows)]
fn pipe_open_error(error: io::Error) -> MessageError {
    #[cfg(debug_assertions)]
    {
        MessageError {
            error,
            stage: WakePipeFailureStage::Open,
        }
    }
    #[cfg(not(debug_assertions))]
    {
        error
    }
}

#[cfg(windows)]
fn pipe_exchange_error(error: io::Error) -> MessageError {
    #[cfg(debug_assertions)]
    {
        MessageError {
            error,
            stage: WakePipeFailureStage::Exchange,
        }
    }
    #[cfg(not(debug_assertions))]
    {
        error
    }
}

/// Requests graceful shutdown of one registered role's exact lock lifetime.
///
/// Success acknowledges delivery only. The caller must confirm actual lock/lifetime exit.
pub fn request_shutdown(root: &Path, role: &str, lifetime: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        send_message(
            &endpoint_name(root, role, Some(lifetime))?,
            SHUTDOWN_MESSAGE,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (root, role, lifetime);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "use platform role termination signals",
        ))
    }
}

/// Requests exact-lifetime shutdown within the caller's remaining absolute budget.
///
/// The retained guard and cached SID avoid filesystem/SID initialization. Success acknowledges
/// delivery only; a timeout can follow queued I/O and never proves nondelivery or actual exit.
#[cfg(windows)]
pub fn request_shutdown_guarded_until(
    guard: &crate::filesystem::DirectoryGuard,
    role: &str,
    lifetime: &str,
    deadline: Instant,
) -> io::Result<()> {
    let entered = Instant::now();
    let deadline = deadline.min(
        entered
            .checked_add(CLIENT_TIMEOUT)
            .ok_or_else(control_deadline_error)?,
    );
    ensure_deadline(Some(deadline))?;
    let lifetime = endpoint_lifetime(role, Some(lifetime))?;
    let sid = crate::windows::cached_current_user_sid()?;
    ensure_deadline(Some(deadline))?;
    let identity = instance_identity_for_sid(guard, &sid)?;
    ensure_deadline(Some(deadline))?;
    let name = endpoint_name_from_identity(&identity, role, lifetime);
    send_message_with_runtime(&name, SHUTDOWN_MESSAGE, Some(deadline), client_runtime)
}

/// Stable account/state identity for local endpoints and registered role names.
#[cfg(windows)]
pub fn instance_identity(root: &Path) -> io::Result<String> {
    instance_identity_guarded(&crate::filesystem::DirectoryGuard::existing_private(root)?)
}

/// Queries the guarded object while its complete no-write/no-delete-sharing chain is live.
#[cfg(windows)]
pub fn instance_identity_guarded(guard: &crate::filesystem::DirectoryGuard) -> io::Result<String> {
    let sid = crate::windows::current_user_sid()?;
    instance_identity_for_sid(guard, &sid)
}

#[cfg(windows)]
fn instance_identity_for_sid(
    guard: &crate::filesystem::DirectoryGuard,
    sid: &str,
) -> io::Result<String> {
    let file_id::FileId::HighRes {
        volume_serial_number,
        file_id,
    } = file_id::get_high_res_file_id(guard.normalized_path()).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("state filesystem cannot provide full file identity: {error}"),
        )
    })?
    else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "state filesystem cannot provide full file identity",
        ));
    };
    identity_digest(sid, volume_serial_number, file_id)
}

#[cfg(windows)]
fn identity_digest(sid: &str, volume: u64, file_id: u128) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"locron-instance/v1\0");
    digest.update(
        u32::try_from(sid.len())
            .map_err(io::Error::other)?
            .to_le_bytes(),
    );
    digest.update(sid.as_bytes());
    digest.update(volume.to_le_bytes());
    digest.update(file_id.to_le_bytes());
    Ok(format!("{:x}", digest.finalize()))
}

/// Stable local-only identity from verified SID, guarded state, role and lifetime.
#[cfg(windows)]
pub fn endpoint_name(root: &Path, role: &str, lifetime: Option<&str>) -> io::Result<String> {
    endpoint_name_guarded(
        &crate::filesystem::DirectoryGuard::existing_private(root)?,
        role,
        lifetime,
    )
}

/// Derives a listener's name from the exact state guard it will retain through teardown.
#[cfg(windows)]
pub fn endpoint_name_guarded(
    guard: &crate::filesystem::DirectoryGuard,
    role: &str,
    lifetime: Option<&str>,
) -> io::Result<String> {
    let lifetime = endpoint_lifetime(role, lifetime)?;
    let identity = instance_identity_guarded(guard)?;
    Ok(endpoint_name_from_identity(&identity, role, lifetime))
}

#[cfg(windows)]
fn endpoint_lifetime(role: &str, lifetime: Option<&str>) -> io::Result<Option<uuid::Uuid>> {
    let lifetime = lifetime
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid role/lifetime endpoint",
            )
        })?;
    if !matches!(
        role,
        "wake"
            | "daemon"
            | "dashboard"
            | "daemon-activation"
            | "dashboard-activation"
            | "daemon-worker"
            | "dashboard-worker"
    ) || role != "wake" && lifetime.is_none()
        || role == "wake" && lifetime.is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid role/lifetime endpoint",
        ));
    }
    Ok(lifetime)
}

#[cfg(windows)]
fn endpoint_name_from_identity(identity: &str, role: &str, lifetime: Option<uuid::Uuid>) -> String {
    let lifetime = lifetime.map(|value| value.simple().to_string());
    match lifetime {
        Some(lifetime) => format!(r"\\.\pipe\locron-v1-{identity}-{role}-{lifetime}"),
        None => format!(r"\\.\pipe\locron-v1-{identity}-{role}"),
    }
}

#[cfg(windows)]
fn send_message(name: &str, message: &'static [u8]) -> io::Result<()> {
    send_message_with_runtime(name, message, None, client_runtime)
}

#[cfg(windows)]
fn client_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

#[cfg(windows)]
fn control_deadline_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "local control deadline elapsed; delivery may be uncertain",
    )
}

#[cfg(windows)]
fn ensure_deadline(deadline: Option<Instant>) -> io::Result<()> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        Err(control_deadline_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
async fn deadline_io<T>(
    deadline: Option<Instant>,
    operation: impl std::future::Future<Output = io::Result<T>>,
) -> io::Result<T> {
    let mut operation = std::pin::pin!(operation);
    std::future::poll_fn(|context| {
        if let Err(error) = ensure_deadline(deadline) {
            return std::task::Poll::Ready(Err(error));
        }
        match operation.as_mut().poll(context) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(result) => {
                // A write polled before expiry may already be queued; do not claim nondelivery.
                std::task::Poll::Ready(ensure_deadline(deadline).and(result))
            }
        }
    })
    .await
}

#[cfg(windows)]
async fn exchange_frame(
    stream: &mut (impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin),
    message: &'static [u8],
    deadline: Option<Instant>,
) -> io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let size = u8::try_from(message.len()).map_err(io::Error::other)?;
    deadline_io(deadline, stream.write_u8(size)).await?;
    deadline_io(deadline, stream.write_all(message)).await?;
    let mut ack = [0; ACK_MESSAGE.len()];
    deadline_io(deadline, stream.read_exact(&mut ack)).await?;
    if ack != ACK_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid local hint acknowledgement",
        ));
    }
    deadline_io(deadline, stream.write_u8(0xff)).await
}

#[cfg(windows)]
fn send_message_with_runtime(
    name: &str,
    message: &'static [u8],
    deadline: Option<Instant>,
    initialize: impl FnOnce() -> io::Result<tokio::runtime::Runtime> + Send + 'static,
) -> io::Result<()> {
    let result = send_message_reply(name, message, deadline, initialize);
    #[cfg(debug_assertions)]
    {
        result.map_err(|error| error.error)
    }
    #[cfg(not(debug_assertions))]
    {
        result
    }
}

#[cfg(windows)]
fn send_message_reply(
    name: &str,
    message: &'static [u8],
    deadline: Option<Instant>,
    initialize: impl FnOnce() -> io::Result<tokio::runtime::Runtime> + Send + 'static,
) -> Result<(), MessageError> {
    ensure_deadline(deadline)?;
    let name = name.to_owned();
    ensure_deadline(deadline)?;
    // The joined worker avoids nested runtime panics and never outlives this call.
    std::thread::Builder::new()
        .name("locron-ipc-hint".into())
        .spawn(move || {
            ensure_deadline(deadline)?;
            let runtime = initialize()?;
            ensure_deadline(deadline)?;
            runtime.block_on(async {
                let exchange = async {
                    let mut options = tokio::net::windows::named_pipe::ClientOptions::new();
                    // SECURITY_IDENTIFICATION; Tokio also sets SECURITY_SQOS_PRESENT.
                    options.security_qos_flags(0x0001_0000);
                    let mut stream = loop {
                        ensure_deadline(deadline)?;
                        match options.open(&name) {
                            Ok(stream) => break stream,
                            Err(error) if error.raw_os_error() == Some(231) => {
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                            Err(error) => return Err(pipe_open_error(error)),
                        }
                    };
                    exchange_frame(&mut stream, message, deadline)
                        .await
                        .map_err(pipe_exchange_error)
                };
                match deadline {
                    Some(deadline) => {
                        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), exchange)
                            .await
                            .map_err(|_| control_deadline_error())?
                    }
                    None => tokio::time::timeout(CLIENT_TIMEOUT, exchange)
                        .await
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::TimedOut, "local hint deadline elapsed")
                        })?,
                }
            })
        })?
        .join()
        .map_err(|_| io::Error::other("local hint worker failed"))?
}

#[cfg(test)]
mod passive_tests {
    #[test]
    fn diagnostic_facts_never_create_missing_state() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing");
        let facts = super::wake_facts(&root);
        assert_eq!(facts.availability, "unprobed");
        assert!(!root.exists());
        #[cfg(windows)]
        {
            assert_eq!(facts.transport, "named_pipe");
            assert_eq!(facts.socket_present, None);
        }
        #[cfg(unix)]
        {
            assert_eq!(facts.transport, "unix_datagram");
            assert_eq!(facts.socket_present, Some(false));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::filesystem::DirectoryGuard;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};

    fn sleep_past(deadline: Instant) {
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
        );
    }

    #[derive(Default)]
    struct WireState {
        written: Vec<u8>,
        receipt_polls: usize,
        receipt_ready: bool,
        ack_crosses: Option<Instant>,
        receipt_crosses: Option<Instant>,
    }

    struct FixtureWire(Arc<Mutex<WireState>>);

    impl tokio::io::AsyncRead for FixtureWire {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let crossing = self.0.lock().unwrap().ack_crosses.take();
            if let Some(deadline) = crossing {
                // Deliberately crosses the deadline inside an already-ready I/O poll.
                sleep_past(deadline);
            }
            buffer.put_slice(&ACK_MESSAGE[..buffer.remaining().min(ACK_MESSAGE.len())]);
            Poll::Ready(Ok(()))
        }
    }

    impl tokio::io::AsyncWrite for FixtureWire {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            buffer: &[u8],
        ) -> Poll<io::Result<usize>> {
            let mut state = self.0.lock().unwrap();
            let crossing = if buffer == [0xff] {
                state.receipt_polls += 1;
                if !state.receipt_ready {
                    return Poll::Pending;
                }
                state.receipt_crosses.take()
            } else {
                None
            };
            state.written.extend_from_slice(buffer);
            drop(state);
            if let Some(deadline) = crossing {
                // A pre-expiry I/O poll can queue the byte before returning after expiry.
                sleep_past(deadline);
            }
            Poll::Ready(Ok(buffer.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn expired_control_refuses_before_worker_initialization() {
        let result = send_message_with_runtime(
            "never opened",
            SHUTDOWN_MESSAGE,
            Some(Instant::now()),
            || panic!("an expired sender initialized a worker"),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);

        let temporary = tempfile::tempdir().unwrap();
        let guard = DirectoryGuard::private(&temporary.path().join("private")).unwrap();
        // Expiry also precedes validation/identity lookup in the public guarded boundary.
        let error = request_shutdown_guarded_until(&guard, "invalid", "invalid", Instant::now())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(guard.normalized_path().is_dir());
    }

    #[test]
    fn worker_startup_cannot_renew_the_control_deadline() {
        let deadline = Instant::now() + CLIENT_TIMEOUT;
        let initialized = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_initialized = Arc::clone(&initialized);
        let worker_finished = Arc::clone(&finished);
        let result = send_message_with_runtime(
            "never opened",
            SHUTDOWN_MESSAGE,
            Some(deadline),
            move || {
                worker_initialized.store(true, std::sync::atomic::Ordering::SeqCst);
                sleep_past(deadline);
                let runtime = client_runtime();
                worker_finished.store(true, std::sync::atomic::Ordering::SeqCst);
                runtime
            },
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(initialized.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            finished.load(std::sync::atomic::Ordering::SeqCst),
            "the delayed worker was left in the background"
        );
    }

    #[test]
    fn ready_acknowledgement_crossing_expiry_never_initiates_the_receipt() {
        let runtime = client_runtime().unwrap();
        let _entered = runtime.enter();
        let deadline = Instant::now() + CLIENT_TIMEOUT;
        let state = Arc::new(Mutex::new(WireState {
            ack_crosses: Some(deadline),
            ..WireState::default()
        }));
        let mut wire = FixtureWire(Arc::clone(&state));
        let mut exchange = std::pin::pin!(tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            exchange_frame(&mut wire, SHUTDOWN_MESSAGE, Some(deadline)),
        ));
        let mut context = Context::from_waker(std::task::Waker::noop());
        let Poll::Ready(Ok(Err(error))) = exchange.as_mut().poll(&mut context) else {
            panic!("the inner ready-I/O expiry gate did not refuse the receipt");
        };
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let state = state.lock().unwrap();
        assert_eq!(state.receipt_polls, 0);
        assert_eq!(state.written[0], SHUTDOWN_MESSAGE.len() as u8);
        assert_eq!(&state.written[1..], SHUTDOWN_MESSAGE);
    }

    #[test]
    fn pending_receipt_is_not_polled_again_after_it_becomes_ready_past_expiry() {
        let runtime = client_runtime().unwrap();
        let _entered = runtime.enter();
        let deadline = Instant::now() + CLIENT_TIMEOUT;
        let state = Arc::new(Mutex::new(WireState::default()));
        let mut wire = FixtureWire(Arc::clone(&state));
        let mut exchange = std::pin::pin!(tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            exchange_frame(&mut wire, SHUTDOWN_MESSAGE, Some(deadline)),
        ));
        let mut context = Context::from_waker(std::task::Waker::noop());
        assert!(exchange.as_mut().poll(&mut context).is_pending());
        assert_eq!(state.lock().unwrap().receipt_polls, 1);
        sleep_past(deadline);
        state.lock().unwrap().receipt_ready = true;
        let Poll::Ready(Ok(Err(error))) = exchange.as_mut().poll(&mut context) else {
            panic!("the expired previously-pending receipt was polled again");
        };
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let state = state.lock().unwrap();
        assert_eq!(state.receipt_polls, 1);
        assert_eq!(state.written[0], SHUTDOWN_MESSAGE.len() as u8);
        assert_eq!(&state.written[1..], SHUTDOWN_MESSAGE);
    }

    #[test]
    fn pre_expiry_queued_receipt_can_be_followed_by_uncertain_timeout() {
        let runtime = client_runtime().unwrap();
        let _entered = runtime.enter();
        let deadline = Instant::now() + CLIENT_TIMEOUT;
        let state = Arc::new(Mutex::new(WireState {
            receipt_ready: true,
            receipt_crosses: Some(deadline),
            ..WireState::default()
        }));
        let mut wire = FixtureWire(Arc::clone(&state));
        let mut exchange = std::pin::pin!(tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            exchange_frame(&mut wire, SHUTDOWN_MESSAGE, Some(deadline)),
        ));
        let mut context = Context::from_waker(std::task::Waker::noop());
        let Poll::Ready(Ok(Err(error))) = exchange.as_mut().poll(&mut context) else {
            panic!("post-Ready expiry did not classify queued delivery as uncertain");
        };
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("uncertain"));
        let state = state.lock().unwrap();
        assert_eq!(state.receipt_polls, 1);
        assert_eq!(state.written.last(), Some(&0xff));
        assert_eq!(&state.written[1..state.written.len() - 1], SHUTDOWN_MESSAGE);
    }

    #[test]
    fn identity_has_a_fixed_wire_vector_and_preserves_all_128_bits() {
        let sid = "S-1-5-21-1234";
        let volume = 0x0123_4567_89ab_cdef;
        let file_id = 0xfedc_ba98_7654_3210_0123_4567_89ab_cdef;
        let identity = identity_digest(sid, volume, file_id).unwrap();
        assert_eq!(
            identity,
            "182ac3cf20299137048bd3f0ac14721db48cac5e3f560f7b2a33ce343e5f324f"
        );
        assert_ne!(
            identity,
            identity_digest("S-1-5-21-1235", volume, file_id).unwrap()
        );
        assert_ne!(
            identity,
            identity_digest(sid, volume ^ (1 << 63), file_id).unwrap()
        );
        assert_ne!(
            identity,
            identity_digest(sid, volume, file_id ^ (1 << 127)).unwrap()
        );
    }

    #[test]
    fn guarded_file_identity_unifies_aliases_and_separates_unicode_names() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("State Mixed");
        let guard = DirectoryGuard::private(&root).unwrap();
        let identity = instance_identity_guarded(&guard).unwrap();
        assert_eq!(identity, instance_identity(&root.join(".")).unwrap());
        assert_eq!(
            identity,
            instance_identity(&temporary.path().join("state mixed")).unwrap()
        );
        let first = temporary.path().join("state ß");
        let second = temporary.path().join("state SS");
        let _first = DirectoryGuard::private(&first).unwrap();
        let _second = DirectoryGuard::private(&second).unwrap();
        assert_ne!(
            instance_identity(&first).unwrap(),
            instance_identity(&second).unwrap()
        );
        let unicode = temporary.path().join("工具 state");
        let _unicode = DirectoryGuard::private(&unicode).unwrap();
        assert_ne!(identity, instance_identity(&unicode).unwrap());
    }

    #[test]
    fn missing_state_identity_and_client_lookups_never_create_a_root() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("absent private state");
        let lifetime = uuid::Uuid::now_v7().to_string();
        for result in [
            instance_identity(&root).map(|_| ()),
            endpoint_name(&root, "wake", None).map(|_| ()),
            send_wake(&root),
            request_shutdown(&root, "dashboard", &lifetime),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
            assert!(
                !root.exists(),
                "an identity or control lookup recreated state"
            );
        }
        assert!(
            std::fs::read_dir(temporary.path())
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn endpoint_roles_and_uuid_lifetimes_are_separate_and_canonical() {
        let temporary = tempfile::tempdir().unwrap();
        let guard = DirectoryGuard::private(&temporary.path().join("private")).unwrap();
        let lifetime = uuid::Uuid::from_u128(0xaabb_ccdd).to_string();
        let wake = endpoint_name_guarded(&guard, "wake", None).unwrap();
        let daemon = endpoint_name_guarded(&guard, "daemon", Some(&lifetime)).unwrap();
        let dashboard = endpoint_name_guarded(&guard, "dashboard", Some(&lifetime)).unwrap();
        let activation =
            endpoint_name_guarded(&guard, "daemon-activation", Some(&lifetime)).unwrap();
        assert_ne!(wake, daemon);
        assert_ne!(daemon, dashboard);
        assert_ne!(daemon, activation);
        assert_ne!(dashboard, activation);
        let mut names =
            std::collections::BTreeSet::from([wake, daemon.clone(), dashboard, activation]);
        for role in ["dashboard-activation", "daemon-worker", "dashboard-worker"] {
            assert!(names.insert(endpoint_name_guarded(&guard, role, Some(&lifetime)).unwrap()));
            assert!(endpoint_name_guarded(&guard, role, None).is_err());
            assert!(endpoint_name_guarded(&guard, role, Some("malformed")).is_err());
        }
        assert_eq!(names.len(), 7);
        assert_eq!(
            daemon,
            endpoint_name_guarded(&guard, "daemon", Some(&lifetime.to_uppercase())).unwrap()
        );
        assert_ne!(
            daemon,
            endpoint_name_guarded(&guard, "daemon", Some(&uuid::Uuid::now_v7().to_string()))
                .unwrap()
        );
        assert!(endpoint_name_guarded(&guard, "wake", Some(&lifetime)).is_err());
        assert!(endpoint_name_guarded(&guard, "daemon", None).is_err());
        assert!(endpoint_name_guarded(&guard, "daemon-activation", None).is_err());
        assert!(endpoint_name_guarded(&guard, "daemon", Some("malformed")).is_err());
        assert!(endpoint_name_guarded(&guard, "job", Some(&lifetime)).is_err());
    }
}
