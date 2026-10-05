//! Private implementation of the dashboard's explicit output-prune operation.

use std::io;
use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use locron_core::filesystem::DirectoryGuard;
use locron_store::{RetentionCandidate, StatePaths, Store, StoreError};
use serde_json::{Value, json};

use super::{ApiError, now_us};

const MAX_CANDIDATES: usize = 100;
const MAX_AGE_US: i64 = 30 * 24 * 60 * 60 * 1_000_000;

pub(super) fn run(store: Option<&Store>, dry_run: bool) -> Result<Value, ApiError> {
    let Some(store) = store else {
        return Ok(json!({"dry_run": true, "candidate_count": 0, "bytes": 0}));
    };
    let settings = store.settings()?;
    let retained = store.retained_output_bytes()?;
    let cutoff = now_us().saturating_sub(MAX_AGE_US);
    let (selected, bytes) = select(
        store.paths(),
        store.output_retention_candidates(MAX_CANDIDATES)?,
        retained,
        settings.output_limit_bytes,
        cutoff,
    )?;

    if !dry_run {
        for (candidate, path) in &selected {
            // All selected database paths were validated before the first intent.
            // A later removal/sync/commit failure leaves the durable intent pending.
            store.mark_output_prune_pending(candidate, now_us())?;
            remove_output(store.paths(), path)?;
            store.finish_output_prune(candidate, now_us())?;
        }
    }
    Ok(json!({
        "dry_run": dry_run,
        "candidate_count": selected.len(),
        "bytes": bytes,
    }))
}

fn select(
    paths: &StatePaths,
    candidates: Vec<RetentionCandidate>,
    mut projected: i64,
    limit: i64,
    cutoff: i64,
) -> Result<(Vec<(RetentionCandidate, PathBuf)>, i64), ApiError> {
    if projected < 0 || limit < 0 {
        return Err(invalid_metadata("output retention byte totals must be non-negative"));
    }
    let mut selected = Vec::new();
    let mut bytes = 0_i64;
    // Store supplies oldest-first terminal artifacts under the existing batch cap.
    for candidate in candidates {
        if candidate.physical_bytes < 0 {
            return Err(invalid_metadata("output artifact byte count must be non-negative"));
        }
        if candidate.finalized_at_us >= cutoff && projected <= limit {
            continue;
        }
        let attempt = u16::try_from(candidate.attempt_number)
            .map_err(|_| invalid_metadata("retention attempt number is outside path range"))?;
        let path = paths.final_output(&candidate.run_id, attempt)?;
        let expected = format!("{}/{attempt}.log", candidate.run_id);
        if candidate.relative_path != expected {
            return Err(invalid_metadata("database output path is not the canonical final path"));
        }
        bytes = bytes
            .checked_add(candidate.physical_bytes)
            .ok_or_else(|| invalid_metadata("selected output byte count overflow"))?;
        projected = projected.saturating_sub(candidate.physical_bytes);
        selected.push((candidate, path));
    }
    Ok((selected, bytes))
}

fn invalid_metadata(message: &str) -> ApiError {
    ApiError::Message(
        StatusCode::INTERNAL_SERVER_ERROR,
        "state_error",
        message.to_owned(),
    )
}

fn unsafe_output(message: &str) -> ApiError {
    ApiError::Message(
        StatusCode::BAD_REQUEST,
        "invalid_request",
        message.to_owned(),
    )
}

/// Missing managed directories remain missing; existing ones are never repaired here.
fn existing_directory(path: &Path) -> Result<Option<DirectoryGuard>, ApiError> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StoreError::Io(error).into()),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(unsafe_output("refusing to traverse unsafe output parent"))
        }
        Ok(_) => DirectoryGuard::existing_private(path)
            .map(Some)
            .map_err(StoreError::Io)
            .map_err(ApiError::from),
    }
}

fn remove_output(paths: &StatePaths, path: &Path) -> Result<(), ApiError> {
    let _root = existing_directory(&paths.root)?
        .ok_or_else(|| invalid_metadata("state directory disappeared during output pruning"))?;
    let Some(_outputs) = existing_directory(&paths.outputs)? else {
        return Ok(());
    };
    let directory = path
        .parent()
        .ok_or_else(|| invalid_metadata("canonical output path has no parent"))?;
    let Some(_run) = existing_directory(directory)? else {
        return Ok(());
    };
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(StoreError::Io(error).into()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(unsafe_output("refusing to prune symbolic-link output"));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(unsafe_output("refusing to prune non-file output"));
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
        Err(error) => return Err(StoreError::Io(error).into()),
    }
    // Preserve the existing Unix maintenance durability order. Windows has no
    // inferred directory-fsync guarantee; its retained guarded adapter owns removal.
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(StoreError::Io)?;
    Ok(())
}
