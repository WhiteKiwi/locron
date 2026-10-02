use std::env;
#[cfg(not(windows))]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::StoreError;

/// All files owned by one locron scheduler instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePaths {
    /// State directory root containing all managed files.
    pub root: PathBuf,
    /// SQLite database file.
    pub database: PathBuf,
    /// Daemon exclusive-lock file.
    pub daemon_lock: PathBuf,
    /// Dashboard lifetime lock, independent of scheduler ownership.
    pub dashboard_lock: PathBuf,
    /// Socket used to wake a running daemon.
    pub wake_socket: PathBuf,
    /// Directory holding run output artifacts.
    pub outputs: PathBuf,
    /// Directory for temporary files.
    pub temporary: PathBuf,
}

impl StatePaths {
    /// Resolves the state root from the override, `LOCRON_STATE_DIR`, or the platform default.
    pub fn discover(override_dir: Option<&Path>) -> Result<Self, StoreError> {
        let root = if let Some(path) = override_dir {
            path.to_path_buf()
        } else if let Some(path) = env::var_os("LOCRON_STATE_DIR") {
            PathBuf::from(path)
        } else {
            platform_default()?
        };
        Ok(Self::new(root))
    }

    /// Builds the full state layout under one root directory.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            database: root.join("state.db"),
            daemon_lock: root.join("daemon.lock"),
            dashboard_lock: root.join("dashboard.lock"),
            wake_socket: root.join("wake.sock"),
            outputs: root.join("outputs"),
            temporary: root.join("tmp"),
            root,
        }
    }

    /// Creates the state layout without following an existing symlink at any managed root.
    pub fn ensure(&self) -> Result<(), StoreError> {
        ensure_private_directory(&self.root)?;
        ensure_private_directory(&self.outputs)?;
        ensure_private_directory(&self.temporary)?;
        Ok(())
    }

    /// Retains validated directory-chain guards for a complete state operation.
    pub fn guard(&self) -> Result<locron_core::filesystem::DirectoryGuard, StoreError> {
        Ok(locron_core::filesystem::DirectoryGuard::private(
            &self.root,
        )?)
    }

    /// Returns the per-run output directory, validating the run identity first.
    pub fn output_directory(&self, run_id: &str) -> Result<PathBuf, StoreError> {
        validate_uuid(run_id)?;
        Ok(self.outputs.join(run_id))
    }

    /// Returns the in-progress output path for an attempt, rejecting attempt zero.
    pub fn partial_output(&self, run_id: &str, attempt: u16) -> Result<PathBuf, StoreError> {
        if attempt == 0 {
            return Err(StoreError::InvalidIdentity(
                "attempt number must be positive".into(),
            ));
        }
        Ok(self
            .output_directory(run_id)?
            .join(format!("{attempt}.partial")))
    }

    /// Returns the finalized output path for an attempt, rejecting attempt zero.
    pub fn final_output(&self, run_id: &str, attempt: u16) -> Result<PathBuf, StoreError> {
        if attempt == 0 {
            return Err(StoreError::InvalidIdentity(
                "attempt number must be positive".into(),
            ));
        }
        Ok(self
            .output_directory(run_id)?
            .join(format!("{attempt}.log")))
    }
}

fn platform_default() -> Result<PathBuf, StoreError> {
    #[cfg(windows)]
    {
        // KnownFolder discovery avoids trusting HOME/XDG or a mutable LocalAppData value.
        let value = locron_core::windows::run_script_json(
            "[Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData) | & $locronToJson -Compress",
            &serde_json::json!({}),
        )?;
        let path = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(StoreError::StateDirectoryUnavailable)?;
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(StoreError::StateDirectoryUnavailable);
        }
        Ok(path.join("locron"))
    }
    #[cfg(not(windows))]
    {
        let home = env::var_os("HOME").ok_or(StoreError::StateDirectoryUnavailable)?;
        #[cfg(target_os = "macos")]
        return Ok(PathBuf::from(home).join("Library/Application Support/locron"));

        #[cfg(not(target_os = "macos"))]
        {
            if let Some(path) = env::var_os("XDG_STATE_HOME").filter(|p| !p.is_empty()) {
                Ok(PathBuf::from(path).join("locron"))
            } else {
                Ok(PathBuf::from(home).join(".local/state/locron"))
            }
        }
    }
}

pub(crate) fn validate_uuid(value: &str) -> Result<(), StoreError> {
    let parsed = uuid::Uuid::parse_str(value)
        .map_err(|_| StoreError::InvalidIdentity(format!("invalid UUID: {value}")))?;
    if parsed.hyphenated().to_string() != value || value.to_ascii_lowercase() != value {
        return Err(StoreError::InvalidIdentity(format!(
            "UUID is not lowercase canonical text: {value}"
        )));
    }
    Ok(())
}

pub(crate) fn ensure_private_directory(path: &Path) -> Result<(), StoreError> {
    #[cfg(windows)]
    {
        locron_core::filesystem::DirectoryGuard::private(path)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(StoreError::UnsafePath(path.to_path_buf()));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(path)?;
            }
            Err(error) => return Err(error.into()),
        }
        set_owner_only(path, true)?;
        Ok(())
    }
}

pub(crate) fn set_owner_only(path: &Path, directory: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if directory { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        // Ordinary state operations validate before use and never silently repair.
        if locron_core::filesystem::is_private(path, directory)? {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("unsafe private permissions: {}", path.display()),
            ))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, directory);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private filesystem permission enforcement is not implemented on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths_require_canonical_components() {
        let paths = StatePaths::new(PathBuf::from("state"));
        assert!(paths.output_directory("../escape").is_err());
        assert!(
            paths
                .partial_output("018f3f74-8d70-7cc0-98a2-eef43f17eab4", 1)
                .unwrap()
                .ends_with("outputs/018f3f74-8d70-7cc0-98a2-eef43f17eab4/1.partial")
        );
    }
}
