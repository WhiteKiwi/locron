use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::set_owner_only;
use crate::{StoreError, StoreResult};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockMetadata {
    pub pid: u32,
    pub lifetime_id: String,
    pub started_at_us: i64,
    pub binary_version: String,
}

/// A permanent file whose OS lock is held until this value is dropped.
pub struct DaemonLock {
    file: File,
}

impl DaemonLock {
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

    pub fn file(&self) -> &File {
        &self.file
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.file);
    }
}
