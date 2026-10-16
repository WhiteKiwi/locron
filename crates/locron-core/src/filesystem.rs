//! Shared private-file and no-reparse access primitives.
//!
//! A successful check is tied to retained handles. Keep the returned guard alive for
//! the complete path-based operation, including SQLite sidecars and file finalization.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod private_directory;
#[cfg(windows)]
pub use private_directory::PrivateDirectoryPlan;

// These macros preserve the original release/non-Windows error construction.
macro_rules! observed_unsafe_path {
    ($path:expr, $object:ident, $predicate:ident, $ace:expr) => {{
        #[cfg(all(windows, debug_assertions))]
        {
            $crate::filesystem::unsafe_path_observed(
                $path,
                $crate::filesystem::RefusalObject::$object,
                $crate::filesystem::RefusalPredicate::$predicate,
                $ace,
            )
        }
        #[cfg(not(all(windows, debug_assertions)))]
        {
            $crate::filesystem::unsafe_path($path)
        }
    }};
}

#[cfg(windows)]
macro_rules! ancestor_refusal {
    ($result:expr, $index:expr) => {{
        #[cfg(debug_assertions)]
        {
            $result.map_err(|error| {
                $crate::filesystem::refusal_context(
                    error,
                    $crate::filesystem::RefusalObject::Ancestor,
                    u32::try_from($index).ok(),
                )
            })
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = $index;
            $result
        }
    }};
}

#[cfg(windows)]
macro_rules! executable_refusal {
    ($result:expr) => {{
        #[cfg(debug_assertions)]
        {
            $result.map_err(|error| {
                $crate::filesystem::refusal_context(
                    error,
                    $crate::filesystem::RefusalObject::Executable,
                    None,
                )
            })
        }
        #[cfg(not(debug_assertions))]
        {
            $result
        }
    }};
}

#[cfg(all(windows, debug_assertions))]
#[derive(Clone, Copy, Debug)]
enum RefusalObject {
    Ancestor,
    Executable,
    Leaf,
}

#[cfg(all(windows, debug_assertions))]
#[derive(Clone, Copy, Debug)]
enum RefusalPredicate {
    Reparse,
    NotDirectory,
    OwnerMissing,
    OwnerUntrusted,
    DaclMissing,
    AceMissing,
    AceShape,
    PrincipalMissing,
    ForeignMutation,
    NotFile,
    OwnedOwnerMismatch,
}

/// Closed observation of an existing custom Windows guard refusal, with no IO authority.
#[cfg(all(windows, debug_assertions))]
#[derive(Clone, Copy, Debug)]
pub struct GuardRefusalObservation {
    object: RefusalObject,
    predicate: RefusalPredicate,
    chain_index: Option<u32>,
    ace_index: Option<u32>,
}

#[cfg(all(windows, debug_assertions))]
impl GuardRefusalObservation {
    /// Object class of the original rejecting guard.
    #[must_use]
    pub const fn object(self) -> &'static str {
        match self.object {
            RefusalObject::Ancestor => "ancestor",
            RefusalObject::Executable => "executable",
            RefusalObject::Leaf => "leaf",
        }
    }

    /// Original failed predicate, without private descriptor or identity values.
    #[must_use]
    pub const fn predicate(self) -> &'static str {
        match self.predicate {
            RefusalPredicate::Reparse => "reparse",
            RefusalPredicate::NotDirectory => "not_directory",
            RefusalPredicate::OwnerMissing => "owner_missing",
            RefusalPredicate::OwnerUntrusted => "owner_untrusted",
            RefusalPredicate::DaclMissing => "dacl_missing",
            RefusalPredicate::AceMissing => "ace_missing",
            RefusalPredicate::AceShape => "ace_shape",
            RefusalPredicate::PrincipalMissing => "principal_missing",
            RefusalPredicate::ForeignMutation => "foreign_mutation",
            RefusalPredicate::NotFile => "not_file",
            RefusalPredicate::OwnedOwnerMismatch => "owned_owner_mismatch",
        }
    }

    /// Checked index of the original root-first traversal, when available.
    #[must_use]
    pub const fn chain_index(self) -> Option<u32> {
        self.chain_index
    }

    /// Checked index of the original descriptor ACE, when available.
    #[must_use]
    pub const fn ace_index(self) -> Option<u32> {
        self.ace_index
    }
}

#[cfg(all(windows, debug_assertions))]
struct GuardRefusalError {
    message: String,
    observation: GuardRefusalObservation,
}

#[cfg(all(windows, debug_assertions))]
impl std::fmt::Display for GuardRefusalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.message, formatter)
    }
}

#[cfg(all(windows, debug_assertions))]
impl std::fmt::Debug for GuardRefusalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.message, formatter)
    }
}

#[cfg(all(windows, debug_assertions))]
impl std::error::Error for GuardRefusalError {}

#[cfg(all(windows, debug_assertions))]
fn unsafe_path_observed(
    path: &Path,
    object: RefusalObject,
    predicate: RefusalPredicate,
    ace_index: Option<u32>,
) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        GuardRefusalError {
            message: format!("unsafe managed path: {}", path.display()),
            observation: GuardRefusalObservation {
                object,
                predicate,
                chain_index: None,
                ace_index,
            },
        },
    )
}

#[cfg(all(windows, debug_assertions))]
fn refusal_context(
    mut error: io::Error,
    object: RefusalObject,
    chain_index: Option<u32>,
) -> io::Error {
    if let Some(payload) = error
        .get_mut()
        .and_then(|inner| inner.downcast_mut::<GuardRefusalError>())
    {
        payload.observation.object = object;
        payload.observation.chain_index = chain_index;
    }
    // Native errors are returned unchanged, never wrapped in a custom IO error.
    error
}

/// Reads only a private, error-bound closed tag; performs no native observation.
#[cfg(all(windows, debug_assertions))]
#[must_use]
pub fn guard_refusal_observation(error: &io::Error) -> Option<GuardRefusalObservation> {
    error
        .get_ref()?
        .downcast_ref::<GuardRefusalError>()
        .map(|payload| payload.observation)
}

/// Live directory-chain handles and the validated absolute directory identity.
#[derive(Debug)]
pub struct DirectoryGuard {
    path: PathBuf,
    _handles: Vec<File>,
}

impl DirectoryGuard {
    /// Creates missing private components, then guards and verifies the directory.
    pub fn private(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            windows::guard_directory(path, true, true)
        }
        #[cfg(not(windows))]
        {
            reject_symlink(path)?;
            fs::create_dir_all(path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
            Self::ancestors(path)
        }
    }

    /// Verifies and guards an existing private directory without creating or repairing it.
    /// Maintenance must use this entry point before accepting a saved state identity.
    pub fn existing_private(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            windows::guard_directory(path, true, false)
        }
        #[cfg(not(windows))]
        {
            let guard = Self::ancestors(path)?;
            if !is_private(path, true)? {
                return Err(unsafe_path(path));
            }
            Ok(guard)
        }
    }

    /// Guards an existing directory chain without requiring a private leaf.
    pub fn ancestors(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            windows::guard_directory(path, false, false)
        }
        #[cfg(not(windows))]
        {
            reject_symlink(path)?;
            if !path.is_dir() {
                return Err(unsafe_path(path));
            }
            Ok(Self {
                path: fs::canonicalize(path)?,
                _handles: vec![File::open(path)?],
            })
        }
    }

    /// Returns the canonical identity obtained while the validated chain is live.
    #[must_use]
    pub fn normalized_path(&self) -> &Path {
        &self.path
    }
}

/// Native directory data access participates in read/write/delete sharing accounting.
/// Descriptor-only opens do not establish this retained-object boundary.
#[cfg(windows)]
pub(crate) fn open_directory_guard_handle(path: &Path) -> io::Result<File> {
    windows::directory_handle(path, false)
}

/// A file whose parent-chain guards live as long as the file.
#[derive(Debug)]
pub struct GuardedFile {
    file: File,
    guard: DirectoryGuard,
    path: PathBuf,
}

impl GuardedFile {
    /// Returns the file path under its retained directory and no-reparse guards.
    #[must_use]
    pub fn normalized_path(&self) -> &Path {
        &self.path
    }

    /// Splits the file from its guard for an async file or path-based adapter.
    /// The caller must retain the guard through that adapter's complete lifetime.
    #[must_use]
    pub fn into_parts(self) -> (File, DirectoryGuard) {
        (self.file, self.guard)
    }
}

impl Deref for GuardedFile {
    type Target = File;
    fn deref(&self) -> &File {
        &self.file
    }
}
impl DerefMut for GuardedFile {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.file
    }
}

/// Opens an existing managed data file after private parent and no-follow checks.
/// Creation flags are cleared; use [`create_private_new`] for a missing file.
pub fn open_private(path: &Path, options: &mut OpenOptions) -> io::Result<GuardedFile> {
    #[cfg(windows)]
    let guard = DirectoryGuard::existing_private(parent(path)?)?;
    #[cfg(not(windows))]
    let guard = DirectoryGuard::private(parent(path)?)?;
    options.create(false).create_new(false);
    open_with_guard(path, options, guard, true)
}

/// Retains an existing private read handle that excludes concurrent write/delete access.
/// Sharing violations are explicit; this never repairs or creates a managed object.
#[cfg(windows)]
pub fn open_private_read_stable(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::existing_private(parent(path)?)?;
    windows::read_private_stable(path, guard)
}

/// Atomically creates an empty private file without replacing any existing object.
/// The returned read/write handle and parent guard are validated before caller data is written.
pub fn create_private_new(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::private(parent(path)?)?;
    #[cfg(windows)]
    {
        let path = guard
            .normalized_path()
            .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
        crate::windows::create_private_file(&path)?;
        open_with_guard(
            &path,
            OpenOptions::new().read(true).write(true),
            guard,
            true,
        )
    }
    #[cfg(not(windows))]
    {
        open_with_guard(
            path,
            OpenOptions::new().read(true).write(true).create_new(true),
            guard,
            true,
        )
    }
}

/// Opens or atomically creates a permanent private read/write file, preserving existing bytes.
/// A creation race reopens and validates the existing object instead of repairing it.
pub fn open_private_or_create(path: &Path) -> io::Result<GuardedFile> {
    match open_private(path, OpenOptions::new().read(true).write(true)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => match create_private_new(path) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                open_private(path, OpenOptions::new().read(true).write(true))
            }
            result => result,
        },
        result => result,
    }
}

/// Opens a user-selected input without traversing a reparse point or symlink.
/// Inputs need not carry a managed state-file descriptor.
pub fn open_read_no_follow(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::ancestors(parent(path)?)?;
    open_with_guard(path, OpenOptions::new().read(true), guard, false)
}

/// Reads a current-user-owned Windows executable with no untrusted write/control grants.
/// SYSTEM/Administrators may retain write access and other accounts may retain read/execute.
/// Retained no-write/no-delete-sharing handles protect the source through hashing and copying.
#[cfg(windows)]
pub fn read_owned_executable(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::ancestors(parent(path)?)?;
    windows::read_owned_executable(path, guard)
}

