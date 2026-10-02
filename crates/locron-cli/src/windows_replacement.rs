//! Native primitive qualification; the production operation engine is not exposed yet.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use fs_at::os::windows::FileExt;
    use locron_core::filesystem::{
        DirectoryGuard, GuardedFile, create_private_new_exclusive, file_identity,
        open_private_exclusive,
    };

    use super::super::sha256_hex;
    use super::super::windows_fixture::PrivateFixture;

    fn private_root() -> PrivateFixture {
        PrivateFixture::new("locron-replacement-fixture-")
    }

    fn new_file(path: &Path, bytes: &[u8]) -> GuardedFile {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        assert_eq!(read(&mut file), bytes);
        file
    }

    fn read(file: &mut GuardedFile) -> Vec<u8> {
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        Read::by_ref(&mut **file)
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() <= 64 * 1024 * 1024);
        bytes
    }

    fn delete_exact(file: GuardedFile) -> DirectoryGuard {
        let (file, parent) = file.into_parts();
        if let Err((file, error)) = file.delete_by_handle() {
            // Every Err is ambiguous; this primitive fixture deliberately stops
            // rather than claiming the old path is unchanged or falling back.
            drop(file);
            panic!("exact deletion failed ambiguously: {error}");
        }
        parent
    }

    #[test]
    fn exact_delete_and_identity_gated_backup_restore_keep_durable_old_bytes() {
        let root = private_root();
        let executable = root.path().join("locron.exe");
        let old_bytes = b"verified old executable fixture";
        let original = new_file(&executable, old_bytes);
        let old_identity = file_identity(&original).unwrap();
        drop(original);
        let mut backup = new_file(&root.path().join("backup-locron.exe"), old_bytes);
        let backup_hash = sha256_hex(&read(&mut backup));
        let mut journal = new_file(&root.path().join("write-ahead.fixture"), b"");
        journal
            .write_all(
                serde_json::to_string(&serde_json::json!({
                    "backup_sha256": backup_hash,
                    "volume": format!("{:016x}", old_identity.volume_serial_number),
                    "old_id": format!("{:032x}", old_identity.file_id), "delete_intent": true
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        journal.sync_all().unwrap();

        let mut gate = open_private_exclusive(&executable).unwrap();
        assert_eq!(file_identity(&gate).unwrap(), old_identity);
        assert_eq!(sha256_hex(&read(&mut gate)), backup_hash);
        let old_parent = delete_exact(gate);
        assert!(!executable.exists());
        let mut replacement = create_private_new_exclusive(&executable).unwrap();
        let created_identity = file_identity(&replacement).unwrap();
        journal
            .write_all(
                format!(
                    "\ncreated={:016x}:{:032x}",
                    created_identity.volume_serial_number, created_identity.file_id
                )
                .as_bytes(),
            )
            .unwrap();
        journal.sync_all().unwrap();
        replacement
            .write_all(b"partial new bytes at a known identity")
            .unwrap();
        replacement.sync_all().unwrap();

        // A known-created leaf can be removed through its exact retained handle.
        // A real crash between CreateNew and the durable ID would instead refuse.
        assert_eq!(file_identity(&replacement).unwrap(), created_identity);
        let created_parent = delete_exact(replacement);
        let mut restored = create_private_new_exclusive(&executable).unwrap();
        let restored_id = file_identity(&restored).unwrap();
        journal
            .write_all(
                format!(
                    "\nrestored={:016x}:{:032x}",
                    restored_id.volume_serial_number, restored_id.file_id
                )
                .as_bytes(),
            )
            .unwrap();
        journal.sync_all().unwrap();
        let bytes = read(&mut backup);
        assert_eq!(sha256_hex(&bytes), backup_hash);
        restored.write_all(&bytes).unwrap();
        restored.sync_all().unwrap();
        assert_eq!(read(&mut restored), old_bytes);
        assert_eq!(sha256_hex(&read(&mut backup)), backup_hash);
        drop((restored, journal, backup, created_parent, old_parent));
        assert_eq!(fs::read(executable).unwrap(), old_bytes);
    }

    #[test]
    fn a_competing_unknown_leaf_is_never_overwritten_or_adopted() {
        let root = private_root();
        let executable = root.path().join("locron.exe");
        let old = new_file(&executable, b"old");
        let parent = delete_exact(old);
        let mut foreign = new_file(&executable, b"unrecognized competing marker");
        let marker_id = file_identity(&foreign).unwrap();
        assert_eq!(
            create_private_new_exclusive(&executable)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(file_identity(&foreign).unwrap(), marker_id);
        assert_eq!(read(&mut foreign), b"unrecognized competing marker");
        drop((foreign, parent));
        assert_eq!(
            fs::read(executable).unwrap(),
            b"unrecognized competing marker"
        );
    }

    struct OwnedCmd(Child);

    impl OwnedCmd {
        fn finish(&mut self) {
            drop(self.0.stdin.take());
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.0.try_wait().unwrap().is_none() {
                assert!(
                    Instant::now() < deadline,
                    "test-owned cmd did not exit after EOF"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for OwnedCmd {
        fn drop(&mut self) {
            // This fixed builtin starts no descendants. Only our own child is
            // terminated on a fixture failure; unrelated mapped holders are untouched.
            let _ = self.0.kill();
            let deadline = Instant::now() + Duration::from_secs(3);
            while matches!(self.0.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn waiting_cmd(path: &Path) -> OwnedCmd {
        let mut command = Command::new(path);
        command
            .args(["/d", "/q", "/c", "set /p LOCRON_FIXTURE_WAIT="])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        OwnedCmd(command.spawn().unwrap())
    }

    #[test]
    fn mapped_image_refuses_share_zero_and_new_leaf_gate_blocks_relaunch() {
        let root = private_root();
        let executable = root.path().join("locron.exe");
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let cmd = fs::read(Path::new(&system_root).join("System32/cmd.exe")).unwrap();
        let old = new_file(&executable, &cmd);
        let identity = file_identity(&old).unwrap();
        drop(old);
        let mut child = waiting_cmd(&executable);
        assert!(child.0.try_wait().unwrap().is_none());
        assert_eq!(
            open_private_exclusive(&executable)
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        child.finish();

        let gate = open_private_exclusive(&executable).unwrap();
        assert_eq!(file_identity(&gate).unwrap(), identity);
        assert!(fs::rename(&executable, root.path().join("renamed.exe")).is_err());
        assert!(
            Command::new(&executable)
                .arg("/c")
                .arg("exit")
                .creation_flags(0x0800_0000)
                .spawn()
                .is_err()
        );
        let parent = delete_exact(gate);
        let mut replacement = create_private_new_exclusive(&executable).unwrap();
        replacement.write_all(&cmd).unwrap();
        replacement.sync_all().unwrap();
        assert!(
            Command::new(&executable)
                .arg("/c")
                .arg("exit")
                .creation_flags(0x0800_0000)
                .spawn()
                .is_err()
        );
        drop((replacement, parent));
        let mut working = waiting_cmd(&executable);
        working.finish();
    }
}
