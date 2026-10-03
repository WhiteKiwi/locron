//! Read-only preparation from retained existing standalone proofs.
//!
//! This factory creates no journal or installed/state/task/PATH effects. The
//! caller still owns the real service snapshot guards; its typed record alone
//! is not effect authority. Helper mapping, activation and recovery are separate
//! gates. In particular, recovery must never recreate an accepted old snapshot.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Result, ensure};
use locron_core::filesystem::file_identity;

use crate::service::ServiceRestoreRecord;

use super::sha256_hex;
use super::windows_bootstrap::Bootstrap;
use super::windows_ownership::{Removal, Standalone, VerifiedFile, native_target};
use super::windows_path::{Insertion, WriteIntent};
use super::windows_payloads::Payloads;
use super::windows_protocol::{Kind, Request, maintenance_path};
use super::windows_receipt::{PAYLOADS, RECEIPT, Receipt, same_path};
use super::windows_transaction::{Content, ExistingInputs, FileFact, Identity, Leaf, Record};

const REQUEST_LIMIT: usize = 128 * 1024;
const PAYLOAD_LIMIT: usize = 64 * 1024 * 1024;

enum Source<'a> {
    Replacement { _proof: &'a mut Standalone },
    Removal { _proof: &'a mut Removal },
}

/// Borrowed proof guards remain retained for the caller's later backup/quiesce.
/// The record cannot authorize effects or substitute for the live snapshot.
pub(super) struct Prepared<'a> {
    record: Record,
    receipt_bytes: Option<Vec<u8>>,
    _bootstrap: &'a mut Bootstrap,
    _source: Source<'a>,
    _payloads: Option<&'a Payloads>,
    _service: &'a ServiceRestoreRecord,
}

impl Prepared<'_> {
    pub(super) fn record(&self) -> &Record {
        &self.record
    }

    pub(super) fn new_receipt_bytes(&self) -> Option<&[u8]> {
        self.receipt_bytes.as_deref()
    }
}

fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("retained maintenance path is not Unicode"))
}

/// Re-read the exact retained handle, without reopening the leaf by path.
fn checked_bytes(
    proof: &mut VerifiedFile,
    expected: &Path,
    limit: usize,
) -> Result<(FileFact, Vec<u8>)> {
    maintenance_path(text(proof.file.normalized_path())?)?;
    ensure!(
        same_path(text(proof.file.normalized_path())?, text(expected)?)?,
        "retained object path differs from its exact logical inventory"
    );
    let identity = file_identity(&proof.file)?;
    ensure!(
        identity == proof.identity,
        "retained full object identity changed"
    );
    let size = proof.file.metadata()?.len();
    ensure!(
        size <= limit as u64,
        "retained object exceeds its byte bound"
    );
    proof.file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut *proof.file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    proof.file.seek(SeekFrom::Start(0))?;
    ensure!(
        bytes.len() as u64 == size
            && proof.file.metadata()?.len() == size
            && file_identity(&proof.file)? == identity,
        "retained object bytes/length/full identity changed"
    );
    let hash = sha256_hex(&bytes);
    ensure!(hash == proof.sha256, "retained exact bytes changed");
    Ok((
        FileFact {
            identity: Identity::from_file(identity),
            content: Content {
                bytes: size,
                sha256: hash,
            },
        },
        bytes,
    ))
}

