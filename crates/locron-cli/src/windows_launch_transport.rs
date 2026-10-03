//! Read-only test transport for the guarded helper launch gate.
//!
//! Peer IDs and a complete wire exchange are metadata, not a live launch proof.
//! The finite launch owner must bracket these operations with its original Child
//! and retain every close task alongside the lease/guards after uncertainty.
//! Synchronous native setup/query calls run only inside that retained owner.

use std::ffi::OsStr;
use std::fs::File;
use std::future::Future;
use std::num::NonZeroU8;
use std::os::windows::io::AsHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use interprocess::os::windows::{
    ToWtf16,
    named_pipe::{
        PipeListenerOptions, PipeStream as SyncPipeStream, pipe_mode,
        tokio::{PipeListener, PipeStream},
    },
    security_descriptor::SecurityDescriptor,
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, PipeMode};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::windows_launch_codec::{Codec, Frame, Phase};

const PIPE_PREFIX: &str = r"\\.\pipe\locron.helper-launch.v1.";
// Microsoft SDK SECURITY_IDENTIFICATION; ClientOptions adds SQOS_PRESENT.
const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;
const ERROR_PIPE_BUSY: i32 = 231;
const CONNECT_POLL: Duration = Duration::from_millis(10);

type Listener = PipeListener<pipe_mode::None, pipe_mode::Bytes>;
type Stream = PipeStream<pipe_mode::None, pipe_mode::Bytes>;

fn remaining(deadline: Instant) -> Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(!remaining.is_zero(), "original launch deadline expired");
    Ok(remaining)
}

async fn until<T>(deadline: Instant, future: impl Future<Output = Result<T>>) -> Result<T> {
    remaining(deadline)?;
    let result = tokio::time::timeout_at(deadline.into(), future)
        .await
        .context("original launch deadline expired")??;
    remaining(deadline)?;
    Ok(result)
}

fn sid_text(sid: &str) -> Result<()> {
    ensure!(
        sid.len() <= 184
            && sid.starts_with("S-1-")
            && sid[4..]
                .split('-')
                .all(|part| { !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()) }),
        "invalid verified SID spelling"
    );
    Ok(())
}

/// Names select two local endpoints; this type provides no account/process proof.
pub(super) struct PipeNames {
    session: Uuid,
    parent: String,
    child: String,
}

impl PipeNames {
    pub(super) fn new(sid: &str, session: Uuid) -> Result<Self> {
        sid_text(sid)?;
        ensure!(!session.is_nil(), "zero launch session UUID");
        let digest = format!("{:x}", Sha256::digest(sid.as_bytes()));
        Ok(Self {
            session,
            parent: format!("{PIPE_PREFIX}{digest}.{session}.parent"),
            child: format!("{PIPE_PREFIX}{digest}.{session}.child"),
        })
    }

    pub(super) fn selectors(&self) -> (Uuid, &str, &str) {
        (self.session, &self.parent, &self.child)
    }

    pub(super) fn parse(session: Uuid, parent: String, child: String) -> Result<Self> {
        ensure!(!session.is_nil(), "zero pipe selector session");
        let rest = parent
            .strip_prefix(PIPE_PREFIX)
            .context("nonlocal pipe selector")?;
        let (digest, _) = rest
            .split_once('.')
            .context("invalid pipe selector grammar")?;
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid SID digest selector"
        );
        ensure!(
            parent == format!("{PIPE_PREFIX}{digest}.{session}.parent")
                && child == format!("{PIPE_PREFIX}{digest}.{session}.child"),
            "pipe selectors have foreign scope, session or direction"
        );
        Ok(Self {
            session,
            parent,
            child,
        })
    }

    pub(super) fn bind_sid(&self, sid: &str) -> Result<()> {
        let actual = Self::new(sid, self.session)?;
        ensure!(
            self.parent == actual.parent && self.child == actual.child,
            "pipe selectors differ from the actual token SID"
        );
        Ok(())
    }
}

fn descriptor(sid: &str) -> Result<SecurityDescriptor> {
    sid_text(sid)?;
    let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})(A;;GA;;;SY)");
    let wide = OsStr::new(&sddl).to_wtf_16()?;
    Ok(SecurityDescriptor::deserialize(&wide)?)
}

