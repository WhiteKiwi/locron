//! Lifetime-owned secured local Windows listeners.

use std::io;
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use interprocess::os::windows::named_pipe::tokio::{DuplexPipeStream, FromHandleErrorKind};
use interprocess::os::windows::named_pipe::{
    DuplexPipeStream as AcceptedPipe, PipeListenerOptions, pipe_mode,
};
use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use locron_core::filesystem::DirectoryGuard;
use locron_core::notification::{
    ACK_MESSAGE, SHUTDOWN_MESSAGE, WAKE_MESSAGE, endpoint_name_guarded,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use widestring::U16CString;

enum Action {
    Wake(Arc<Notify>),
    Stop(CancellationToken),
}

struct BoundedClient(DuplexPipeStream<pipe_mode::Bytes>);

#[derive(Clone, Copy, Debug)]
enum ConversionStage {
    WaitMode,
    OwnershipTransfer,
    Tokio(FromHandleErrorKind),
}

impl std::fmt::Display for ConversionStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WaitMode => formatter.write_str("wait-mode"),
            Self::OwnershipTransfer => formatter.write_str("ownership-transfer"),
            Self::Tokio(details) => write!(formatter, "Tokio ({details})"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("accepted local pipe conversion failed at {stage}: {cause:?}")]
struct ClientConversionFailure {
    stage: ConversionStage,
    #[source]
    cause: Option<io::Error>,
}

impl Drop for BoundedClient {
    fn drop(&mut self) {
        // Includes cancellation/abort while an ACK write is pending.
        self.0.assume_flushed();
    }
}

fn async_client(
    stream: AcceptedPipe<pipe_mode::Bytes>,
) -> Result<BoundedClient, ClientConversionFailure> {
    // No synchronous payload I/O occurs. Ownership transfer must never leave a flush worker.
    stream.assume_flushed();
    stream
        .set_nonblocking(false)
        .map_err(|error| ClientConversionFailure {
            stage: ConversionStage::WaitMode,
            cause: Some(error),
        })?;
    let handle = OwnedHandle::try_from(stream).map_err(|stream| {
        drop(stream);
        ClientConversionFailure {
            stage: ConversionStage::OwnershipTransfer,
            cause: None,
        }
    })?;
    DuplexPipeStream::try_from(handle)
        .map(BoundedClient)
        .map_err(|error| {
            let failure = ClientConversionFailure {
                stage: ConversionStage::Tokio(error.details),
                cause: error.cause,
            };
            // A failed safe conversion returns the original owned handle. Close only it;
            // the listener already retains its independently replenished instance.
            drop(error.source);
            failure
        })
}

/// Binds wake after owner-lock acquisition; endpoint failure keeps durable fallback active.
pub fn bind_wake(root: &Path, wake: Arc<Notify>) -> io::Result<tokio::task::JoinHandle<()>> {
    bind(root, "wake", None, Action::Wake(wake))
}

/// Binds one registered role/lifetime's cooperative shutdown endpoint.
///
/// The owning composition must abort this handle before releasing the role lock.
pub fn bind_role_control(
    root: &Path,
    role: &str,
    lifetime: &str,
    cancellation: CancellationToken,
) -> io::Result<tokio::task::JoinHandle<()>> {
    bind(root, role, Some(lifetime), Action::Stop(cancellation))
}

fn bind(
    root: &Path,
    role: &str,
    lifetime: Option<&str>,
    action: Action,
) -> io::Result<tokio::task::JoinHandle<()>> {
    bind_with_converter(root, role, lifetime, action, async_client)
}

fn bind_with_converter<C>(
    root: &Path,
    role: &str,
    lifetime: Option<&str>,
    action: Action,
    mut convert: C,
) -> io::Result<tokio::task::JoinHandle<()>>
where
    C: FnMut(AcceptedPipe<pipe_mode::Bytes>) -> Result<BoundedClient, ClientConversionFailure>
        + Send
        + 'static,
{
    let guard = DirectoryGuard::private(root)?;
    let sid = locron_core::windows::current_user_sid()?;
    let name = endpoint_name_guarded(&guard, role, lifetime)?;
    let sddl = U16CString::from_str(format!("O:{sid}G:{sid}D:P(A;;GA;;;SY)(A;;GA;;;{sid})"))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let descriptor = SecurityDescriptor::deserialize(&sddl)?;
    let listener = PipeListenerOptions::new()
        .path(Path::new(&name))
        .security_descriptor(Some(descriptor))
        .accept_remote(false)
        .inheritable(false)
        .nonblocking(true)
        .instance_limit(std::num::NonZeroU8::new(2))
        .create_duplex::<pipe_mode::Bytes>()?;
    let message = match &action {
        Action::Wake(_) => WAKE_MESSAGE,
        Action::Stop(_) => SHUTDOWN_MESSAGE,
    };
    #[cfg(debug_assertions)]
    let wake_diagnostic =
        role == "wake" && lifetime.is_none() && matches!(&action, Action::Wake(_));
    Ok(tokio::spawn(async move {
        let _guard = guard;
        loop {
            let accepted = match listener.accept() {
                Ok(client) => client,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
                Err(error) => {
                    #[cfg(debug_assertions)]
                    if wake_diagnostic {
                        let raw = error.raw_os_error();
                        tracing::trace!(target: "locron::windows_wake_diagnostics",
                            op = "pipe_accept", edge = "err", value_present = false, value = 0_u64,
                            kind = wake_pipe_io_kind(error.kind()),
                            raw_present = raw.is_some(), raw = i64::from(raw.unwrap_or(0)));
                    }
                    tracing::warn!(%error, "local endpoint accept failed");
                    break;
                }
            };
            let mut client = match convert(accepted) {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(
                        stage = %error.stage,
                        raw_os_error = ?error.cause.as_ref().and_then(io::Error::raw_os_error),
                        %error,
                        "accepted local pipe conversion failed"
                    );
                    // Refuse this peer without releasing the already replenished listener.
                    // A disconnected-peer storm must still yield to cancellation/abort.
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
            };
            let _accepted = tokio::time::timeout(Duration::from_millis(200), async {
                let size = usize::from(client.0.read_u8().await?);
                if size != message.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid local hint size",
                    ));
                }
                let mut buffer = [0; 64];
                client.0.read_exact(&mut buffer[..size]).await?;
                if &buffer[..size] != message {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid local hint version",
                    ));
                }
                client.0.write_all(ACK_MESSAGE).await?;
                // Client confirmation proves it consumed the ack. A peer that stops reading
                // times out without an unbounded FlushFileBuffers/limbo worker.
                if client.0.read_u8().await? != 0xff {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid acknowledgement receipt",
                    ));
                }
                match &action {
                    Action::Wake(wake) => wake.notify_one(),
                    Action::Stop(cancel) => cancel.cancel(),
                }
                Ok::<(), io::Error>(())
            })
            .await;
        }
    }))
}

