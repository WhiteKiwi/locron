//! Lifetime-owned secured local Windows listeners.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use interprocess::os::windows::named_pipe::tokio::DuplexPipeStream;
use interprocess::os::windows::named_pipe::{PipeListenerOptions, pipe_mode};
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

impl Drop for BoundedClient {
    fn drop(&mut self) {
        // Includes cancellation/abort while an ACK write is pending.
        self.0.assume_flushed();
    }
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
        .instance_limit(std::num::NonZeroU8::new(2))
        .create_tokio_duplex::<pipe_mode::Bytes>()?;
    let message = match &action {
        Action::Wake(_) => WAKE_MESSAGE,
        Action::Stop(_) => SHUTDOWN_MESSAGE,
    };
    Ok(tokio::spawn(async move {
        let _guard = guard;
        loop {
            let mut client = match listener.accept().await {
                Ok(client) => BoundedClient(client),
                Err(error) => {
                    tracing::warn!(%error, "local endpoint accept failed");
                    break;
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

#[cfg(test)]
mod tests {
    use super::*;
    use locron_core::notification::{endpoint_name, request_shutdown, send_wake};
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
            r#"$client=[System.IO.Pipes.NamedPipeClientStream]::new('localhost',[string]$request.name,[System.IO.Pipes.PipeDirection]::InOut); try { $client.Connect(200); @{rejected=$false} | ConvertTo-Json -Compress } catch { @{rejected=$true} | ConvertTo-Json -Compress } finally { $client.Dispose() }"#,
            &serde_json::json!({"name":pipe}),
        )).await.unwrap().unwrap();
        assert_eq!(result["rejected"], true, "remote pipe view connected");
        close(listener).await;
    }
}