fn listen(name: &str, sid: &str, deadline: Instant) -> Result<Listener> {
    remaining(deadline)?;
    let listener = PipeListenerOptions::new()
        .path(OsStr::new(name))
        .mode(interprocess::os::windows::named_pipe::PipeMode::Bytes)
        .security_descriptor(Some(descriptor(sid)?))
        .accept_remote(false)
        .inheritable(false)
        .instance_limit(NonZeroU8::new(2))
        .create_tokio_send_only::<pipe_mode::Bytes>()?;
    remaining(deadline)?;
    Ok(listener)
}

/// The first-instance listener is created before the corresponding peer spawn.
pub(super) struct SendListener {
    listener: Listener,
    phases: [Phase; 2],
}

impl SendListener {
    pub(super) fn parent(names: &PipeNames, sid: &str, deadline: Instant) -> Result<Self> {
        Ok(Self {
            listener: listen(&names.parent, sid, deadline)?,
            phases: [Phase::Challenge, Phase::Permit],
        })
    }

    pub(super) fn child(names: &PipeNames, sid: &str, deadline: Instant) -> Result<Self> {
        Ok(Self {
            listener: listen(&names.child, sid, deadline)?,
            phases: [Phase::Ready, Phase::Qualified],
        })
    }

    pub(super) async fn accept(self, deadline: Instant) -> Result<SendEndpoint> {
        let stream = until(deadline, async { Ok(self.listener.accept().await?) }).await?;
        // accept creates a replacement instance. Only one accepted peer is used.
        let phases = self.phases;
        drop(self);
        Ok(SendEndpoint {
            stream: Some(stream),
            frames: 0,
            phases,
        })
    }
}

/// Owns one unsplit server and always avoids interprocess default-drop limbo.
pub(super) struct SendEndpoint {
    stream: Option<Stream>,
    frames: usize,
    phases: [Phase; 2],
}

impl Drop for SendEndpoint {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.evade_limbo();
        }
    }
}

impl SendEndpoint {
    pub(super) fn peer_pid(&self, deadline: Instant) -> Result<u32> {
        remaining(deadline)?;
        let stream = self.stream.as_ref().context("sender already closed")?;
        ensure!(stream.is_server(), "outgoing endpoint is not a server");
        let pid = stream.client_process_id()?;
        remaining(deadline)?;
        ensure!(pid != 0, "named-pipe client returned a zero PID");
        Ok(pid)
    }

    pub(super) async fn send(
        &mut self,
        codec: &mut Codec,
        frame: &Frame,
        deadline: Instant,
    ) -> Result<()> {
        ensure!(self.frames < 2, "outgoing endpoint has extra frames");
        ensure!(
            frame.phase() == self.phases[self.frames],
            "wrong outgoing launch phase"
        );
        let bytes = codec.encode(frame)?;
        let stream = self.stream.as_mut().context("sender already closed")?;
        until(deadline, async { Ok(stream.write_all(&bytes).await?) }).await?;
        self.frames += 1;
        Ok(())
    }

    pub(super) fn close(self, deadline: Instant) -> Result<CloseTask> {
        ensure!(self.frames == 2, "sender requires its exact two frames");
        self.raw_close(deadline)
    }

    fn raw_close(self, deadline: Instant) -> Result<CloseTask> {
        remaining(deadline)?;
        let stream = self.stream.as_ref().context("sender already closed")?;
        let file = File::from(stream.as_handle().try_clone_to_owned()?);
        remaining(deadline)?;
        // The actual owner keeps this task. A timeout does not cancel sync_all.
        let worker = tokio::task::spawn_blocking(move || {
            let result = file.sync_all().context("raw named-pipe flush failed");
            drop(file);
            // The temporary native handle is closed before the original sender.
            drop(self);
            result
        });
        Ok(CloseTask {
            worker: Some(worker),
        })
    }
}

