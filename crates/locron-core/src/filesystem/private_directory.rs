//! Existing-only fresh-root inspection with retained native ancestry.

use std::fs;
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use windows_permissions::constants::{AceType, SeObjectType, SecurityInformation};
use windows_permissions::{SecurityDescriptor, wrappers};

use super::{DirectoryGuard, FileIdentity, open_directory_guard_handle};

const PATH_UNITS: usize = 4096;
const SYSTEM: &str = "S-1-5-18";
const ADMIN: &str = "S-1-5-32-544";
const INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
const REPARSE_POINT: u32 = 0x400;
const FULL_CONTROL: u32 = 0x001f_01ff;
// Creating a sibling is distinct from mutating a retained existing directory object/child.
const DIRECTORY_MUTATION: u32 = 0x500d_0156 & !0x06;

/// A passive fresh-root preflight, never a creation or rollback receipt.
///
/// The existing prefix remains guarded against write/delete sharing and reparse changes.
/// Missing components are observations only; a later creator must refuse unknown new objects.
#[derive(Debug)]
pub struct PrivateDirectoryPlan {
    path: PathBuf,
    existing: DirectoryGuard,
    identity: FileIdentity,
    root: Option<FileIdentity>,
    missing: Vec<String>,
}

impl PrivateDirectoryPlan {
    /// Inspects an existing private root or its missing suffix without creating or repairing it.
    ///
    /// Call only inside an admitted finite owner worker. Native I/O cannot be cancelled by a
    /// deadline; an expired driver must retain that worker and its handles without joining it.
    pub fn inspect_until(path: &Path, deadline: Instant) -> io::Result<Self> {
        inspect_observed_until(path, deadline, |_| Ok(()))
    }

    /// Canonical retained existing prefix joined with the validated still-missing components.
    #[must_use]
    pub fn normalized_path(&self) -> &Path {
        &self.path
    }

    /// The complete retained existing directory chain, which can precede a missing root.
    #[must_use]
    pub fn existing_guard(&self) -> &DirectoryGuard {
        &self.existing
    }

    /// Full volume/file identity of the retained nearest existing directory.
    #[must_use]
    pub fn existing_identity(&self) -> FileIdentity {
        self.identity
    }

    /// Full identity only when the final root already exists with verified private permissions.
    #[must_use]
    pub fn root_identity(&self) -> Option<FileIdentity> {
        self.root
    }

    /// Ordered absence observations, never authority to adopt or remove a later directory.
    #[must_use]
    pub fn missing_components(&self) -> &[String] {
        &self.missing
    }
}

fn inspect_observed_until(
    path: &Path,
    deadline: Instant,
    observe: impl FnOnce(&DirectoryGuard) -> io::Result<()>,
) -> io::Result<PrivateDirectoryPlan> {
    admit(deadline)?;
    let absolute = validated_absolute(path)?;
    admit(deadline)?;
    let sid = crate::windows::current_user_sid_until(deadline)?;
    admit(deadline)?;
    let ancestors = absolute.ancestors().collect::<Vec<_>>();
    let chain = ancestors.into_iter().rev().collect::<Vec<_>>();
    let mut handles = Vec::with_capacity(chain.len());
    let mut missing = Vec::new();
    let mut existing_path = None;
    for (index, component) in chain.iter().enumerate() {
        let file = match checked(deadline, || open_directory_guard_handle(component)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound && !handles.is_empty() => {
                for absent in &chain[index..] {
                    let name = absent
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(invalid_path)?;
                    missing.push(name.to_owned());
                }
                break;
            }
            Err(error) => return Err(error),
        };
        let metadata = checked(deadline, || file.metadata())?;
        if !metadata.is_dir() || metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(untrusted_directory());
        }
        let descriptor = checked(deadline, || {
            wrappers::GetSecurityInfo(
                &file,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Owner | SecurityInformation::Dacl,
            )
        })?;
        validate_descriptor(&descriptor, &sid, *component == absolute, deadline)?;
        handles.push(file);
        existing_path = Some(*component);
    }
    let existing_path = existing_path.ok_or_else(invalid_path)?;
    let canonical = checked(deadline, || fs::canonicalize(existing_path))?;
    let existing = DirectoryGuard {
        path: validated_absolute(&canonical)?,
        _handles: handles,
    };
    checked(deadline, || observe(&existing))?;
    let identity = checked(deadline, || full_identity(existing.normalized_path()))?;
    let mut normalized = existing.normalized_path().to_owned();
    for component in &missing {
        normalized.push(component);
    }
    let path = validated_absolute(&normalized)?;
    admit(deadline)?;
    Ok(PrivateDirectoryPlan {
        path,
        existing,
        identity,
        root: missing.is_empty().then_some(identity),
        missing,
    })
}

