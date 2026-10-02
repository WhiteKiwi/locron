//! SID-independent trust and lifetime guards for the fixed Windows servicing binaries.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf, Prefix};
use std::time::Instant;

use windows_permissions::constants::{AceType, SeObjectType, SecurityInformation};
use windows_permissions::{SecurityDescriptor, wrappers};

use super::remaining;

const TRUSTED: [&str; 3] = [
    "S-1-5-18",
    "S-1-5-32-544",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];
const GAC: &str = "Microsoft.NET/assembly/GAC_MSIL";
const VERSION: &str = "v4.0_3.0.0.0__31bf3856ad364e35";
const READ_CONTROL_ATTRIBUTES: u32 = 0x0002_0080;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const BACKUP_SEMANTICS: u32 = 0x0200_0000;
const REPARSE_POINT: u32 = 0x400;
const FILE_MUTATION: u32 = 0x500d_0156;
// Sibling creation is not mutation of this existing directory object or its retained child.
const DIRECTORY_MUTATION: u32 = FILE_MUTATION & !0x06;

/// Fixed library set, never an arbitrary module name/path supplied by request data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StockModuleSet {
    /// Structured filesystem or Task Scheduler operations need only Utility cmdlets.
    Utility,
    /// Reviewed generic junction/registry scripts also need binary Management cmdlets.
    UtilityAndManagement,
}

#[derive(Debug)]
struct StockFile {
    path: PathBuf,
    _file: File,
    _ancestors: Vec<File>,
    _identity: file_id::FileId,
}

/// Retained exact stock files and every ancestor, independent of current-SID discovery.
///
/// Acquire this value only in an admitted finite owner worker. Native I/O is uncancellable;
/// the result driver must not join a blocked owner or reuse its permit after timeout.
#[derive(Debug)]
pub struct StockAdapterGuard {
    powershell: StockFile,
    utility: StockFile,
    management: Option<StockFile>,
}

impl StockAdapterGuard {
    /// Guards only fixed stock locations against the caller's original absolute deadline.
    pub fn acquire_until(modules: StockModuleSet, deadline: Instant) -> io::Result<Self> {
        remaining(deadline)?;
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| io::Error::other("Windows SystemRoot is unavailable"))?;
        let powershell = guard_file(
            &root.join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            deadline,
        )?;
        let utility = guard_file(&library_path(&root, "Utility"), deadline)?;
        let management = match modules {
            StockModuleSet::Utility => None,
            StockModuleSet::UtilityAndManagement => {
                Some(guard_file(&library_path(&root, "Management"), deadline)?)
            }
        };
        remaining(deadline)?;
        Ok(Self {
            powershell,
            utility,
            management,
        })
    }

    /// Exact retained executable. Keep this complete guard through child/Job/I/O cleanup.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.powershell.path
    }

    /// Exact retained Utility path supplied as data to the fixed binary bootstrap.
    #[must_use]
    pub fn utility(&self) -> &Path {
        &self.utility.path
    }

    /// Optional exact retained Management path; remove inherited data when absent.
    #[must_use]
    pub fn management(&self) -> Option<&Path> {
        self.management.as_ref().map(|file| file.path.as_path())
    }
}

fn library_path(root: &Path, suffix: &str) -> PathBuf {
    let name = format!("Microsoft.PowerShell.Commands.{suffix}");
    root.join(GAC)
        .join(&name)
        .join(VERSION)
        .join(format!("{name}.dll"))
}

fn checked<T>(deadline: Instant, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    remaining(deadline)?;
    let result = operation();
    remaining(deadline)?;
    result
}

fn unsafe_stock() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "unsafe stock Windows servicing path",
    )
}

fn absolute_local(path: &Path) -> io::Result<PathBuf> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(unsafe_stock());
    };
    if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        || components.next() != Some(Component::RootDir)
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(unsafe_stock());
    }
    if matches!(prefix.kind(), Prefix::Disk(_)) {
        let mut absolute = std::ffi::OsString::from(r"\\?\");
        absolute.push(path);
        Ok(PathBuf::from(absolute))
    } else {
        Ok(path.to_path_buf())
    }
}

