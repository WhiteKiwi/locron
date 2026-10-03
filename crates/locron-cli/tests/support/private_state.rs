//! Disposable state roots with the platform's actual private-directory contract.

use std::path::{Path, PathBuf};

/// Owns temporary cleanup while exposing the state path used by every adapter.
pub struct PrivateState {
    path: PathBuf,
    _temporary: tempfile::TempDir,
}

impl PrivateState {
    /// Returns the private Windows child or the original non-Windows root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Releases setup guards before subprocesses while retaining temporary cleanup.
#[must_use]
pub fn private_state_fixture() -> PrivateState {
    let temporary = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let path = {
        let guard =
            locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("state"))
                .unwrap();
        let path = guard.normalized_path().to_path_buf();
        drop(guard);
        path
    };
    #[cfg(not(windows))]
    let path = temporary.path().to_path_buf();
    PrivateState {
        path,
        _temporary: temporary,
    }
}
