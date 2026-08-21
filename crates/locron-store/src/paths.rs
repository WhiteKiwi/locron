use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::StoreError;

/// All files owned by one locron scheduler instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub daemon_lock: PathBuf,
    pub wake_socket: PathBuf,
    pub outputs: PathBuf,
    pub temporary: PathBuf,
}

impl StatePaths {
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

    pub fn new(root: PathBuf) -> Self {
        Self {
            database: root.join("state.db"),
            daemon_lock: root.join("daemon.lock"),
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

    pub fn output_directory(&self, run_id: &str) -> Result<PathBuf, StoreError> {
        validate_uuid(run_id)?;
        Ok(self.outputs.join(run_id))
    }

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

pub(crate) fn set_owner_only(path: &Path, directory: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if directory { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, directory);
        Ok(())
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
