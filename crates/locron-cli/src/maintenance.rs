//! Bounded filesystem and retention maintenance for the CLI composition adapter.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use locron_store::{
    OutputRecord, RetentionCandidate, StatePaths, Store, StoreError, repair_partial,
};

const MAX_ACTIONS: usize = 100;
const OUTPUT_MAX_AGE_US: i64 = 30 * 24 * 60 * 60 * 1_000_000;
const ORPHAN_GRACE_US: i64 = 60 * 60 * 1_000_000;

/// Counts durable artifact or run actions performed by one maintenance pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceReport {
    /// Total artifact or run actions consumed from the pass budget.
    pub actions: usize,
    /// Referenced partial or final artifacts repaired and finalized.
    pub outputs_recovered: usize,
    /// Referenced artifacts reconciled as missing.
    pub outputs_missing: usize,
    /// Output artifacts durably pruned.
    pub outputs_pruned: usize,
    /// Terminal run metadata records durably pruned.
    pub runs_pruned: usize,
    /// Verified unreferenced regular files removed after the grace period.
    pub orphans_removed: usize,
}

struct Pass {
    report: MaintenanceReport,
    errors: Vec<String>,
}

impl Pass {
    fn new() -> Self {
        Self {
            report: MaintenanceReport::default(),
            errors: Vec::new(),
        }
    }

    fn remaining(&self) -> usize {
        MAX_ACTIONS.saturating_sub(self.report.actions)
    }

    fn take_action(&mut self) -> bool {
        if self.remaining() == 0 {
            return false;
        }
        self.report.actions += 1;
        true
    }

    fn record(&mut self, context: &str, error: impl std::fmt::Display) {
        self.errors.push(format!("{context}: {error}"));
    }

    fn finish(self) -> Result<MaintenanceReport> {
        if self.errors.is_empty() {
            Ok(self.report)
        } else {
            Err(anyhow!(
                "maintenance completed with {} error(s): {}",
                self.errors.len(),
                self.errors.join("; ")
            ))
        }
    }
}

/// Runs one deterministic maintenance pass with a shared 100-action budget.
pub fn maintain(store: &Store, paths: &StatePaths, now_us: i64) -> Result<MaintenanceReport> {
    if store.paths() != paths {
        bail!("maintenance store and state paths do not match");
    }
    require_directory(&paths.outputs).context("validate managed output root")?;

    let mut pass = Pass::new();
    resume_output_prunes(store, paths, now_us, &mut pass)?;
    recover_referenced_outputs(store, paths, now_us, &mut pass)?;

    let pending_runs = store
        .pending_run_retention(MAX_ACTIONS)
        .context("list pending run retention")?;
    let pending_ids = pending_runs
        .iter()
        .map(|candidate| candidate.run_id.clone())
        .collect::<BTreeSet<_>>();
    prune_pending_run_outputs(store, paths, now_us, &pending_ids, &mut pass)?;
    prune_output_limits(store, paths, now_us, &mut pass)?;
    for candidate in &pending_runs {
        if !pass.take_action() {
            break;
        }
        match store.finish_run_retention(candidate) {
            Ok(()) => pass.report.runs_pruned += 1,
            Err(StoreError::Conflict(_)) => {}
            Err(error) => pass.record("finish pending run retention", error),
        }
    }
    select_run_retention(store, now_us, &mut pass)?;
    remove_verified_orphans(store, paths, now_us, &mut pass)?;
    pass.finish()
}

fn resume_output_prunes(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pass: &mut Pass,
) -> Result<()> {
    let candidates = store
        .pending_output_prunes(pass.remaining())
        .context("list pending output prunes")?;
    for candidate in candidates {
        if !pass.take_action() {
            break;
        }
        match remove_and_finish_output(store, paths, &candidate, now_us, false) {
            Ok(()) => pass.report.outputs_pruned += 1,
            Err(error) => pass.record("resume output prune", error),
        }
    }
    Ok(())
}

