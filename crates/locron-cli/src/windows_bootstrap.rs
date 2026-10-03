//! Read-only binding of protected bootstrap metadata to its retained helper.
//! Canonical archive/receipt verification and complete typed preflight follow
//! this gate; protected request JSON alone never authorizes installation effects.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use locron_core::filesystem::{DirectoryGuard, same_file};
use uuid::Uuid;

use super::windows_ownership::{VerifiedFile, immutable_private, native_target};
use super::windows_protocol::{Kind, Request};
use super::windows_receipt::same_path;

const REQUEST_LIMIT: usize = 128 * 1024;
const HELPER_LIMIT: usize = 64 * 1024 * 1024;

pub(super) struct Bootstrap {
    pub root: DirectoryGuard,
    pub directory: DirectoryGuard,
    pub request_file: VerifiedFile,
    pub original_file: VerifiedFile,
    pub helper_file: VerifiedFile,
    pub request: Request,
    pub original: Request,
}

fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("operation path is not Unicode"))
}

fn canonical_uuid(value: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(value)?;
    ensure!(
        !id.is_nil() && id.to_string() == value,
        "operation/request UUID is not canonical"
    );
    Ok(id)
}

fn request_kind(name: &str) -> Result<Option<Kind>> {
    if name == "request.json" {
        return Ok(None);
    }
    for (prefix, kind) in [
        ("request-recover-", Kind::Recover),
        ("request-maintenance_complete-", Kind::MaintenanceComplete),
    ] {
        if let Some(id) = name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".json"))
        {
            canonical_uuid(id)?;
            return Ok(Some(kind));
        }
    }
    anyhow::bail!("operation request filename is not permitted")
}

fn validate_followup(request: &Request, original: &Request) -> Result<()> {
    ensure!(
        request.operation_id == original.operation_id
            && request.sid == original.sid
            && request.target == original.target
            && request.helper_sha256 == original.helper_sha256,
        "follow-up differs from its protected original operation/helper"
    );
    match request.kind {
        Kind::Recover => {
            ensure!(
                matches!(
                    original.kind,
                    Kind::Install | Kind::SelfUpdate | Kind::Uninstall
                ) && request.version == original.version
                    && request.archive_sha256 == original.archive_sha256
                    && same_path(&request.executable, &original.executable)?,
                "recovery cannot change the protected standalone operation"
            );
        }
        Kind::MaintenanceComplete => {
            ensure!(
                original.kind == Kind::MaintenancePrepare,
                "completion requires protected package preparation"
            );
            let previous = original
                .package
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("original package binding is missing"))?;
            let selected = request
                .package
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("new package binding is missing"))?;
            ensure!(
                previous.package_id == selected.package_id
                    && previous.source_id == selected.source_id,
                "completion changes the protected package/source"
            );
        }
        _ => anyhow::bail!("unsupported follow-up operation kind"),
    }
    Ok(())
}

fn within<T>(deadline: Instant, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    ensure!(
        Instant::now() < deadline,
        "original bootstrap deadline expired"
    );
    let result = operation()?;
    ensure!(
        Instant::now() < deadline,
        "original bootstrap deadline expired"
    );
    Ok(result)
}

fn verify_at(request_path: &Path, root: &Path, current_executable: &Path) -> Result<Bootstrap> {
    verify_at_until(
        request_path,
        root,
        current_executable,
        Instant::now() + Duration::from_secs(30),
    )
}

/// Guard queries run in the retained launch owner; the deadline precedes its first SID/guard work.
pub(super) fn verify_at_until(
    request_path: &Path,
    root: &Path,
    current_executable: &Path,
    deadline: Instant,
) -> Result<Bootstrap> {
    let sid = locron_core::windows::current_user_sid_until(deadline)?;
    ensure!(
        request_path.is_absolute(),
        "helper request must be absolute"
    );
    let name = request_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("operation request has no valid filename"))?;
    let followup = request_kind(name)?;
    let directory_path = request_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("operation request has no directory"))?;
    let id = canonical_uuid(
        directory_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("operation directory has no UUID"))?,
    )?;
    let parent = directory_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("operation directory has no root"))?;
    ensure!(
        same_path(text(parent)?, text(root)?)?,
        "request is outside the owning account's operation root"
    );
    let root = within(deadline, || Ok(DirectoryGuard::existing_private(root)?))?;
    let directory = within(deadline, || {
        Ok(DirectoryGuard::existing_private(directory_path)?)
    })?;
    ensure!(
        same_path(
            text(
                directory
                    .normalized_path()
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("canonical operation parent is missing"))?
            )?,
            text(root.normalized_path())?,
        )?,
        "canonical operation directory differs from its guarded root"
    );
    let target = native_target()?;
    let (request_file, bytes) = within(deadline, || {
        immutable_private(&directory.normalized_path().join(name), REQUEST_LIMIT)
    })?;
    let request = Request::parse(&bytes, &sid, target, id)?;
    let (original_file, bytes) = within(deadline, || {
        immutable_private(
            &directory.normalized_path().join("request.json"),
            REQUEST_LIMIT,
        )
    })?;
    let original = Request::parse(&bytes, &sid, target, id)?;
    ensure!(
        !matches!(original.kind, Kind::Recover | Kind::MaintenanceComplete),
        "original operation cannot be a follow-up"
    );
    if let Some(kind) = followup {
        ensure!(request.kind == kind, "request filename and kind differ");
        validate_followup(&request, &original)?;
    } else {
        ensure!(
            within(deadline, || Ok(same_file(
                &request_file.file,
                &original_file.file
            )?))?
                && request_file.sha256 == original_file.sha256,
            "initial request differs from its protected original"
        );
    }
    let (helper_file, _) = within(deadline, || {
        immutable_private(
            &directory.normalized_path().join("locron-helper.exe"),
            HELPER_LIMIT,
        )
    })?;
    ensure!(
        helper_file.sha256 == original.helper_sha256
            && request.helper_sha256 == original.helper_sha256,
        "retained helper bytes differ from the protected original request"
    );
    let (current_file, _) = within(deadline, || {
        immutable_private(current_executable, HELPER_LIMIT)
    })?;
    // The frontend must retain its immutable helper guard through launch and
    // acceptance, until this helper guard is acquired. current_exe is a path,
    // not an independent identity query for the already mapped process image.
    ensure!(
        same_path(
            text(current_file.file.normalized_path())?,
            text(helper_file.file.normalized_path())?
        )? && within(deadline, || Ok(same_file(
            &current_file.file,
            &helper_file.file
        )?))?,
        "current executable path is not the exact protected retained helper"
    );
    within(deadline, || {
        Ok(Bootstrap {
            root,
            directory,
            request_file,
            original_file,
            helper_file,
            request,
            original,
        })
    })
}

