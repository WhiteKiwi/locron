//! SID-independent trust and lifetime guards for the fixed Windows servicing binaries.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
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
    identity: file_id::FileId,
}

#[derive(Debug)]
struct StockLibrary {
    path: PathBuf,
    _native: StockFile,
}

/// Retained exact stock files and every ancestor, independent of current-SID discovery.
///
/// Acquire this value only in an admitted finite owner worker. Native I/O is uncancellable;
/// the result driver must not join a blocked owner or reuse its permit after timeout.
#[derive(Debug)]
pub struct StockAdapterGuard {
    powershell: StockFile,
    utility: StockLibrary,
    management: Option<StockLibrary>,
}

impl StockAdapterGuard {
    /// Guards only fixed stock locations against the caller's original absolute deadline.
    pub fn acquire_until(modules: StockModuleSet, deadline: Instant) -> io::Result<Self> {
        remaining(deadline)?;
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| io::Error::other("Windows SystemRoot is unavailable"))?;
        let powershell = guard_file(&powershell_path(&root), deadline)?;
        let utility = guard_library(&library_path(&root, "Utility"), deadline)?;
        let management = match modules {
            StockModuleSet::Utility => None,
            StockModuleSet::UtilityAndManagement => {
                Some(guard_library(&library_path(&root, "Management"), deadline)?)
            }
        };
        remaining(deadline)?;
        Ok(Self {
            powershell,
            utility,
            management,
        })
    }

    #[cfg(test)]
    pub(super) fn acquire_with_stall_until(
        modules: StockModuleSet,
        deadline: Instant,
        mut stall: GuardStall,
    ) -> io::Result<Self> {
        use std::io::Read;
        let guard = Self::acquire_until(modules, deadline)?;
        checked(deadline, || {
            stall.entered.try_send(()).map_err(io::Error::other)?;
            stall.reader.read_exact(&mut [0_u8; 1])
        })?;
        Ok(guard)
    }

    /// Exact retained executable. Keep this complete guard through child/Job/I/O cleanup.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.powershell.path
    }

    /// Verified DOS spelling of the retained Utility file for Framework assembly loading.
    #[must_use]
    pub fn utility(&self) -> &Path {
        &self.utility.path
    }

    /// Verified DOS spelling of retained Management; remove inherited data when absent.
    #[must_use]
    pub fn management(&self) -> Option<&Path> {
        self.management.as_ref().map(|file| file.path.as_path())
    }
}

#[cfg(test)]
pub(super) struct GuardStall {
    pub(super) reader: std::io::PipeReader,
    pub(super) entered: std::sync::mpsc::SyncSender<()>,
}

fn library_path(root: &Path, suffix: &str) -> PathBuf {
    let name = format!("Microsoft.PowerShell.Commands.{suffix}");
    root.join("Microsoft.NET")
        .join("assembly")
        .join("GAC_MSIL")
        .join(&name)
        .join(VERSION)
        .join(format!("{name}.dll"))
}

fn powershell_path(root: &Path) -> PathBuf {
    root.join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
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
    let (Prefix::Disk(volume) | Prefix::VerbatimDisk(volume)) = prefix.kind() else {
        return Err(unsafe_stock());
    };
    if components.next() != Some(Component::RootDir) {
        return Err(unsafe_stock());
    }
    // Verbatim names suppress slash normalization. Rebuild from validated components;
    // never prepend that prefix to an ordinary path's original slash-containing spelling.
    let mut absolute = PathBuf::from(format!(r"\\?\{}:\", char::from(volume)));
    for component in components {
        let Component::Normal(value) = component else {
            return Err(unsafe_stock());
        };
        if value
            .encode_wide()
            .any(|unit| unit == u16::from(b'/') || unit == u16::from(b'\\'))
        {
            return Err(unsafe_stock());
        }
        absolute.push(value);
    }
    Ok(absolute)
}

/// Framework's `CodeBase` parser treats a verbatim prefix as a UNC file URI. Convert only
/// canonical native local paths whose components do not change under ordinary DOS parsing.
fn framework_local(path: &Path) -> io::Result<PathBuf> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(unsafe_stock());
    };
    let Prefix::VerbatimDisk(volume) = prefix.kind() else {
        return Err(unsafe_stock());
    };
    if components.next() != Some(Component::RootDir) {
        return Err(unsafe_stock());
    }
    let mut ordinary = PathBuf::from(format!("{}:\\", char::from(volume)));
    for component in components {
        let Component::Normal(value) = component else {
            return Err(unsafe_stock());
        };
        let name = value.to_str().ok_or_else(unsafe_stock)?;
        if name.ends_with(['.', ' '])
            || name.chars().any(|ch| {
                ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
            })
        {
            return Err(unsafe_stock());
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
            return Err(unsafe_stock());
        }
        ordinary.push(value);
    }
    if ordinary.file_name().is_none() {
        return Err(unsafe_stock());
    }
    Ok(ordinary)
}