/// A borrowed deadline wait leaves the actual outstanding worker owned here.
/// On uncertainty the finite launch owner must retain this object and its runtime
/// together with Child/lease/guards. Dropping its JoinHandle would detach the task.
#[must_use = "retain the close task until its native operation is confirmed"]
pub(super) struct CloseTask {
    worker: Option<JoinHandle<Result<()>>>,
}

impl CloseTask {
    pub(super) fn is_pending(&self) -> bool {
        self.worker.is_some()
    }

    pub(super) async fn wait_until(&mut self, deadline: Instant) -> Result<()> {
        let worker = self
            .worker
            .as_mut()
            .context("close task already observed")?;
        remaining(deadline)?;
        let result = tokio::time::timeout_at(deadline.into(), worker)
            .await
            .context("raw pipe flush is still owned after launch timeout")?;
        self.worker.take();
        let result = result.context("raw pipe flush worker failed")?;
        remaining(deadline)?;
        result
    }
}

pub(super) struct ReceiveEndpoint {
    client: NamedPipeClient,
    frames: usize,
    phases: [Phase; 2],
}

impl ReceiveEndpoint {
    pub(super) async fn parent(names: &PipeNames, deadline: Instant) -> Result<Self> {
        Self::connect(&names.child, [Phase::Ready, Phase::Qualified], deadline).await
    }

    pub(super) async fn child(names: &PipeNames, deadline: Instant) -> Result<Self> {
        Self::connect(&names.parent, [Phase::Challenge, Phase::Permit], deadline).await
    }