fn admit(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "private directory preflight deadline elapsed; native observation may remain pending",
        ))
    } else {
        Ok(())
    }
}

fn checked<T>(deadline: Instant, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    admit(deadline)?;
    let result = operation();
    admit(deadline)?;
    result
}

fn invalid_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "private directory preflight requires a normalized local path within 4096 UTF-16 units",
    )
}

fn untrusted_directory() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "private directory preflight ancestry is mutable, untrusted or not a private final root",
    )
}

fn validated_absolute(path: &Path) -> io::Result<PathBuf> {
    let value = path.to_str().ok_or_else(invalid_path)?;
    let native_prefix = if value.starts_with(r"\\?\") { 0 } else { 4 };
    if value.encode_utf16().take(PATH_UNITS + 1).count() + native_prefix > PATH_UNITS {
        return Err(invalid_path());
    }
    let drive = value.strip_prefix(r"\\?\").unwrap_or(value);
    let bytes = drive.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1..3] != *b":\\" {
        return Err(invalid_path());
    }
    let mut absolute = PathBuf::from(format!(r"\\?\{}:\", char::from(bytes[0])));
    let tail = &drive[3..];
    if !tail.is_empty() {
        for component in tail.split('\\') {
            validate_component(component)?;
            absolute.push(component);
        }
    }
    Ok(absolute)
}

fn validate_component(component: &str) -> io::Result<()> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with(['.', ' '])
        || component.chars().any(|ch| {
            ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
        })
    {
        return Err(invalid_path());
    }
    let device = component
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
        return Err(invalid_path());
    }
    Ok(())
}

fn trusted(principal: &str, sid: &str) -> bool {
    principal == sid || matches!(principal, SYSTEM | ADMIN | INSTALLER)
}

fn validate_descriptor(
    descriptor: &SecurityDescriptor,
    sid: &str,
    private: bool,
    deadline: Instant,
) -> io::Result<()> {
    let owner = checked(deadline, || {
        descriptor
            .owner()
            .map(ToString::to_string)
            .ok_or_else(untrusted_directory)
    })?;
    if !trusted(&owner, sid) || private && owner != sid {
        return Err(untrusted_directory());
    }
    let acl = checked(deadline, || {
        descriptor.dacl().ok_or_else(untrusted_directory)
    })?;
    let mut user = false;
    let mut system = false;
    for index in 0..acl.len() {
        let ace = checked(deadline, || {
            acl.get_ace(index).ok_or_else(untrusted_directory)
        })?;
        if !private && ace.ace_type() == AceType::ACCESS_DENIED_ACE_TYPE {
            continue;
        }
        if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE {
            return Err(untrusted_directory());
        }
        let inherit_only = ace.flags().bits() & 0x08 != 0;
        if !private && inherit_only {
            continue;
        }
        let principal = checked(deadline, || {
            ace.sid()
                .map(ToString::to_string)
                .ok_or_else(untrusted_directory)
        })?;
        if private {
            if (principal != sid && principal != SYSTEM)
                || inherit_only
                || ace.flags().bits() & 0x03 != 0x03
                || ace.mask().bits() & FULL_CONTROL != FULL_CONTROL
            {
                return Err(untrusted_directory());
            }
            user |= principal == sid;
            system |= principal == SYSTEM;
        } else if !trusted(&principal, sid) && ace.mask().bits() & DIRECTORY_MUTATION != 0 {
            return Err(untrusted_directory());
        }
    }
    if private {
        let sddl = checked(deadline, || {
            wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
                descriptor,
                SecurityInformation::Dacl,
            )
        })?;
        let protected = sddl
            .to_string_lossy()
            .strip_prefix("D:")
            .and_then(|dacl| dacl.split('(').next())
            .is_some_and(|flags| flags.contains('P'));
        if !(user && system && protected) {
            return Err(untrusted_directory());
        }
    }
    admit(deadline)
}

