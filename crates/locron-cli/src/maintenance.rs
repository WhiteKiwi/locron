//! Bounded filesystem and retention maintenance for the CLI composition adapter.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use locron_store::{
    OutputRecord, RetentionCandidate, StatePaths, Store, StoreError, repair_partial,
};

const MAX_ACTIONS: usize = 100;
const MAX_ELAPSED: Duration = Duration::from_millis(250);
const OUTPUT_MAX_AGE_US: i64 = 30 * 24 * 60 * 60 * 1_000_000;
const ORPHAN_GRACE_US: i64 = 60 * 60 * 1_000_000;

#[cfg(test)]
type ScanHook = Box<dyn FnMut(&Path)>;
#[cfg(test)]
thread_local! {
    static SCAN_ENTRY: std::cell::RefCell<Option<ScanHook>> = const { std::cell::RefCell::new(None) };
    static BEFORE_PRUNE_IO: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

/// Bounded in-memory continuation of one automatic orphan sweep.
/// Each iterator retains its admitted parent until it closes, including between passes.
#[derive(Default)]
pub struct MaintenanceCursor {
    outputs: Option<PathBuf>,
    directories: Option<DirectoryScan>,
    current: Option<RunScan>,
    output_guard: Option<locron_core::filesystem::DirectoryGuard>,
}

struct DirectoryScan {
    entries: fs::ReadDir,
}

struct RunScan {
    directory: PathBuf,
    run_id: String,
    entries: fs::ReadDir,
    pending: Option<fs::DirEntry>,
    _guard: locron_core::filesystem::DirectoryGuard,
}

impl MaintenanceCursor {
    fn reset(&mut self) {
        self.current = None;
        self.directories = None;
        self.output_guard = None;
        self.outputs = None;
    }
}

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

struct Pass<'a> {
    store: &'a Store,
    deadline: Instant,
    deferred: bool,
    report: MaintenanceReport,
    errors: Vec<String>,
}

fn is_deferred(error: &anyhow::Error) -> bool {
    error.downcast_ref::<StoreError>().is_some_and(|error| {
        matches!(error, StoreError::MaintenanceDeferred)
            || matches!(error.original_error(), StoreError::Sqlite(error) if error.sqlite_error().is_some_and(|code| code.extended_code & 0xff == 5))
    })
}

impl<'a> Pass<'a> {
    fn new(store: &'a Store, deadline: Instant) -> Self {
        Self {
            store,
            deadline,
            deferred: false,
            report: MaintenanceReport::default(),
            errors: Vec::new(),
        }
    }

    fn remaining(&self) -> usize {
        if self.deferred || Instant::now() >= self.deadline {
            0
        } else {
            MAX_ACTIONS.saturating_sub(self.report.actions)
        }
    }

    fn ready(&self) -> Result<bool> {
        Ok(self.remaining() > 0 && self.unit_ready()?)
    }

    fn unit_ready(&self) -> Result<bool> {
        Ok(!self.deferred
            && Instant::now() < self.deadline
            && !self.store.maintenance_should_yield()?)
    }

    fn take_action(&mut self) -> bool {
        match self.ready() {
            Ok(true) => {
                self.report.actions += 1;
                true
            }
            Ok(false) => {
                self.deferred = true;
                false
            }
            Err(error) => {
                self.record("check maintenance admission", error);
                false
            }
        }
    }