pub(super) fn verify(request_path: &Path) -> Result<Bootstrap> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("current user's LocalAppData is unavailable"))?;
    ensure!(local.is_absolute(), "LocalAppData must be absolute");
    let root = local.join("locron-distribution").join("operations");
    let current = within(deadline, || Ok(std::env::current_exe()?))?;
    verify_at_until(request_path, &root, &current, deadline)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use locron_core::filesystem::create_private_new_exclusive;
    use serde_json::{Value, json};

    use super::super::sha256_hex;
    use super::*;

    const ID: &str = "e41c210d-c98d-47fb-9975-a5af66d01346";
    const FOLLOWUP: &str = "14ce596a-a977-41e2-a3ca-c0611ff05f65";

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn fixture() -> (
        super::super::windows_fixture::PrivateFixture,
        PathBuf,
        PathBuf,
        Value,
    ) {
        let temp = super::super::windows_fixture::PrivateFixture::new("locron-bootstrap-fixture-");
        let root = temp.path().join("operations");
        let directory = root.join(ID);
        let guard = DirectoryGuard::private(&directory).unwrap();
        let bytes = b"test-owned helper identity bytes, never executed";
        write_new(&directory.join("locron-helper.exe"), bytes);
        let request = json!({
            "schema":"locron.windows-operation/v1", "operation_id":ID,
            "kind":"install", "sid":locron_core::windows::current_user_sid().unwrap(),
            "executable":text(&temp.path().join("installation").join("locron.exe")).unwrap(),
            "target":native_target().unwrap(), "version":"0.10.0",
            "archive_sha256":"ab".repeat(32), "helper_sha256":sha256_hex(bytes),
            "caller_pid":null, "no_service":false, "dashboard":false,
            "add_to_path":false, "state_root":null
        });
        write_new(
            &directory.join("request.json"),
            &serde_json::to_vec(&request).unwrap(),
        );
        drop(guard);
        (temp, root, directory, request)
    }

    #[test]
    fn expired_original_budget_refuses_existing_bootstrap_without_effects() {
        let (_temp, root, directory, _) = fixture();
        let request = directory.join("request.json");
        let bytes = fs::read(&request).unwrap();
        let deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(
            verify_at_until(
                &request,
                &root,
                &directory.join("locron-helper.exe"),
                deadline
            )
            .is_err()
        );
        assert_eq!(fs::read(request).unwrap(), bytes);
        assert!(!directory.join("journal.bin").exists());
        assert!(!directory.join("status.json").exists());
    }

    #[test]
    fn bootstrap_binds_existing_private_root_request_and_exact_helper_without_effects() {
        let (_temp, root, directory, _) = fixture();
        let request = directory.join("request.json");
        let helper = directory.join("locron-helper.exe");
        let proof = verify_at(&request, &root, &helper).unwrap();
        assert_eq!(proof.request.operation_id, proof.original.operation_id);
        assert_eq!(proof.request_file.identity, proof.original_file.identity);
        assert_eq!(proof.helper_file.sha256, proof.original.helper_sha256);
        assert_eq!(
            proof.root.normalized_path(),
            proof.directory.normalized_path().parent().unwrap()
        );
        assert!(!directory.join("journal.bin").exists());
        assert!(!directory.join("status.json").exists());
        for path in [&request, &helper] {
            assert!(OpenOptions::new().write(true).open(path).is_err());
            assert!(fs::rename(path, directory.join("changed-leaf")).is_err());
        }
        drop(proof);
        assert!(verify(&request).is_err());
        assert!(verify_at(&request, &root, &std::env::current_exe().unwrap()).is_err());
        assert!(!directory.join("journal.bin").exists());
        assert!(verify_at(Path::new("request.json"), &root, &helper).is_err());
        assert!(verify_at(&request, &root.join("foreign-root"), &helper).is_err());
        let other = directory.join("same-bytes-other-helper.exe");
        write_new(&other, &fs::read(&helper).unwrap());
        assert!(verify_at(&request, &root, &other).is_err());
        fs::write(&helper, b"changed helper").unwrap();
        assert!(verify_at(&request, &root, &helper).is_err());
        assert_eq!(fs::read(&helper).unwrap(), b"changed helper");
    }

    #[test]
    fn followup_cannot_change_original_authority_or_invent_a_request_name() {
        let (_temp, root, directory, original) = fixture();
        let helper = directory.join("locron-helper.exe");
        let path = directory.join(format!("request-recover-{FOLLOWUP}.json"));
        let mut request = original.clone();
        request["kind"] = json!("recover");
        write_new(&path, &serde_json::to_vec(&request).unwrap());
        assert!(verify_at(&path, &root, &helper).is_ok());
        for (field, value) in [
            ("kind", json!("uninstall")),
            ("operation_id", json!(FOLLOWUP)),
            ("sid", json!("S-1-5-21-9-8-7-1001")),
            ("helper_sha256", json!("00".repeat(32))),
            ("archive_sha256", json!("00".repeat(32))),
            ("version", json!("0.10.1")),
            ("executable", json!("C:\\different\\locron.exe")),
        ] {
            let mut changed = request.clone();
            changed[field] = value;
            let bytes = serde_json::to_vec(&changed).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(verify_at(&path, &root, &helper).is_err(), "{field}");
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        for name in [
            "request-recover-invalid.json",
            "request-recover-00000000-0000-0000-0000-000000000000.json",
            "request-recover-14CE596A-A977-41E2-A3CA-C0611FF05F65.json",
            "request-other.json",
            "request-maintenance_remove-14ce596a-a977-41e2-a3ca-c0611ff05f65.json",
        ] {
            assert!(request_kind(name).is_err());
        }
        assert!(!directory.join("journal.bin").exists());
    }

    #[test]
    fn package_completion_binds_prepare_and_same_source_but_accepts_future_path_metadata() {
        let (temp, root, directory, mut original) = fixture();
        let helper = directory.join("locron-helper.exe");
        let old_location = temp.path().join("test-package-old");
        original["kind"] = json!("maintenance_prepare");
        original["executable"] = json!(text(&old_location.join("locron.exe")).unwrap());
        original["package"] = json!({
            "key":"test-owned-locron-old", "package_id":"WhiteKiwi.locron",
            "source_id":"test-owned-source", "install_location":text(&old_location).unwrap(),
            "executable":original["executable"], "version":original["version"],
            "target":original["target"], "binary_sha256":"cd".repeat(32),
            "archive_sha256":original["archive_sha256"]
        });
        fs::write(
            directory.join("request.json"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        let path = directory.join(format!("request-maintenance_complete-{FOLLOWUP}.json"));
        let new_location = temp.path().join("test-package-new");
        let mut request = original.clone();
        request["kind"] = json!("maintenance_complete");
        request["version"] = json!("0.10.1");
        request["executable"] = json!(text(&new_location.join("locron.exe")).unwrap());
        request["archive_sha256"] = json!("de".repeat(32));
        request["archive_file"] = json!(format!("archive-{FOLLOWUP}.zip"));
        request["package"]["key"] = json!("test-owned-locron-new");
        request["package"]["install_location"] = json!(text(&new_location).unwrap());
        for field in ["executable", "version", "archive_sha256"] {
            request["package"][field] = request[field].clone();
        }
        request["package"]["binary_sha256"] = json!("ef".repeat(32));
        write_new(&path, &serde_json::to_vec(&request).unwrap());
        let proof = verify_at(&path, &root, &helper).unwrap();
        assert_eq!(proof.original.kind, Kind::MaintenancePrepare);
        assert_eq!(proof.request.kind, Kind::MaintenanceComplete);
        assert_ne!(proof.request.version, proof.original.version);
        assert_ne!(proof.request.executable, proof.original.executable);
        drop(proof);
        // Live package registration/canonical archive proof follows this gate;
        // this read-only fixture never creates a package or runs its fake bytes.
        for (field, value) in [
            ("source_id", json!("foreign-source")),
            ("package_id", json!("foreign.package")),
        ] {
            let mut changed = request.clone();
            changed["package"][field] = value;
            let bytes = serde_json::to_vec(&changed).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(verify_at(&path, &root, &helper).is_err(), "{field}");
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
        let mut changed = original;
        changed["kind"] = json!("maintenance_remove");
        fs::write(
            directory.join("request.json"),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        assert!(verify_at(&path, &root, &helper).is_err());
        assert!(!directory.join("journal.bin").exists());
        assert!(!directory.join("status.json").exists());
        assert!(!old_location.exists());
        assert!(!new_location.exists());
    }
}