fn recover_referenced_outputs(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pass: &mut Pass,
) -> Result<()> {
    let candidates = store
        .referenced_partial_artifacts(pass.remaining())
        .context("list referenced partial outputs")?;
    for candidate in candidates {
        if !pass.take_action() {
            break;
        }
        let context = format!(
            "recover output {}/{}",
            candidate.run_id, candidate.attempt_number
        );
        match recover_output(
            store,
            paths,
            &candidate.run_id,
            candidate.attempt_number,
            &candidate.relative_path,
            now_us,
        ) {
            Ok(true) => pass.report.outputs_recovered += 1,
            Ok(false) => pass.report.outputs_missing += 1,
            Err(error) => pass.record(&context, error),
        }
    }
    Ok(())
}

fn recover_output(
    store: &Store,
    paths: &StatePaths,
    run_id: &str,
    attempt_number: i64,
    relative_path: &str,
    now_us: i64,
) -> Result<bool> {
    let attempt = u16::try_from(attempt_number).context("attempt number is outside path range")?;
    let partial = paths.partial_output(run_id, attempt)?;
    let final_path = paths.final_output(run_id, attempt)?;
    let expected_relative = format!("{run_id}/{attempt}.partial");
    if relative_path != expected_relative {
        bail!("database output path is not the canonical partial path");
    }

    let directory = partial
        .parent()
        .ok_or_else(|| anyhow!("output path has no parent"))?;
    if !is_safe_directory(directory)? {
        store.reconcile_output_missing(run_id, attempt_number, now_us)?;
        return Ok(false);
    }

    let partial_kind = file_kind(&partial)?;
    let final_kind = file_kind(&final_path)?;
    let repaired = match (partial_kind, final_kind) {
        (FileKind::Regular, FileKind::Missing) => {
            let repair = repair_partial(&partial).context("repair partial frame tail")?;
            fs::rename(&partial, &final_path).context("atomically finalize repaired output")?;
            sync_directory(directory)?;
            repair
        }
        (FileKind::Missing | FileKind::Unsafe, FileKind::Regular) => {
            repair_partial(&final_path).context("repair finalized frame tail")?
        }
        (FileKind::Regular, FileKind::Regular) => {
            bail!("both partial and final output files exist")
        }
        (FileKind::Regular, FileKind::Unsafe) => {
            bail!("final output path is occupied by an unsafe filesystem object")
        }
        (FileKind::Missing | FileKind::Unsafe, FileKind::Missing | FileKind::Unsafe) => {
            store.reconcile_output_missing(run_id, attempt_number, now_us)?;
            return Ok(false);
        }
    };
    store.reconcile_output_finalized(
        &OutputRecord {
            run_id: run_id.to_owned(),
            attempt_number,
            relative_path: format!("{run_id}/{attempt}.log"),
            state: "finalized".into(),
            retained_payload_bytes: i64::try_from(repaired.payload_bytes).unwrap_or(i64::MAX),
            physical_bytes: i64::try_from(repaired.physical_bytes).unwrap_or(i64::MAX),
            discarded_bytes: 0,
            truncated: false,
        },
        now_us,
    )?;
    Ok(true)
}

fn prune_pending_run_outputs(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pending_ids: &BTreeSet<String>,
    pass: &mut Pass,
) -> Result<()> {
    if pending_ids.is_empty() || pass.remaining() == 0 {
        return Ok(());
    }
    let candidates = store
        .output_retention_candidates(MAX_ACTIONS)
        .context("list outputs for pending metadata retention")?;
    for candidate in candidates
        .into_iter()
        .filter(|candidate| pending_ids.contains(&candidate.run_id))
    {
        if !pass.take_action() {
            break;
        }
        match remove_and_finish_output(store, paths, &candidate, now_us, true) {
            Ok(()) => pass.report.outputs_pruned += 1,
            Err(error) => pass.record("prune output before metadata", error),
        }
    }
    Ok(())
}

