//! Bounded local hints and lifetime-scoped role control, without public async types.

use std::io;
use std::path::Path;

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

/// Stable account/state identity for local endpoints and registered role names.
#[cfg(windows)]
pub fn instance_identity(root: &Path) -> io::Result<String> {
    instance_identity_guarded(&crate::filesystem::DirectoryGuard::existing_private(root)?)
}

/// Queries the guarded object while its complete no-write/no-delete-sharing chain is live.
#[cfg(windows)]
pub fn instance_identity_guarded(guard: &crate::filesystem::DirectoryGuard) -> io::Result<String> {
    let sid = crate::windows::current_user_sid()?;
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
    identity_digest(&sid, volume_serial_number, file_id)
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
    let identity = instance_identity_guarded(guard)?;
    let lifetime = lifetime.map(|value| value.simple().to_string());
    Ok(match lifetime {
        Some(lifetime) => format!(r"\\.\pipe\locron-v1-{identity}-{role}-{lifetime}"),
        None => format!(r"\\.\pipe\locron-v1-{identity}-{role}"),
    })
}

#[cfg(windows)]
fn send_message(name: &str, message: &'static [u8]) -> io::Result<()> {
    let name = name.to_owned();
    // The joined worker avoids nested runtime panics. One deadline covers all native I/O.
    std::thread::Builder::new()
        .name("locron-ipc-hint".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()?;
            runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_millis(200), async {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut options = tokio::net::windows::named_pipe::ClientOptions::new();
                    // SECURITY_IDENTIFICATION; Tokio also sets SECURITY_SQOS_PRESENT.
                    options.security_qos_flags(0x0001_0000);
                    let mut stream = loop {
                        match options.open(&name) {
                            Ok(stream) => break stream,
                            Err(error) if error.raw_os_error() == Some(231) => {
                                tokio::time::sleep(std::time::Duration::from_millis(5)).await
                            }
                            Err(error) => return Err(error),
                        }
                    };
                    stream
                        .write_u8(u8::try_from(message.len()).map_err(io::Error::other)?)
                        .await?;
                    stream.write_all(message).await?;
                    let mut ack = [0; ACK_MESSAGE.len()];
                    stream.read_exact(&mut ack).await?;
                    if ack != ACK_MESSAGE {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid local hint acknowledgement",
                        ));
                    }
                    stream.write_u8(0xff).await?;
                    Ok(())
                })
                .await
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "local hint deadline elapsed")
                })?
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
