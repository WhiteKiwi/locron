//! Shared private-file and no-reparse access primitives.
//!
//! A successful check is tied to retained handles. Keep the returned guard alive for
//! the complete path-based operation, including SQLite sidecars and file finalization.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

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
            windows::guard_directory(path, true)
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

    /// Guards an existing directory chain without requiring a private leaf.
    pub fn ancestors(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            windows::guard_directory(path, false)
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

/// A file whose parent-chain guards live as long as the file.
#[derive(Debug)]
pub struct GuardedFile {
    file: File,
    guard: DirectoryGuard,
}

impl GuardedFile {
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

/// Opens a managed data file only after private parent and no-follow leaf checks.
pub fn open_private(path: &Path, options: &mut OpenOptions) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::private(parent(path)?)?;
    open_with_guard(path, options, guard, true)
}

/// Opens a user-selected input without traversing a reparse point or symlink.
/// Inputs need not carry a managed state-file descriptor.
pub fn open_read_no_follow(path: &Path) -> io::Result<GuardedFile> {
    let guard = DirectoryGuard::ancestors(parent(path)?)?;
    open_with_guard(path, OpenOptions::new().read(true), guard, false)
}

fn open_with_guard(
    path: &Path,
    options: &mut OpenOptions,
    guard: DirectoryGuard,
    private: bool,
) -> io::Result<GuardedFile> {
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
        return Err(unsafe_path(path));
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
    Ok(GuardedFile { file, guard })
}

/// Renames a private managed file while both directory chains remain guarded.
pub fn rename_private(source: &Path, destination: &Path) -> io::Result<()> {
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
            return Ok(
                (metadata.permissions().mode() & 0o777).trailing_zeros() >= 6
                    && metadata.is_dir() == directory,
            );
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
    use super::*;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Component, Prefix};
    use windows_permissions::constants::{AceType, SeObjectType, SecurityInformation};
    use windows_permissions::{LocalBox, SecurityDescriptor, wrappers};

    const READ_CONTROL: u32 = 0x0002_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FULL_CONTROL: u32 = 0x001f_01ff;
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
            return Err(unsafe_path(path));
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
        Ok(normalized)
    }

    fn directory_handle(path: &Path, repair: bool) -> io::Result<File> {
        OpenOptions::new().access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES | if repair { WRITE_DAC } else { 0 })
            // No delete sharing prevents rename; no write sharing prevents reparse mutation.
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).open(path)
    }

    pub(super) fn reject_reparse(file: &File, path: &Path) -> io::Result<()> {
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(unsafe_path(path));
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

    fn trusted_owner(file: &File, path: &Path, sid: &str) -> io::Result<()> {
        let descriptor = descriptor(file)?;
        let owner = descriptor
            .owner()
            .ok_or_else(|| unsafe_path(path))?
            .to_string();
        if owner != sid && ![SYSTEM_SID, ADMIN_SID, INSTALLER_SID].contains(&owner.as_str()) {
            return Err(unsafe_path(path));
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
            let sddl = wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
                &descriptor,
                SecurityInformation::Dacl,
            )?;
            sddl.to_string_lossy()
                .strip_prefix("D:")
                .and_then(|sddl| sddl.split('(').next())
                .is_some_and(|flags| flags.contains('P'))
        } else {
            true
        };
        Ok(user && system && protected)
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

    pub(super) fn guard_directory(path: &Path, private: bool) -> io::Result<DirectoryGuard> {
        let absolute = normalized_absolute(path)?;
        let sid = crate::windows::current_user_sid()?;
        let chain = absolute.ancestors().collect::<Vec<_>>();
        let mut handles = Vec::with_capacity(chain.len());
        let mut created = false;
        for component in chain.iter().rev() {
            let file = match directory_handle(component, false) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound && private => {
                    crate::windows::create_private_directory(component)?;
                    created = true;
                    directory_handle(component, false)?
                }
                Err(error) => return Err(error),
            };
            reject_reparse(&file, component)?;
            if !file.metadata()?.is_dir() {
                return Err(unsafe_path(component));
            }
            trusted_owner(&file, component, &sid)?;
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

    #[test]
    fn created_private_root_and_file_have_real_acl_facts() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("비공개 state");
        let guard = DirectoryGuard::private(&root).unwrap();
        assert!(is_private(&root, true).unwrap());
        let path = root.join("secret.txt");
        let mut file =
            open_private(&path, OpenOptions::new().write(true).create_new(true)).unwrap();
        use std::io::Write;
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
        crate::windows::run_script_json(r#"
            $acl = Get-Acl -LiteralPath ([string]$request.path);
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'Read', 'Allow'));
            Set-Acl -LiteralPath ([string]$request.path) -AclObject $acl;
            @{changed=$true} | ConvertTo-Json -Compress
        "#, &json!({"path": root})).unwrap();
        assert!(!is_private(&root, true).unwrap());
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
    fn junction_in_any_ancestor_is_refused() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("target");
        let link = temporary.path().join("junction");
        fs::create_dir(&target).unwrap();
        crate::windows::run_script_json(
            "New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null; @{created=$true} | ConvertTo-Json -Compress",
            &json!({"link": link, "target": target}),
        ).unwrap();
        let result = DirectoryGuard::private(&link.join("private"));
        // Remove only the junction, never its target, before temporary cleanup.
        crate::windows::run_script_json(
            "[IO.Directory]::Delete([string]$request.link); @{removed=$true} | ConvertTo-Json -Compress",
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
        fs::write(&path, b"preserve").unwrap();
        crate::windows::run_script_json(r#"
            $acl = Get-Acl -LiteralPath ([string]$request.path);
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'Read', 'Allow'));
            Set-Acl -LiteralPath ([string]$request.path) -AclObject $acl;
            @{changed=$true} | ConvertTo-Json -Compress
        "#, &json!({"path": path})).unwrap();
        assert!(!is_private(&path, false).unwrap());
        assert!(open_private(&path, OpenOptions::new().write(true).truncate(true)).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
    }
}
