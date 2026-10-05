//! Canonical path preflight and guarded removal for explicit CLI output pruning.

use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use locron_core::filesystem::DirectoryGuard;
use locron_store::{RetentionCandidate, StatePaths};

/// Validate the complete selected batch before the caller records any prune intent.
/// This is a pure identity check: dry-run does not open or create output directories.
pub(super) fn validated_paths(
    paths: &StatePaths,
    candidates: &[RetentionCandidate],
) -> Result<Vec<PathBuf>> {
    candidates
        .iter()
        .map(|candidate| {
            let attempt = u16::try_from(candidate.attempt_number)
                .context("retention attempt number is outside path range")?;
            let path = paths.final_output(&candidate.run_id, attempt)?;
            if candidate.relative_path != format!("{}/{attempt}.log", candidate.run_id) {
                return Err(anyhow!(
                    "database output path is not the canonical final path"
                ));
            }
            Ok(path)
        })
        .collect()
}

/// Missing managed directories stay missing; existing directories are never repaired here.
fn existing_directory(path: &Path) -> Result<Option<DirectoryGuard>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(anyhow!("refusing to traverse unsafe output parent"))
        }
        Ok(_) => DirectoryGuard::existing_private(path)
            .map(Some)
            .context("validate private output directory"),
    }
}

/// Remove one prevalidated canonical path, retaining managed-parent guards through sync.
/// The caller owns the durable pending intent and records completion only after success.
pub(super) fn remove_output(paths: &StatePaths, path: &Path) -> Result<()> {
    let _root = existing_directory(&paths.root)?
        .ok_or_else(|| anyhow!("state directory disappeared during output pruning"))?;
    let Some(_outputs) = existing_directory(&paths.outputs)? else {
        return Ok(());
    };
    let directory = path
        .parent()
        .ok_or_else(|| anyhow!("canonical output path has no parent"))?;
    let Some(_run) = existing_directory(directory)? else {
        return Ok(());
    };
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(anyhow!("refusing to prune symbolic-link output"));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(anyhow!("refusing to prune non-file output"));
        }
        Ok(_) => {}
    }
    #[cfg(windows)]
    let removed = locron_core::filesystem::remove_private_file(path);
    #[cfg(not(windows))]
    let removed = std::fs::remove_file(path);
    match removed {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("remove retained output"),
    }
    // Match Unix automatic maintenance: directory durability precedes Store completion.
    // Windows retains its native guarded-removal contract without an inferred fsync claim.
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .context("sync pruned output directory")?;
    Ok(())
}