    fn record(&mut self, context: &str, error: impl Into<anyhow::Error>) {
        let error = error.into();
        if is_deferred(&error) {
            self.deferred = true;
        } else {
            self.errors.push(format!("{context}: {error}"));
        }
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

/// Runs one maintenance pass with 100 actions and a 250 ms soft elapsed budget.
///
/// `lifetime_id` is the live daemon's scheduler lifetime. Output recovery only
/// considers terminal attempts or attempts owned by another lifetime, so a
/// just-admitted attempt whose partial file does not exist yet is never
/// reconciled as missing while the live daemon owns it.
#[cfg(test)]
pub fn maintain(
    store: &Store,
    paths: &StatePaths,
    lifetime_id: &str,
    now_us: i64,
) -> Result<MaintenanceReport> {
    maintain_with_cursor(
        store,
        paths,
        lifetime_id,
        now_us,
        &mut MaintenanceCursor::default(),
    )
}

/// Runs one bounded pass, preserving at most two guarded scan iterators for later passes.
pub fn maintain_with_cursor(
    store: &Store,
    paths: &StatePaths,
    lifetime_id: &str,
    now_us: i64,
    cursor: &mut MaintenanceCursor,
) -> Result<MaintenanceReport> {
    if store.paths() != paths {
        bail!("maintenance store and state paths do not match");
    }
    if cursor
        .outputs
        .as_ref()
        .is_some_and(|outputs| outputs != &paths.outputs)
    {
        cursor.reset();
    }
    let deadline = Instant::now() + MAX_ELAPSED;
    let result = store.with_background_budget(deadline, || {
        maintain_inner(store, paths, lifetime_id, now_us, deadline, cursor)
    });
    if result.is_err() {
        cursor.reset();
    }
    result
}

fn maintain_inner(
    store: &Store,
    paths: &StatePaths,
    lifetime_id: &str,
    now_us: i64,
    deadline: Instant,
    cursor: &mut MaintenanceCursor,
) -> Result<MaintenanceReport> {
    if cursor.output_guard.is_none() {
        cursor.output_guard = Some(
            locron_core::filesystem::DirectoryGuard::existing_private(&paths.outputs)
                .context("validate managed output root")?,
        );
        cursor.outputs = Some(paths.outputs.clone());
    }
    #[cfg(not(windows))]
    require_directory(&paths.outputs).context("validate managed output root")?;

    let mut pass = Pass::new(store, deadline);
    macro_rules! phase {
        ($action:expr) => {
            if let Err(error) = $action {
                if is_deferred(&error) {
                    return pass.finish();
                }
                return Err(error);
            }
        };
    }
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(resume_output_prunes(store, paths, now_us, &mut pass));
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(recover_referenced_outputs(
        store,
        paths,
        lifetime_id,
        now_us,
        &mut pass
    ));

    if !pass.ready()? {
        return pass.finish();
    }
    let pending_runs = match store
        .pending_run_retention(MAX_ACTIONS)
        .context("list pending run retention")
    {
        Ok(candidates) => candidates,
        Err(error) if is_deferred(&error) => return pass.finish(),
        Err(error) => return Err(error),
    };
    let pending_ids = pending_runs
        .iter()
        .map(|candidate| candidate.run_id.clone())
        .collect::<BTreeSet<_>>();
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(prune_pending_run_outputs(
        store,
        paths,
        now_us,
        &pending_ids,
        &mut pass
    ));
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(prune_output_limits(store, paths, now_us, &mut pass));
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
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(select_run_retention(store, now_us, &mut pass));
    if !pass.ready()? {
        return pass.finish();
    }
    phase!(remove_verified_orphans(
        store, paths, now_us, &mut pass, cursor
    ));
    pass.finish()
}

fn resume_output_prunes(
    store: &Store,
    paths: &StatePaths,
    now_us: i64,
    pass: &mut Pass<'_>,
) -> Result<()> {
    let candidates = store
        .pending_output_prunes(pass.remaining())
        .context("list pending output prunes")?;
    for candidate in candidates {
        if !pass.take_action() {
            break;
        }
        match remove_and_finish_output(store, paths, &candidate, now_us, false, pass) {
            Ok(()) => pass.report.outputs_pruned += 1,
            Err(error) => pass.record("resume output prune", error),
        }
    }
    Ok(())
}

fn recover_referenced_outputs(
    store: &Store,
    paths: &StatePaths,
    lifetime_id: &str,
    now_us: i64,
    pass: &mut Pass<'_>,
) -> Result<()> {
    let candidates = store
        .referenced_partial_artifacts(pass.remaining(), lifetime_id)
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
            pass,
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
    pass: &Pass<'_>,
) -> Result<bool> {
    if !pass.unit_ready()? {
        return Err(StoreError::MaintenanceDeferred.into());
    }
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
    #[cfg(windows)]
    let _directory_guard =
        match locron_core::filesystem::DirectoryGuard::existing_private(directory) {
            Ok(guard) => guard,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                store.reconcile_output_missing(run_id, attempt_number, now_us)?;
                return Ok(false);
            }
            Err(error) => return Err(error).context("validate managed output directory"),
        };
    #[cfg(not(windows))]
    if !is_safe_directory(directory)? {
        store.reconcile_output_missing(run_id, attempt_number, now_us)?;
        return Ok(false);
    }

    let partial_kind = file_kind(&partial)?;
    let final_kind = file_kind(&final_path)?;
    #[cfg(windows)]
    if partial_kind == FileKind::Unsafe || final_kind == FileKind::Unsafe {
        bail!("refusing to recover unsafe output objects");
    }
    let repaired = match (partial_kind, final_kind) {
        (FileKind::Regular, FileKind::Missing) => {
            if !pass.unit_ready()? {
                return Err(StoreError::MaintenanceDeferred.into());
            }
            let repair = repair_partial(&partial).context("repair partial frame tail")?;
            if !pass.unit_ready()? {
                return Err(StoreError::MaintenanceDeferred.into());
            }
            #[cfg(windows)]
            locron_core::filesystem::rename_private(&partial, &final_path)
                .context("atomically finalize repaired output")?;
            #[cfg(not(windows))]
            fs::rename(&partial, &final_path).context("atomically finalize repaired output")?;
            #[cfg(unix)]
            sync_directory(directory)?;
            repair
        }
        (FileKind::Missing | FileKind::Unsafe, FileKind::Regular) => {
            if !pass.unit_ready()? {
                return Err(StoreError::MaintenanceDeferred.into());
            }
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
    pass: &mut Pass<'_>,
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
        match remove_and_finish_output(store, paths, &candidate, now_us, true, pass) {
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
    pass: &mut Pass<'_>,
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
        match remove_and_finish_output(store, paths, &candidate, now_us, true, pass) {
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
    pass: &Pass<'_>,
) -> Result<()> {
    let attempt = u16::try_from(candidate.attempt_number)
        .context("retention attempt number is outside path range")?;
    let path = paths.final_output(&candidate.run_id, attempt)?;
    let expected_relative = format!("{}/{attempt}.log", candidate.run_id);
    if candidate.relative_path != expected_relative {
        bail!("database output path is not the canonical final path");
    }
    if mark_pending {
        store.mark_output_prune_pending(candidate, now_us)?;
    }
    #[cfg(test)]
    BEFORE_PRUNE_IO.with(|hook| {
        if let Some(action) = hook.borrow_mut().take() {
            action();
        }
    });
    if !pass.unit_ready()? {
        return Err(StoreError::MaintenanceDeferred.into());
    }
    #[cfg(windows)]
    locron_core::filesystem::remove_private_file(&path).context("remove retained output")?;
    #[cfg(not(windows))]
    {
        let directory = path
            .parent()
            .ok_or_else(|| anyhow!("output path has no parent"))?;
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
                #[cfg(unix)]
                sync_directory(directory)?;
            }
            FileKind::Missing => {}
            FileKind::Unsafe => bail!("refusing to remove symbolic link or non-file output"),
        }
    }
    store.finish_output_prune(candidate, now_us)?;
    Ok(())
}

fn select_run_retention(store: &Store, now_us: i64, pass: &mut Pass<'_>) -> Result<()> {
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
    pass: &mut Pass<'_>,
    cursor: &mut MaintenanceCursor,
) -> Result<()> {
    if !pass.ready()? || now_us < ORPHAN_GRACE_US {
        return Ok(());
    }
    let cutoff = UNIX_EPOCH
        .checked_add(Duration::from_micros(
            u64::try_from(now_us - ORPHAN_GRACE_US).unwrap_or(0),
        ))
        .ok_or_else(|| anyhow!("orphan cutoff is outside system time range"))?;
    if cursor.directories.is_none() {
        cursor.directories = Some(DirectoryScan {
            entries: fs::read_dir(&paths.outputs)?,
        });
        cursor.outputs = Some(paths.outputs.clone());
    }
    loop {
        if !pass.ready()? {
            break;
        }
        if cursor.current.is_none() {
            let next = cursor
                .directories
                .as_mut()
                .and_then(|scan| scan.entries.next());
            let Some(directory) = next else {
                cursor.current = None;
                cursor.directories = None;
                break;
            };
            let directory = directory?;
            let directory_name = directory.file_name();
            let Some(run_id) = directory_name
                .to_str()
                .filter(|name| is_canonical_uuid(name))
            else {
                continue;
            };
            let directory_path = directory.path();
            #[cfg(not(windows))]
            if !is_safe_directory(&directory_path)? {
                continue;
            }
            let guard =
                match locron_core::filesystem::DirectoryGuard::existing_private(&directory_path) {
                    Ok(guard) => guard,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error).context("validate orphan output directory"),
                };
            cursor.current = Some(RunScan {
                entries: fs::read_dir(&directory_path)?,
                directory: directory_path,
                run_id: run_id.to_owned(),
                pending: None,
                _guard: guard,
            });
        }
        if !pass.ready()? {
            break;
        }
        let current = cursor
            .current
            .as_mut()
            .ok_or_else(|| anyhow!("orphan cursor has no current directory"))?;
        #[cfg(not(windows))]
        if !is_safe_directory(&current.directory)? {
            cursor.current = None;
            continue;
        }
        let next = current
            .pending
            .take()
            .map(Ok)
            .or_else(|| current.entries.next());
        let Some(entry) = next else {
            cursor.current = None;
            continue;
        };
        let entry = entry?;
        let path = entry.path();
        if !is_canonical_output_name(&entry.file_name()) || file_kind(&path)? != FileKind::Regular {
            continue;
        }
        let relative_path = format!("{}/{}", current.run_id, entry.file_name().to_string_lossy());
        match store.output_artifact_references(&current.run_id, &relative_path) {
            Ok(true) => {
                #[cfg(test)]
                SCAN_ENTRY.with(|hook| {
                    if let Some(hook) = hook.borrow_mut().as_mut() {
                        hook(&path);
                    }
                });
                continue;
            }
            Ok(false) => {}
            Err(error) => {
                pass.record("verify output artifact reference", error);
                current.pending = Some(entry);
                break;
            }
        }
        #[cfg(windows)]
        let metadata =
            locron_core::filesystem::open_private(&path, fs::OpenOptions::new().read(true))?
                .metadata()?;
        #[cfg(not(windows))]
        let metadata = fs::symlink_metadata(&path)?;
        if metadata
            .modified()
            .context("read orphan modification time")?
            > cutoff
        {
            continue;
        }
        if !pass.take_action() {
            current.pending = Some(entry);
            break;
        }
        #[cfg(windows)]
        let removal = locron_core::filesystem::remove_private_file(&path);
        #[cfg(not(windows))]
        let removal = fs::remove_file(&path);
        match removal {
            Ok(()) => {
                pass.report.orphans_removed += 1;
                #[cfg(unix)]
                if let Err(error) = sync_directory(&current.directory) {
                    pass.record("sync orphan output directory", error);
                }
            }
            Err(error) => pass.record("remove verified orphan output", error),
        }
    }
    Ok(())
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

#[cfg(windows)]
fn file_kind(path: &Path) -> Result<FileKind> {
    match locron_core::filesystem::open_private(path, fs::OpenOptions::new().read(true)) {
        Ok(_) => Ok(FileKind::Regular),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileKind::Missing),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(FileKind::Unsafe),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(windows))]
fn file_kind(path: &Path) -> Result<FileKind> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(FileKind::Unsafe),
        Ok(metadata) if metadata.is_file() => Ok(FileKind::Regular),
        Ok(_) => Ok(FileKind::Unsafe),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileKind::Missing),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(windows))]