fn prune_output_limits(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pass: &mut Pass,
) -> Result<()> {
    if pass.remaining() == 0 {
        return Ok(());
    }
    let settings = store.settings().context("read output retention settings")?;
    let mut retained = store
        .retained_output_bytes()
        .context("read retained output bytes")?;
    let age_cutoff = now_us.saturating_sub(OUTPUT_MAX_AGE_US);
    let candidates = store
        .output_retention_candidates(MAX_ACTIONS)
        .context("list output retention candidates")?;
    for candidate in candidates {
        if candidate.finalized_at_us >= age_cutoff && retained <= settings.output_limit_bytes {
            continue;
        }
        if !pass.take_action() {
            break;
        }
        match remove_and_finish_output(store, paths, &candidate, now_us, true) {
            Ok(()) => {
                retained = retained.saturating_sub(candidate.physical_bytes);
                pass.report.outputs_pruned += 1;
            }
            Err(error) => pass.record("prune retained output", error),
        }
    }
    Ok(())
}

fn remove_and_finish_output(
    store: &Store,
    paths: &StatePaths,
    candidate: &RetentionCandidate,
    now_us: i64,
    mark_pending: bool,
) -> Result<()> {
    let attempt = u16::try_from(candidate.attempt_number)
        .context("retention attempt number is outside path range")?;
    let path = paths.final_output(&candidate.run_id, attempt)?;
    let expected_relative = format!("{}/{attempt}.log", candidate.run_id);
    if candidate.relative_path != expected_relative {
        bail!("database output path is not the canonical final path");
    }
    let directory = path
        .parent()
        .ok_or_else(|| anyhow!("output path has no parent"))?;
    if mark_pending {
        store.mark_output_prune_pending(candidate, now_us)?;
    }
    match fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            store.finish_output_prune(candidate, now_us)?;
            return Ok(());
        }
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!("refusing to traverse unsafe output parent")
        }
        Ok(_) => {}
        Err(error) => return Err(error.into()),
    }
    match file_kind(&path)? {
        FileKind::Regular => {
            fs::remove_file(&path).context("remove retained output")?;
            sync_directory(directory)?;
        }
        FileKind::Missing => {}
        FileKind::Unsafe => bail!("refusing to remove symbolic link or non-file output"),
    }
    store.finish_output_prune(candidate, now_us)?;
    Ok(())
}

fn select_run_retention(store: &Store, now_us: i64, pass: &mut Pass) -> Result<()> {
    let candidates = store
        .run_retention_candidates(now_us, pass.remaining())
        .context("list run metadata retention candidates")?;
    for candidate in candidates {
        if !pass.take_action() {
            break;
        }
        match store.mark_run_retention_pending(&candidate, now_us) {
            Ok(()) => match store.finish_run_retention(&candidate) {
                Ok(()) => pass.report.runs_pruned += 1,
                Err(StoreError::Conflict(_)) => {}
                Err(error) => pass.record("finish selected run retention", error),
            },
            Err(error) => pass.record("select run metadata retention", error),
        }
    }
    Ok(())
}

fn remove_verified_orphans(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pass: &mut Pass,
) -> Result<()> {
    if pass.remaining() == 0 || now_us < ORPHAN_GRACE_US {
        return Ok(());
    }
    let cutoff = UNIX_EPOCH
        .checked_add(Duration::from_micros(
            u64::try_from(now_us - ORPHAN_GRACE_US).unwrap_or(0),
        ))
        .ok_or_else(|| anyhow!("orphan cutoff is outside system time range"))?;
    let mut directories = sorted_entries(&paths.outputs)?;
    for directory in directories.drain(..) {
        if pass.remaining() == 0 {
            break;
        }
        let directory_name = directory.file_name();
        let Some(run_id) = directory_name.to_str() else {
            continue;
        };
        if !is_canonical_uuid(run_id) || !is_safe_directory(&directory.path())? {
            continue;
        }
        match store.run(run_id) {
            Ok(_) => continue,
            Err(StoreError::NotFound(_)) => {}
            Err(error) => {
                pass.record("verify orphan run identity", error);
                continue;
            }
        }
        for entry in sorted_entries(&directory.path())? {
            if pass.remaining() == 0 {
                break;
            }
            let path = entry.path();
            if !is_canonical_output_name(&entry.file_name())
                || file_kind(&path)? != FileKind::Regular
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata
                .modified()
                .context("read orphan modification time")?
                > cutoff
            {
                continue;
            }
            if !pass.take_action() {
                break;
            }
            match fs::remove_file(&path) {
                Ok(()) => {
                    pass.report.orphans_removed += 1;
                    if let Err(error) = sync_directory(&directory.path()) {
                        pass.record("sync orphan output directory", error);
                    }
                }
                Err(error) => pass.record("remove verified orphan output", error),
            }
        }
    }
    Ok(())
}