/// Exclusively guards an existing package executable to prove mapped holders are gone.
/// Permits trusted SYSTEM/Administrators rights and other accounts' read/execute rights.
/// This proof gate never creates, repairs or writes package bytes; callers release it
/// before handing mutation to the package manager. Standalone replacement stays private.
#[cfg(windows)]
pub fn open_owned_executable_exclusive(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::ancestors(parent(path)?)?;
    windows::owned_executable_exclusive(path, guard)
}

/// Opens an existing private replacement leaf without sharing read, write or delete access.
/// The existing-only parent, protected descriptor and single-link object remain guarded.
/// This gate refuses mapped images; it never truncates or repairs an existing object.
#[cfg(windows)]
pub fn open_private_exclusive(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::existing_private(parent(path)?)?;
    windows::exclusive_file(path, guard, false)
}

/// Creates a new exclusive private replacement leaf and initializes its exact empty handle.
/// No caller bytes are written before explicit owner/protected-DACL readback succeeds.
/// Initialization failure leaves an unrecognized leaf for explicit refusal/recovery.
#[cfg(windows)]
pub fn create_private_new_exclusive(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::existing_private(parent(path)?)?;
    windows::exclusive_file(path, guard, true)
}

/// The complete Windows filesystem object identity, including the full 128-bit file ID.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    /// Filesystem volume serial number.
    pub volume_serial_number: u64,
    /// Full file identity, without a low-resolution fallback.
    pub file_id: u128,
}

/// Queries full file identity while the no-delete leaf and directory guards remain live.
#[cfg(windows)]
pub fn file_identity(file: &GuardedFile) -> io::Result<FileIdentity> {
    let file_id::FileId::HighRes {
        volume_serial_number,
        file_id,
    } = file_id::get_high_res_file_id(file.normalized_path())?
    else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "filesystem cannot provide full executable identity",
        ));
    };
    Ok(FileIdentity {
        volume_serial_number,
        file_id,
    })
}

/// Compares complete filesystem object identities rather than path spelling.
#[cfg(windows)]
pub fn same_file(left: &GuardedFile, right: &GuardedFile) -> io::Result<bool> {
    Ok(file_identity(left)? == file_identity(right)?)
}

fn open_with_guard(
    path: &Path,
    options: &mut OpenOptions,
    guard: DirectoryGuard,
    private: bool,
) -> io::Result<GuardedFile> {
    #[cfg(windows)]
    let guarded_path = guard
        .normalized_path()
        .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
    #[cfg(windows)]
    let path = guarded_path.as_path();
    #[cfg(windows)]
    windows::file_options(options);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    reject_symlink(path)?;
    #[cfg(windows)]
    let _inspection = windows::inspect_existing_file(path, private)?;
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(observed_unsafe_path!(path, Leaf, NotFile, None));
    }
    #[cfg(windows)]
    {
        windows::reject_reparse(&file, path)?;
        if private {
            windows::verify_private(&file, path, false)?;
        }
    }
    #[cfg(not(windows))]
    let _ = private;
    let path = guard
        .normalized_path()
        .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
    Ok(GuardedFile { file, guard, path })
}

/// Renames a private managed file while both directory chains remain guarded.
/// Windows sharing violations are retried for at most five seconds, allowing
/// concurrent short-lived output snapshots to release their no-delete handles.
pub fn rename_private(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        rename_private_bounded(source, destination, std::time::Duration::from_secs(5))
    }
    #[cfg(not(windows))]
    {
        rename_private_once(source, destination)
    }
}

/// Renames an existing Windows private file within the caller's absolute budget.
/// Only sharing violations are retried; absent parents are never created.
/// A native operation finishing after expiry is uncertain, even if it succeeded.
/// Uncancellable calls must remain owned by the caller's quarantine worker.
#[cfg(windows)]
pub fn rename_private_until(
    source: &Path,
    destination: &Path,
    deadline: std::time::Instant,
) -> io::Result<()> {
    loop {
        ensure_rename_deadline(deadline)?;
        match rename_private_attempt_until(source, destination, deadline) {
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                ensure_rename_deadline(deadline)?;
                std::thread::sleep(
                    deadline
                        .saturating_duration_since(std::time::Instant::now())
                        .min(std::time::Duration::from_millis(25)),
                );
            }
            result => return result,
        }
    }
}

#[cfg(windows)]
fn ensure_rename_deadline(deadline: std::time::Instant) -> io::Result<()> {
    if std::time::Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "private rename deadline elapsed; an admitted native operation may have completed",
        ))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn rename_operation_until<T>(
    deadline: std::time::Instant,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    ensure_rename_deadline(deadline)?;
    let result = operation();
    ensure_rename_deadline(deadline)?;
    result
}

#[cfg(windows)]
fn rename_private_attempt_until(
    source: &Path,
    destination: &Path,
    deadline: std::time::Instant,
) -> io::Result<()> {
    let source_file = rename_operation_until(deadline, || {
        open_private(source, OpenOptions::new().read(true))
    })?;
    let guarded_source = source_file.normalized_path().to_owned();
    let destination_guard = rename_operation_until(deadline, || {
        DirectoryGuard::existing_private(parent(destination)?)
    })?;
    let guarded_destination = destination_guard.normalized_path().join(
        destination
            .file_name()
            .ok_or_else(|| unsafe_path(destination))?,
    );
    match rename_operation_until(deadline, || {
        open_private(&guarded_destination, OpenOptions::new().read(true))
    }) {
        Ok(file) => drop(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let (source_file, source_guard) = source_file.into_parts();
    // Keep both directory chains, releasing only the leaf that would deny rename itself.
    drop(source_file);
    let result = rename_operation_until(deadline, || {
        fs::rename(&guarded_source, &guarded_destination)
    });
    drop((source_guard, destination_guard));
    result
}

#[cfg(windows)]
fn rename_private_bounded(
    source: &Path,
    destination: &Path,
    timeout: std::time::Duration,
) -> io::Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match rename_private_once(source, destination) {
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Err(error);
                }
                std::thread::sleep(remaining.min(std::time::Duration::from_millis(25)));
            }
            result => return result,
        }
    }
}

fn rename_private_once(source: &Path, destination: &Path) -> io::Result<()> {
    let source_file = open_private(source, OpenOptions::new().read(true))?;
    let (source_file, source_guard) = source_file.into_parts();
    drop(source_file);
    let destination_guard = DirectoryGuard::private(parent(destination)?)?;
    // An existing destination must itself be safe before a replacement is attempted.
    match open_private(destination, OpenOptions::new().read(true)) {
        Ok(file) => drop(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination)?;
    drop((source_guard, destination_guard));
    Ok(())
}

/// Removes a verified private managed file; a missing file is harmless.
pub fn remove_private_file(path: &Path) -> io::Result<()> {
    let file = match open_private(path, OpenOptions::new().read(true)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let (file, guard) = file.into_parts();
    drop(file);
    fs::remove_file(path)?;
    drop(guard);
    Ok(())
}

/// Removes an existing private Windows file with the native readonly/sharing policy.
/// The admitted parent remains owned through actual native return and cleanup.
/// Absence succeeds; this never clears attributes, repairs permissions or retries.
#[cfg(windows)]
pub fn remove_private_file_strict(path: &Path) -> io::Result<()> {
    let deadline = crate::windows::filesystem_deadline();
    let file = match private_operation_until(deadline, || {
        open_private(path, OpenOptions::new().read(true))
    }) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let path = file.normalized_path().to_owned();
    let (file, guard) = file.into_parts();
    drop(file);
    ensure_private_deadline(deadline)?;
    crate::windows::remove_private_file_strict(&path, guard, deadline)
}

#[cfg(windows)]
fn ensure_private_deadline(deadline: std::time::Instant) -> io::Result<()> {
    if std::time::Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "guarded filesystem deadline elapsed; native completion may be uncertain",
        ))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn private_operation_until<T>(
    deadline: std::time::Instant,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    ensure_private_deadline(deadline)?;
    let result = operation();
    ensure_private_deadline(deadline)?;
    result
}

/// Initializes and moves a caller-created, still-empty disposable Windows directory.
///
/// The caller must hold a genuine exclusive creation receipt for `basename` beneath
/// the private source parent, immediately disable recursive cleanup, and provide no
/// caller bytes or children. This is not authority to adopt or repair an existing or
/// already-refused root. The helper owns every source-leaf guard and performs only
/// DACL initialization; actual TokenUser ownership is required before mutation.
///
/// One native no-replace move is followed by private/full-identity destination
/// admission while the original physical object remains alive. Unknown completion
/// retains one process-wide owner and refuses further moves. There is no replay,
/// rollback or path-based deletion. Synchronous OS calls cannot be preempted; late
/// results fail under the same absolute deadline.
#[cfg(windows)]
pub fn move_empty_private_directory(
    source_parent: std::sync::Arc<DirectoryGuard>,
    destination_parent: std::sync::Arc<DirectoryGuard>,
    basename: &std::ffi::OsStr,
    deadline: std::time::Instant,
) -> io::Result<DirectoryGuard> {
    let lease = MoveLease::acquire(&MOVE_STATE)?;
    let owner =
        prepare_empty_directory_move(source_parent, destination_parent, basename, deadline)?;
    let result = crate::windows::move_empty_directory_new(std::sync::Arc::clone(&owner), deadline);
    finish_empty_directory_move(lease, owner, result, deadline)
}

/// Private immutable authority shared only with the fixed native move request.
#[cfg(windows)]
pub(crate) struct MoveOwnership {
    _source_parent: std::sync::Arc<DirectoryGuard>,
    _destination_parent: std::sync::Arc<DirectoryGuard>,
    _original: File,
    identity: FileIdentity,
    source: PathBuf,
    destination: PathBuf,
}

#[cfg(windows)]
impl MoveOwnership {
    pub(crate) fn source(&self) -> &Path {
        &self.source
    }

    pub(crate) fn destination(&self) -> &Path {
        &self.destination
    }
}

#[cfg(windows)]
enum MoveState {
    Ready,
    Active,
    Quarantined {
        _owner: std::sync::Arc<MoveOwnership>,
    },
}

#[cfg(windows)]
static MOVE_STATE: std::sync::Mutex<MoveState> = std::sync::Mutex::new(MoveState::Ready);

#[cfg(windows)]
struct MoveLease<'a>(&'a std::sync::Mutex<MoveState>);

#[cfg(windows)]
impl<'a> MoveLease<'a> {
    fn acquire(state: &'a std::sync::Mutex<MoveState>) -> io::Result<Self> {
        let mut slot = state
            .lock()
            .map_err(|_| io::Error::other("private directory move admission is unavailable"))?;
        if !matches!(*slot, MoveState::Ready) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "private directory move ownership is active or quarantined",
            ));
        }
        *slot = MoveState::Active;
        Ok(Self(state))
    }

    fn quarantine(self, owner: std::sync::Arc<MoveOwnership>) {
        // Poison never restores admission. Retain the sole owner even on that path.
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = MoveState::Quarantined { _owner: owner };
    }
}