    async fn connect(name: &str, phases: [Phase; 2], deadline: Instant) -> Result<Self> {
        loop {
            remaining(deadline)?;
            let result = ClientOptions::new()
                .read(true)
                .write(false)
                .pipe_mode(PipeMode::Byte)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(name);
            remaining(deadline)?;
            match result {
                Ok(client) => {
                    return Ok(Self {
                        client,
                        frames: 0,
                        phases,
                    });
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(ERROR_PIPE_BUSY) =>
                {
                    until(deadline, async {
                        tokio::time::sleep(CONNECT_POLL).await;
                        Ok(())
                    })
                    .await?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(super) fn peer_pid(&self, deadline: Instant) -> Result<u32> {
        remaining(deadline)?;
        let owned = self.client.as_handle().try_clone_to_owned()?;
        // Both mode tags expose safe deterministic disposal. This private wrapper
        // performs only native metadata queries, never I/O or IOCP registration.
        // The actual Tokio client remains read-only; ReOpenFile may fail back to
        // the cloned original handle. Neither outcome supplies protocol authority.
        let wrapper = SyncPipeStream::<pipe_mode::Bytes, pipe_mode::Bytes>::try_from(owned)
            .map_err(|error| {
                // ConversionError's details implement Display, not StdError. Its
                // returned source is still a plain OwnedHandle, never a stream.
                let details = format!("pipe metadata handle conversion failed: {}", error.details);
                drop(error.source);
                match error.cause {
                    Some(cause) => anyhow::Error::new(cause).context(details),
                    None => anyhow::anyhow!(details),
                }
            })?;
        let is_client = wrapper.is_client();
        let result = wrapper.server_process_id();
        // Clear unknown flush state and close on EVERY result, including errors;
        // plain Drop could enter limbo, and extraction needs an Err fallback.
        wrapper.evade_limbo();
        ensure!(is_client, "incoming endpoint is not a client");
        let pid = result?;
        remaining(deadline)?;
        ensure!(pid != 0, "named-pipe server returned a zero PID");
        Ok(pid)
    }

    pub(super) async fn receive(&mut self, codec: &mut Codec, deadline: Instant) -> Result<Frame> {
        ensure!(self.frames < 2, "incoming endpoint has extra frames");
        let frame = until(deadline, async {
            let mut prefix = [0; 4];
            self.client.read_exact(&mut prefix).await?;
            let length = Codec::body_length(prefix)?;
            let mut body = vec![0; length];
            self.client.read_exact(&mut body).await?;
            codec.decode(prefix, &body)
        })
        .await?;
        ensure!(
            frame.phase() == self.phases[self.frames],
            "wrong incoming launch phase"
        );
        self.frames += 1;
        Ok(frame)
    }

    pub(super) async fn terminal_eof(&mut self, deadline: Instant) -> Result<()> {
        ensure!(self.frames == 2, "terminal read precedes required frames");
        until(deadline, async {
            let mut extra = [0];
            ensure!(
                self.client.read(&mut extra).await? == 0,
                "extra transport bytes follow the terminal frame"
            );
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    use super::super::windows_launch_codec::{Bindings, HelperIdentity};
    use super::*;

    static FIXTURE_OWNER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    struct FixturePermit;

    impl Drop for FixturePermit {
        fn drop(&mut self) {
            FIXTURE_OWNER.store(false, std::sync::atomic::Ordering::Release);
        }
    }

    fn owned_fixture<F, Fut>(fixture: F)
    where
        F: FnOnce(Instant) -> Fut + Send + 'static,
        Fut: Future<Output = ()>,
    {
        // The harness caller never drops/joins a runtime with uncertain native I/O.
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(
            FIXTURE_OWNER
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                )
                .is_ok(),
            "previous transport fixture owner is still live/quarantined"
        );
        let (finished, completion) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let permit = FixturePermit;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(fixture(deadline));
            }));
            // Any uncertain native flush stays in this finite owner, not the caller.
            drop(runtime);
            drop(permit);
            let _ = finished.send(result);
        });
        let limit = (deadline + Duration::from_secs(3)).saturating_duration_since(Instant::now());
        match completion.recv_timeout(limit) {
            Ok(Ok(())) => {}
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
            Err(_) => panic!("transport fixture owner remains quarantined after deadline/cleanup"),
        }
    }

    fn names(deadline: Instant) -> PipeNames {
        // This is the actual bounded provider, not a warmed/substituted cache.
        let sid = locron_core::windows::current_user_sid_until(deadline).unwrap();
        PipeNames::new(&sid, Uuid::now_v7()).unwrap()
    }

    fn frame() -> Frame {
        Frame::challenge(
            Bindings::new(
                Uuid::now_v7(),
                Uuid::now_v7(),
                "a".repeat(64),
                "b".repeat(64),
                HelperIdentity::new(0xf000_0000_0000_0001, u128::MAX, "c".repeat(64)).unwrap(),
            )
            .unwrap(),
            std::process::id(),
            30_000,
        )
        .unwrap()
    }

    async fn endpoints(
        names: &PipeNames,
        deadline: Instant,
        child_server: bool,
    ) -> (SendEndpoint, ReceiveEndpoint) {
        let sid = locron_core::windows::current_user_sid_until(deadline).unwrap();
        if child_server {
            let listener = SendListener::child(names, &sid, deadline).unwrap();
            let receiver = ReceiveEndpoint::parent(names, deadline).await.unwrap();
            let sender = listener.accept(deadline).await.unwrap();
            (sender, receiver)
        } else {
            let listener = SendListener::parent(names, &sid, deadline).unwrap();
            let receiver = ReceiveEndpoint::child(names, deadline).await.unwrap();
            let sender = listener.accept(deadline).await.unwrap();
            (sender, receiver)
        }
    }

    #[test]
    fn actual_peer_queries_and_terminal_eof_need_no_process_exit() {
        owned_fixture(|deadline| async move {
            let names = names(deadline);
            for child_server in [false, true] {
                let (mut sender, mut receiver) = endpoints(&names, deadline, child_server).await;
                assert_eq!(sender.peer_pid(deadline).unwrap(), std::process::id());
                // Native client-side GetNamedPipeServerProcessId must actually work.
                assert_eq!(receiver.peer_pid(deadline).unwrap(), std::process::id());
                let challenge = frame();
                let nonce = Uuid::now_v7();
                let (first, second) = if child_server {
                    (
                        challenge
                            .next(Phase::Ready, std::process::id(), nonce)
                            .unwrap(),
                        challenge
                            .next(Phase::Qualified, std::process::id(), nonce)
                            .unwrap(),
                    )
                } else {
                    let second = challenge
                        .next(Phase::Permit, std::process::id(), nonce)
                        .unwrap();
                    (challenge, second)
                };
                let mut sent = Codec::default();
                let mut received = Codec::default();
                sender.send(&mut sent, &first, deadline).await.unwrap();
                sender.send(&mut sent, &second, deadline).await.unwrap();
                let mut close = sender.close(deadline).unwrap();
                assert_eq!(
                    receiver.receive(&mut received, deadline).await.unwrap(),
                    first
                );
                assert_eq!(
                    receiver.receive(&mut received, deadline).await.unwrap(),
                    second
                );
                close.wait_until(deadline).await.unwrap();
                assert!(!close.is_pending());
                receiver.terminal_eof(deadline).await.unwrap();
                // The process is still running after EOF; closure is endpoint-owned.
                assert_ne!(std::process::id(), 0);
            }
        });
    }

    #[test]
    fn first_instance_collision_refuses_and_listener_accepts_only_once() {
        owned_fixture(|deadline| async move {
            let sid = locron_core::windows::current_user_sid_until(deadline).unwrap();
            let names = PipeNames::new(&sid, Uuid::now_v7()).unwrap();
            let listener = SendListener::parent(&names, &sid, deadline).unwrap();
            assert!(SendListener::parent(&names, &sid, deadline).is_err());
            let mut receiver = ReceiveEndpoint::child(&names, deadline).await.unwrap();
            // Consuming accept exposes only this stream. Native IOCP completions
            // can retain the unused replacement after its listener is dropped.
            let mut sender = listener.accept(deadline).await.unwrap();
            remaining(deadline).unwrap();
            let late = ClientOptions::new()
                .read(true)
                .write(false)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&names.parent);
            remaining(deadline).unwrap();
            let first = frame();
            let second = first
                .next(Phase::Permit, std::process::id(), Uuid::now_v7())
                .unwrap();
            let mut sent = Codec::default();
            sender.send(&mut sent, &first, deadline).await.unwrap();
            sender.send(&mut sent, &second, deadline).await.unwrap();
            let mut close = sender.close(deadline).unwrap();
            let mut received = Codec::default();
            assert_eq!(
                receiver.receive(&mut received, deadline).await.unwrap(),
                first
            );
            assert_eq!(
                receiver.receive(&mut received, deadline).await.unwrap(),
                second
            );
            close.wait_until(deadline).await.unwrap();
            receiver.terminal_eof(deadline).await.unwrap();
            if let Ok(mut late) = late {
                let mut byte = [0];
                // Require actual EOF/read failure, not a quiet interval or an
                // assumed synchronous native close. This endpoint has no authority.
                let result = tokio::time::timeout_at(deadline.into(), late.read(&mut byte))
                    .await
                    .expect("unused replacement disposal unconfirmed at original deadline");
                assert!(
                    !matches!(result, Ok(count) if count != 0),
                    "unused replacement received qualification bytes"
                );
            }
        });
    }

    #[test]
    fn held_open_terminal_is_timeout_and_extra_byte_is_refusal() {
        owned_fixture(|deadline| async move {
            let names = names(deadline);
            let (mut sender, mut receiver) = endpoints(&names, deadline, false).await;
            let first = frame();
            let second = first
                .next(Phase::Permit, std::process::id(), Uuid::now_v7())
                .unwrap();
            let mut sent = Codec::default();
            let mut received = Codec::default();
            sender.send(&mut sent, &first, deadline).await.unwrap();
            sender.send(&mut sent, &second, deadline).await.unwrap();
            receiver.receive(&mut received, deadline).await.unwrap();
            receiver.receive(&mut received, deadline).await.unwrap();
            assert!(
                receiver
                    .terminal_eof(Instant::now() + Duration::from_millis(30))
                    .await
                    .is_err()
            );
            // An extra byte must be refused rather than skipped or treated as EOF.
            sender
                .stream
                .as_mut()
                .unwrap()
                .write_all(&[1])
                .await
                .unwrap();
            assert!(receiver.terminal_eof(deadline).await.is_err());
            drop(receiver);
            drop(sender);
        });
    }

    #[test]
    fn buffered_reader_disconnect_raw_flush_reports_native_error() {
        owned_fixture(|deadline| async move {
            let sid = locron_core::windows::current_user_sid_until(deadline).unwrap();
            let names = PipeNames::new(&sid, Uuid::now_v7()).unwrap();
            let listener = SendListener::parent(&names, &sid, deadline).unwrap();
            remaining(deadline).unwrap();
            // Plain File owns one native receive-only handle. It is not registered
            // with IOCP and cannot prefetch bytes behind this unread fixture.
            let reader = OpenOptions::new()
                .read(true)
                .write(false)
                .share_mode(3)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&names.parent)
                .unwrap();
            remaining(deadline).unwrap();
            let mut sender = listener.accept(deadline).await.unwrap();
            until(deadline, async {
                Ok(sender
                    .stream
                    .as_mut()
                    .unwrap()
                    .write_all(b"actually unread native bytes")
                    .await?)
            })
            .await
            .unwrap();
            let mut close = sender.raw_close(deadline).unwrap();
            assert!(
                close
                    .wait_until(deadline.min(Instant::now() + Duration::from_millis(30)))
                    .await
                    .is_err()
            );
            assert!(
                close.is_pending(),
                "unread flush must remain actually owned"
            );
            drop(reader);
            let error = close.wait_until(deadline).await.unwrap_err();
            assert!(!close.is_pending(), "native flush error must be observed");
            assert!(
                error
                    .chain()
                    .any(|cause| cause.downcast_ref::<std::io::Error>().is_some()),
                "deadline/join refusal is not the required native flush error: {error:#}"
            );
        });
    }