fn is_safe_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(!metadata.file_type().is_symlink() && metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(windows))]
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

#[cfg(unix)]
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
        let paths = StatePaths::new(temp.path().join("private"));
        let store = Store::open(paths.clone(), "test", 1).unwrap();
        (temp, paths, store)
    }

    fn write_private(path: &Path, bytes: &[u8]) {
        let mut file = locron_core::filesystem::create_private_new(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn orphan_pass_time() -> i64 {
        i64::try_from(UNIX_EPOCH.elapsed().unwrap().as_micros()).unwrap() + 2 * 60 * 60 * 1_000_000
    }

    fn admit_one(store: &Store, run_id: &str) -> String {
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
        lifetime
    }

    /// Begins a fresh scheduler lifetime over an existing store, exactly as a
    /// daemon restart does: any stale attempts from earlier lifetimes become
    /// terminal before the new lifetime's startup maintenance runs.
    fn restart_lifetime(store: &Store, now_us: i64) -> String {
        let restarted = uuid::Uuid::from_u128(21).to_string();
        store.begin_lifetime(&restarted, now_us, "test").unwrap();
        restarted
    }

    #[test]
    fn repairs_and_finalizes_a_referenced_partial() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(3).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"hello").unwrap();
        writer.sync().unwrap();
        drop(writer);
        locron_core::filesystem::open_private(&partial, fs::OpenOptions::new().append(true))
            .unwrap()
            .write_all(b"incomplete")
            .unwrap();

        // The daemon owning this attempt dies; the restarted daemon's
        // maintenance recovers the partial.
        let restarted = restart_lifetime(&store, 9);
        let report = maintain(&store, &paths, &restarted, 10).unwrap();

        assert_eq!(report.outputs_recovered, 1);
        assert!(!partial.exists());
        let final_path = paths.final_output(&run_id, 1).unwrap();
        let mut reader = FrameReader::open(&final_path).unwrap();
        assert_eq!(reader.next_frame().unwrap().unwrap().payload, b"hello");
        assert!(reader.next_frame().unwrap().is_none());
        assert!(
            store
                .referenced_partial_artifacts(1, &restarted)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reconciles_an_already_renamed_final_and_a_missing_artifact() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(6).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let final_path = paths.final_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&final_path).unwrap();
        writer.write(FrameChannel::Stderr, 1, b"renamed").unwrap();
        writer.sync().unwrap();
        drop(writer);

        let restarted = restart_lifetime(&store, 9);
        let report = maintain(&store, &paths, &restarted, 10).unwrap();

        assert_eq!(report.outputs_recovered, 1);
        assert!(final_path.is_file());

        let (_missing_temp, missing_paths, missing_store) = open_store();
        let missing_run = uuid::Uuid::from_u128(7).to_string();
        admit_one(&missing_store, &missing_run);
        let missing_restarted = restart_lifetime(&missing_store, 9);
        let report = maintain(&missing_store, &missing_paths, &missing_restarted, 10).unwrap();
        assert_eq!(report.outputs_missing, 1);
        assert!(
            missing_store
                .referenced_partial_artifacts(1, &missing_restarted)
                .unwrap()
                .is_empty()
        );
        assert!(
            !missing_paths
                .output_directory(&missing_run)
                .unwrap()
                .exists()
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
                http_content_type: None,
                reason: "test".into(),
                retry: None,
            })
            .unwrap();
    }

    #[test]
    fn global_output_limit_prunes_terminal_output() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(8).to_string();
        let lifetime = admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        // While the attempt is still starting under the live lifetime,
        // maintenance must not recover (and thereby reconcile) its output.
        maintain(&store, &paths, &lifetime, 10).unwrap();
        terminalize(&store, &run_id, 11);
        store.set_setting("output_limit_bytes", "0", 13).unwrap();

        // The attempt is terminal now, so the pass recovers the partial and
        // then prunes it under the zero-byte limit.
        let report = maintain(&store, &paths, &lifetime, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert!(!paths.final_output(&run_id, 1).unwrap().exists());
        assert_eq!(store.retained_output_bytes().unwrap(), 0);
    }

    fn finalized_fixture() -> (
        tempfile::TempDir,
        StatePaths,
        Store,
        String,
        RetentionCandidate,
    ) {
        let (temporary, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(180).to_string();
        let lifetime = admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"retained").unwrap();
        writer.sync().unwrap();
        drop(writer);
        terminalize(&store, &run_id, 11);
        maintain(&store, &paths, &lifetime, 12).unwrap();
        let candidate = store.output_retention_candidates(1).unwrap().remove(0);
        (temporary, paths, store, lifetime, candidate)
    }

    #[test]
    fn foreground_after_prune_intent_preserves_file_and_next_pass_resumes() {
        let (_temporary, paths, store, lifetime, candidate) = finalized_fixture();
        store.set_setting("output_limit_bytes", "0", 13).unwrap();
        let intent_path = paths.root.join("writer-intent.lock");
        let held = std::rc::Rc::new(std::cell::RefCell::new(None));
        let captured = held.clone();
        BEFORE_PRUNE_IO.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let intent = locron_core::filesystem::open_private_or_create(&intent_path).unwrap();
                intent.try_lock_shared().unwrap();
                *captured.borrow_mut() = Some(intent);
            }))
        });
        let report = maintain(&store, &paths, &lifetime, 14).unwrap();
        assert_eq!(report.outputs_pruned, 0);
        assert!(paths.outputs.join(&candidate.relative_path).is_file());
        assert_eq!(store.pending_output_prunes(10).unwrap().len(), 1);
        drop(held.borrow_mut().take());
        let report = maintain(&store, &paths, &lifetime, 15).unwrap();
        assert_eq!(report.outputs_pruned, 1);
        assert!(!paths.outputs.join(&candidate.relative_path).exists());
        assert!(store.pending_output_prunes(10).unwrap().is_empty());
    }

    #[test]
    fn hundredth_reserved_prune_unit_executes_without_starting_a_hundred_and_first() {
        let (_temporary, paths, store, _lifetime, candidate) = finalized_fixture();
        // Isolate the action dimension from the separately tested elapsed dimension.
        let deadline = Instant::now() + Duration::from_secs(5);
        store.with_background_budget(deadline, || {
            let mut pass = Pass::new(&store, deadline);
            pass.report.actions = MAX_ACTIONS - 1;
            assert!(pass.take_action());
            remove_and_finish_output(&store, &paths, &candidate, 13, true, &pass).unwrap();
            assert_eq!(pass.report.actions, MAX_ACTIONS);
            assert!(!pass.take_action());
        });
        assert!(!paths.outputs.join(&candidate.relative_path).exists());
        assert!(store.pending_output_prunes(10).unwrap().is_empty());
    }

    #[test]
    fn expired_pass_preserves_pending_work_and_a_later_pass_completes_it() {
        let (_temporary, paths, store, lifetime, candidate) = finalized_fixture();
        store.mark_output_prune_pending(&candidate, 13).unwrap();
        let deadline = Instant::now();
        let started = Instant::now();
        let report = store
            .with_background_budget(deadline, || {
                maintain_inner(
                    &store,
                    &paths,
                    &lifetime,
                    14,
                    deadline,
                    &mut MaintenanceCursor::default(),
                )
            })
            .unwrap();
        assert_eq!(report.actions, 0);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(paths.outputs.join(&candidate.relative_path).is_file());
        assert_eq!(store.pending_output_prunes(10).unwrap().len(), 1);
        assert_eq!(
            maintain(&store, &paths, &lifetime, 15)
                .unwrap()
                .outputs_pruned,
            1
        );
    }

    #[test]
    fn retained_orphan_cursor_crosses_slow_referenced_prefix_between_passes() {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                SCAN_ENTRY.with(|hook| hook.borrow_mut().take());
            }
        }
        let (_temporary, paths, store, lifetime, candidate) = finalized_fixture();
        let run_id = candidate.run_id.clone();
        let directory = paths.output_directory(&run_id).unwrap();
        for number in 2..=10 {
            write_private(
                &directory.join(format!("{number}.log")),
                b"retained or orphan",
            );
        }
        let entries = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let orphans = entries
            .iter()
            .rev()
            .filter(|path| path.file_name().unwrap() != "1.log")
            .take(2)
            .cloned()
            .collect::<BTreeSet<_>>();
        let referenced = entries
            .iter()
            .filter(|path| !orphans.contains(*path))
            .cloned()
            .collect::<Vec<_>>();
        let now_us = orphan_pass_time();
        store
            .set_setting("run_retention_age_us", "none", now_us)
            .unwrap();
        let external = rusqlite::Connection::open(&paths.database).unwrap();
        for path in &referenced {
            let number: i64 = path.file_stem().unwrap().to_str().unwrap().parse().unwrap();
            if number == 1 {
                continue;
            }
            external.execute("INSERT INTO attempts(run_id,attempt_number,lifetime_id,state,started_at_us,finished_at_us) VALUES(?1,?2,?3,'succeeded',?4,?4)", rusqlite::params![run_id,number,lifetime,now_us]).unwrap();
            external.execute("INSERT INTO output_artifacts(run_id,attempt_number,relative_path,state,finalized_at_us) VALUES(?1,?2,?3,'finalized',?4)", rusqlite::params![run_id,number,format!("{run_id}/{number}.log"),now_us]).unwrap();
        }
        external
            .execute("UPDATE output_artifacts SET finalized_at_us=?1", [now_us])
            .unwrap();
        drop(external);
        let visited = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = visited.clone();
        SCAN_ENTRY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |path| {
                captured.borrow_mut().push(path.to_path_buf());
                // One completed read consumes this pass; restarting enumeration would
                // repeatedly inspect its first referenced entry and never reach the tail.
                std::thread::sleep(MAX_ELAPSED + Duration::from_millis(5));
            }))
        });
        let _reset = Reset;
        let mut cursor = MaintenanceCursor {
            output_guard: Some(
                locron_core::filesystem::DirectoryGuard::existing_private(&paths.outputs).unwrap(),
            ),
            outputs: Some(paths.outputs.clone()),
            ..MaintenanceCursor::default()
        };
        let mut removed = 0;
        let mut passes = 0;
        while orphans.iter().any(|path| path.exists()) {
            passes += 1;
            assert!(passes <= entries.len() + 2, "orphan scan did not resume");
            let deadline = Instant::now() + MAX_ELAPSED;
            let report = store.with_background_budget(deadline, || {
                let mut pass = Pass::new(&store, deadline);
                remove_verified_orphans(&store, &paths, now_us, &mut pass, &mut cursor).unwrap();
                pass.finish().unwrap()
            });
            removed += report.orphans_removed;
            assert!(report.actions <= MAX_ACTIONS);
        }
        assert!(passes > 1);
        assert_eq!(removed, orphans.len());
        let visited = visited.borrow();
        assert_eq!(visited.len(), visited.iter().collect::<BTreeSet<_>>().len());
        assert!(referenced.iter().all(|path| path.is_file()));
        assert_eq!(
            store
                .output_retention_candidates(MAX_ACTIONS)
                .unwrap()
                .len(),
            referenced.len()
        );
    }

    #[test]
    fn resumes_pending_output_prune_before_deleting_run_metadata() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(9).to_string();
        let lifetime = admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Body, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        terminalize(&store, &run_id, 11);
        // The terminal attempt's output is now a recovery candidate.
        maintain(&store, &paths, &lifetime, 12).unwrap();
        let output = store.output_retention_candidates(1).unwrap().remove(0);
        store.mark_output_prune_pending(&output, 13).unwrap();
        store.set_setting("run_retention_count", "0", 13).unwrap();

        let report = maintain(&store, &paths, &lifetime, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert_eq!(report.runs_pruned, 1);
        assert!(matches!(store.run(&run_id), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn completes_pending_prune_when_the_output_directory_is_already_missing() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(10).to_string();
        let lifetime = admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"payload").unwrap();
        writer.sync().unwrap();
        drop(writer);
        terminalize(&store, &run_id, 11);
        maintain(&store, &paths, &lifetime, 12).unwrap();
        let output = store.output_retention_candidates(1).unwrap().remove(0);
        store.mark_output_prune_pending(&output, 13).unwrap();
        fs::remove_dir_all(&directory).unwrap();

        let report = maintain(&store, &paths, &lifetime, 14).unwrap();

        assert_eq!(report.outputs_pruned, 1);
        assert!(store.pending_output_prunes(1).unwrap().is_empty());
        assert!(!directory.exists());
    }

    #[test]
    fn orphan_cleanup_is_bounded_and_ignores_unexpected_objects() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(4).to_string();
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        for attempt in 1..=101 {
            write_private(&directory.join(format!("{attempt}.log")), b"old");
        }
        fs::create_dir(directory.join("unexpected")).unwrap();

        let report = maintain(&store, &paths, "no-lifetime", orphan_pass_time()).unwrap();

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

    #[test]
    fn orphan_cleanup_removes_unreferenced_canonical_file_inside_existing_run() {
        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(11).to_string();
        admit_one(&store, &run_id);
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let partial = paths.partial_output(&run_id, 1).unwrap();
        let mut writer = FrameWriter::create(&partial).unwrap();
        writer
            .write(FrameChannel::Stdout, 1, b"referenced")
            .unwrap();
        writer.sync().unwrap();
        drop(writer);
        let orphan = directory.join("2.log");
        write_private(&orphan, b"unreferenced");

        // The owning daemon dies; the restarted daemon recovers the
        // referenced partial and removes the unreferenced 2.log.
        let restarted = restart_lifetime(&store, 9);
        let report = maintain(&store, &paths, &restarted, orphan_pass_time()).unwrap();

        assert_eq!(report.outputs_recovered, 1);
        assert_eq!(report.orphans_removed, 1);
        assert!(paths.final_output(&run_id, 1).unwrap().is_file());
        assert!(!orphan.exists());
    }

    #[cfg(unix)]
    #[test]
    fn orphan_cleanup_never_follows_or_removes_symlinks() {
        use std::os::unix::fs::symlink;

        let (_temp, paths, store) = open_store();
        let run_id = uuid::Uuid::from_u128(5).to_string();
        let directory = paths.output_directory(&run_id).unwrap();
        locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
        let target = paths.root.join("target");
        fs::write(&target, b"keep").unwrap();
        let link = directory.join("1.log");
        symlink(&target, &link).unwrap();

        let report = maintain(&store, &paths, "no-lifetime", orphan_pass_time()).unwrap();

        assert_eq!(report.orphans_removed, 0);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }

    #[cfg(windows)]
    mod windows_contracts {
        use std::path::PathBuf;
        use std::sync::{Arc, mpsc};
        use std::time::{Duration, Instant};

        use super::*;

        fn partial_fixture(
            run_id: &str,
        ) -> (tempfile::TempDir, StatePaths, Store, String, PathBuf) {
            let (temporary, paths, store) = open_store();
            admit_one(&store, run_id);
            let partial = paths.partial_output(run_id, 1).unwrap();
            let mut writer = FrameWriter::create(&partial).unwrap();
            writer
                .write(FrameChannel::Stdout, 1, b"recover me")
                .unwrap();
            writer.sync().unwrap();
            drop(writer);
            let lifetime = restart_lifetime(&store, 9);
            (temporary, paths, store, lifetime, partial)
        }

        fn add_world_read(path: &Path) {
            locron_core::windows::run_script_json(
                r"$acl=[System.IO.File]::GetAccessControl([string]$request.path);
                $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'),'Read','Allow'));
                [System.IO.File]::SetAccessControl([string]$request.path,$acl);
                @{changed=$true} | ConvertTo-Json -Compress",
                &serde_json::json!({"path":path}),
            )
            .unwrap();
        }

        fn junction(link: &Path, target: &Path) {
            locron_core::windows::run_script_json(
                "New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null; @{created=$true} | ConvertTo-Json -Compress",
                &serde_json::json!({"link":link,"target":target}),
            )
            .unwrap();
        }

        #[test]
        fn missing_output_root_is_not_recreated_by_maintenance() {
            let (_temporary, paths, store) = open_store();
            fs::remove_dir(&paths.outputs).unwrap();

            assert!(maintain(&store, &paths, "no-lifetime", 10).is_err());
            assert!(!paths.outputs.exists());
            assert!(maintain(&store, &paths, "no-lifetime", 11).is_err());
            assert!(!paths.outputs.exists());
        }

        #[test]
        fn recovery_waits_for_an_owned_reader_and_preserves_frames() {
            let run_id = uuid::Uuid::from_u128(22).to_string();
            let (_temporary, paths, store, lifetime, partial) = partial_fixture(&run_id);
            let final_path = paths.final_output(&run_id, 1).unwrap();
            let mut reader = FrameReader::open(&partial).unwrap();
            assert_eq!(reader.next_frame().unwrap().unwrap().payload, b"recover me");
            assert!(matches!(
                fs::rename(&partial, &final_path)
                    .unwrap_err()
                    .raw_os_error(),
                Some(32 | 33)
            ));
            let store = Arc::new(store);
            let maintenance_store = store.clone();
            let maintenance_paths = paths.clone();
            let maintenance_lifetime = lifetime.clone();
            let (sender, receiver) = mpsc::channel();
            let recovery = std::thread::spawn(move || {
                sender
                    .send(maintain(
                        &maintenance_store,
                        &maintenance_paths,
                        &maintenance_lifetime,
                        10,
                    ))
                    .unwrap();
            });
            assert!(matches!(
                receiver.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            drop(reader);
            let report = receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            recovery.join().unwrap();

            assert_eq!(report.outputs_recovered, 1);
            assert!(!partial.exists());
            assert!(
                store
                    .referenced_partial_artifacts(10, &lifetime)
                    .unwrap()
                    .is_empty()
            );
            let mut finalized = FrameReader::open(&final_path).unwrap();
            assert_eq!(
                finalized.next_frame().unwrap().unwrap().payload,
                b"recover me"
            );
            assert!(finalized.next_frame().unwrap().is_none());
        }

        #[test]
        fn reader_sharing_timeout_keeps_recovery_pending_until_a_later_pass() {
            let run_id = uuid::Uuid::from_u128(23).to_string();
            let (_temporary, paths, store, lifetime, partial) = partial_fixture(&run_id);
            let final_path = paths.final_output(&run_id, 1).unwrap();
            let reader = FrameReader::open(&partial).unwrap();
            assert!(matches!(
                fs::rename(&partial, &final_path)
                    .unwrap_err()
                    .raw_os_error(),
                Some(32 | 33)
            ));
            let started = Instant::now();
            assert!(maintain(&store, &paths, &lifetime, 10).is_err());
            let elapsed = started.elapsed();
            assert!(
                elapsed >= Duration::from_millis(4500),
                "elapsed={elapsed:?}"
            );
            assert!(elapsed < Duration::from_secs(8), "elapsed={elapsed:?}");
            assert!(partial.is_file());
            assert!(!final_path.exists());
            assert_eq!(
                store
                    .referenced_partial_artifacts(10, &lifetime)
                    .unwrap()
                    .len(),
                1
            );
            drop(reader);

            let report = maintain(&store, &paths, &lifetime, 11).unwrap();

            assert_eq!(report.outputs_recovered, 1);
            assert!(!partial.exists());
            let mut finalized = FrameReader::open(&final_path).unwrap();
            assert_eq!(
                finalized.next_frame().unwrap().unwrap().payload,
                b"recover me"
            );
            assert!(finalized.next_frame().unwrap().is_none());
        }

        // Descriptor and junction setup uses the one generic adapter sequentially;
        // the other cases exercise actual filesystem sharing without that adapter.
        #[test]
        fn unsafe_objects_preserve_recovery_prunes_and_unrelated_targets() {
            for unsafe_parent in [false, true] {
                let run_id = uuid::Uuid::from_u128(24).to_string();
                let (_temporary, paths, store, lifetime, partial) = partial_fixture(&run_id);
                let original = fs::read(&partial).unwrap();
                let directory = partial.parent().unwrap();
                let unsafe_path = if unsafe_parent { directory } else { &partial };
                add_world_read(unsafe_path);
                assert!(!locron_core::filesystem::is_private(unsafe_path, unsafe_parent).unwrap());

                assert!(maintain(&store, &paths, &lifetime, 10).is_err());

                assert_eq!(fs::read(&partial).unwrap(), original);
                assert!(!paths.final_output(&run_id, 1).unwrap().exists());
                assert_eq!(
                    store
                        .referenced_partial_artifacts(10, &lifetime)
                        .unwrap()
                        .len(),
                    1
                );
                assert!(!locron_core::filesystem::is_private(unsafe_path, unsafe_parent).unwrap());
            }

            let run_id = uuid::Uuid::from_u128(25).to_string();
            let (_temporary, paths, store, lifetime, _partial) = partial_fixture(&run_id);
            maintain(&store, &paths, &lifetime, 10).unwrap();
            let final_path = paths.final_output(&run_id, 1).unwrap();
            let original = fs::read(&final_path).unwrap();
            let output = store.output_retention_candidates(1).unwrap().remove(0);
            store.mark_output_prune_pending(&output, 11).unwrap();
            add_world_read(&final_path);

            assert!(maintain(&store, &paths, &lifetime, 12).is_err());

            assert_eq!(fs::read(&final_path).unwrap(), original);
            assert_eq!(store.pending_output_prunes(1).unwrap().len(), 1);
            drop(store);
            let store = Store::open(paths.clone(), "test", 13).unwrap();
            assert_eq!(store.pending_output_prunes(1).unwrap().len(), 1);
            drop(store);

            let (temporary, paths, store) = open_store();
            let target = temporary.path().join("outside private target");
            locron_core::filesystem::DirectoryGuard::private(&target).unwrap();
            let target_file = target.join("1.log");
            write_private(&target_file, b"keep target");
            let directory_link = paths
                .output_directory(&uuid::Uuid::from_u128(26).to_string())
                .unwrap();
            junction(&directory_link, &target);
            let directory = paths
                .output_directory(&uuid::Uuid::from_u128(27).to_string())
                .unwrap();
            locron_core::filesystem::DirectoryGuard::private(&directory).unwrap();
            let leaf_link = directory.join("1.log");
            junction(&leaf_link, &target);
            let broad_leaf = directory.join("2.log");
            write_private(&broad_leaf, b"keep broad leaf");
            add_world_read(&broad_leaf);

            let result = maintain(&store, &paths, "no-lifetime", orphan_pass_time());
            // Remove only these exact fixture junctions before temporary-tree cleanup.
            fs::remove_dir(&leaf_link).unwrap();
            fs::remove_dir(&directory_link).unwrap();
            let report = result.unwrap();

            assert_eq!(report.orphans_removed, 0);
            assert_eq!(fs::read(target_file).unwrap(), b"keep target");
            assert_eq!(fs::read(broad_leaf).unwrap(), b"keep broad leaf");
        }
    }

    mod output_repair_qualification {
        use std::io::Read as _;
        use std::time::Instant;

        use super::*;

        const SEED: [u8; 39] = [
            0x4c, 0x4f, 0x43, 0x52, 0x4f, 0x4e, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
            0x00, 0xcc, 0x96, 0x40, 0x6d, 0x62, 0x61, 0x73, 0x65, 0x00, 0xff,
        ];
        const KEYS: [&str; 6] = [
            "C-LIVE-STARTING",
            "C-LIVE-RUNNING",
            "C-TERMINAL",
            "C-STALE-STARTING",
            "C-STALE-RUNNING",
            "C-REFUSAL",
        ];

        #[derive(Clone, Copy)]
        struct Horizon(Instant);

        impl Horizon {
            fn new() -> Self {
                Self(Instant::now() + Duration::from_secs(30))
            }

            fn check(self, key: &str, phase: &str) {
                assert!(
                    Instant::now() < self.0,
                    "output-qualification key={key} phase={phase} deadline"
                );
            }

            fn require(self, key: &str, phase: &str, condition: bool) {
                self.check(key, phase);
                assert!(condition, "output-qualification key={key} phase={phase}");
            }

            fn result<T, E>(
                self,
                key: &str,
                phase: &str,
                operation: impl FnOnce() -> std::result::Result<T, E>,
            ) -> std::result::Result<T, E> {
                self.check(key, phase);
                let result = operation();
                self.check(key, phase);
                result
            }

            fn need<T, E>(
                self,
                key: &str,
                phase: &str,
                operation: impl FnOnce() -> std::result::Result<T, E>,
            ) -> T {
                self.result(key, phase, operation).unwrap_or_else(|_| {
                    panic!("output-qualification key={key} phase={phase} refused")
                })
            }
        }

        fn exact_bytes(h: Horizon, key: &str, path: &Path, expected: &[u8]) {
            h.require(key, "oracle-cap", expected.len() <= 64);
            let mut file = h.need(key, "oracle-open", || {
                locron_core::filesystem::open_private(path, fs::OpenOptions::new().read(true))
            });
            let mut bytes = Vec::new();
            h.need(key, "oracle-read", || {
                (&mut *file).take(65).read_to_end(&mut bytes)
            });
            h.require(key, "oracle-bytes", bytes == expected);
            drop(file);
            h.check(key, "oracle-released");
        }

        fn absent(h: Horizon, key: &str, path: &Path) {
            let result = h.result(key, "absent-oracle", || fs::symlink_metadata(path));
            let Err(error) = result else {
                panic!("output-qualification key={key} phase=unexpected-object");
            };
            h.require(
                key,
                "actual-notfound",
                error.kind() == std::io::ErrorKind::NotFound,
            );
        }

        #[derive(Clone, Eq, PartialEq)]
        struct OutputFacts {
            state: String,
            retained_payload_bytes: i64,
            physical_bytes: i64,
            discarded_bytes: i64,
            truncated: bool,
            truncated_at_us: Option<i64>,
            finalized_at_us: Option<i64>,
            prune_started_at_us: Option<i64>,
            pruned_at_us: Option<i64>,
        }

        struct AttemptFacts {
            state: String,
            error: Option<String>,
            output: Option<OutputFacts>,
            durable: serde_json::Value,
        }

        fn records(
            h: Horizon,
            key: &str,
            store: &Store,
            run_id: &str,
        ) -> (locron_store::RunRecord, AttemptFacts) {
            let run = h.need(key, "read-run", || store.run(run_id));
            let mut attempts = h.need(key, "read-attempts", || store.attempts_for_run(run_id));
            h.require(
                key,
                "one-actual-attempt",
                attempts.len() == 1 && attempts[0].attempt_number == 1,
            );
            // The Store's returned attempt type is usable by inference, but is not re-exported.
            let mut attempt = attempts.remove(0);
            let output = attempt.output.take().map(|output| OutputFacts {
                state: output.state,
                retained_payload_bytes: output.retained_payload_bytes,
                physical_bytes: output.physical_bytes,
                discarded_bytes: output.discarded_bytes,
                truncated: output.truncated,
                truncated_at_us: output.truncated_at_us,
                finalized_at_us: output.finalized_at_us,
                prune_started_at_us: output.prune_started_at_us,
                pruned_at_us: output.pruned_at_us,
            });
            let state = attempt.state.clone();
            let error = attempt.error.clone();
            let durable = h.need(key, "saved-attempt-facts", || {
                serde_json::to_value(&attempt)
            });
            (
                run,
                AttemptFacts {
                    state,
                    error,
                    output,
                    durable,
                },
            )
        }

        fn durable_facts(
            h: Horizon,
            key: &str,
            run: &locron_store::RunRecord,
            attempt: &AttemptFacts,
        ) -> serde_json::Value {
            // Output is checked separately because only its finalization facts may change.
            h.need(key, "saved-durable-facts", || {
                serde_json::to_value((run, &attempt.durable))
            })
        }

        fn row(h: Horizon, key: &str) {
            h.check(key, "row-start");
            let temporary = h.need(key, "temporary", tempfile::tempdir);
            let paths = StatePaths::new(temporary.path().join("private"));
            let store = h.need(key, "store-open", || Store::open(paths.clone(), "test", 1));
            let job_id = uuid::Uuid::from_u128(1).to_string();
            let run_id = uuid::Uuid::from_u128(3).to_string();
            let first_lifetime = uuid::Uuid::from_u128(2).to_string();
            h.need(key, "create-job", || {
                store.create_job(&CreateJob {
                    id: job_id,
                    name: "repair qualification".into(),
                    description: None,
                    tags_json: "[]".into(),
                    enabled: true,
                    definition_json: "{}".into(),
                    now_us: 1,
                    cursor_us: 1,
                })
            });
            h.need(key, "enqueue", || {
                store.enqueue_manual("repair qualification", &run_id, 2)
            });
            h.need(key, "first-lifetime", || {
                store.begin_lifetime(&first_lifetime, 3, "test")
            });
            let admission = h.need(key, "first-admission", || {
                store.admit(&first_lifetime, 3, 1)
            });
            h.require(
                key,
                "one-admission",
                admission.attempts.len() == 1 && admission.attempts[0].run_id == run_id,
            );

            let partial = h.need(key, "partial-path", || paths.partial_output(&run_id, 1));
            let final_path = h.need(key, "final-path", || paths.final_output(&run_id, 1));
            let mut expected = SEED.to_vec();
            expected.extend_from_slice(&[1, 1, 0]);
            if key == "C-LIVE-STARTING" {
                absent(h, key, &partial);
            } else {
                if key == "C-REFUSAL" {
                    expected = b"LOCRON\0\x02".to_vec();
                    let mut writer = h.need(key, "create-bad-magic", || {
                        locron_core::filesystem::create_private_new(&partial)
                    });
                    h.need(key, "bad-magic-write", || writer.write_all(&expected));
                    h.need(key, "bad-magic-sync", || writer.sync_all());
                    drop(writer);
                } else {
                    let mut writer = h.need(key, "frame-create", || FrameWriter::create(&partial));
                    h.need(key, "frame-write", || {
                        writer.write(FrameChannel::Stdout, 1, &[0x62, 0x61, 0x73, 0x65, 0, 0xff])
                    });
                    let physical = h.need(key, "frame-sync", || writer.sync());
                    h.require(key, "seed-length", physical == 39);
                    drop(writer);
                    h.check(key, "writer-released");
                    let mut tail = h.need(key, "tail-open", || {
                        locron_core::filesystem::open_private(
                            &partial,
                            fs::OpenOptions::new().append(true),
                        )
                    });
                    h.need(key, "tail-write", || tail.write_all(&[1, 1, 0]));
                    h.need(key, "tail-sync", || tail.sync_all());
                    drop(tail);
                }
                h.check(key, "all-writers-released");
                exact_bytes(h, key, &partial, &expected);
            }

            if matches!(
                key,
                "C-LIVE-RUNNING" | "C-STALE-RUNNING" | "C-TERMINAL" | "C-REFUSAL"
            ) {
                let decision = h.need(key, "mark-running", || {
                    store.mark_attempt_running(&run_id, 1, 4)
                });
                h.require(key, "actual-ready", decision == StartDecision::Ready);
            }
            if matches!(key, "C-TERMINAL" | "C-REFUSAL") {
                h.need(key, "terminal-completion", || {
                    store.complete_attempt(&AttemptCompletion {
                        run_id: run_id.clone(),
                        attempt_number: 1,
                        now_us: 5,
                        duration_us: 1,
                        state: "succeeded".into(),
                        exit_code: Some(0),
                        http_status: None,
                        http_content_type: None,
                        reason: "test".into(),
                        retry: None,
                    })
                });
            }
            let lifetime = if matches!(key, "C-STALE-STARTING" | "C-STALE-RUNNING") {
                let second_lifetime = uuid::Uuid::from_u128(21).to_string();
                let recovered = h.need(key, "actual-restart", || {
                    store.begin_lifetime(&second_lifetime, 9, "test")
                });
                h.require(key, "one-stale-attempt", recovered == 1);
                second_lifetime
            } else {
                first_lifetime
            };
            let (before_run, before_attempt) = records(h, key, &store, &run_id);
            let state = match key {
                "C-LIVE-STARTING" => "starting",
                "C-LIVE-RUNNING" => "running",
                "C-STALE-STARTING" | "C-STALE-RUNNING" => "interrupted_unknown",
                _ => "succeeded",
            };
            h.require(
                key,
                "actual-durable-state",
                before_run.state == state && before_attempt.state == state,
            );
            if matches!(key, "C-STALE-STARTING" | "C-STALE-RUNNING") {
                h.require(
                    key,
                    "unknown-classification",
                    before_run.reason.as_deref()
                        == Some("scheduler lifetime ended without a durable result")
                        && before_attempt.error.as_deref()
                            == Some("scheduler lifetime ended without a durable result"),
                );
            }
            let before_facts = durable_facts(h, key, &before_run, &before_attempt);
            let before_output = before_attempt.output.clone();
            let selected = h.need(key, "actual-sql-candidates", || {
                store.referenced_partial_artifacts(2, &lifetime)
            });
            let live = matches!(key, "C-LIVE-STARTING" | "C-LIVE-RUNNING");
            h.require(
                key,
                "sql-eligibility",
                if live {
                    selected.is_empty()
                } else {
                    selected.len() == 1
                        && selected[0].run_id == run_id
                        && selected[0].attempt_number == 1
                        && matches!(selected[0].state.as_str(), "pending" | "active")
                },
            );

            let result = h.result(key, "actual-maintenance", || {
                maintain(&store, &paths, &lifetime, 10)
            });
            let refused = key == "C-REFUSAL";
            if refused {
                h.require(key, "maintenance-refused", result.is_err());
            } else {
                let report = result.unwrap_or_else(|_| {
                    panic!("output-qualification key={key} phase=maintenance-refused")
                });
                let wanted = if live {
                    MaintenanceReport::default()
                } else {
                    MaintenanceReport {
                        actions: 1,
                        outputs_recovered: 1,
                        ..MaintenanceReport::default()
                    }
                };
                h.require(key, "maintenance-counts", report == wanted);
            }
            let (after_run, after_attempt) = records(h, key, &store, &run_id);
            h.require(
                key,
                "durable-facts-conserved",
                durable_facts(h, key, &after_run, &after_attempt) == before_facts,
            );
            if live || refused {
                h.require(
                    key,
                    "artifact-unmodified",
                    before_output == after_attempt.output,
                );
                h.require(
                    key,
                    "partial-reference-retained",
                    h.need(key, "artifact-reference", || {
                        store.output_artifact_references(&run_id, &format!("{run_id}/1.partial"))
                    }),
                );
                h.require(
                    key,
                    "no-finalized-success",
                    h.need(key, "finalized-artifacts", || {
                        store.output_retention_candidates(2)
                    })
                    .is_empty(),
                );
                absent(h, key, &final_path);
                if key == "C-LIVE-STARTING" {
                    absent(h, key, &partial);
                } else {
                    exact_bytes(h, key, &partial, &expected);
                }
            } else {
                let Some(output) = &after_attempt.output else {
                    panic!("output-qualification key={key} phase=missing-output-row");
                };
                h.require(
                    key,
                    "exact-finalized-facts",
                    output.state == "finalized"
                        && output.retained_payload_bytes == 6
                        && output.physical_bytes == 39
                        && output.discarded_bytes == 0
                        && !output.truncated
                        && output.truncated_at_us.is_none()
                        && output.prune_started_at_us.is_none()
                        && output.pruned_at_us.is_none()
                        && output.finalized_at_us == Some(10),
                );
                h.require(
                    key,
                    "finalized-sql-count",
                    h.need(key, "retained-physical", || store.retained_output_bytes()) == 39
                        && h.need(key, "retained-payload", || {
                            store.retained_run_output_bytes(&run_id)
                        }) == 6,
                );
                h.require(
                    key,
                    "no-recovery-candidates",
                    h.need(key, "post-sql-candidates", || {
                        store.referenced_partial_artifacts(2, &lifetime)
                    })
                    .is_empty(),
                );
                absent(h, key, &partial);
                exact_bytes(h, key, &final_path, &SEED);
                let mut reader = h.need(key, "public-reader", || FrameReader::open(&final_path));
                let frame = h.need(key, "public-frame", || reader.next_frame());
                h.require(
                    key,
                    "exact-public-frame",
                    frame.is_some_and(|frame| {
                        frame.channel == FrameChannel::Stdout
                            && frame.sequence == 0
                            && frame.elapsed_us == 1
                            && frame.payload == [0x62, 0x61, 0x73, 0x65, 0, 0xff]
                    }),
                );
                h.require(
                    key,
                    "public-end",
                    h.need(key, "public-end", || reader.next_frame()).is_none(),
                );
                drop(reader);
                h.check(key, "public-reader-released");
            }
            if matches!(key, "C-STALE-STARTING" | "C-STALE-RUNNING") {
                h.require(
                    key,
                    "no-retry-due",
                    h.need(key, "pending-eligibility", || {
                        store.earliest_pending_eligible_at_us()
                    })
                    .is_none(),
                );
                let next = h.need(key, "actual-no-retry-admission", || {
                    store.admit(&lifetime, 11, 1)
                });
                h.require(key, "no-retry-admitted", next.attempts.is_empty());
                let (run, attempt) = records(h, key, &store, &run_id);
                h.require(
                    key,
                    "no-new-outcome",
                    durable_facts(h, key, &run, &attempt) == before_facts,
                );
            }
            drop(store);
            h.check(key, "store-released");
            h.need(key, "explicit-cleanup", || temporary.close());
        }

        #[test]
        fn caller_contracts() {
            let h = Horizon::new();
            let mut completed = BTreeSet::new();
            for key in KEYS {
                row(h, key);
                h.require(key, "unique-completion", completed.insert(key));
                println!("output-qualification PASS {key}");
            }
            h.require("caller:group", "row-count", completed.len() == 6);
        }
    }
}