#[cfg(windows)]
impl Drop for MoveLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.lock()
            && matches!(*slot, MoveState::Active)
        {
            *slot = MoveState::Ready;
        }
    }
}

#[cfg(windows)]
fn move_basename(basename: &std::ffi::OsStr) -> io::Result<&str> {
    use std::path::Component;

    let name = basename.to_str().ok_or_else(move_binding_refused)?;
    let mut components = Path::new(basename).components();
    if !matches!(components.next(), Some(Component::Normal(value)) if value == basename)
        || components.next().is_some()
        || name.ends_with(['.', ' '])
        || name.chars().any(|ch| {
            ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
        })
    {
        return Err(move_binding_refused());
    }
    let device = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let port = device
        .strip_prefix("COM")
        .or_else(|| device.strip_prefix("LPT"));
    if matches!(
        device.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || port.is_some_and(|number| {
        matches!(
            number,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        )
    }) {
        return Err(move_binding_refused());
    }
    Ok(name)
}

#[cfg(windows)]
fn move_binding_refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "private directory move identity, namespace or empty-root binding refused",
    )
}

#[cfg(windows)]
fn directory_identity(guard: &DirectoryGuard) -> io::Result<FileIdentity> {
    let file_id::FileId::HighRes {
        volume_serial_number,
        file_id,
    } = file_id::get_high_res_file_id(guard.normalized_path())?
    else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "full directory identity is unavailable",
        ));
    };
    Ok(FileIdentity {
        volume_serial_number,
        file_id,
    })
}

#[cfg(windows)]
fn empty_directory(path: &Path) -> io::Result<()> {
    match fs::read_dir(path)?.next() {
        None => Ok(()),
        Some(Err(error)) => Err(error),
        Some(Ok(_)) => Err(move_binding_refused()),
    }
}

#[cfg(windows)]
fn prepare_empty_directory_move(
    source_parent: std::sync::Arc<DirectoryGuard>,
    destination_parent: std::sync::Arc<DirectoryGuard>,
    basename: &std::ffi::OsStr,
    deadline: std::time::Instant,
) -> io::Result<std::sync::Arc<MoveOwnership>> {
    ensure_private_deadline(deadline)?;
    let basename = move_basename(basename)?;
    let admitted_source = private_operation_until(deadline, || {
        DirectoryGuard::existing_private(source_parent.normalized_path())
    })?;
    let admitted_destination = private_operation_until(deadline, || {
        DirectoryGuard::ancestors(destination_parent.normalized_path())
    })?;
    if admitted_source.normalized_path() != source_parent.normalized_path()
        || admitted_destination.normalized_path() != destination_parent.normalized_path()
        || source_parent.normalized_path() == destination_parent.normalized_path()
    {
        return Err(move_binding_refused());
    }
    let source_volume = private_operation_until(deadline, || directory_identity(&admitted_source))?
        .volume_serial_number;
    let destination_volume =
        private_operation_until(deadline, || directory_identity(&admitted_destination))?
            .volume_serial_number;
    if source_volume != destination_volume {
        return Err(move_binding_refused());
    }
    let source = source_parent.normalized_path().join(basename);
    let destination = destination_parent.normalized_path().join(basename);
    let initial = private_operation_until(deadline, || DirectoryGuard::ancestors(&source))?;
    private_operation_until(deadline, || empty_directory(&source))?;
    // The caller's genuine fresh receipt and private parent authorize initialization.
    // Never first refuse private admission and then attempt to repair that object.
    private_operation_until(deadline, || restrict_owned(&source, true))?;
    let private = private_operation_until(deadline, || DirectoryGuard::existing_private(&source))?;
    let identity = private_operation_until(deadline, || directory_identity(&private))?;
    let original =
        private_operation_until(deadline, || windows::movable_directory_handle(&source))?;
    private_operation_until(deadline, || empty_directory(&source))?;
    let owner = std::sync::Arc::new(MoveOwnership {
        _source_parent: source_parent,
        _destination_parent: destination_parent,
        _original: original,
        identity,
        source,
        destination,
    });
    // Only parent guards and the DELETE-sharing physical handle survive native issue.
    drop((private, initial, admitted_source, admitted_destination));
    ensure_private_deadline(deadline)?;
    Ok(owner)
}

#[cfg(windows)]
fn bind_moved_directory(
    owner: &MoveOwnership,
    deadline: std::time::Instant,
) -> io::Result<DirectoryGuard> {
    let destination = private_operation_until(deadline, || {
        DirectoryGuard::existing_private(owner.destination())
    })?;
    if private_operation_until(deadline, || directory_identity(&destination))? != owner.identity {
        return Err(move_binding_refused());
    }
    private_operation_until(deadline, || empty_directory(owner.destination()))?;
    private_operation_until(deadline, || match fs::symlink_metadata(owner.source()) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(move_binding_refused()),
    })?;
    ensure_private_deadline(deadline)?;
    Ok(destination)
}

#[cfg(windows)]
fn resolve_refused_move(owner: &MoveOwnership, deadline: std::time::Instant) -> io::Result<()> {
    let source = private_operation_until(deadline, || {
        DirectoryGuard::existing_private(owner.source())
    })?;
    if private_operation_until(deadline, || directory_identity(&source))? != owner.identity {
        return Err(move_binding_refused());
    }
    private_operation_until(deadline, || empty_directory(owner.source()))?;
    // Observe only the fixed destination; existence grants no adoption/deletion authority.
    private_operation_until(deadline, || {
        match fs::symlink_metadata(owner.destination()) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    })
}

#[cfg(windows)]
fn finish_empty_directory_move(
    lease: MoveLease<'_>,
    owner: std::sync::Arc<MoveOwnership>,
    result: io::Result<()>,
    deadline: std::time::Instant,
) -> io::Result<DirectoryGuard> {
    match result {
        Ok(()) => match bind_moved_directory(&owner, deadline) {
            Ok(destination) => Ok(destination),
            Err(error) => {
                lease.quarantine(owner);
                Err(error)
            }
        },
        Err(error) => {
            // The bridge reserves raw OS codes for validated on-time native refusals.
            if error.raw_os_error().is_none() || resolve_refused_move(&owner, deadline).is_err() {
                lease.quarantine(owner);
            }
            Err(error)
        }
    }
}

/// Explicitly restricts a current-user-owned object, without taking foreign ownership.
/// This operation is intended for explicit repairs and disposable test setup.
pub fn restrict_owned(path: &Path, directory: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::restrict_owned(path, directory)
    }
    #[cfg(not(windows))]
    {
        reject_symlink(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                path,
                fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
            )?;
        }
        Ok(())
    }
}

/// Measures whether an existing object has the required private permission posture.
/// An unreadable descriptor returns an error, rather than an invented successful fact.
pub fn is_private(path: &Path, directory: bool) -> io::Result<bool> {
    #[cfg(windows)]
    {
        windows::is_private(path, directory)
    }
    #[cfg(not(windows))]
    {
        reject_symlink(path)?;
        let metadata = fs::metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            Ok(
                (metadata.permissions().mode() & 0o777).trailing_zeros() >= 6
                    && metadata.is_dir() == directory,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (metadata, directory);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "private filesystem permissions are unavailable",
            ))
        }
    }
}

fn parent(path: &Path) -> io::Result<&Path> {
    path.parent()
        .map(|parent| {
            if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            }
        })
        .ok_or_else(|| unsafe_path(path))
}