fn full_identity(path: &Path) -> io::Result<FileIdentity> {
    let file_id::FileId::HighRes {
        volume_serial_number,
        file_id,
    } = file_id::get_high_res_file_id(path)?
    else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private directory preflight requires full filesystem object identity",
        ));
    };
    Ok(FileIdentity {
        volume_serial_number,
        file_id,
    })
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::sync::mpsc;
    use std::time::Duration;

    use serde_json::json;
    use windows_permissions::LocalBox;

    use super::{
        DirectoryGuard, Instant, PATH_UNITS, Path, PathBuf, PrivateDirectoryPlan, REPARSE_POINT,
        SeObjectType, SecurityDescriptor, SecurityInformation, fs, full_identity,
        inspect_observed_until, io, validate_descriptor, validated_absolute, wrappers,
    };

    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    fn private_fixture() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path())
            .unwrap()
            .join("private 日本語 % #");
        drop(DirectoryGuard::private(&root).unwrap());
        (temporary, root)
    }

    fn replace_dacl(path: &Path, extra: &str) {
        let sid = crate::windows::current_user_sid().unwrap();
        let security: LocalBox<SecurityDescriptor> =
            format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY){extra}")
                .parse()
                .unwrap();
        let mut file = OpenOptions::new()
            .access_mode(0x0006_0080)
            .share_mode(3)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
        wrappers::SetSecurityInfo(
            &mut file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            security.dacl(),
            None,
        )
        .unwrap();
    }

    fn descriptor_text(path: &Path) -> std::ffi::OsString {
        let file = OpenOptions::new()
            .access_mode(0x0002_0080)
            .share_mode(3)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
        let information = SecurityInformation::Owner | SecurityInformation::Dacl;
        let descriptor =
            wrappers::GetSecurityInfo(&file, SeObjectType::SE_FILE_OBJECT, information).unwrap();
        wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(&descriptor, information)
            .unwrap()
    }

    #[test]
    fn invalid_shapes_and_normalized_utf16_overflow_refuse_before_native_inspection() {
        for invalid in [
            "relative\\root",
            r"C:relative",
            r"C:/root",
            r"\\server\share\root",
            r"\\?\UNC\server\share\root",
            r"\\.\C:\root",
            r"C:\root\..\other",
            r"C:\root\.\other",
            r"C:\root\\other",
            "C:\\root\\",
            r"C:\root\trailing.",
            r"C:\root\trailing ",
            r"C:\root\alternate:stream",
            r"C:\root\NUL.txt",
            r"C:\root\COM¹",
            "C:\\root\\control\u{1f}",
        ] {
            assert_eq!(
                validated_absolute(Path::new(invalid)).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "invalid path shape was accepted"
            );
            assert_eq!(
                PrivateDirectoryPlan::inspect_until(
                    Path::new(invalid),
                    Instant::now() + Duration::from_secs(30)
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let invalid_utf16 = std::ffi::OsString::from_wide(&[
            u16::from(b'C'),
            u16::from(b':'),
            u16::from(b'\\'),
            0xd800,
        ]);
        assert!(validated_absolute(Path::new(&invalid_utf16)).is_err());
        let mut bounded = String::from(r"\\?\C:\");
        while bounded.encode_utf16().count() + 201 < PATH_UNITS {
            bounded.push_str(&"x".repeat(200));
            bounded.push('\\');
        }
        bounded.push_str(&"x".repeat(PATH_UNITS - bounded.encode_utf16().count()));
        assert_eq!(bounded.encode_utf16().count(), PATH_UNITS);
        assert_eq!(
            validated_absolute(Path::new(&bounded)).unwrap(),
            PathBuf::from(&bounded)
        );
        let ordinary = bounded.strip_prefix(r"\\?\").unwrap();
        assert_eq!(
            validated_absolute(Path::new(ordinary)).unwrap(),
            PathBuf::from(&bounded)
        );
        for overlong in [format!("{bounded}x"), format!("{ordinary}x")] {
            assert_eq!(
                PrivateDirectoryPlan::inspect_until(
                    Path::new(&overlong),
                    Instant::now() + Duration::from_secs(30)
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let unicode = validated_absolute(Path::new(r"C:\한국 日本語\😀 % #")).unwrap();
        assert_eq!(unicode, PathBuf::from(r"\\?\C:\한국 日本語\😀 % #"));
    }

    #[test]
    fn existing_and_missing_long_unicode_roots_preserve_exact_identity_and_absence() {
        let (_temporary, root) = private_fixture();
        let expected = full_identity(&root).unwrap();
        let present =
            PrivateDirectoryPlan::inspect_until(&root, Instant::now() + Duration::from_secs(30))
                .unwrap();
        assert_eq!(present.normalized_path(), fs::canonicalize(&root).unwrap());
        assert_eq!(present.root_identity(), Some(expected));
        assert_eq!(present.existing_identity(), expected);
        assert!(present.missing_components().is_empty());
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        drop(present);

        let missing = [
            "欠".repeat(110),
            "漢".repeat(110),
            "leaf 日本語 % #".to_owned(),
        ];
        let mut target = root.clone();
        for component in &missing {
            target.push(component);
        }
        assert!(target.to_str().unwrap().encode_utf16().count() > 260);
        let plan =
            PrivateDirectoryPlan::inspect_until(&target, Instant::now() + Duration::from_secs(30))
                .unwrap();
        assert_eq!(plan.existing_identity(), expected);
        assert_eq!(plan.root_identity(), None);
        assert_eq!(plan.missing_components(), missing);
        assert_eq!(
            plan.existing_guard().normalized_path(),
            fs::canonicalize(&root).unwrap()
        );
        assert_eq!(plan.normalized_path(), target);
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        drop(plan);
        assert!(!root.join(&missing[0]).exists());
        assert!(!target.exists());
        assert_eq!(full_identity(&root).unwrap(), expected);
    }

    #[test]
    fn broad_root_and_foreign_mutable_ancestor_refuse_without_descriptor_repair() {
        let (_temporary, root) = private_fixture();
        replace_dacl(&root, "(A;OICI;FR;;;WD)");
        let broad = descriptor_text(&root);
        let error =
            PrivateDirectoryPlan::inspect_until(&root, Instant::now() + Duration::from_secs(30))
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(descriptor_text(&root), broad);
        assert!(fs::read_dir(&root).unwrap().next().is_none());

        replace_dacl(&root, "(A;;WD;;;WD)");
        let mutable = descriptor_text(&root);
        let missing = root.join("still missing").join("private");
        let error =
            PrivateDirectoryPlan::inspect_until(&missing, Instant::now() + Duration::from_secs(30))
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(descriptor_text(&root), mutable);
        assert!(!root.join("still missing").exists());
    }

    #[test]
    fn list_denied_existing_prefix_refuses_without_repair_or_missing_suffix_creation() {
        let (_temporary, ancestor) = private_fixture();
        let sid = crate::windows::current_user_sid().unwrap();
        let mut metadata = OpenOptions::new()
            .access_mode(0x0006_0080)
            .share_mode(3)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
            .open(&ancestor)
            .unwrap();
        let original = wrappers::GetSecurityInfo(
            &metadata,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .unwrap();
        let denied: LocalBox<SecurityDescriptor> =
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
        let before = descriptor_text(&ancestor);
        let missing = ancestor.join("missing suffix").join("private");
        let refused = PrivateDirectoryPlan::inspect_until(
            &missing,
            Instant::now().checked_add(Duration::from_secs(30)).unwrap(),
        );
        let unchanged = descriptor_text(&ancestor) == before;
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
        assert!(unchanged);
        assert!(!ancestor.join("missing suffix").exists());
        assert!(fs::read_dir(&ancestor).unwrap().next().is_none());
    }

    #[test]
    fn foreign_owner_and_untrusted_control_grants_never_become_a_private_root() {
        let sid = crate::windows::current_user_sid().unwrap();
        for sddl in [
            format!("O:SYD:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)"),
            format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;WD;;;WD)"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = sddl.parse().unwrap();
            assert_eq!(
                validate_descriptor(
                    &descriptor,
                    &sid,
                    true,
                    Instant::now() + Duration::from_secs(30)
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::PermissionDenied
            );
        }
        let valid: LocalBox<SecurityDescriptor> =
            format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)")
                .parse()
                .unwrap();
        validate_descriptor(&valid, &sid, true, Instant::now() + Duration::from_secs(30)).unwrap();
    }

    #[test]
    fn an_existing_file_is_refused_without_changing_bytes_or_creating_children() {
        let (_temporary, root) = private_fixture();
        let file = root.join("plain file");
        super::super::create_private_new(&file)
            .unwrap()
            .write_all(b"unchanged")
            .unwrap();
        for path in [&file, &file.join("missing")] {
            assert!(
                PrivateDirectoryPlan::inspect_until(path, Instant::now() + Duration::from_secs(30))
                    .is_err()
            );
        }
        assert_eq!(fs::read(&file).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn retained_ancestor_blocks_rename_and_junction_replacement_then_reparse_is_refused() {
        let (temporary, ancestor) = private_fixture();
        let target = fs::canonicalize(temporary.path())
            .unwrap()
            .join("junction target");
        drop(DirectoryGuard::private(&target).unwrap());
        let missing = ancestor.join("absent root");
        let plan =
            PrivateDirectoryPlan::inspect_until(&missing, Instant::now() + Duration::from_secs(30))
                .unwrap();
        let moved = ancestor.with_file_name("moved ancestor");
        assert_eq!(
            fs::rename(&ancestor, &moved).unwrap_err().raw_os_error(),
            Some(32)
        );
        let replace = r"
            [IO.Directory]::Delete([string]$request.link);
            New-Item -ItemType Junction -Path ([string]$request.link) -Target ([string]$request.target) | Out-Null;
            @{created=$true} | & $locronToJson -Compress
        ";
        let request = json!({"link": ancestor, "target": target});
        assert!(crate::windows::run_script_json(replace, &request).is_err());
        assert_eq!(
            fs::symlink_metadata(&ancestor).unwrap().file_attributes() & REPARSE_POINT,
            0
        );
        assert!(!missing.exists());
        drop(plan);
        crate::windows::run_script_json(replace, &request).unwrap();
        let rejected =
            PrivateDirectoryPlan::inspect_until(&missing, Instant::now() + Duration::from_secs(30));
        // Remove only the owned junction before temporary cleanup, preserving its target.
        crate::windows::run_script_json(
            "[IO.Directory]::Delete([string]$request.link); @{removed=$true} | & $locronToJson -Compress",
            &json!({"link": ancestor}),
        ).unwrap();
        assert_eq!(
            rejected.unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(fs::read_dir(&target).unwrap().next().is_none());
    }

    #[test]
    fn delayed_native_observation_retains_ancestry_after_the_driver_deadline() {
        let (_temporary, ancestor) = private_fixture();
        let target = ancestor.join("never created");
        let owned_target = target.clone();
        let (mut reader, mut writer) = io::pipe().unwrap();
        let (entered_send, entered) = mpsc::channel();
        let (result_send, result) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(2);
        let worker = std::thread::spawn(move || {
            let observed = inspect_observed_until(&owned_target, deadline, |_| {
                entered_send.send(()).unwrap();
                reader.read_exact(&mut [0])
            });
            result_send.send(observed).unwrap();
        });
        entered
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        assert!(matches!(
            result.recv_timeout(deadline.saturating_duration_since(Instant::now())),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(!worker.is_finished());
        let moved = ancestor.with_file_name("late moved");
        assert_eq!(
            fs::rename(&ancestor, &moved).unwrap_err().raw_os_error(),
            Some(32)
        );
        assert!(!target.exists());
        writer.write_all(&[1]).unwrap();
        drop(writer);
        let cleanup = Instant::now() + Duration::from_secs(3);
        let error = result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        while !worker.is_finished() {
            assert!(
                Instant::now() < cleanup,
                "owned inspector did not complete after native release"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.join().unwrap();
        assert!(!target.exists());
        fs::rename(&ancestor, &moved).unwrap();
    }
}