    #[test]
    fn wrong_direction_phase_and_over_limit_prefix_refuse_on_actual_channel() {
        owned_fixture(|deadline| async move {
            let first_names = names(deadline);
            let (mut sender, mut receiver) = endpoints(&first_names, deadline, false).await;
            let ready = frame()
                .next(Phase::Ready, std::process::id(), Uuid::now_v7())
                .unwrap();
            assert!(
                sender
                    .send(&mut Codec::default(), &ready, deadline)
                    .await
                    .is_err()
            );
            // Model a malicious peer bypassing the outgoing wrapper.
            let wire = Codec::default().encode(&ready).unwrap();
            until(deadline, async {
                Ok(sender.stream.as_mut().unwrap().write_all(&wire).await?)
            })
            .await
            .unwrap();
            assert!(
                receiver
                    .receive(&mut Codec::default(), deadline)
                    .await
                    .is_err()
            );
            drop(receiver);
            drop(sender);
            let second_names = names(deadline);
            let (mut sender, mut receiver) = endpoints(&second_names, deadline, false).await;
            until(deadline, async {
                Ok(sender
                    .stream
                    .as_mut()
                    .unwrap()
                    .write_all(&4096_u32.to_le_bytes())
                    .await?)
            })
            .await
            .unwrap();
            // Prefix-inclusive 4 KiB means this body is too large; no body follows.
            assert!(
                receiver
                    .receive(&mut Codec::default(), deadline)
                    .await
                    .is_err()
            );
            drop(receiver);
            drop(sender);
        });
    }

    #[test]
    fn unread_close_keeps_actual_worker_until_known_fixture_peer_drains() {
        owned_fixture(|deadline| async move {
            let names = names(deadline);
            let (mut sender, mut receiver) = endpoints(&names, deadline, false).await;
            let first = frame();
            let second = first
                .next(Phase::Permit, std::process::id(), Uuid::now_v7())
                .unwrap();
            let mut sent = Codec::default();
            sender.send(&mut sent, &first, deadline).await.unwrap();
            sender.send(&mut sent, &second, deadline).await.unwrap();
            let mut close = sender.close(deadline).unwrap();
            assert!(
                close
                    .wait_until(Instant::now() + Duration::from_millis(30))
                    .await
                    .is_err()
            );
            assert!(
                close.is_pending(),
                "timeout must retain the actual join handle"
            );
            // Test-owned cleanup drains this known receiver; it is not qualification.
            let mut received = Codec::default();
            receiver.receive(&mut received, deadline).await.unwrap();
            receiver.receive(&mut received, deadline).await.unwrap();
            close.wait_until(deadline).await.unwrap();
            receiver.terminal_eof(deadline).await.unwrap();
            assert!(!close.is_pending());
        });
    }
}
