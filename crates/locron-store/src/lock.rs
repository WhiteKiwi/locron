use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

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

/// A permanent file whose OS lock is held until this value is dropped.
pub struct DaemonLock {
    file: File,
}

impl DaemonLock {
    /// Acquires an exclusive OS lock at the path and records diagnostic metadata.
    ///
    /// Fails with [`StoreError::DaemonAlreadyRunning`] when another process
    /// already holds the lock.
    pub fn acquire(path: &Path, metadata: &LockMetadata) -> StoreResult<Self> {
        if let Some(parent) = path.parent() {
            crate::paths::ensure_private_directory(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
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
        Ok(Self { file })
    }

    /// Proves the lock is free without holding it, failing with
    /// [`StoreError::MigrationRequiresDaemonRestart`] when it is held.
    pub fn try_prove_free(path: &Path) -> StoreResult<()> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
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
        let _ = File::unlock(&self.file);
    }
}