fn unsafe_path(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("unsafe managed path: {}", path.display()),
    )
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(unsafe_path(path)),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
mod windows {
    use super::{DirectoryGuard, GuardedFile, parent, unsafe_path};
    use std::fs::{self, File, OpenOptions};
    use std::io;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Component, Path, PathBuf, Prefix};
    use windows_permissions::constants::{AceType, SeObjectType, SecurityInformation};
    use windows_permissions::{LocalBox, SecurityDescriptor, wrappers};

    const READ_CONTROL: u32 = 0x0002_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_LIST_DIRECTORY: u32 = 1;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_ATTRIBUTE_READONLY: u32 = 1;
    const DELETE: u32 = 0x0001_0000;
    const GENERIC_READ_WRITE: u32 = 0xc000_0000;
    const FULL_CONTROL: u32 = 0x001f_01ff;
    // Match Stock/passive ancestry: sibling creation is separate from object/child mutation.
    const DIRECTORY_MUTATION: u32 = 0x500d_0156 & !0x06;
    const SYSTEM_SID: &str = "S-1-5-18";
    const ADMIN_SID: &str = "S-1-5-32-544";
    const INSTALLER_SID: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

    pub(super) fn file_options(options: &mut OpenOptions) {
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }

    pub(super) fn inspect_existing_file(path: &Path, private: bool) -> io::Result<Option<File>> {
        let file = match OpenOptions::new()
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        reject_reparse(&file, path)?;
        if !file.metadata()?.is_file() {
            return Err(observed_unsafe_path!(path, Leaf, NotFile, None));
        }
        if private {
            verify_private(&file, path, false)?;
        }
        Ok(Some(file))
    }

    fn normalized_absolute(path: &Path) -> io::Result<PathBuf> {
        let mut components = path.components();
        match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) if path.is_absolute() => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "state requires a local absolute drive or an unambiguous relative path",
                    ));
                }
            },
            Some(Component::RootDir) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "drive-root-relative state paths are ambiguous",
                ));
            }
            _ => {}
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut normalized = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(unsafe_path(path));
                    }
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        if !normalized.is_absolute() {
            return Err(unsafe_path(path));
        }
        match normalized.components().next() {
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)) => {
                let mut verbatim = std::ffi::OsString::from(r"\\?\");
                verbatim.push(normalized.as_os_str());
                Ok(PathBuf::from(verbatim))
            }
            _ => Ok(normalized),
        }
    }

    pub(super) fn directory_handle(path: &Path, repair: bool) -> io::Result<File> {
        OpenOptions::new()
            .access_mode(
                READ_CONTROL
                    | FILE_READ_ATTRIBUTES
                    | FILE_LIST_DIRECTORY
                    | if repair { WRITE_DAC } else { 0 },
            )
            // Child rename/link admission needs parent write access; retain the directory itself.
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    pub(super) fn movable_directory_handle(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | 4)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        reject_reparse(&file, path)?;
        if !file.metadata()?.is_dir() {
            return Err(unsafe_path(path));
        }
        verify_private(&file, path, true)?;
        Ok(file)
    }

    pub(super) fn reject_reparse(file: &File, path: &Path) -> io::Result<()> {
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(observed_unsafe_path!(path, Leaf, Reparse, None));
        }
        Ok(())
    }

    fn descriptor(file: &File) -> io::Result<LocalBox<SecurityDescriptor>> {
        wrappers::GetSecurityInfo(
            file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
    }

    fn trusted_directory(file: &File, path: &Path, sid: &str) -> io::Result<()> {
        let descriptor = descriptor(file)?;
        let owner = descriptor
            .owner()
            .ok_or_else(|| observed_unsafe_path!(path, Ancestor, OwnerMissing, None))?
            .to_string();
        if owner != sid && ![SYSTEM_SID, ADMIN_SID, INSTALLER_SID].contains(&owner.as_str()) {
            return Err(observed_unsafe_path!(path, Ancestor, OwnerUntrusted, None));
        }
        let acl = descriptor
            .dacl()
            .ok_or_else(|| observed_unsafe_path!(path, Ancestor, DaclMissing, None))?;
        for index in 0..acl.len() {
            let ace = acl
                .get_ace(index)
                .ok_or_else(|| observed_unsafe_path!(path, Ancestor, AceMissing, Some(index)))?;
            if ace.ace_type() == AceType::ACCESS_DENIED_ACE_TYPE {
                continue;
            }
            if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE {
                return Err(observed_unsafe_path!(path, Ancestor, AceShape, Some(index)));
            }
            if ace.flags().bits() & 0x08 != 0 {
                continue;
            }
            let principal = ace
                .sid()
                .ok_or_else(|| {
                    observed_unsafe_path!(path, Ancestor, PrincipalMissing, Some(index))
                })?
                .to_string();
            if principal != sid
                && ![SYSTEM_SID, ADMIN_SID, INSTALLER_SID].contains(&principal.as_str())
                && ace.mask().bits() & DIRECTORY_MUTATION != 0
            {
                return Err(observed_unsafe_path!(
                    path,
                    Ancestor,
                    ForeignMutation,
                    Some(index)
                ));
            }
        }
        Ok(())
    }

    fn private_descriptor(file: &File, directory: bool) -> io::Result<bool> {
        let sid = crate::windows::current_user_sid()?;
        let descriptor = descriptor(file)?;
        if descriptor
            .owner()
            .is_none_or(|owner| owner.to_string() != sid)
        {
            return Ok(false);
        }
        let Some(acl) = descriptor.dacl() else {
            return Ok(false);
        };
        let mut user = false;
        let mut system = false;
        for index in 0..acl.len() {
            let Some(ace) = acl.get_ace(index) else {
                return Ok(false);
            };
            if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE {
                return Ok(false);
            }
            let Some(principal) = ace.sid() else {
                return Ok(false);
            };
            let principal = principal.to_string();
            if principal != sid && principal != SYSTEM_SID {
                return Ok(false);
            }
            if ace.flags().bits() & 0x08 != 0 || ace.mask().bits() & FULL_CONTROL != FULL_CONTROL {
                return Ok(false);
            }
            if directory && ace.flags().bits() & 0x03 != 0x03 {
                return Ok(false);
            }
            user |= principal == sid;
            system |= principal == SYSTEM_SID;
        }
        let protected = if directory {
            protected_descriptor(&descriptor)?
        } else {
            true
        };
        Ok(user && system && protected)
    }

    fn protected_descriptor(descriptor: &SecurityDescriptor) -> io::Result<bool> {
        let sddl = wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
            descriptor,
            SecurityInformation::Dacl,
        )?;
        Ok(sddl
            .to_string_lossy()
            .strip_prefix("D:")
            .and_then(|sddl| sddl.split('(').next())
            .is_some_and(|flags| flags.contains('P')))
    }

    pub(super) fn exclusive_file(
        path: &Path,
        guard: DirectoryGuard,
        create: bool,
    ) -> io::Result<GuardedFile> {
        let path = guard
            .normalized_path()
            .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .access_mode(if create {
                // The exact newly created empty object needs owner/DACL initialization access.
                FULL_CONTROL
            } else {
                GENERIC_READ_WRITE | DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES
            })
            .share_mode(0)
            .custom_flags(
                FILE_FLAG_OPEN_REPARSE_POINT | if create { FILE_FLAG_WRITE_THROUGH } else { 0 },
            );
        if create {
            options.create_new(true);
        }
        let mut file = options.open(&path)?;
        reject_reparse(&file, &path)?;
        if !file.metadata()?.is_file() {
            return Err(unsafe_path(&path));
        }
        if create {
            let sid = crate::windows::current_user_sid()?;
            let security: LocalBox<SecurityDescriptor> =
                format!("O:{sid}D:P(A;;FA;;;{sid})(A;;FA;;;SY)").parse()?;
            wrappers::SetSecurityInfo(
                &mut file,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Owner
                    | SecurityInformation::Dacl
                    | SecurityInformation::ProtectedDacl,
                security.owner(),
                None,
                security.dacl(),
                None,
            )?;
        }
        verify_private(&file, &path, false)?;
        let security = descriptor(&file)?;
        if !protected_descriptor(&security)? {
            return Err(unsafe_path(&path));
        }
        let information = winapi_util::file::information(&file)?;
        if information.file_attributes() & u64::from(FILE_ATTRIBUTE_READONLY) != 0
            || information.number_of_links() != 1
        {
            return Err(unsafe_path(&path));
        }
        Ok(GuardedFile { file, guard, path })
    }

    pub(super) fn verify_private(file: &File, path: &Path, directory: bool) -> io::Result<()> {
        if !private_descriptor(file, directory)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} must be owned by the current account with a private current-SID/SYSTEM DACL",
                    path.display()
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn guard_directory(
        path: &Path,
        private: bool,
        create: bool,
    ) -> io::Result<DirectoryGuard> {
        let absolute = normalized_absolute(path)?;
        let sid = crate::windows::current_user_sid()?;
        let chain = absolute.ancestors().collect::<Vec<_>>();
        let mut handles = Vec::with_capacity(chain.len());
        let mut created = false;
        for (chain_index, component) in chain.iter().rev().enumerate() {
            let file = match directory_handle(component, false) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                    crate::windows::create_private_directory(component)?;
                    created = true;
                    directory_handle(component, false)?
                }
                Err(error) => return Err(error),
            };
            ancestor_refusal!(reject_reparse(&file, component), chain_index)?;
            if !file.metadata()?.is_dir() {
                return ancestor_refusal!(
                    Err(observed_unsafe_path!(
                        component,
                        Ancestor,
                        NotDirectory,
                        None
                    )),
                    chain_index
                );
            }
            ancestor_refusal!(trusted_directory(&file, component, &sid), chain_index)?;
            if created || (private && *component == absolute) {
                verify_private(&file, component, true)?;
            }
            handles.push(file);
        }
        let identity = fs::canonicalize(&absolute)?;
        Ok(DirectoryGuard {
            path: identity,
            _handles: handles,
        })
    }

    pub(super) fn read_owned_executable(
        path: &Path,
        guard: DirectoryGuard,
    ) -> io::Result<GuardedFile> {
        let path = guard
            .normalized_path()
            .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        verify_owned_executable(&file, &path)?;
        Ok(GuardedFile { file, guard, path })
    }

    pub(super) fn read_private_stable(
        path: &Path,
        guard: DirectoryGuard,
    ) -> io::Result<GuardedFile> {
        let path = guard
            .normalized_path()
            .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        reject_reparse(&file, &path)?;
        if !file.metadata()?.is_file() {
            return Err(unsafe_path(&path));
        }
        verify_private(&file, &path, false)?;
        Ok(GuardedFile { file, guard, path })
    }

    pub(super) fn owned_executable_exclusive(
        path: &Path,
        guard: DirectoryGuard,
    ) -> io::Result<GuardedFile> {
        let path = guard
            .normalized_path()
            .join(path.file_name().ok_or_else(|| unsafe_path(path))?);
        let file = OpenOptions::new()
            .access_mode(GENERIC_READ_WRITE | DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        verify_owned_executable(&file, &path)?;
        let information = winapi_util::file::information(&file)?;
        if information.file_attributes() & u64::from(FILE_ATTRIBUTE_READONLY) != 0
            || information.number_of_links() != 1
        {
            return Err(unsafe_path(&path));
        }
        let file = GuardedFile { file, guard, path };
        super::file_identity(&file)?;
        Ok(file)
    }

    fn verify_owned_executable(file: &File, path: &Path) -> io::Result<()> {
        executable_refusal!(reject_reparse(file, path))?;
        if !file.metadata()?.is_file() {
            return Err(observed_unsafe_path!(path, Executable, NotFile, None));
        }
        let sid = crate::windows::current_user_sid()?;
        let descriptor = descriptor(file)?;
        if descriptor
            .owner()
            .is_none_or(|owner| owner.to_string() != sid)
        {
            return Err(observed_unsafe_path!(
                path,
                Executable,
                OwnedOwnerMismatch,
                None
            ));
        }
        let acl = descriptor
            .dacl()
            .ok_or_else(|| observed_unsafe_path!(path, Executable, DaclMissing, None))?;
        for index in 0..acl.len() {
            let ace = acl
                .get_ace(index)
                .ok_or_else(|| observed_unsafe_path!(path, Executable, AceMissing, Some(index)))?;
            if ace.ace_type() == AceType::ACCESS_DENIED_ACE_TYPE {
                continue;
            }
            if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE {
                return Err(observed_unsafe_path!(
                    path,
                    Executable,
                    AceShape,
                    Some(index)
                ));
            }
            // Inherit-only entries do not grant rights on this file object.
            if ace.flags().bits() & 0x08 != 0 {
                continue;
            }
            let principal = ace
                .sid()
                .ok_or_else(|| {
                    observed_unsafe_path!(path, Executable, PrincipalMissing, Some(index))
                })?
                .to_string();
            if principal != sid
                && principal != SYSTEM_SID
                && principal != ADMIN_SID
                // Generic write/all and file write/append/EA/attributes/delete/control rights.
                && ace.mask().bits() & 0x500D_0156 != 0
            {
                return Err(observed_unsafe_path!(
                    path,
                    Executable,
                    ForeignMutation,
                    Some(index)
                ));
            }
        }
        Ok(())
    }

    pub(super) fn restrict_owned(path: &Path, directory: bool) -> io::Result<()> {
        let guard = DirectoryGuard::ancestors(parent(path)?)?;
        let mut file = if directory {
            directory_handle(path, true)?
        } else {
            OpenOptions::new()
                .access_mode(READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?
        };
        reject_reparse(&file, path)?;
        if file.metadata()?.is_dir() != directory {
            return Err(unsafe_path(path));
        }
        let sid = crate::windows::current_user_sid()?;
        if descriptor(&file)?
            .owner()
            .is_none_or(|owner| owner.to_string() != sid)
        {
            return Err(unsafe_path(path));
        }
        let flags = if directory { "OICI" } else { "" };
        let descriptor: LocalBox<SecurityDescriptor> =
            format!("O:{sid}D:P(A;{flags};FA;;;{sid})(A;{flags};FA;;;SY)").parse()?;
        wrappers::SetSecurityInfo(
            &mut file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            descriptor.dacl(),
            None,
        )?;
        verify_private(&file, path, directory)?;
        drop(guard);
        Ok(())
    }

    pub(super) fn is_private(path: &Path, directory: bool) -> io::Result<bool> {
        let _guard = DirectoryGuard::ancestors(parent(path)?)?;
        let file = if directory {
            directory_handle(path, false)?
        } else {
            OpenOptions::new()
                .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?
        };
        reject_reparse(&file, path)?;
        if file.metadata()?.is_dir() != directory {
            return Ok(false);
        }
        private_descriptor(&file, directory)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::wrappers;

    fn fixture_descriptor(file: &File) -> std::ffi::OsString {
        let information = SecurityInformation::Owner | SecurityInformation::Dacl;
        let descriptor =
            wrappers::GetSecurityInfo(file, SeObjectType::SE_FILE_OBJECT, information).unwrap();
        wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(&descriptor, information)
            .unwrap()
    }

    fn fixture_directory_descriptor(path: &Path) -> std::ffi::OsString {
        use std::os::windows::fs::OpenOptionsExt;

        let metadata = OpenOptions::new()
            .access_mode(0x0002_0080)
            .share_mode(3)
            .custom_flags(0x0220_0000)
            .open(path)
            .unwrap();
        fixture_descriptor(&metadata)
    }

    // Failed controls expose a closed fact, never SID/DACL/path/identity or unexpected bytes.
    #[track_caller]
    fn assert_private_fact(fact: bool) {
        assert!(fact, "private filesystem control fact failed");
    }

    struct MoveFixture {
        source_parent: std::sync::Arc<DirectoryGuard>,
        destination_parent: std::sync::Arc<DirectoryGuard>,
        name: std::ffi::OsString,
        source_id: FileIdentity,
        _temporary: tempfile::TempDir,
    }

    impl MoveFixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let source_parent = std::sync::Arc::new(
                DirectoryGuard::private(&temporary.path().join("private empty staging")).unwrap(),
            );
            let destination_parent = std::sync::Arc::new(
                DirectoryGuard::private(&temporary.path().join("outside evidence")).unwrap(),
            );
            let name = std::ffi::OsString::from("empty movable root");
            // Core controls run in the original PowerShell context. Its default
            // owner may be Administrators, so use the existing explicit-private
            // creator for this known disposable control. This does NOT qualify
            // the exclusive TempDir producer used by the original PR144 rows.
            let source =
                DirectoryGuard::private(&source_parent.normalized_path().join(&name)).unwrap();
            let source_id = directory_identity(&source).unwrap();
            drop(source);
            Self {
                _temporary: temporary,
                source_parent,
                destination_parent,
                name,
                source_id,
            }
        }

        fn source(&self) -> PathBuf {
            self.source_parent.normalized_path().join(&self.name)
        }

        fn destination(&self) -> PathBuf {
            self.destination_parent.normalized_path().join(&self.name)
        }

        fn prepare(&self, deadline: Instant) -> std::sync::Arc<MoveOwnership> {
            prepare_empty_directory_move(
                std::sync::Arc::clone(&self.source_parent),
                std::sync::Arc::clone(&self.destination_parent),
                &self.name,
                deadline,
            )
            .unwrap()
        }

        fn move_root(&self, deadline: Instant) -> io::Result<DirectoryGuard> {
            move_empty_private_directory(
                std::sync::Arc::clone(&self.source_parent),
                std::sync::Arc::clone(&self.destination_parent),
                &self.name,
                deadline,
            )
        }
    }

    #[test]
    fn private_empty_move_preserves_full_identity_and_refuses_a_retained_source_leaf() {
        let fixture = MoveFixture::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let state = std::sync::Mutex::new(MoveState::Ready);
        let lease = MoveLease::acquire(&state).unwrap();
        let owner = fixture.prepare(deadline);
        let pin = DirectoryGuard::existing_private(&fixture.source()).unwrap();
        let result =
            crate::windows::move_empty_directory_new(std::sync::Arc::clone(&owner), deadline);
        let error = finish_empty_directory_move(lease, owner, result, deadline).unwrap_err();
        assert_private_fact(error.raw_os_error().is_some_and(|code| code != 0));
        assert_private_fact(directory_identity(&pin).unwrap() == fixture.source_id);
        assert_private_fact(fs::read_dir(fixture.source()).unwrap().next().is_none());
        assert_private_fact(!fixture.destination().exists());
        assert_private_fact(matches!(*state.lock().unwrap(), MoveState::Ready));
        drop(pin);
        let moved = fixture.move_root(deadline).unwrap();
        assert_private_fact(directory_identity(&moved).unwrap() == fixture.source_id);
        assert_private_fact(moved.normalized_path() == fixture.destination());
        assert_private_fact(is_private(moved.normalized_path(), true).unwrap());
        assert_private_fact(
            fs::read_dir(moved.normalized_path())
                .unwrap()
                .next()
                .is_none(),
        );
        assert_private_fact(!fixture.source().exists());
        drop(moved);
    }

    #[test]
    fn private_empty_move_never_replaces_a_file_or_empty_or_populated_directory() {
        use std::os::windows::fs::OpenOptionsExt;

        for shape in ["file", "empty", "populated"] {
            let fixture = MoveFixture::new();
            let destination = fixture.destination();
            let (original, expected_id) = if shape == "file" {
                let mut file = create_private_new(&destination).unwrap();
                file.write_all(b"preserve collision").unwrap();
                file.sync_all().unwrap();
                let identity = file_identity(&file).unwrap();
                drop(file);
                let original = OpenOptions::new()
                    .read(true)
                    .share_mode(7)
                    .open(&destination)
                    .unwrap();
                (original, identity)
            } else {
                let guard = DirectoryGuard::private(&destination).unwrap();
                if shape == "populated" {
                    create_private_new(&destination.join("preserved child"))
                        .unwrap()
                        .write_all(b"preserve collision")
                        .unwrap();
                }
                let identity = directory_identity(&guard).unwrap();
                let original = windows::movable_directory_handle(&destination).unwrap();
                // An empty target must be genuinely replaceable by a bad rename;
                // retaining this guard would hide Rust's replacement fallback.
                drop(guard);
                (original, identity)
            };
            let expected_descriptor = fixture_descriptor(&original);
            let error = fixture
                .move_root(Instant::now() + Duration::from_secs(30))
                .unwrap_err();
            assert_private_fact(error.raw_os_error().is_some());
            if shape == "file" {
                let file = open_private(&destination, OpenOptions::new().read(true)).unwrap();
                assert_private_fact(file_identity(&file).unwrap() == expected_id);
                assert_private_fact(fixture_descriptor(&file) == expected_descriptor);
                assert_private_fact(fs::read(&destination).unwrap() == b"preserve collision");
            } else {
                let guard = DirectoryGuard::existing_private(&destination).unwrap();
                assert_private_fact(directory_identity(&guard).unwrap() == expected_id);
                assert_private_fact(
                    fixture_directory_descriptor(&destination) == expected_descriptor,
                );
                let names: Vec<_> = fs::read_dir(&destination)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect();
                if shape == "empty" {
                    assert_private_fact(names.is_empty());
                } else {
                    assert_private_fact(names == [std::ffi::OsString::from("preserved child")]);
                    assert_private_fact(
                        fs::read(destination.join("preserved child")).unwrap()
                            == b"preserve collision",
                    );
                }
            }
            let source = DirectoryGuard::existing_private(&fixture.source()).unwrap();
            assert_private_fact(directory_identity(&source).unwrap() == fixture.source_id);
            assert_private_fact(fs::read_dir(fixture.source()).unwrap().next().is_none());
            drop(original);
        }
    }

    #[test]
    fn private_empty_move_expiry_and_nonempty_preparation_never_issue_or_repair() {
        let fixture = MoveFixture::new();
        let descriptor = fixture_directory_descriptor(&fixture.source());
        let expired = Instant::now();
        assert_private_fact(
            fixture.move_root(expired).unwrap_err().kind() == io::ErrorKind::TimedOut,
        );
        assert_private_fact(!fixture.destination().exists());
        assert_private_fact(fixture_directory_descriptor(&fixture.source()) == descriptor);
        let path = fixture.source().join("existing child");
        create_private_new(&path)
            .unwrap()
            .write_all(b"unchanged")
            .unwrap();
        assert_private_fact(
            fixture
                .move_root(Instant::now() + Duration::from_secs(30))
                .unwrap_err()
                .kind()
                == io::ErrorKind::PermissionDenied,
        );
        assert_private_fact(!fixture.destination().exists());
        assert_private_fact(fs::read(&path).unwrap() == b"unchanged");
        assert_private_fact(fixture_directory_descriptor(&fixture.source()) == descriptor);
        for invalid in [
            "", ".", "..", "a/b", "a\\b", "a:b", "CON", "LPT1.txt", "bad.",
        ] {
            assert_private_fact(move_basename(std::ffi::OsStr::new(invalid)).is_err());
        }
    }

    #[test]
    fn private_empty_move_wrong_and_missing_postbindings_quarantine_original_without_bytes() {
        for wrong_identity in [false, true] {
            let fixture = MoveFixture::new();
            let state = std::sync::Mutex::new(MoveState::Ready);
            let deadline = Instant::now() + Duration::from_secs(30);
            let lease = MoveLease::acquire(&state).unwrap();
            let owner = fixture.prepare(deadline);
            let original = std::sync::Arc::downgrade(&owner);
            if wrong_identity {
                drop(DirectoryGuard::private(&fixture.destination()).unwrap());
            }
            // Inject a claimed completion without native mutation. Real filesystem
            // admission must reject absence or a different private empty object.
            assert_private_fact(
                finish_empty_directory_move(lease, owner, Ok(()), deadline).is_err(),
            );
            assert_private_fact(original.upgrade().is_some());
            assert_private_fact(
                MoveLease::acquire(&state).err().unwrap().kind() == io::ErrorKind::WouldBlock,
            );
            let source = DirectoryGuard::existing_private(&fixture.source()).unwrap();
            assert_private_fact(directory_identity(&source).unwrap() == fixture.source_id);
            assert_private_fact(fs::read_dir(fixture.source()).unwrap().next().is_none());
            if wrong_identity {
                let destination = DirectoryGuard::existing_private(&fixture.destination()).unwrap();
                assert_ne!(directory_identity(&destination).unwrap(), fixture.source_id);
                assert_private_fact(
                    fs::read_dir(fixture.destination())
                        .unwrap()
                        .next()
                        .is_none(),
                );
            }
            // This local test slot has no pending native operation. Production's
            // single static quarantine has no reset or recursive cleanup path.
            drop(source);
            drop(state);
            assert_private_fact(original.upgrade().is_none());
        }
    }

    #[test]
    fn private_empty_move_expired_postbinding_retains_owner_after_confirmed_native_move() {
        let fixture = MoveFixture::new();
        let state = std::sync::Mutex::new(MoveState::Ready);
        let lease = MoveLease::acquire(&state).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let owner = fixture.prepare(deadline);
        let original = std::sync::Arc::downgrade(&owner);
        crate::windows::move_empty_directory_new(std::sync::Arc::clone(&owner), deadline).unwrap();
        // Native movement really returned; now exercise the expired delivery gate
        // with an already-expired budget, without sleeping or claiming OS preemption.
        assert_private_fact(
            finish_empty_directory_move(lease, owner, Ok(()), Instant::now())
                .unwrap_err()
                .kind()
                == io::ErrorKind::TimedOut,
        );
        assert_private_fact(original.upgrade().is_some());
        assert_private_fact(MoveLease::acquire(&state).is_err());
        assert_private_fact(!fixture.source().exists());
        let destination = DirectoryGuard::existing_private(&fixture.destination()).unwrap();
        assert_private_fact(directory_identity(&destination).unwrap() == fixture.source_id);
        assert_private_fact(
            fs::read_dir(fixture.destination())
                .unwrap()
                .next()
                .is_none(),
        );
        drop(destination);
        drop(state);
        assert_private_fact(original.upgrade().is_none());
    }

    #[test]
    fn private_empty_move_unknown_result_keeps_the_sole_parent_guards() {
        let fixture = MoveFixture::new();
        let state = std::sync::Mutex::new(MoveState::Ready);
        let deadline = Instant::now() + Duration::from_secs(30);
        let lease = MoveLease::acquire(&state).unwrap();
        let owner = fixture.prepare(deadline);
        let source = owner.source().to_owned();
        let parent = fixture.source_parent.normalized_path().to_owned();
        let displaced = parent.with_file_name("parent must remain anchored");
        let ancestry = std::sync::Arc::downgrade(&fixture.source_parent);
        let original = std::sync::Arc::downgrade(&owner);
        let MoveFixture {
            source_parent,
            destination_parent,
            _temporary: temporary,
            ..
        } = fixture;
        drop((source_parent, destination_parent));
        // Inject an unknown transport outcome. No native action runs in this
        // control; real pending-child ownership is qualified by the worker tests.
        let error = io::Error::other("injected unknown move result");
        assert_private_fact(
            finish_empty_directory_move(lease, owner, Err(error), deadline).is_err(),
        );
        assert_private_fact(original.upgrade().is_some());
        assert_private_fact(ancestry.upgrade().is_some());
        assert_private_fact(fs::read_dir(&source).unwrap().next().is_none());
        assert_private_fact(fs::rename(&parent, &displaced).is_err());
        assert_private_fact(MoveLease::acquire(&state).is_err());
        // This local slot has no admitted native work. Release the exact control
        // owner, then require the same parent operation to succeed without it.
        drop(state);
        assert_private_fact(original.upgrade().is_none());
        assert_private_fact(ancestry.upgrade().is_none());
        fs::rename(&parent, &displaced).unwrap();
        assert_private_fact(displaced.join("empty movable root").is_dir());
        drop(temporary);
    }

    #[test]
    fn strict_private_remover_preserves_actual_readonly_and_sharing_refusals() {
        let temporary = tempfile::tempdir().unwrap();
        let root = DirectoryGuard::private(&temporary.path().join("strict removal")).unwrap();
        let path = root.normalized_path().join("readonly.txt");
        let mut file = create_private_new(&path).unwrap();
        file.write_all(b"preserved native refusal").unwrap();
        file.sync_all().unwrap();
        let identity = file_identity(&file).unwrap();
        let descriptor = fixture_descriptor(&file);
        drop(file);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        assert_private_fact(
            remove_private_file_strict(&path)
                .unwrap_err()
                .raw_os_error()
                == Some(5),
        );
        let file = open_private(&path, OpenOptions::new().read(true)).unwrap();
        assert_private_fact(file_identity(&file).unwrap() == identity);
        assert_private_fact(fixture_descriptor(&file) == descriptor);
        assert_private_fact(fs::read(&path).unwrap() == b"preserved native refusal");
        assert_private_fact(file.metadata().unwrap().permissions().readonly());
        drop(file);
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Clear only this verified test-owned Windows attribute after native refusal; owner and DACL remain unchanged."
        )]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
        let held = open_private(&path, OpenOptions::new().read(true)).unwrap();
        assert_private_fact(matches!(
            remove_private_file_strict(&path)
                .unwrap_err()
                .raw_os_error(),
            Some(32 | 33)
        ));
        assert_private_fact(file_identity(&held).unwrap() == identity);
        assert_private_fact(fs::read(&path).unwrap() == b"preserved native refusal");
        drop(held);
        remove_private_file_strict(&path).unwrap();
        remove_private_file_strict(&path).unwrap();
        assert_private_fact(!path.exists());
        let absent = temporary.path().join("never created").join("leaf");
        remove_private_file_strict(&absent).unwrap();
        assert_private_fact(!absent.parent().unwrap().exists());
    }

    #[test]
    fn stable_private_reader_excludes_writes_and_never_creates_a_parent() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("absent").join("database.db");
        assert_eq!(
            open_private_read_stable(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!path.parent().unwrap().exists());
        let path = temporary.path().join("private").join("database.db");
        create_private_new(&path)
            .unwrap()
            .write_all(b"closed")
            .unwrap();
        let writer = open_private(&path, OpenOptions::new().read(true).write(true)).unwrap();
        assert_eq!(
            open_private_read_stable(&path).unwrap_err().raw_os_error(),
            Some(32)
        );
        drop(writer);
        let reader = open_private_read_stable(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"closed");
        assert_eq!(
            open_private(&path, OpenOptions::new().write(true))
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        assert_eq!(fs::remove_file(&path).unwrap_err().raw_os_error(), Some(32));
        drop(reader);
        assert!(open_private(&path, OpenOptions::new().write(true)).is_ok());
    }

    #[test]
    fn writable_mapping_refuses_stable_gate_after_original_file_handle_closes() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("private").join("mapped.db");
        let mut file = create_private_new(&path).unwrap();
        file.set_len(4096).unwrap();
        file.write_all(b"mapped").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let entered = temporary.path().join("mapping-entered");
        let release = temporary.path().join("mapping-release");
        let input = json!({"path": path, "entered": entered, "release": release});
        let deadline = Instant::now() + Duration::from_secs(30);
        let worker = std::thread::spawn(move || {
            crate::windows::run_script_json(
                r"
                $stream = [IO.File]::Open([string]$request.path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite);
                $mapping = $null;
                $view = $null;
                try {
                    $mapping = [IO.MemoryMappedFiles.MemoryMappedFile]::CreateFromFile($stream, [System.Management.Automation.Language.NullString]::Value, 0, [IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite, [IO.HandleInheritability]::None, $true);
                    $view = $mapping.CreateViewAccessor();
                    $stream.Dispose();
                    [IO.File]::WriteAllText([string]$request.entered, 'original-handle-closed');
                    while (-not [IO.File]::Exists([string]$request.release)) { [Threading.Thread]::Sleep(10) }
                } finally {
                    if ($null -ne $view) { $view.Dispose() }
                    if ($null -ne $mapping) { $mapping.Dispose() }
                    $stream.Dispose();
                }
                @{closed=$true} | & $locronToJson -Compress
            ",
                &input,
            )
        });
        while !entered.exists() && !worker.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Always release and join the owned adapter before asserting its observations.
        let marker = fs::read_to_string(&entered);
        let gate = open_private_read_stable(&path);
        let ancestry = (
            temporary.path().is_dir(),
            path.parent().unwrap().is_dir(),
            path.is_file(),
            release.parent().unwrap().is_dir(),
        );
        let released = fs::write(&release, b"release");
        let result = worker.join();
        let result = result.unwrap_or_else(|_| {
            panic!("mapping helper panicked after release attempt; ancestry={ancestry:?}, release={released:?}, marker={marker:?}");
        });
        let result = result.unwrap_or_else(|error| {
            panic!("mapping helper failed after release/join: {error}; ancestry={ancestry:?}, release={released:?}, marker={marker:?}");
        });
        released.unwrap();
        assert_eq!(marker.unwrap(), "original-handle-closed");
        assert_eq!(gate.unwrap_err().raw_os_error(), Some(32));
        assert_eq!(result["closed"], true);
        assert!(open_private_read_stable(&path).is_ok());
        assert_eq!(&fs::read(&path).unwrap()[..6], b"mapped");
    }

    #[test]
    fn passive_file_observers_leave_an_absent_parent_absent() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("absent");
        let path = root.join("nested").join("secret.txt");
        let error = open_private(&path, OpenOptions::new().read(true).create(true)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!root.exists());
        assert_eq!(
            open_read_no_follow(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!root.exists());
        remove_private_file(&path).unwrap();
        assert!(!root.exists());
        drop(open_private_or_create(&path).unwrap());
        assert!(is_private(path.parent().unwrap(), true).unwrap());
        assert!(is_private(&path, false).unwrap());
    }

    struct OwnedFixtureChild(Child);

    impl OwnedFixtureChild {
        fn wait_until(&mut self, deadline: Instant) -> std::process::ExitStatus {
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    return status;
                }
                assert!(
                    Instant::now() < deadline,
                    "owned fixture child did not exit"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for OwnedFixtureChild {
        fn drop(&mut self) {
            if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            let _ = self.0.kill();
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn exclusive_creation_initializes_owner_before_bytes_and_preserves_existing_leaf() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let path = root.join("replacement.exe");
        let mut gate = create_private_new_exclusive(&path).unwrap();
        assert_eq!(gate.metadata().unwrap().len(), 0);
        assert_eq!(
            winapi_util::file::information(&*gate)
                .unwrap()
                .number_of_links(),
            1
        );
        let identity = file_identity(&gate).unwrap();
        assert_eq!(File::open(&path).unwrap_err().raw_os_error(), Some(32));
        gate.write_all(b"verified replacement").unwrap();
        gate.sync_all().unwrap();
        drop(gate);
        assert!(is_private(&path, false).unwrap());
        assert_eq!(
            create_private_new_exclusive(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let mut gate = open_private_exclusive(&path).unwrap();
        assert_eq!(file_identity(&gate).unwrap(), identity);
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut *gate, &mut bytes).unwrap();
        assert_eq!(bytes, b"verified replacement");
    }

    #[test]
    fn exclusive_existing_refuses_readonly_and_multiple_links_without_mutation() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let path = root.join("replacement.exe");
        let alias = root.join("alias.exe");
        create_private_new_exclusive(&path)
            .unwrap()
            .write_all(b"original")
            .unwrap();
        fs::hard_link(&path, &alias).unwrap();
        assert_eq!(
            open_private_exclusive(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            open_owned_executable_exclusive(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&alias).unwrap(), b"original");
        fs::remove_file(&alias).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        assert!(open_private_exclusive(&path).is_err());
        assert!(open_owned_executable_exclusive(&path).is_err());
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        // Reset only the test-owned attribute so temporary-directory cleanup remains possible.
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "This Windows-only fixture clears FILE_ATTRIBUTE_READONLY on its verified private leaf; the owner/DACL is unchanged."
        )]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
    }

    #[test]
    fn exclusive_gate_refuses_mapped_image_and_blocks_new_launch_and_path_mutation() {
        use std::os::windows::process::CommandExt;

        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let executable = root.join("owned-command.exe");
        let stock = crate::windows::stock_powershell()
            .unwrap()
            .ancestors()
            .nth(3)
            .unwrap()
            .join("cmd.exe");
        let mut source = File::open(stock).unwrap();
        let mut gate = create_private_new_exclusive(&executable).unwrap();
        std::io::copy(&mut source, &mut *gate).unwrap();
        gate.sync_all().unwrap();
        drop(gate);
        let mut child = OwnedFixtureChild(
            Command::new(&executable)
                .args(["/D", "/Q", "/C", "set /p LOCRON_EXCLUSIVE_FIXTURE="])
                .creation_flags(0x0800_0000)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(child.0.try_wait().unwrap().is_none());
        assert_eq!(
            open_private_exclusive(&executable)
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        assert_eq!(
            open_owned_executable_exclusive(&executable)
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        child
            .0
            .stdin
            .take()
            .unwrap()
            .write_all(b"owned\r\n")
            .unwrap();
        assert!(
            child
                .wait_until(Instant::now() + Duration::from_secs(5))
                .success()
        );
        let bytes = fs::read(&executable).unwrap();
        let descriptor = fixture_descriptor(&File::open(&executable).unwrap());
        let gate_functions: [fn(&Path) -> io::Result<GuardedFile>; 2] =
            [open_private_exclusive, open_owned_executable_exclusive];
        for open_gate in gate_functions {
            let gate = open_gate(&executable).unwrap();
            assert!(file_identity(&gate).is_ok());
            assert_eq!(fixture_descriptor(&gate), descriptor);
            for options in [
                OpenOptions::new().read(true),
                OpenOptions::new().write(true),
            ] {
                assert_eq!(
                    options.open(&executable).unwrap_err().raw_os_error(),
                    Some(32)
                );
            }
            assert_eq!(
                fs::rename(&executable, root.join("moved.exe"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert_eq!(
                Command::new(&executable)
                    .args(["/D", "/Q", "/C", "exit 0"])
                    .creation_flags(0x0800_0000)
                    .spawn()
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            drop(gate);
            assert_eq!(fs::read(&executable).unwrap(), bytes);
            assert_eq!(
                fixture_descriptor(&File::open(&executable).unwrap()),
                descriptor
            );
        }
        let mut relaunched = OwnedFixtureChild(
            Command::new(&executable)
                .args(["/D", "/Q", "/C", "exit 0"])
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(
            relaunched
                .wait_until(Instant::now() + Duration::from_secs(5))
                .success()
        );
    }

    #[test]
    fn existing_private_guard_refuses_missing_root_without_creation() {
        let temporary = tempfile::tempdir().unwrap();
        let missing_parent = temporary.path().join("missing");
        let root = missing_parent.join("private");
        assert_eq!(
            DirectoryGuard::existing_private(&root).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!missing_parent.exists());
        assert_eq!(
            open_owned_executable_exclusive(&root.join("missing.exe"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!missing_parent.exists());
    }

    #[test]
    fn package_source_allows_readers_and_trusted_admin_but_guards_mutation() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let path = root.join("package.exe");
        create_private_new(&path)
            .unwrap()
            .write_all(b"package")
            .unwrap();
        crate::windows::run_script_json(
            r"
            $file = [IO.FileInfo]::new([string]$request.path)
            $acl = $file.GetAccessControl()
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'ReadAndExecute', 'Allow'))
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'), 'FullControl', 'Allow'))
            $file.SetAccessControl($acl)
            @{changed=$true} | & $locronToJson -Compress
            ",
            &json!({"path": path}),
        ).unwrap();
        assert!(!is_private(&path, false).unwrap());
        let source = read_owned_executable(&path).unwrap();
        let descriptor = fixture_descriptor(&source);
        assert_eq!(fs::read(source.normalized_path()).unwrap(), b"package");
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        let moved = root.join("moved.exe");
        assert_eq!(
            fs::rename(&path, &moved).unwrap_err().raw_os_error(),
            Some(32)
        );
        drop(source);
        let gate = open_owned_executable_exclusive(&path).unwrap();
        assert_eq!(fixture_descriptor(&gate), descriptor);
        assert_eq!(File::open(&path).unwrap_err().raw_os_error(), Some(32));
        drop(gate);
        assert_eq!(fs::read(&path).unwrap(), b"package");
        assert_eq!(fixture_descriptor(&File::open(&path).unwrap()), descriptor);
        fs::rename(&path, &moved).unwrap();
    }

    #[test]
    fn package_source_refuses_every_untrusted_mutating_grant() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let rights = [
            "WriteData",
            "AppendData",
            "WriteExtendedAttributes",
            "WriteAttributes",
            "Delete",
            "ChangePermissions",
            "TakeOwnership",
        ];
        for right in rights {
            let path = root.join(format!("{right}.exe"));
            drop(create_private_new(&path).unwrap());
            crate::windows::run_script_json(
                r"
                $file = [IO.FileInfo]::new([string]$request.path)
                $acl = $file.GetAccessControl()
                $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), [Security.AccessControl.FileSystemRights][Enum]::Parse([Security.AccessControl.FileSystemRights], [string]$request.right), 'Allow'))
                $file.SetAccessControl($acl)
                @{changed=$true} | & $locronToJson -Compress
                ",
                &json!({"path": path, "right": right}),
            ).unwrap();
            let reader_error = read_owned_executable(&path).unwrap_err();
            assert_eq!(
                reader_error.kind(),
                io::ErrorKind::PermissionDenied,
                "{right}"
            );
            #[cfg(debug_assertions)]
            {
                let observation = guard_refusal_observation(&reader_error).unwrap();
                assert_eq!(observation.object(), "executable");
                assert_eq!(observation.predicate(), "foreign_mutation");
                assert_eq!(observation.chain_index(), None);
                assert!(observation.ace_index().is_some());
            }
            let descriptor = fixture_descriptor(&File::open(&path).unwrap());
            let exclusive_error = open_owned_executable_exclusive(&path).unwrap_err();
            assert_eq!(
                exclusive_error.kind(),
                io::ErrorKind::PermissionDenied,
                "{right}"
            );
            #[cfg(debug_assertions)]
            {
                let observation = guard_refusal_observation(&exclusive_error).unwrap();
                assert_eq!(observation.object(), "executable");
                assert_eq!(observation.predicate(), "foreign_mutation");
                assert_eq!(observation.chain_index(), None);
                assert!(observation.ace_index().is_some());
            }
            assert_eq!(fixture_descriptor(&File::open(&path).unwrap()), descriptor);
            assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        }
    }

    #[test]
    fn full_executable_identity_matches_hard_link_and_refuses_equal_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let source = root.join("source.exe");
        let alias = root.join("alias.exe");
        let different = root.join("different.exe");
        create_private_new(&source)
            .unwrap()
            .write_all(b"same")
            .unwrap();
        create_private_new(&different)
            .unwrap()
            .write_all(b"same")
            .unwrap();
        fs::hard_link(&source, &alias).unwrap();
        let source = read_owned_executable(&source).unwrap();
        let alias = read_owned_executable(&alias).unwrap();
        let different = read_owned_executable(&different).unwrap();
        assert!(same_file(&source, &alias).unwrap());
        assert!(!same_file(&source, &different).unwrap());
    }

    #[test]
    fn created_private_root_and_file_have_real_acl_facts() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("비공개 state");
        let guard = DirectoryGuard::private(&root).unwrap();
        assert!(is_private(&root, true).unwrap());
        let path = root.join("secret.txt");
        let mut file = create_private_new(&path).unwrap();
        file.write_all(b"private").unwrap();
        drop(file);
        assert!(is_private(&path, false).unwrap());
        assert_eq!(guard.normalized_path(), fs::canonicalize(&root).unwrap());
    }

    #[test]
    fn broad_owned_root_requires_explicit_repair() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("owned");
        drop(DirectoryGuard::private(&root).unwrap());
        crate::windows::run_script_json(r"
            $acl = [IO.Directory]::GetAccessControl([string]$request.path);
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'Read', 'Allow'));
            [IO.Directory]::SetAccessControl([string]$request.path, $acl);
            @{changed=$true} | & $locronToJson -Compress
        ", &json!({"path": root})).unwrap();
        assert!(!is_private(&root, true).unwrap());
        assert!(DirectoryGuard::existing_private(&root).is_err());
        assert!(DirectoryGuard::private(&root).is_err());
        restrict_owned(&root, true).unwrap();
        assert!(is_private(&root, true).unwrap());
        assert!(DirectoryGuard::private(&root).is_ok());
    }

    #[test]
    fn guarded_ancestor_cannot_be_renamed() {
        let temporary = tempfile::tempdir().unwrap();
        let parent = temporary.path().join("ancestor");
        let root = parent.join("private");
        let guard = DirectoryGuard::private(&root).unwrap();
        let moved = temporary.path().join("moved");
        assert!(fs::rename(&parent, &moved).is_err());
        drop(guard);
        fs::rename(&parent, &moved).unwrap();
    }

    #[test]
    fn bare_empty_directory_guard_retains_identity_and_blocks_rename_and_reparse() {
        use std::os::windows::fs::MetadataExt;

        let temporary = tempfile::tempdir().unwrap();
        let parent = fs::canonicalize(temporary.path()).unwrap();
        let empty = parent.join("empty guarded directory");
        let target = parent.join("separate junction target");
        drop(DirectoryGuard::private(&empty).unwrap());
        drop(DirectoryGuard::private(&target).unwrap());
        let guard = DirectoryGuard::existing_private(&empty).unwrap();
        let identity = file_id::get_high_res_file_id(guard.normalized_path()).unwrap();
        assert!(matches!(identity, file_id::FileId::HighRes { .. }));
        let descriptor = fixture_directory_descriptor(&empty);
        assert!(fs::read_dir(&empty).unwrap().next().is_none());
        let moved = parent.join("moved empty directory");
        assert_eq!(
            fs::rename(&empty, &moved).unwrap_err().raw_os_error(),
            Some(32)
        );
        assert_eq!(fs::remove_dir(&empty).unwrap_err().raw_os_error(), Some(32));
        let replacement = r"
            [IO.Directory]::Delete([string]$request.link);
            New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null;
            @{created=$true} | & $locronToJson -Compress
        ";
        let request = json!({"link":empty,"target":target});
        assert!(crate::windows::run_script_json(replacement, &request).is_err());
        assert_eq!(
            fs::symlink_metadata(&empty).unwrap().file_attributes() & 0x400,
            0
        );
        assert_eq!(file_id::get_high_res_file_id(&empty).unwrap(), identity);
        assert_eq!(fixture_directory_descriptor(&empty), descriptor);
        assert!(fs::read_dir(&empty).unwrap().next().is_none());
        assert!(!moved.exists());
        drop(guard);
        fs::rename(&empty, &moved).unwrap();
        fs::rename(&moved, &empty).unwrap();
        assert_eq!(file_id::get_high_res_file_id(&empty).unwrap(), identity);
        crate::windows::run_script_json(replacement, &request).unwrap();
        let refused = DirectoryGuard::existing_private(&empty);
        // Remove only this owned junction before checking the refusal and cleaning up.
        fs::remove_dir(&empty).unwrap();
        assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert!(fs::read_dir(&target).unwrap().next().is_none());
    }

    #[test]
    fn list_denied_ancestor_refuses_without_creating_or_repairing_its_suffix() {
        use std::os::windows::fs::OpenOptionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let ancestor = fs::canonicalize(temporary.path())
            .unwrap()
            .join("list denied");
        drop(DirectoryGuard::private(&ancestor).unwrap());
        let sid = crate::windows::current_user_sid().unwrap();
        let mut metadata = OpenOptions::new()
            .access_mode(0x0006_0080)
            .share_mode(3)
            .custom_flags(0x0220_0000)
            .open(&ancestor)
            .unwrap();
        let original = wrappers::GetSecurityInfo(
            &metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .unwrap();
        let denied: windows_permissions::LocalBox<windows_permissions::SecurityDescriptor> =
            format!("D:P(D;;0x00000001;;;{sid})(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)")
                .parse()
                .unwrap();
        wrappers::SetSecurityInfo(
            &mut metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            denied.dacl(),
            None,
        )
        .unwrap();
        let denied_descriptor = fixture_descriptor(&metadata);
        let missing = ancestor.join("never created").join("private");
        let refused = DirectoryGuard::private(&missing);
        let suffix_absent = !ancestor.join("never created").exists();
        let unchanged = fixture_descriptor(&metadata) == denied_descriptor;
        wrappers::SetSecurityInfo(
            &mut metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            original.dacl(),
            None,
        )
        .unwrap();
        assert_eq!(refused.unwrap_err().raw_os_error(), Some(5));
        assert!(suffix_absent);
        assert!(unchanged);
        // Check absence again after restoring list access, so a permission-denied
        // path observation cannot conceal an unexpectedly created suffix.
        assert!(!ancestor.join("never created").exists());
        assert!(fs::read_dir(&ancestor).unwrap().next().is_none());
        assert!(DirectoryGuard::existing_private(&ancestor).is_ok());
    }

    #[test]
    fn common_ancestry_refuses_foreign_mutation_before_creating_a_suffix() {
        use std::os::windows::fs::OpenOptionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let ancestor = fs::canonicalize(temporary.path())
            .unwrap()
            .join("foreign mutation ancestor");
        drop(DirectoryGuard::private(&ancestor).unwrap());
        let sid = crate::windows::current_user_sid().unwrap();
        let mut metadata = OpenOptions::new()
            .access_mode(0x0006_0080)
            .share_mode(3)
            .custom_flags(0x0220_0000)
            .open(&ancestor)
            .unwrap();
        let original = wrappers::GetSecurityInfo(
            &metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .unwrap();
        // Actual-object generic write/all, DELETE, DELETE_CHILD, EA/attributes and DAC/owner.
        for mask in [
            0x4000_0000_u32,
            0x1000_0000,
            0x0001_0000,
            0x0000_0040,
            0x0000_0010,
            0x0000_0100,
            0x0004_0000,
            0x0008_0000,
        ] {
            let altered: windows_permissions::LocalBox<windows_permissions::SecurityDescriptor> =
                format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;;0x{mask:08x};;;WD)")
                    .parse()
                    .unwrap();
            wrappers::SetSecurityInfo(
                &mut metadata,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                altered.dacl(),
                None,
            )
            .unwrap();
            let descriptor = fixture_descriptor(&metadata);
            let missing = ancestor.join("must remain absent").join("private");
            let refused = DirectoryGuard::private(&missing);
            let unchanged = fixture_descriptor(&metadata) == descriptor;
            let absent = !ancestor.join("must remain absent").exists();
            let empty = fs::read_dir(&ancestor).unwrap().next().is_none();
            let error = match refused {
                Ok(guard) => {
                    drop(guard);
                    None
                }
                Err(error) => Some(error),
            };
            wrappers::SetSecurityInfo(
                &mut metadata,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                original.dacl(),
                None,
            )
            .unwrap();
            #[cfg(debug_assertions)]
            {
                let observation = guard_refusal_observation(error.as_ref().unwrap()).unwrap();
                assert_eq!(observation.object(), "ancestor");
                assert_eq!(observation.predicate(), "foreign_mutation");
                assert_eq!(
                    observation.chain_index(),
                    ancestor
                        .ancestors()
                        .count()
                        .checked_sub(1)
                        .and_then(|index| u32::try_from(index).ok())
                );
                assert_eq!(observation.ace_index(), Some(2));
            }
            assert_eq!(error.unwrap().kind(), io::ErrorKind::PermissionDenied);
            assert!(unchanged);
            assert!(absent);
            assert!(empty);
            assert!(!ancestor.join("must remain absent").exists());
            assert!(fs::read_dir(&ancestor).unwrap().next().is_none());
            assert!(DirectoryGuard::existing_private(&ancestor).is_ok());
        }
    }

    #[test]
    fn common_ancestry_accepts_read_inherit_only_and_sibling_creation_without_acl_repair() {
        use std::os::windows::fs::OpenOptionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let ancestor = fs::canonicalize(temporary.path())
            .unwrap()
            .join("permitted sibling ancestor");
        drop(DirectoryGuard::private(&ancestor).unwrap());
        let sid = crate::windows::current_user_sid().unwrap();
        let mut metadata = OpenOptions::new()
            .access_mode(0x0006_0080)
            .share_mode(3)
            .custom_flags(0x0220_0000)
            .open(&ancestor)
            .unwrap();
        let original = wrappers::GetSecurityInfo(
            &metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .unwrap();
        for entry in [
            "(A;;GR;;;WD)",
            "(A;OICIIO;GA;;;WD)",
            "(A;;0x00000006;;;WD)",
            "(D;;0x00010000;;;WD)(A;;GR;;;WD)",
        ] {
            let altered: windows_permissions::LocalBox<windows_permissions::SecurityDescriptor> =
                format!("D:P{entry}(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)")
                    .parse()
                    .unwrap();
            wrappers::SetSecurityInfo(
                &mut metadata,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                altered.dacl(),
                None,
            )
            .unwrap();
            let descriptor = fixture_descriptor(&metadata);
            let accepted = DirectoryGuard::ancestors(&ancestor);
            let private_refused = DirectoryGuard::existing_private(&ancestor);
            let unchanged = fixture_descriptor(&metadata) == descriptor;
            wrappers::SetSecurityInfo(
                &mut metadata,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                original.dacl(),
                None,
            )
            .unwrap();
            drop(accepted.unwrap());
            assert_eq!(
                private_refused.unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
            assert!(unchanged);
            assert!(fs::read_dir(&ancestor).unwrap().next().is_none());
            assert!(DirectoryGuard::existing_private(&ancestor).is_ok());
        }
    }

    #[test]
    fn junction_in_any_ancestor_is_refused() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("target");
        let link = temporary.path().join("junction");
        fs::create_dir(&target).unwrap();
        crate::windows::run_script_json(
            "New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null; @{created=$true} | & $locronToJson -Compress",
            &json!({"link": link, "target": target}),
        ).unwrap();
        let result = DirectoryGuard::private(&link.join("private"));
        // Remove only the junction, never its target, before temporary cleanup.
        crate::windows::run_script_json(
            "[IO.Directory]::Delete([string]$request.link); @{removed=$true} | & $locronToJson -Compress",
            &json!({"link": link}),
        ).unwrap();
        assert!(result.is_err());
        assert!(!target.join("private").exists());
    }

    #[test]
    fn broad_existing_file_is_refused_before_truncation() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let path = root.join("foreign-access.txt");
        create_private_new(&path)
            .unwrap()
            .write_all(b"preserve")
            .unwrap();
        crate::windows::run_script_json(r"
            $acl = [IO.File]::GetAccessControl([string]$request.path);
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'Read', 'Allow'));
            [IO.File]::SetAccessControl([string]$request.path, $acl);
            @{changed=$true} | & $locronToJson -Compress
        ", &json!({"path": path})).unwrap();
        assert!(!is_private(&path, false).unwrap());
        assert!(open_private(&path, OpenOptions::new().write(true).truncate(true)).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
    }

    #[test]
    fn persistent_reader_bounds_rename_failure_and_preserves_source() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let source = root.join("output.partial");
        let destination = root.join("output.log");
        create_private_new(&source)
            .unwrap()
            .write_all(b"captured")
            .unwrap();
        let reader = open_private(&source, OpenOptions::new().read(true)).unwrap();
        let start = std::time::Instant::now();
        let error =
            rename_private_bounded(&source, &destination, std::time::Duration::from_millis(100))
                .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(32));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert!(!destination.exists());
        assert_eq!(fs::read(&source).unwrap(), b"captured");
        drop(reader);
        rename_private(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"captured");
    }

    #[test]
    fn caller_deadline_rename_waits_only_for_the_held_reader() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let source = root.join("caller.partial");
        let destination = root.join("caller.log");
        create_private_new(&source)
            .unwrap()
            .write_all(b"complete captured bytes")
            .unwrap();
        let reader = open_private(&source, OpenOptions::new().read(true)).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(reader);
        });
        let result = rename_private_until(
            &source,
            &destination,
            Instant::now() + Duration::from_secs(1),
        );
        release.join().unwrap();
        result.unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"complete captured bytes");
    }

    #[test]
    fn caller_deadline_rename_preserves_a_partial_and_never_creates_missing_parents() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let source = root.join("caller.partial");
        let destination = root.join("caller.log");
        create_private_new(&source)
            .unwrap()
            .write_all(b"captured")
            .unwrap();
        let reader = open_private(&source, OpenOptions::new().read(true)).unwrap();
        let entered = Instant::now();
        let error =
            rename_private_until(&source, &destination, entered + Duration::from_millis(80))
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(entered.elapsed() < Duration::from_secs(1));
        assert!(!destination.exists());
        assert_eq!(fs::read(&source).unwrap(), b"captured");
        drop(reader);
        let absent = root.join("absent").join("caller.log");
        assert_eq!(
            rename_private_until(&source, &absent, Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!absent.parent().unwrap().exists());
        assert_eq!(fs::read(&source).unwrap(), b"captured");
    }

    #[test]
    fn expired_caller_deadline_never_admits_a_native_rename() {
        let temporary = tempfile::tempdir().unwrap();
        let absent = temporary.path().join("absent");
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert_eq!(
            rename_private_until(&absent.join("source"), &absent.join("destination"), expired)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(!absent.exists());
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let source = root.join("caller.partial");
        let destination = root.join("caller.log");
        create_private_new(&source)
            .unwrap()
            .write_all(b"captured")
            .unwrap();
        assert_eq!(
            rename_private_until(&source, &destination, expired)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(fs::read(&source).unwrap(), b"captured");
        assert!(!destination.exists());
    }

    #[test]
    fn explicit_creation_refuses_existing_bytes_and_supports_long_unicode_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary
            .path()
            .join("private")
            .join("a".repeat(120))
            .join("b".repeat(120))
            .join("긴 경로");
        let guard = DirectoryGuard::private(&root).unwrap();
        let path = guard.normalized_path().join("secret.txt");
        assert!(path.as_os_str().len() > 260);
        let mut file = create_private_new(&path).unwrap();
        file.write_all(b"preserve").unwrap();
        drop(file);
        assert!(is_private(&path, false).unwrap());
        assert_eq!(
            create_private_new(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
    }
}