fn guard_library(path: &Path, deadline: Instant) -> io::Result<StockLibrary> {
    let stock = guard_file(path, deadline)?;
    let framework = framework_local(&stock.path)?;
    let identity = checked(deadline, || file_id::get_high_res_file_id(&framework))?;
    if identity != stock.identity {
        return Err(unsafe_stock());
    }
    remaining(deadline)?;
    Ok(StockLibrary {
        path: framework,
        _native: stock,
    })
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
        identity,
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
    use super::{
        StockAdapterGuard, StockLibrary, StockModuleSet, absolute_local, framework_local,
        library_path, powershell_path, verify_descriptor,
    };
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use windows_permissions::{LocalBox, SecurityDescriptor};

    #[test]
    fn stock_paths_rebuild_native_components_before_verbatim_opens() {
        for root in [
            r"C:\Windows",
            "C:/Windows",
            r"C:/윈도우\Stock",
            r"\\?\C:\Windows",
        ] {
            for path in [
                powershell_path(Path::new(root)),
                library_path(Path::new(root), "Utility"),
                library_path(Path::new(root), "Management"),
            ] {
                let absolute = absolute_local(&path).unwrap();
                assert_eq!(absolute_local(&absolute).unwrap(), absolute);
                for ancestor in absolute.ancestors() {
                    assert!(
                        !ancestor
                            .as_os_str()
                            .encode_wide()
                            .any(|unit| unit == u16::from(b'/'))
                    );
                }
            }
        }
        let expected =
            PathBuf::from(r"\\?\C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        assert_eq!(
            absolute_local(Path::new(
                "C:/Windows\\System32/WindowsPowerShell\\v1.0/powershell.exe"
            ))
            .unwrap(),
            expected
        );
        assert_eq!(
            absolute_local(&powershell_path(Path::new("C:/Windows"))).unwrap(),
            expected
        );
        assert_eq!(
            absolute_local(Path::new("C:/윈도우\\Stock/파일.dll")).unwrap(),
            PathBuf::from(r"\\?\C:\윈도우\Stock\파일.dll")
        );
        for rejected in [
            r"Windows\stock.dll",
            r"C:Windows\stock.dll",
            r"\Windows\stock.dll",
            r"\\server\share\stock.dll",
            r"\\?\UNC\server\share\stock.dll",
            r"\\.\C:\Windows\stock.dll",
            r"C:\Windows\..\stock.dll",
            r"\\?\C:\Windows/stock.dll",
            r"\\?\C:\Windows\..\stock.dll",
        ] {
            assert!(absolute_local(Path::new(rejected)).is_err(), "{rejected}");
        }
    }

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
    fn framework_spelling_preserves_canonical_local_components_without_uri_parsing() {
        for native in [
            r"\\?\C:\Windows\Microsoft.NET\assembly\Utility.dll",
            r"\\?\D:\윈도우 日本語\100% # $ & ` [test]\Utility.dll",
        ] {
            let ordinary = framework_local(Path::new(native)).unwrap();
            assert_eq!(ordinary, PathBuf::from(&native[4..]));
            assert_eq!(absolute_local(&ordinary).unwrap(), PathBuf::from(native));
        }
        for rejected in [
            "relative.dll",
            r"C:\Windows\Utility.dll",
            r"C:Utility.dll",
            r"\\server\share\Utility.dll",
            r"\\?\UNC\server\share\Utility.dll",
            r"\\.\C:\Windows\Utility.dll",
            "file:///C:/Windows/Utility.dll",
            r"\\?\C:\",
            r"\\?\C:\Windows\..\Utility.dll",
            r"\\?\C:\Windows\.\Utility.dll",
            r"\\?\C:\Windows \Utility.dll",
            r"\\?\C:\Windows.\Utility.dll",
            r"\\?\C:\Windows\Utility.dll:stream",
            r"\\?\C:\Windows\Utility/other.dll",
            r"\\?\C:\Windows\NUL.dll",
            r"\\?\C:\CON\Utility.dll",
            r"\\?\C:\Windows\com1.dll",
            r"\\?\C:\Windows\LPT².dll",
            r"\\?\C:\Windows\CONOUT$.dll",
            "\\\\?\\C:\\Windows\\bad\u{1f}name.dll",
        ] {
            assert!(framework_local(Path::new(rejected)).is_err(), "{rejected}");
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
                StockAdapterGuard::acquire_until(StockModuleSet::UtilityAndManagement, deadline)
                    .and_then(|guard| {
                        for library in [&guard.utility, guard.management.as_ref().unwrap()] {
                            let StockLibrary {
                                _native: native,
                                path: framework,
                            } = library;
                            let identity = super::checked(deadline, || {
                                file_id::get_high_res_file_id(framework)
                            })?;
                            let canonical =
                                super::checked(deadline, || std::fs::canonicalize(framework))?;
                            if identity != native.identity || canonical != native.path {
                                return Err(super::unsafe_stock());
                            }
                        }
                        Ok(guard)
                    });
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
        for library in [&guard.utility, guard.management.as_ref().unwrap()] {
            let StockLibrary {
                _native: native,
                path: framework,
            } = library;
            assert!(matches!(native.identity, file_id::FileId::HighRes { .. }));
            assert!(
                !framework
                    .as_os_str()
                    .encode_wide()
                    .collect::<Vec<_>>()
                    .starts_with(&[92, 92])
            );
        }
    }
}