fn initial_request(bootstrap: &mut Bootstrap) -> Result<String> {
    ensure!(
        matches!(
            bootstrap.request.kind,
            Kind::Install | Kind::SelfUpdate | Kind::Uninstall
        ) && bootstrap.request.kind == bootstrap.original.kind,
        "fresh/package/recovery cannot rebuild an existing accepted snapshot"
    );
    let directory = bootstrap.directory.normalized_path();
    let expected_name = bootstrap.original.operation_id.to_string();
    ensure!(
        directory.file_name().and_then(|name| name.to_str()) == Some(expected_name.as_str())
            && same_path(
                text(
                    directory
                        .parent()
                        .ok_or_else(|| anyhow::anyhow!("operation parent missing"))?
                )?,
                text(bootstrap.root.normalized_path())?,
            )?,
        "retained operation directory differs from its protected UUID/root"
    );
    let original_path = directory.join("request.json");
    let (original_fact, bytes) =
        checked_bytes(&mut bootstrap.original_file, &original_path, REQUEST_LIMIT)?;
    let sid = locron_core::windows::current_user_sid()?;
    let original = Request::parse(
        &bytes,
        &sid,
        native_target()?,
        bootstrap.original.operation_id,
    )?;
    ensure!(
        serde_json::to_vec(&original)? == serde_json::to_vec(&bootstrap.original)?
            && serde_json::to_vec(&original)? == serde_json::to_vec(&bootstrap.request)?,
        "cached initial request differs from its retained protected bytes"
    );
    let (request_fact, _) =
        checked_bytes(&mut bootstrap.request_file, &original_path, REQUEST_LIMIT)?;
    ensure!(
        request_fact == original_fact,
        "initial request is not the full original protected object"
    );
    let (helper_fact, _) = checked_bytes(
        &mut bootstrap.helper_file,
        &directory.join("locron-helper.exe"),
        PAYLOAD_LIMIT,
    )?;
    ensure!(
        helper_fact.content.sha256 == original.helper_sha256,
        "retained helper differs from its protected original digest"
    );
    Ok(original_fact.content.sha256)
}

fn receipt_fact(
    directory: &Path,
    receipt: &Receipt,
    proof: &mut VerifiedFile,
    request: &Request,
) -> Result<FileFact> {
    maintenance_path(text(directory)?)?;
    receipt.validate(&request.sid, text(directory)?, &request.target)?;
    ensure!(
        same_path(&request.executable, &receipt.executable)?,
        "request does not select the exact owned installation"
    );
    let (fact, bytes) = checked_bytes(proof, &directory.join(RECEIPT), REQUEST_LIMIT)?;
    let parsed = Receipt::parse(&bytes, &request.sid, text(directory)?, &request.target)?;
    ensure!(
        serde_json::to_vec(&parsed)? == serde_json::to_vec(receipt)?,
        "cached ownership receipt differs from its retained exact bytes"
    );
    Ok(fact)
}

fn file_leaves(
    directory: &Path,
    receipt: &Receipt,
    files: &mut BTreeMap<String, VerifiedFile>,
    desired: Option<&BTreeMap<String, Vec<u8>>>,
) -> Result<BTreeMap<String, Leaf>> {
    let mut leaves = BTreeMap::new();
    for (name, proof) in files {
        ensure!(PAYLOADS.contains(&name.as_str()), "unknown owned payload");
        let (original, _) = checked_bytes(proof, &directory.join(name), PAYLOAD_LIMIT)?;
        ensure!(
            receipt.files.get(name) == Some(&original.content.sha256),
            "exact retained payload differs from its owned receipt"
        );
        let content = match desired {
            None => None,
            Some(selected) => {
                let bytes = selected
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("selected payload inventory is incomplete"))?;
                ensure!(
                    bytes.len() <= PAYLOAD_LIMIT,
                    "selected payload is oversized"
                );
                Some(Content {
                    bytes: bytes.len() as u64,
                    sha256: sha256_hex(bytes),
                })
            }
        };
        leaves.insert(name.clone(), Leaf::original(original, content));
    }
    Ok(leaves)
}