fn sorted_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn is_canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.hyphenated().to_string() == value)
        && value == value.to_ascii_lowercase()
}

fn is_canonical_output_name(value: &std::ffi::OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    let Some((attempt, extension)) = value.split_once('.') else {
        return false;
    };
    !attempt.starts_with('0')
        && attempt.parse::<u16>().is_ok_and(|number| number > 0)
        && matches!(extension, "partial" | "log")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileKind {
    Missing,
    Regular,
    Unsafe,
}

fn file_kind(path: &Path) -> Result<FileKind> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(FileKind::Unsafe),
        Ok(metadata) if metadata.is_file() => Ok(FileKind::Regular),
        Ok(_) => Ok(FileKind::Unsafe),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileKind::Missing),
        Err(error) => Err(error.into()),
    }
}

fn is_safe_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(!metadata.file_type().is_symlink() && metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn require_directory(path: &Path) -> Result<()> {
    if is_safe_directory(path)? {
        Ok(())
    } else {
        bail!(
            "managed path is missing, symbolic, or not a directory: {}",
            path.display()
        )
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use locron_store::{
        AttemptCompletion, CreateJob, FrameChannel, FrameReader, FrameWriter, StartDecision,
    };

    use super::*;

    fn open_store() -> (tempfile::TempDir, StatePaths, Store) {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Store::open(paths.clone(), "test", 1).unwrap();
        (temp, paths, store)
    }

    fn admit_one(store: &Store, run_id: &str) {
        let job_id = uuid::Uuid::from_u128(1).to_string();
        store
            .create_job(&CreateJob {
                id: job_id,
                name: "maintenance".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: "{}".into(),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
        store.enqueue_manual("maintenance", run_id, 2).unwrap();
        let lifetime = uuid::Uuid::from_u128(2).to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        assert_eq!(store.admit(&lifetime, 3, 1).unwrap().attempts.len(), 1);
    }

    #[test]
    fn repairs_and_finalizes_a_referenced_partial() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(3).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"hello").unwrap();
        writer.sync().unwrap();
        drop(writer);
        fs::OpenOptions::new()
            .append(true)
            .open(&partial)
            .unwrap()
            .write_all(b"incomplete")
            .unwrap();

        let report = maintain(&store, &paths, 10).unwrap();

        assert_eq!(report.outputs_recovered, 1);
        assert!(!partial.exists());
        let final_path = paths.final_output(&run_id, 1).unwrap();
        let mut reader = FrameReader::open(&final_path).unwrap();
        assert_eq!(reader.next_frame().unwrap().unwrap().payload, b"hello");
        assert!(reader.next_frame().unwrap().is_none());
        assert!(store.referenced_partial_artifacts(1).unwrap().is_empty());
    }

    #[test]
    fn reconciles_an_already_renamed_final_and_a_missing_artifact() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(6).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let final_path = paths.final_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&final_path).unwrap();
        writer.write(FrameChannel::Stderr, 1, b"renamed").unwrap();
        writer.sync().unwrap();
        drop(writer);

        let report = maintain(&store, &paths, 10).unwrap();

        assert_eq!(report.outputs_recovered, 1);
        assert!(final_path.is_file());

        let (_missing_temp, missing_paths, missing_store) = open_store();
        let missing_run = uuid::Uuid::from_u128(7).to_string();
        admit_one(&missing_store, &missing_run);
        let report = maintain(&missing_store, &missing_paths, 10).unwrap();
        assert_eq!(report.outputs_missing, 1);
        assert!(
            missing_store
                .referenced_partial_artifacts(1)
                .unwrap()
                .is_empty()
        );
    }

    fn terminalize(store: &Store, run_id: &str, now_us: i64) {
        assert_eq!(
            store.mark_attempt_running(run_id, 1, now_us).unwrap(),
            StartDecision::Ready
        );
        store
            .complete_attempt(&AttemptCompletion {
                run_id: run_id.into(),
                attempt_number: 1,
                now_us: now_us + 1,
                duration_us: 1,
                state: "succeeded".into(),
                exit_code: Some(0),
                http_status: None,
                reason: "test".into(),
                retry: None,
            })
            .unwrap();
    }

    #[test]
    fn global_output_limit_prunes_terminal_output() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(8).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        maintain(&store, &paths, 10).unwrap();
        terminalize(&store, &run_id, 11);
        store.set_setting("output_limit_bytes", "0", 13).unwrap();

        let report = maintain(&store, &paths, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert!(!paths.final_output(&run_id, 1).unwrap().exists());
        assert_eq!(store.retained_output_bytes().unwrap(), 0);
    }

    #[test]
    fn resumes_pending_output_prune_before_deleting_run_metadata() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(9).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Body, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        maintain(&store, &paths, 10).unwrap();
        terminalize(&store, &run_id, 11);
        let output = store.output_retention_candidates(1).unwrap().remove(0);
        store.mark_output_prune_pending(&output, 13).unwrap();
        store.set_setting("run_retention_count", "0", 13).unwrap();

        let report = maintain(&store, &paths, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert_eq!(report.runs_pruned, 1);
        assert!(matches!(store.run(&run_id), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn completes_pending_prune_when_the_output_directory_is_already_missing() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(10).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        maintain(&store, &paths, 10).unwrap();
        terminalize(&store, &run_id, 11);
        let output = store.output_retention_candidates(1).unwrap().remove(0);
        store.mark_output_prune_pending(&output, 13).unwrap();
        fs::remove_dir_all(&directory).unwrap();

        let report = maintain(&store, &paths, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert!(store.pending_output_prunes(1).unwrap().is_empty());
    }

    #[test]
    fn orphan_cleanup_is_bounded_and_ignores_unexpected_objects() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(4).to_string();
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        for attempt in 1..=101 {
            fs::write(directory.join(format!("{attempt}.log")), b"old").unwrap();
        }
        fs::create_dir(directory.join("unexpected")).unwrap();

        let report = maintain(&store, &paths, i64::MAX / 2).unwrap();

        assert_eq!(report.actions, MAX_ACTIONS);
        assert_eq!(report.orphans_removed, MAX_ACTIONS);
        assert_eq!(
            fs::read_dir(&directory)
                .unwrap()
                .filter_map(std::result::Result::ok)
                .filter(|entry| entry.file_type().unwrap().is_file())
                .count(),
            1
        );
        assert!(directory.join("unexpected").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn orphan_cleanup_never_follows_or_removes_symlinks() {
        use std::os::unix::fs::symlink;

        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(5).to_string();
        let directory = paths.output_directory(&run_id).unwrap();
        fs::create_dir(&directory).unwrap();
        let target = paths.root.join("target");
        fs::write(&target, b"keep").unwrap();
        let link = directory.join("1.log");
        symlink(&target, &link).unwrap();

        let report = maintain(&store, &paths, i64::MAX / 2).unwrap();

        assert_eq!(report.orphans_removed, 0);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }
}
