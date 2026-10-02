use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::set_owner_only;
use crate::{StoreError, StoreResult};

/// Diagnostic metadata written into the daemon lock file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockMetadata {
    /// Process ID of the owning daemon.
    pub pid: u32,
    /// Scheduler lifetime identity of the owning daemon.
    pub lifetime_id: String,
    /// Wall-clock instant the daemon acquired the lock, in microseconds.
    pub started_at_us: i64,
    /// Version of the binary that acquired the lock.
    pub binary_version: String,
}

/// Diagnostic role identity published separately from mandatory Windows lock bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoleLockMetadata {
    /// Advisory process/lifetime facts; the permanent OS lock remains authoritative.
    pub metadata: LockMetadata,
    /// True only for an invocation explicitly marked as a registered-service role.
    pub service_mode: bool,
}

/// A permanent file whose OS lock is held until this value is dropped.
pub struct DaemonLock {
    file: File,
    _guard: locron_core::filesystem::DirectoryGuard,
    #[cfg(windows)]
    owner_sidecar: PathBuf,
}

impl DaemonLock {
    /// Acquires an exclusive OS lock at the path and records diagnostic metadata.
    ///
    /// Fails with [`StoreError::DaemonAlreadyRunning`] when another process
    /// already holds the lock.
    pub fn acquire(path: &Path, metadata: &LockMetadata) -> StoreResult<Self> {
        Self::acquire_role(path, metadata, false)
    }

    /// Acquires a role lock with an explicit registered-service mode marker.
    pub fn acquire_role(
        path: &Path,
        metadata: &LockMetadata,
        service_mode: bool,
    ) -> StoreResult<Self> {
        if let Some(parent) = path.parent() {
            crate::paths::ensure_private_directory(parent)?;
        }
        let (mut file, guard) = locron_core::filesystem::open_private_or_create(path)?.into_parts();
        set_owner_only(path, false)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StoreError::DaemonAlreadyRunning,
            std::fs::TryLockError::Error(error) => StoreError::Io(error),
        })?;

        let diagnostic = serde_json::to_vec(metadata)?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&diagnostic)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        #[cfg(windows)]
        let owner_sidecar = {
            let owner_path = Self::owner_sidecar(path);
            let temporary = owner_path.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
            let role = RoleLockMetadata {
                metadata: metadata.clone(),
                service_mode,
            };
            let mut owner = locron_core::filesystem::create_private_new(&temporary)?;
            owner.write_all(&serde_json::to_vec(&role)?)?;
            owner.sync_all()?;
            drop(owner);
            if let Err(error) = locron_core::filesystem::rename_private(&temporary, &owner_path) {
                let _ = locron_core::filesystem::remove_private_file(&temporary);
                return Err(error.into());
            }
            owner_path
        };
        #[cfg(not(windows))]
        let _ = service_mode;
        Ok(Self {
            file,
            _guard: guard,
            #[cfg(windows)]
            owner_sidecar,
        })
    }

    /// Returns the advisory sidecar name for a permanent role lock.
    #[must_use]
    pub fn owner_sidecar(path: &Path) -> PathBuf {
        let mut name = path.as_os_str().to_os_string();
        name.push(".owner.json");
        PathBuf::from(name)
    }

    /// Reads bounded diagnostic metadata without acquiring or reading owned Windows lock bytes.
    pub fn read_role_metadata(path: &Path) -> StoreResult<Option<RoleLockMetadata>> {
        #[cfg(windows)]
        let source = Self::owner_sidecar(path);
        #[cfg(not(windows))]
        let source = path.to_path_buf();
        let mut file =
            match locron_core::filesystem::open_private(&source, OpenOptions::new().read(true)) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
        let mut contents = Vec::new();
        (&mut *file)
            .take(16 * 1024 + 1)
            .read_to_end(&mut contents)?;
        if contents.len() > 16 * 1024 {
            return Err(StoreError::Conflict(
                "role owner metadata exceeds its bound".into(),
            ));
        }
        #[cfg(windows)]
        let metadata = serde_json::from_slice(&contents)?;
        #[cfg(not(windows))]
        let metadata = RoleLockMetadata {
            metadata: serde_json::from_slice(&contents)?,
            service_mode: false,
        };
        Ok(Some(metadata))
    }

    /// Proves the lock is free without holding it, failing with
    /// [`StoreError::MigrationRequiresDaemonRestart`] when it is held.
    pub fn try_prove_free(path: &Path) -> StoreResult<()> {
        let file = locron_core::filesystem::open_private_or_create(path)?;
        set_owner_only(path, false)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StoreError::MigrationRequiresDaemonRestart,
            std::fs::TryLockError::Error(error) => StoreError::Io(error),
        })?;
        File::unlock(&file)?;
        Ok(())
    }

    /// Returns a reference to the underlying locked file.
    #[must_use]
    pub fn file(&self) -> &File {
        &self.file
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = locron_core::filesystem::remove_private_file(&self.owner_sidecar);
        let _ = File::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn missing_role_observation_does_not_create_its_state_root() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("absent");
        let path = root.join("daemon.lock");
        assert_eq!(DaemonLock::read_role_metadata(&path).unwrap(), None);
        assert!(!root.exists());
        let metadata = LockMetadata {
            pid: std::process::id(),
            lifetime_id: uuid::Uuid::now_v7().to_string(),
            started_at_us: 1,
            binary_version: "test".into(),
        };
        let lock = DaemonLock::acquire_role(&path, &metadata, true).unwrap();
        assert!(locron_core::filesystem::is_private(&root, true).unwrap());
        assert_eq!(
            DaemonLock::read_role_metadata(&path)
                .unwrap()
                .unwrap()
                .metadata,
            metadata
        );
        drop(lock);
    }

    #[test]
    fn ownership_observer_does_not_read_windows_locked_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        locron_core::filesystem::DirectoryGuard::private(&root).unwrap();
        let path = root.join("daemon.lock");
        let metadata = LockMetadata {
            pid: std::process::id(),
            lifetime_id: uuid::Uuid::now_v7().to_string(),
            started_at_us: 1,
            binary_version: "test".into(),
        };
        let lock = DaemonLock::acquire_role(&path, &metadata, true).unwrap();
        let observed = DaemonLock::read_role_metadata(&path).unwrap().unwrap();
        assert_eq!(observed.metadata, metadata);
        #[cfg(windows)]
        assert!(observed.service_mode);
        assert!(matches!(
            DaemonLock::try_prove_free(&path),
            Err(StoreError::MigrationRequiresDaemonRestart)
        ));
        drop(lock);
        DaemonLock::try_prove_free(&path).unwrap();
    }
}