/// Existing standalone installation/update only. All proof guards stay borrowed.
/// The caller must retain the actual snapshot that supplied `service`.
pub(super) fn replacement<'a>(
    bootstrap: &'a mut Bootstrap,
    source: &'a mut Standalone,
    payloads: &'a Payloads,
    service: &'a ServiceRestoreRecord,
    insertion: Option<&Insertion>,
) -> Result<Prepared<'a>> {
    let protected_request_sha256 = initial_request(bootstrap)?;
    let request = &bootstrap.original;
    ensure!(
        matches!(request.kind, Kind::Install | Kind::SelfUpdate)
            && !request.no_service
            && !request.dashboard,
        "existing replacement accepts no fresh service choices"
    );
    ensure!(
        payloads.version == request.version
            && payloads.target == request.target
            && payloads.archive_sha256 == request.archive_sha256,
        "selected payloads differ from the protected release choice"
    );
    // This qualifies retained bytes, not the already mapped helper process.
    ensure!(
        request.helper_sha256 == source.receipt.binary_sha256
            || request.helper_sha256 == payloads.binary_sha256,
        "helper bytes have no verified old/new executable provenance"
    );
    let directory = source.directory.normalized_path();
    let original_receipt = receipt_fact(
        directory,
        &source.receipt,
        &mut source.receipt_file,
        request,
    )?;
    let needs_insertion =
        request.kind == Kind::Install && request.add_to_path && source.receipt.user_path.is_none();
    ensure!(
        insertion.is_some() == needs_insertion,
        "PATH planning must match the explicit new literal-insertion choice"
    );
    let (user_path, path_edit) = match insertion {
        None => (source.receipt.user_path.clone(), None),
        Some(insertion) => (
            insertion.ownership.clone(),
            insertion
                .intent
                .as_ref()
                .map(|intent| (intent.expected.clone(), intent.desired.clone())),
        ),
    };
    let (new_receipt, receipt_bytes) =
        payloads.receipt(&request.sid, text(directory)?, user_path)?;
    let mut leaves = file_leaves(
        directory,
        &source.receipt,
        &mut source.files,
        Some(payloads.files()),
    )?;
    leaves.insert(
        RECEIPT.into(),
        Leaf::original(
            original_receipt,
            Some(Content {
                bytes: receipt_bytes.len() as u64,
                sha256: sha256_hex(&receipt_bytes),
            }),
        ),
    );
    let original_executable = leaves
        .get("locron.exe")
        .ok_or_else(|| anyhow::anyhow!("owned executable is missing"))?
        .original
        .identity
        .clone();
    let record = Record::existing(ExistingInputs {
        request,
        protected_request_sha256,
        original_executable,
        service: service.clone(),
        original_receipt: Some(source.receipt.clone()),
        new_receipt: Some(new_receipt),
        leaves,
        retained: Vec::new(),
        path_edit,
    })?;
    Ok(Prepared {
        record,
        receipt_bytes: Some(receipt_bytes),
        _bootstrap: bootstrap,
        _source: Source::Replacement { _proof: source },
        _payloads: Some(payloads),
        _service: service,
    })
}