fn guard_file(path: &Path, deadline: Instant) -> io::Result<StockFile> {
    remaining(deadline)?;
    let path = absolute_local(path)?;
    let parent = path.parent().ok_or_else(unsafe_stock)?;
    let mut ancestors = Vec::new();
    for component in parent.ancestors().collect::<Vec<_>>().iter().rev() {
        let file = checked(deadline, || {
            OpenOptions::new()
                .access_mode(READ_CONTROL_ATTRIBUTES)
                .share_mode(1)
                .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
                .open(component)
        })?;
        verify_file(&file, true, deadline)?;
        ancestors.push(file);
    }
    let file = checked(deadline, || {
        OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(OPEN_REPARSE_POINT)
            .open(&path)
    })?;
    verify_file(&file, false, deadline)?;
    // All queried path components/leaf remain protected against write/delete while queried.
    let path = checked(deadline, || fs::canonicalize(&path))?;
    let identity = checked(deadline, || file_id::get_high_res_file_id(&path))?;
    if !matches!(identity, file_id::FileId::HighRes { .. }) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "stock file identity is unavailable",
        ));
    }
    Ok(StockFile {
        path,
        _file: file,
        _ancestors: ancestors,
        _identity: identity,
    })
}

fn verify_file(file: &File, directory: bool, deadline: Instant) -> io::Result<()> {
    let metadata = checked(deadline, || file.metadata())?;
    if metadata.file_attributes() & REPARSE_POINT != 0 || metadata.is_dir() != directory {
        return Err(unsafe_stock());
    }
    let descriptor = checked(deadline, || {
        wrappers::GetSecurityInfo(
            file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
    })?;
    checked(deadline, || verify_descriptor(&descriptor, directory))
}

fn verify_descriptor(descriptor: &SecurityDescriptor, directory: bool) -> io::Result<()> {
    let owner = descriptor.owner().ok_or_else(unsafe_stock)?.to_string();
    if !TRUSTED.contains(&owner.as_str()) {
        return Err(unsafe_stock());
    }
    let acl = descriptor.dacl().ok_or_else(unsafe_stock)?;
    let mutation = if directory {
        DIRECTORY_MUTATION
    } else {
        FILE_MUTATION
    };
    for index in 0..acl.len() {
        let ace = acl.get_ace(index).ok_or_else(unsafe_stock)?;
        if ace.ace_type() == AceType::ACCESS_DENIED_ACE_TYPE {
            continue;
        }
        if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE {
            return Err(unsafe_stock());
        }
        if ace.flags().bits() & 0x08 != 0 {
            continue;
        }
        let principal = ace.sid().ok_or_else(unsafe_stock)?.to_string();
        if !TRUSTED.contains(&principal.as_str()) && ace.mask().bits() & mutation != 0 {
            return Err(unsafe_stock());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{StockAdapterGuard, StockModuleSet, verify_descriptor};
    use std::time::{Duration, Instant};
    use windows_permissions::{LocalBox, SecurityDescriptor};

    #[test]
    fn servicing_trust_refuses_foreign_mutation_but_preserves_readers_and_siblings() {
        for (sddl, directory, permitted) in [
            ("O:SYD:(A;;FA;;;SY)(A;;FR;;;WD)", false, true),
            ("O:BAD:(A;;FA;;;BA)(A;;0x4;;;WD)", true, true),
            ("O:SYD:(A;;FA;;;SY)(A;;0x2;;;WD)", false, false),
            ("O:SYD:(A;;FA;;;SY)(A;;0x40;;;WD)", true, false),
            ("O:SYD:(A;;FA;;;SY)(A;;0x100;;;WD)", true, false),
            ("O:SYD:(A;;FA;;;SY)(A;;WDWO;;;WD)", true, false),
            ("O:WDD:(A;;FR;;;WD)", false, false),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = sddl.parse().unwrap();
            assert_eq!(
                verify_descriptor(&descriptor, directory).is_ok(),
                permitted,
                "{sddl}"
            );
        }
    }

    #[test]
    fn actual_stock_files_have_full_guarded_identity_without_sid_dependency() {
        let deadline = Instant::now() + Duration::from_secs(30);
        let permit = super::super::ADAPTER_WORKERS.acquire(deadline).unwrap();
        let (reply, receiver) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let _permit = permit;
            let guard =
                StockAdapterGuard::acquire_until(StockModuleSet::UtilityAndManagement, deadline);
            let _ = reply.send(guard);
        });
        let guard = receiver
            .recv_timeout(
                (deadline + Duration::from_secs(3)).saturating_duration_since(Instant::now()),
            )
            .unwrap()
            .unwrap();
        super::remaining(deadline).unwrap();
        if worker.is_finished() {
            worker.join().unwrap();
        }
        assert!(guard.executable().ends_with("powershell.exe"));
        assert!(
            guard
                .utility()
                .ends_with("Microsoft.PowerShell.Commands.Utility.dll")
        );
        assert!(
            guard
                .management()
                .unwrap()
                .ends_with("Microsoft.PowerShell.Commands.Management.dll")
        );
        assert!(matches!(
            guard.utility._identity,
            file_id::FileId::HighRes { .. }
        ));
    }
}
