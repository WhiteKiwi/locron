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