/// Existing offline removal only; changed/unverifiable companions stay retained.
pub(super) fn removal<'a>(
    bootstrap: &'a mut Bootstrap,
    source: &'a mut Removal,
    service: &'a ServiceRestoreRecord,
    path_intent: Option<&WriteIntent>,
) -> Result<Prepared<'a>> {
    let protected_request_sha256 = initial_request(bootstrap)?;
    let request = &bootstrap.original;
    ensure!(
        request.kind == Kind::Uninstall,
        "not an existing offline removal"
    );
    ensure!(
        request.helper_sha256 == source.receipt.binary_sha256,
        "offline helper is not the exact owned executable bytes"
    );
    let directory = source.directory.normalized_path();
    let original_receipt = receipt_fact(
        directory,
        &source.receipt,
        &mut source.receipt_file,
        request,
    )?;
    let mut leaves = file_leaves(directory, &source.receipt, &mut source.files, None)?;
    leaves.insert(RECEIPT.into(), Leaf::original(original_receipt, None));
    let original_executable = leaves
        .get("locron.exe")
        .ok_or_else(|| anyhow::anyhow!("owned executable is missing"))?
        .original
        .identity
        .clone();
    let record = Record::existing(ExistingInputs {
        request,
        protected_request_sha256,
        original_executable,
        service: service.clone(),
        original_receipt: Some(source.receipt.clone()),
        new_receipt: None,
        leaves,
        retained: source.retained.clone(),
        path_edit: path_intent.map(|intent| (intent.expected.clone(), intent.desired.clone())),
    })?;
    Ok(Prepared {
        record,
        receipt_bytes: None,
        _bootstrap: bootstrap,
        _source: Source::Removal { _proof: source },
        _payloads: None,
        _service: service,
    })
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write};

    use locron_core::filesystem::{DirectoryGuard, create_private_new_exclusive};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use zip::write::SimpleFileOptions;

    use super::super::windows_fixture::PrivateFixture;
    use super::super::windows_ownership::{immutable_private, verify, verify_removal};
    use super::super::windows_path::{PathValue, plan_insertion};
    use super::super::windows_receipt::PathKind;
    use super::super::windows_release_source::{Release, payload_inventory};
    use super::*;

    struct Fixture {
        bootstrap: Bootstrap,
        source: Standalone,
        service: ServiceRestoreRecord,
        payloads: Payloads,
        // Drop every retained proof before the test-owned root is removed.
        _private: PrivateFixture,
    }

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn selected_payloads() -> Payloads {
        let target = native_target().unwrap();
        let machine = match target {
            "x86_64-pc-windows-msvc" => 0x8664_u16,
            "aarch64-pc-windows-msvc" => 0xaa64_u16,
            _ => unreachable!(),
        };
        let mut binary = vec![0; 512];
        binary[..2].copy_from_slice(b"MZ");
        binary[60..64].copy_from_slice(&128_u32.to_le_bytes());
        binary[128..132].copy_from_slice(b"PE\0\0");
        binary[132..134].copy_from_slice(&machine.to_le_bytes());
        binary[134..136].copy_from_slice(&1_u16.to_le_bytes());
        binary[148..150].copy_from_slice(&240_u16.to_le_bytes());
        binary[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
        binary[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        binary[260..264].copy_from_slice(&16_u32.to_le_bytes());
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in super::super::windows_package::FILES {
            writer
                .start_file(
                    format!("locron-v0.10.1-{target}/{name}"),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(if name == "locron.exe" {
                    &binary
                } else {
                    b"verified test-only candidate payload"
                })
                .unwrap();
        }
        let archive = writer.finish().unwrap().into_inner();
        let installer = b"test-only script bytes; never executed";
        let uninstaller = b"test-only offline script bytes; never executed";
        let mut digests: BTreeMap<_, _> = payload_inventory("0.10.1")
            .unwrap()
            .into_iter()
            .map(|name| (name, "ab".repeat(32)))
            .collect();
        digests.insert(format!("locron-v0.10.1-{target}.zip"), sha256_hex(&archive));
        let sums: Vec<u8> = digests
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>()
            .into_bytes();
        digests.insert("SHA256SUMS.txt".into(), sha256_hex(&sums));
        digests.insert("install.ps1".into(), sha256_hex(installer));
        digests.insert("uninstall.ps1".into(), sha256_hex(uninstaller));
        digests.insert("install.sh".into(), "ab".repeat(32));
        let metadata = json!({"tag_name":"v0.10.1", "draft":false, "prerelease":false,
            "assets":digests.into_iter().map(|(name, hash)|json!({
                "browser_download_url":format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.1/{name}"),
                "name":name, "digest":format!("sha256:{hash}")
            })).collect::<Vec<_>>()});
        let release =
            Release::parse(&serde_json::to_vec(&metadata).unwrap(), Some("0.10.1")).unwrap();
        Payloads::verify(&release, target, &sums, &archive, installer, uninstaller).unwrap()
    }

    impl Fixture {
        fn new(kind: Kind, add_to_path: bool, no_service: bool) -> Self {
            let private = PrivateFixture::new("locron-preparation-fixture-");
            let installed = DirectoryGuard::private(&private.path().join("installed")).unwrap();
            let directory = text(installed.normalized_path()).unwrap().to_owned();
            let mut files = BTreeMap::new();
            let old_binary = b"test-only owned executable bytes; never executed";
            for name in PAYLOADS {
                let bytes = if name == "locron.exe" {
                    old_binary.to_vec()
                } else {
                    format!("test-only owned {name}").into_bytes()
                };
                write_new(&installed.normalized_path().join(name), &bytes);
                files.insert(name.to_owned(), sha256_hex(&bytes));
            }
            let sid = locron_core::windows::current_user_sid().unwrap();
            let target = native_target().unwrap();
            let receipt = Receipt {
                schema: "locron.install/windows-v1".into(),
                sid: sid.clone(),
                channel: "standalone".into(),
                directory: directory.clone(),
                executable: format!("{directory}\\locron.exe"),
                target: target.into(),
                version: "0.10.0".into(),
                archive_url: format!(
                    "https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{target}.zip"
                ),
                archive_sha256: "ab".repeat(32),
                binary_sha256: files["locron.exe"].clone(),
                files,
                user_path: None,
            };
            write_new(
                &installed.normalized_path().join(RECEIPT),
                &serde_json::to_vec_pretty(&receipt).unwrap(),
            );
            let source = verify(installed.normalized_path()).unwrap();
            let payloads = selected_payloads();
            let request = Request {
                schema: "locron.windows-operation/v1".into(),
                operation_id: Uuid::now_v7(),
                kind,
                sid: sid.clone(),
                executable: source.receipt.executable.clone(),
                target: target.into(),
                version: if kind == Kind::Uninstall {
                    receipt.version.clone()
                } else {
                    payloads.version.clone()
                },
                archive_sha256: if kind == Kind::Uninstall {
                    receipt.archive_sha256.clone()
                } else {
                    payloads.archive_sha256.clone()
                },
                helper_sha256: sha256_hex(old_binary),
                caller_pid: Some(std::process::id()),
                no_service,
                dashboard: false,
                add_to_path,
                state_root: None,
                package: None,
                archive_file: None,
            };
            let root = DirectoryGuard::private(&private.path().join("operations")).unwrap();
            let operation = DirectoryGuard::private(
                &root
                    .normalized_path()
                    .join(request.operation_id.to_string()),
            )
            .unwrap();
            let original_path = operation.normalized_path().join("request.json");
            write_new(
                &original_path,
                &serde_json::to_vec_pretty(&request).unwrap(),
            );
            write_new(
                &operation.normalized_path().join("locron-helper.exe"),
                old_binary,
            );
            let (original_file, original_bytes) =
                immutable_private(&original_path, REQUEST_LIMIT).unwrap();
            let (request_file, _) = immutable_private(&original_path, REQUEST_LIMIT).unwrap();
            let (helper_file, _) = immutable_private(
                &operation.normalized_path().join("locron-helper.exe"),
                PAYLOAD_LIMIT,
            )
            .unwrap();
            let parsed =
                Request::parse(&original_bytes, &sid, target, request.operation_id).unwrap();
            let identity = source.files["locron.exe"].identity;
            // Real typed deserialization is a pure fixture, not a mock live snapshot.
            let service: ServiceRestoreRecord = serde_json::from_value(json!({
                "version":1, "sid":sid,
                "previous":{"path":source.files["locron.exe"].file.normalized_path(),
                    "volume":format!("{:016x}",identity.volume_serial_number),
                    "file":format!("{:032x}",identity.file_id)},
                "roles":[], "phase":"snapshot", "next":null, "forced":[],
            }))
            .unwrap();
            service.validate_for_sid(&parsed.sid).unwrap();
            Self {
                bootstrap: Bootstrap {
                    root,
                    directory: operation,
                    request_file,
                    original_file,
                    helper_file,
                    request: parsed.clone(),
                    original: parsed,
                },
                source,
                service,
                payloads,
                _private: private,
            }
        }
    }

    fn installed_bytes(source: &Standalone) -> BTreeMap<String, Vec<u8>> {
        PAYLOADS
            .into_iter()
            .chain([RECEIPT])
            .map(|name| {
                (
                    name.into(),
                    fs::read(source.directory.normalized_path().join(name)).unwrap(),
                )
            })
            .collect()
    }

    fn value(prepared: &Prepared<'_>) -> Value {
        serde_json::to_value(prepared.record()).unwrap()
    }

    fn entries(directory: &Path) -> std::collections::BTreeSet<String> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn retained_replacement_binds_exact_inventory_and_raw_original_bytes_without_effects() {
        let mut fixture = Fixture::new(Kind::SelfUpdate, false, false);
        let before = installed_bytes(&fixture.source);
        let original_hash = fixture.bootstrap.original_file.sha256.clone();
        assert_ne!(
            original_hash,
            sha256_hex(&serde_json::to_vec(&fixture.bootstrap.original).unwrap())
        );
        let old_identity = fixture.source.files["locron.exe"].identity;
        let old_directory = fixture.source.directory.normalized_path().to_owned();
        let operation_directory = fixture.bootstrap.directory.normalized_path().to_owned();
        let old_entries = entries(&old_directory);
        let operation_entries = entries(&operation_directory);
        let prepared = replacement(
            &mut fixture.bootstrap,
            &mut fixture.source,
            &fixture.payloads,
            &fixture.service,
            None,
        )
        .unwrap();
        let record = value(&prepared);
        assert_eq!(record["protected_request_sha256"], original_hash);
        assert_eq!(record["phase"], "accepted");
        assert_eq!(record["leaves"].as_object().unwrap().len(), 7);
        assert_eq!(
            record["original_executable"]["volume"],
            format!("{:016x}", old_identity.volume_serial_number)
        );
        assert_eq!(
            record["original_executable"]["file"],
            format!("{:032x}", old_identity.file_id)
        );
        for (name, old_bytes) in &before {
            assert_eq!(
                record["leaves"][name]["original"]["content"]["bytes"],
                old_bytes.len() as u64
            );
            assert_eq!(
                record["leaves"][name]["original"]["content"]["sha256"],
                sha256_hex(old_bytes)
            );
            assert_eq!(fs::read(old_directory.join(name)).unwrap(), *old_bytes);
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(old_directory.join(name))
                    .is_err()
            );
        }
        let new_receipt = prepared.new_receipt_bytes().unwrap();
        assert_eq!(
            record["leaves"][RECEIPT]["desired"]["bytes"],
            new_receipt.len() as u64
        );
        assert_eq!(
            record["leaves"][RECEIPT]["desired"]["sha256"],
            sha256_hex(new_receipt)
        );
        assert_eq!(entries(&old_directory), old_entries);
        assert_eq!(entries(&operation_directory), operation_entries);
    }

    #[test]
    fn stale_high_identity_bit_or_exact_digest_refuses_without_repair() {
        let mut fixture = Fixture::new(Kind::SelfUpdate, false, false);
        let before = installed_bytes(&fixture.source);
        let old_identity = fixture.source.files["locron.exe"].identity;
        fixture
            .source
            .files
            .get_mut("locron.exe")
            .unwrap()
            .identity
            .file_id ^= 1_u128 << 127;
        assert!(
            replacement(
                &mut fixture.bootstrap,
                &mut fixture.source,
                &fixture.payloads,
                &fixture.service,
                None
            )
            .is_err()
        );
        fixture.source.files.get_mut("locron.exe").unwrap().identity = old_identity;
        fixture.source.files.get_mut("README.md").unwrap().sha256 = "ff".repeat(32);
        assert!(
            replacement(
                &mut fixture.bootstrap,
                &mut fixture.source,
                &fixture.payloads,
                &fixture.service,
                None
            )
            .is_err()
        );
        assert_eq!(installed_bytes(&fixture.source), before);
    }

    #[test]
    fn swapped_logical_guards_are_refused_before_record_construction() {
        let mut fixture = Fixture::new(Kind::SelfUpdate, false, false);
        let before = installed_bytes(&fixture.source);
        let readme = fixture.source.files.remove("README.md").unwrap();
        let license = fixture.source.files.remove("LICENSE-MIT").unwrap();
        fixture.source.files.insert("README.md".into(), license);
        fixture.source.files.insert("LICENSE-MIT".into(), readme);
        let error = replacement(
            &mut fixture.bootstrap,
            &mut fixture.source,
            &fixture.payloads,
            &fixture.service,
            None,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("exact logical inventory"));
        assert_eq!(installed_bytes(&fixture.source), before);
    }

    #[test]
    fn protected_recovery_followup_and_existing_fresh_service_choices_never_rebuild_accepted() {
        let mut fixture = Fixture::new(Kind::SelfUpdate, false, false);
        let before = installed_bytes(&fixture.source);
        let mut recovery = fixture.bootstrap.original.clone();
        recovery.kind = Kind::Recover;
        let path = fixture
            .bootstrap
            .directory
            .normalized_path()
            .join(format!("request-recover-{}.json", Uuid::now_v7()));
        write_new(&path, &serde_json::to_vec(&recovery).unwrap());
        fixture.bootstrap.request_file = immutable_private(&path, REQUEST_LIMIT).unwrap().0;
        fixture.bootstrap.request = recovery;
        assert!(
            replacement(
                &mut fixture.bootstrap,
                &mut fixture.source,
                &fixture.payloads,
                &fixture.service,
                None
            )
            .is_err()
        );
        assert_eq!(installed_bytes(&fixture.source), before);
        drop(fixture);

        let mut fixture = Fixture::new(Kind::Install, false, true);
        let before = installed_bytes(&fixture.source);
        assert!(
            replacement(
                &mut fixture.bootstrap,
                &mut fixture.source,
                &fixture.payloads,
                &fixture.service,
                None
            )
            .is_err()
        );
        assert_eq!(installed_bytes(&fixture.source), before);
    }

    #[test]
    fn explicit_path_plan_binds_raw_kind_and_duplicates_claim_no_new_ownership() {
        let mut fixture = Fixture::new(Kind::Install, true, false);
        let directory = fixture.source.receipt.directory.clone();
        let current = PathValue {
            value: Some(r"%USERPROFILE%\bin;".into()),
            kind: Some(PathKind::ExpandString),
        };
        let insertion =
            plan_insertion(&current, &directory, None, &fixture.bootstrap.original.sid).unwrap();
        {
            let prepared = replacement(
                &mut fixture.bootstrap,
                &mut fixture.source,
                &fixture.payloads,
                &fixture.service,
                Some(&insertion),
            )
            .unwrap();
            let record = value(&prepared);
            assert_eq!(
                record["path_edit"]["expected"]["value"],
                current.value.as_ref().unwrap().as_str()
            );
            assert_eq!(record["path_edit"]["expected"]["kind"], "ExpandString");
            assert_eq!(
                record["new_receipt"]["user_path"]["before_kind"],
                "ExpandString"
            );
        }
        let duplicate = PathValue {
            value: Some(directory),
            kind: Some(PathKind::String),
        };
        let insertion = plan_insertion(
            &duplicate,
            &fixture.source.receipt.directory,
            None,
            &fixture.bootstrap.original.sid,
        )
        .unwrap();
        let prepared = replacement(
            &mut fixture.bootstrap,
            &mut fixture.source,
            &fixture.payloads,
            &fixture.service,
            Some(&insertion),
        )
        .unwrap();
        assert!(value(&prepared)["path_edit"].is_null());
        assert!(value(&prepared)["new_receipt"]["user_path"].is_null());
    }

    #[test]
    fn unchanged_removal_inventory_retains_changed_companions_without_adopting_them() {
        let mut fixture = Fixture::new(Kind::Uninstall, false, false);
        let directory = fixture.source.directory.normalized_path().to_owned();
        drop(fixture.source);
        fs::write(directory.join("README.md"), b"operator changes remain").unwrap();
        let mut proof = verify_removal(&directory).unwrap();
        let prepared = removal(&mut fixture.bootstrap, &mut proof, &fixture.service, None).unwrap();
        let record = value(&prepared);
        assert_eq!(record["retained"]["README.md"], "changed");
        assert!(record["leaves"].get("README.md").is_none());
        assert_eq!(record["leaves"].as_object().unwrap().len(), 6);
        assert!(prepared.new_receipt_bytes().is_none());
        assert!(record["new_receipt"].is_null());
        assert!(
            record["leaves"]
                .as_object()
                .unwrap()
                .values()
                .all(|leaf| leaf["desired"].is_null())
        );
        assert_eq!(
            fs::read(directory.join("README.md")).unwrap(),
            b"operator changes remain"
        );
        assert!(directory.join("locron.exe").exists());
        assert!(directory.join(RECEIPT).exists());
    }
}