#[cfg(debug_assertions)]
fn wake_pipe_io_kind(kind: io::ErrorKind) -> &'static str {
    use io::ErrorKind;
    match kind {
        ErrorKind::NotFound => "NotFound",
        ErrorKind::PermissionDenied => "PermissionDenied",
        ErrorKind::ConnectionRefused => "ConnectionRefused",
        ErrorKind::ConnectionReset => "ConnectionReset",
        ErrorKind::HostUnreachable => "HostUnreachable",
        ErrorKind::NetworkUnreachable => "NetworkUnreachable",
        ErrorKind::ConnectionAborted => "ConnectionAborted",
        ErrorKind::NotConnected => "NotConnected",
        ErrorKind::AddrInUse => "AddrInUse",
        ErrorKind::AddrNotAvailable => "AddrNotAvailable",
        ErrorKind::NetworkDown => "NetworkDown",
        ErrorKind::BrokenPipe => "BrokenPipe",
        ErrorKind::AlreadyExists => "AlreadyExists",
        ErrorKind::WouldBlock => "WouldBlock",
        ErrorKind::NotADirectory => "NotADirectory",
        ErrorKind::IsADirectory => "IsADirectory",
        ErrorKind::DirectoryNotEmpty => "DirectoryNotEmpty",
        ErrorKind::ReadOnlyFilesystem => "ReadOnlyFilesystem",
        ErrorKind::StaleNetworkFileHandle => "StaleNetworkFileHandle",
        ErrorKind::InvalidInput => "InvalidInput",
        ErrorKind::InvalidData => "InvalidData",
        ErrorKind::TimedOut => "TimedOut",
        ErrorKind::WriteZero => "WriteZero",
        ErrorKind::StorageFull => "StorageFull",
        ErrorKind::NotSeekable => "NotSeekable",
        ErrorKind::QuotaExceeded => "QuotaExceeded",
        ErrorKind::FileTooLarge => "FileTooLarge",
        ErrorKind::ResourceBusy => "ResourceBusy",
        ErrorKind::ExecutableFileBusy => "ExecutableFileBusy",
        ErrorKind::Deadlock => "Deadlock",
        ErrorKind::CrossesDevices => "CrossesDevices",
        ErrorKind::TooManyLinks => "TooManyLinks",
        ErrorKind::InvalidFilename => "InvalidFilename",
        ErrorKind::ArgumentListTooLong => "ArgumentListTooLong",
        ErrorKind::Interrupted => "Interrupted",
        ErrorKind::Unsupported => "Unsupported",
        ErrorKind::UnexpectedEof => "UnexpectedEof",
        ErrorKind::OutOfMemory => "OutOfMemory",
        ErrorKind::Other => "Other",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interprocess::os::windows::named_pipe::PipeListener;
    use locron_core::notification::{
        endpoint_name, request_shutdown, request_shutdown_guarded_until, send_wake,
    };
    use std::time::Instant;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    struct FixtureRoot {
        _temporary: tempfile::TempDir,
        path: std::path::PathBuf,
    }
    impl FixtureRoot {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("private");
            let _guard = DirectoryGuard::private(&path).unwrap();
            Self {
                _temporary: temporary,
                path,
            }
        }
    }

    async fn client(name: &str) -> NamedPipeClient {
        let mut options = ClientOptions::new();
        options.security_qos_flags(0x0001_0000);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match options.open(name) {
                    Ok(client) => return client,
                    Err(error) if error.raw_os_error() == Some(231) => {
                        tokio::time::sleep(Duration::from_millis(5)).await
                    }
                    Err(error) => panic!("local fixture connection failed: {error}"),
                }
            }
        })
        .await
        .unwrap()
    }

    async fn notify(root: &Path) {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || send_wake(&root))
            .await
            .unwrap()
            .unwrap();
    }

    async fn close(listener: tokio::task::JoinHandle<()>) {
        listener.abort();
        tokio::time::timeout(Duration::from_secs(1), listener)
            .await
            .unwrap()
            .unwrap_err();
    }

    async fn listener_with_closed_accepted_peer(
        root: &Path,
        role: &str,
        lifetime: Option<&str>,
        action: Action,
    ) -> tokio::task::JoinHandle<()> {
        let peer_slot = Arc::new(std::sync::Mutex::new(None::<NamedPipeClient>));
        let closing_peer = Arc::clone(&peer_slot);
        let (boundary_sender, boundary_receiver) = tokio::sync::oneshot::channel();
        let mut boundary_sender = Some(boundary_sender);
        let listener = bind_with_converter(root, role, lifetime, action, move |accepted| {
            if let Some(sender) = boundary_sender.take() {
                // The owner has accepted and retained its replacement. Close the actual
                // connected client here, then run the real native conversion exactly once.
                let peer = closing_peer.lock().unwrap().take().unwrap();
                drop(peer);
                let result = async_client(accepted);
                let evidence = result.as_ref().err().map(|error| {
                    (
                        error.stage.to_string(),
                        error.cause.as_ref().and_then(io::Error::raw_os_error),
                    )
                });
                sender.send(evidence).unwrap();
                result
            } else {
                async_client(accepted)
            }
        })
        .unwrap();
        let name = endpoint_name(root, role, lifetime).unwrap();
        let mut options = ClientOptions::new();
        options.security_qos_flags(0x0001_0000);
        // No yield before this connection is retained. The current-thread owner cannot
        // accept it until the next await, which makes the close boundary deterministic.
        *peer_slot.lock().unwrap() = Some(options.open(&name).unwrap());
        let evidence = tokio::time::timeout(Duration::from_secs(1), boundary_receiver)
            .await
            .unwrap()
            .unwrap();
        // Depending on the native stage, a closed peer either fails conversion or converts
        // and returns EOF. Do not assume an error stage or fabricate an OS error.
        assert!(
            !listener.is_finished(),
            "accepted peer close released listener ownership: {evidence:?}"
        );
        listener
    }

    fn held_fixture_listener(name: &str) -> PipeListener<pipe_mode::Bytes, pipe_mode::Bytes> {
        let sid = locron_core::windows::current_user_sid().unwrap();
        let sddl =
            U16CString::from_str(format!("O:{sid}G:{sid}D:P(A;;GA;;;SY)(A;;GA;;;{sid})")).unwrap();
        let descriptor = SecurityDescriptor::deserialize(&sddl).unwrap();
        PipeListenerOptions::new()
            .path(Path::new(name))
            .security_descriptor(Some(descriptor))
            .accept_remote(false)
            .inheritable(false)
            .nonblocking(true)
            .instance_limit(std::num::NonZeroU8::new(2))
            .create_duplex::<pipe_mode::Bytes>()
            .unwrap()
    }

    async fn accepted_fixture_client(
        listener: &PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>,
    ) -> BoundedClient {
        let accepted = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match listener.accept() {
                    Ok(peer) => break peer,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Err(error) => panic!("owned fixture accept failed: {error}"),
                }
            }
        })
        .await
        .unwrap();
        async_client(accepted).unwrap()
    }

    #[tokio::test]
    async fn guarded_control_expiry_never_cancels_and_on_time_targets_the_exact_lifetime() {
        let root = FixtureRoot::new();
        let guard = Arc::new(DirectoryGuard::existing_private(&root.path).unwrap());
        let lifetime = uuid::Uuid::now_v7().to_string();
        let cancellation = CancellationToken::new();
        let listener =
            bind_role_control(&root.path, "dashboard", &lifetime, cancellation.clone()).unwrap();
        let caller_guard = Arc::clone(&guard);
        let expected = lifetime.clone();
        tokio::task::spawn_blocking(move || {
            let error = request_shutdown_guarded_until(
                &caller_guard,
                "dashboard",
                &expected,
                Instant::now(),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(
                request_shutdown_guarded_until(
                    &caller_guard,
                    "daemon",
                    &expected,
                    Instant::now() + Duration::from_secs(1),
                )
                .is_err()
            );
            assert!(
                request_shutdown_guarded_until(
                    &caller_guard,
                    "dashboard",
                    &uuid::Uuid::now_v7().to_string(),
                    Instant::now() + Duration::from_secs(1),
                )
                .is_err()
            );
        })
        .await
        .unwrap();
        assert!(!cancellation.is_cancelled());
        assert!(!listener.is_finished());
        tokio::task::spawn_blocking(move || {
            request_shutdown_guarded_until(
                &guard,
                "dashboard",
                &lifetime,
                Instant::now() + Duration::from_secs(1),
            )
        })
        .await
        .unwrap()
        .unwrap();
        tokio::time::timeout(Duration::from_secs(1), cancellation.cancelled())
            .await
            .unwrap();
        // Delivery leaves the owning role/control lifetime alive until explicit teardown.
        assert!(!listener.is_finished());
        close(listener).await;
    }

    #[tokio::test]
    async fn guarded_stalled_ack_uses_remaining_budget_and_closes_the_sender() {
        let root = FixtureRoot::new();
        let guard = Arc::new(DirectoryGuard::existing_private(&root.path).unwrap());
        let lifetime = uuid::Uuid::now_v7().to_string();
        let name = endpoint_name_guarded(&guard, "dashboard", Some(&lifetime)).unwrap();
        let listener = held_fixture_listener(&name);
        let caller_guard = Arc::clone(&guard);
        let sender = tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            let deadline = started + Duration::from_millis(100);
            let result =
                request_shutdown_guarded_until(&caller_guard, "dashboard", &lifetime, deadline);
            (result, started, Instant::now())
        });
        let mut peer = accepted_fixture_client(&listener).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            assert_eq!(
                peer.0.read_u8().await.unwrap(),
                SHUTDOWN_MESSAGE.len() as u8
            );
            let mut payload = [0; SHUTDOWN_MESSAGE.len()];
            peer.0.read_exact(&mut payload).await.unwrap();
            assert_eq!(payload.as_slice(), SHUTDOWN_MESSAGE);
        })
        .await
        .unwrap();
        // The peer deliberately withholds ACK while the exact caller deadline expires.
        let (result, started, returned) = sender.await.unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let elapsed = returned.saturating_duration_since(started);
        assert!(
            elapsed < Duration::from_millis(180),
            "100ms remaining budget was renewed to 200ms: {elapsed:?}"
        );
        assert!(error.to_string().contains("uncertain"));
        assert!(
            tokio::time::timeout(Duration::from_secs(1), peer.0.read_u8())
                .await
                .unwrap()
                .is_err(),
            "a timed-out sender remained connected or wrote a receipt"
        );
        assert_eq!(Arc::strong_count(&guard), 1);
        drop(peer);
        drop(listener);
        drop(guard);
        std::fs::rename(&root.path, root.path.with_file_name("closed")).unwrap();
    }

    #[tokio::test]
    async fn guarded_busy_endpoint_cannot_renew_the_connect_retry_budget() {
        let root = FixtureRoot::new();
        let guard = Arc::new(DirectoryGuard::existing_private(&root.path).unwrap());
        let lifetime = uuid::Uuid::now_v7().to_string();
        let name = endpoint_name_guarded(&guard, "dashboard", Some(&lifetime)).unwrap();
        let listener = held_fixture_listener(&name);
        let occupying_peer = client(&name).await;
        let mut options = ClientOptions::new();
        options.security_qos_flags(0x0001_0000);
        assert_eq!(options.open(&name).unwrap_err().raw_os_error(), Some(231));
        let caller_guard = Arc::clone(&guard);
        let (result, started, returned) = tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            let deadline = started + Duration::from_millis(100);
            let result =
                request_shutdown_guarded_until(&caller_guard, "dashboard", &lifetime, deadline);
            (result, started, Instant::now())
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        let elapsed = returned.saturating_duration_since(started);
        assert!(
            elapsed < Duration::from_millis(180),
            "100ms busy-endpoint budget was renewed across retries: {elapsed:?}"
        );
        assert_eq!(Arc::strong_count(&guard), 1);
        drop(occupying_peer);
        drop(listener);
        drop(guard);
        std::fs::rename(&root.path, root.path.with_file_name("closed")).unwrap();
    }

    fn disconnect_before_first_poll(name: &str) {
        let mut options = ClientOptions::new();
        options.security_qos_flags(0x0001_0000);
        // This function never yields: on the current-thread test runtime, the spawned owner
        // has not called accept yet. Its stored instance must reset this dead connection.
        let peer = options.open(name).unwrap();
        drop(peer);
    }

    #[tokio::test]
    async fn pre_accept_disconnect_preserves_the_owned_wake_endpoint() {
        let root = FixtureRoot::new();
        let wake = Arc::new(Notify::new());
        let listener = bind_wake(&root.path, Arc::clone(&wake)).unwrap();
        let name = endpoint_name(&root.path, "wake", None).unwrap();
        disconnect_before_first_poll(&name);
        assert!(bind_wake(&root.path, Arc::clone(&wake)).is_err());
        notify(&root.path).await;
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .unwrap();
        assert!(
            !listener.is_finished(),
            "wake ownership exited after an early peer close"
        );
        assert!(bind_wake(&root.path, Arc::clone(&wake)).is_err());
        close(listener).await;
        assert!(send_wake(&root.path).is_err());
    }

    #[tokio::test]
    async fn pre_accept_disconnect_preserves_exact_lifetime_control() {
        let root = FixtureRoot::new();
        let lifetime = uuid::Uuid::now_v7().to_string();
        let cancellation = CancellationToken::new();
        let listener =
            bind_role_control(&root.path, "dashboard", &lifetime, cancellation.clone()).unwrap();
        let name = endpoint_name(&root.path, "dashboard", Some(&lifetime)).unwrap();
        disconnect_before_first_poll(&name);
        assert!(
            bind_role_control(&root.path, "dashboard", &lifetime, cancellation.clone()).is_err()
        );
        assert!(!cancellation.is_cancelled());
        let path = root.path.clone();
        let expected = lifetime.clone();
        tokio::task::spawn_blocking(move || request_shutdown(&path, "dashboard", &expected))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), cancellation.cancelled())
            .await
            .unwrap();
        assert!(
            !listener.is_finished(),
            "control ownership exited after an early peer close"
        );
        assert!(bind_role_control(&root.path, "dashboard", &lifetime, cancellation).is_err());
        close(listener).await;
        assert!(request_shutdown(&root.path, "dashboard", &lifetime).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepted_peer_close_preserves_wake_ownership_and_teardown() {
        let root = FixtureRoot::new();
        let wake = Arc::new(Notify::new());
        let listener = listener_with_closed_accepted_peer(
            &root.path,
            "wake",
            None,
            Action::Wake(Arc::clone(&wake)),
        )
        .await;
        assert!(bind_wake(&root.path, Arc::clone(&wake)).is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), wake.notified())
                .await
                .is_err(),
            "closed accepted peer acted as a wake hint"
        );
        notify(&root.path).await;
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .unwrap();
        assert!(!listener.is_finished());
        assert!(bind_wake(&root.path, Arc::clone(&wake)).is_err());
        close(listener).await;
        assert!(send_wake(&root.path).is_err());
        std::fs::rename(&root.path, root.path.with_file_name("closed")).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepted_peer_close_preserves_exact_control_ownership_and_teardown() {
        let root = FixtureRoot::new();
        let lifetime = uuid::Uuid::now_v7().to_string();
        let cancellation = CancellationToken::new();
        let listener = listener_with_closed_accepted_peer(
            &root.path,
            "dashboard",
            Some(&lifetime),
            Action::Stop(cancellation.clone()),
        )
        .await;
        assert!(!cancellation.is_cancelled());
        assert!(
            bind_role_control(&root.path, "dashboard", &lifetime, cancellation.clone()).is_err()
        );
        let path = root.path.clone();
        let expected = lifetime.clone();
        tokio::task::spawn_blocking(move || request_shutdown(&path, "dashboard", &expected))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), cancellation.cancelled())
            .await
            .unwrap();
        assert!(!listener.is_finished());
        assert!(bind_role_control(&root.path, "dashboard", &lifetime, cancellation).is_err());
        close(listener).await;
        assert!(request_shutdown(&root.path, "dashboard", &lifetime).is_err());
        std::fs::rename(&root.path, root.path.with_file_name("closed")).unwrap();
    }

    #[tokio::test]
    async fn wake_hints_are_versioned_coalesced_and_collision_safe() {
        let root = FixtureRoot::new();
        let wake = Arc::new(Notify::new());
        let listener = bind_wake(&root.path, Arc::clone(&wake)).unwrap();
        assert!(
            bind_wake(&root.path, Arc::clone(&wake)).is_err(),
            "first-instance collision was accepted"
        );
        notify(&root.path).await;
        notify(&root.path).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), wake.notified())
                .await
                .is_err()
        );
        close(listener).await;
        assert!(send_wake(&root.path).is_err());
    }

    #[tokio::test]
    async fn malformed_and_shutdown_frames_never_act_as_wake() {
        let root = FixtureRoot::new();
        let wake = Arc::new(Notify::new());
        let listener = bind_wake(&root.path, Arc::clone(&wake)).unwrap();
        let name = endpoint_name(&root.path, "wake", None).unwrap();
        for payload in [
            vec![255],
            vec![0],
            [
                vec![SHUTDOWN_MESSAGE.len() as u8],
                SHUTDOWN_MESSAGE.to_vec(),
            ]
            .concat(),
        ] {
            let mut peer = client(&name).await;
            peer.write_all(&payload).await.unwrap();
            drop(peer);
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(
                !listener.is_finished(),
                "malformed peer released the owned listener"
            );
            assert!(
                bind_wake(&root.path, Arc::clone(&wake)).is_err(),
                "malformed peer released first-instance collision protection"
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), wake.notified())
                .await
                .is_err()
        );
        notify(&root.path).await;
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .unwrap();
        close(listener).await;
    }

    #[tokio::test]
    async fn idle_and_nonreading_clients_expire_without_unbounded_flush() {
        let root = FixtureRoot::new();
        let wake = Arc::new(Notify::new());
        let listener = bind_wake(&root.path, Arc::clone(&wake)).unwrap();
        let name = endpoint_name(&root.path, "wake", None).unwrap();
        let idle = client(&name).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(idle);
        let mut nonreading = client(&name).await;
        nonreading.write_u8(WAKE_MESSAGE.len() as u8).await.unwrap();
        nonreading.write_all(WAKE_MESSAGE).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(nonreading);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), wake.notified())
                .await
                .is_err()
        );
        notify(&root.path).await;
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .unwrap();
        let mut pending = client(&name).await;
        pending.write_u8(WAKE_MESSAGE.len() as u8).await.unwrap();
        pending.write_all(WAKE_MESSAGE).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        close(listener).await;
        drop(pending);
        std::fs::rename(&root.path, root.path.with_file_name("closed")).unwrap();
    }

    #[tokio::test]
    async fn control_requires_the_exact_registered_role_and_lifetime() {
        let root = FixtureRoot::new();
        let lifetime = uuid::Uuid::now_v7().to_string();
        let cancellation = CancellationToken::new();
        let listener =
            bind_role_control(&root.path, "dashboard", &lifetime, cancellation.clone()).unwrap();
        let wrong = uuid::Uuid::now_v7().to_string();
        let path = root.path.clone();
        let expected = lifetime.clone();
        tokio::task::spawn_blocking(move || {
            assert!(request_shutdown(&path, "daemon", &expected).is_err());
            assert!(request_shutdown(&path, "dashboard", &wrong).is_err());
        })
        .await
        .unwrap();
        assert!(!cancellation.is_cancelled());
        let path = root.path.clone();
        tokio::task::spawn_blocking(move || request_shutdown(&path, "dashboard", &lifetime))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), cancellation.cancelled())
            .await
            .unwrap();
        close(listener).await;
    }

    #[tokio::test]
    async fn remote_named_pipe_view_is_refused() {
        let root = FixtureRoot::new();
        let listener = bind_wake(&root.path, Arc::new(Notify::new())).unwrap();
        let name = endpoint_name(&root.path, "wake", None).unwrap();
        let pipe = name.strip_prefix(r"\\.\pipe\").unwrap().to_owned();
        let result = tokio::task::spawn_blocking(move || locron_core::windows::run_script_json(
            r"$client=[System.IO.Pipes.NamedPipeClientStream]::new('localhost',[string]$request.name,[System.IO.Pipes.PipeDirection]::InOut); try { $client.Connect(200); @{rejected=$false} | & $locronToJson -Compress } catch { @{rejected=$true} | & $locronToJson -Compress } finally { $client.Dispose() }",
            &serde_json::json!({"name":pipe}),
        )).await.unwrap().unwrap();
        assert_eq!(result["rejected"], true, "remote pipe view connected");
        close(listener).await;
    }
}
