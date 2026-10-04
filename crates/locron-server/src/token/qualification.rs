//! Functional drivers: eleven portable, two Windows, and one separate support entry.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use locron_core::filesystem::{DirectoryGuard, create_private_new, open_private};

use super::test_support::{self as support, CANARY, Event, Fault, Frame, Harness, Operation, SEED};

fn private_bytes(path: &Path) -> Vec<u8> {
    let mut file =
        open_private(path, fs::OpenOptions::new().read(true)).expect("actual private fixture read");
    let mut bytes = Vec::new();
    (&mut *file)
        .take(4098)
        .read_to_end(&mut bytes)
        .expect("bounded actual fixture bytes");
    assert!(bytes.len() <= 4097, "fixture file exceeds selected bound");
    bytes
}

fn same_bytes(path: &Path, expected: &[u8]) {
    let equal = private_bytes(path) == expected;
    assert!(equal, "actual private fixture bytes changed");
    assert!(
        locron_core::filesystem::is_private(path, false).expect("actual fixture private posture")
    );
}

fn put(path: &Path, bytes: &[u8]) {
    let mut file = create_private_new(path).expect("actual exclusive private fixture create");
    file.write_all(bytes).expect("actual fixture write");
    file.sync_all().expect("actual fixture sync");
    drop(file);
}

fn seed(harness: &Harness, bytes: &[u8]) {
    let root = DirectoryGuard::private(&harness.paths.root).expect("actual private fixture root");
    put(&root.normalized_path().join(super::TOKEN_FILE_NAME), bytes);
    drop(root);
}

fn sentinels(harness: &Harness) {
    put(
        &harness.paths.root.join("unrelated.tmp"),
        b"unrelated temporary sentinel",
    );
    put(&harness.paths.root.join("sentinel"), b"ordinary sentinel");
    let directory = harness.paths.root.join("sentinel-directory");
    drop(DirectoryGuard::private(&directory).expect("private directory sentinel"));
    put(&directory.join("leaf"), b"directory sentinel");
}

fn check_sentinels(harness: &Harness) {
    same_bytes(
        &harness.paths.root.join("unrelated.tmp"),
        b"unrelated temporary sentinel",
    );
    same_bytes(&harness.paths.root.join("sentinel"), b"ordinary sentinel");
    same_bytes(
        &harness.paths.root.join("sentinel-directory/leaf"),
        b"directory sentinel",
    );
}

fn scratch(harness: &Harness) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut count = 0;
    for entry in fs::read_dir(&harness.paths.root).expect("actual fixture inventory") {
        count += 1;
        assert!(count <= 16, "fixture path inventory exceeds bound");
        let path = entry.expect("actual fixture entry").path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("fixture leaf UTF-8");
        if name.starts_with("dashboard.token.")
            && name
                .rsplit_once('.')
                .is_some_and(|(_, extension)| extension == "tmp")
        {
            found.push(path);
        }
    }
    found
}

fn permanent(harness: &Harness) {
    same_bytes(&harness.paths.root.join(super::TOKEN_LOCK_FILE_NAME), b"");
}

fn persisted(harness: &Harness, frame: &Frame) -> String {
    assert!(
        frame.ok && frame.length == 64,
        "token operation did not return a valid token"
    );
    let bytes = private_bytes(&super::token_path(&harness.paths));
    let token = std::str::from_utf8(&bytes).expect("actual token UTF-8");
    assert!(
        super::valid_token(token),
        "actual token violates the 64-hex contract"
    );
    assert!(
        locron_core::filesystem::is_private(&super::token_path(&harness.paths), false)
            .expect("actual token privacy")
    );
    let digest = support::digest(&bytes);
    assert_eq!(
        digest, frame.digest,
        "returned and persisted token digests differ"
    );
    digest
}

fn run(harness: &mut Harness, operation: Operation) -> Frame {
    let index = harness.spawn(harness.config(operation));
    harness.ready_all();
    harness.go(&[index]);
    let frame = harness.done(index);
    harness.finish();
    frame
}

fn success(frame: &Frame) {
    assert!(frame.ok, "actual fixture operation failed");
}
fn refused(frame: &Frame, kind: ErrorKind) {
    assert!(!frame.ok, "expected token refusal");
    assert_eq!(
        frame.kind,
        format!("{kind:?}"),
        "token refusal category changed"
    );
}

#[test]
fn fixture_child() {
    let Some(config) = std::env::var_os(support::FIXTURE_ENV) else {
        return;
    };
    let config: support::Config =
        serde_json::from_str(config.to_str().expect("fixture config UTF-8"))
            .expect("fixture config");
    support::fixture(&config);
}

#[test]
fn simultaneous_first_use_cross_process() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    for _ in 0..4 {
        let mut config = harness.config(Operation::Ensure);
        config.gates = vec![Event::Decision];
        harness.spawn(config);
    }
    harness.ready_all();
    assert!(
        !harness.paths.root.exists(),
        "first-use Ready created state"
    );
    harness.go(&[0, 1, 2, 3]);
    // Whichever actual first-use decision wins is held with its real lock.
    let winner = harness.wait_any(Event::Decision, harness.interleave());
    for index in 0..4 {
        if index != winner {
            let busy = harness.wait(index, Event::Busy, harness.interleave());
            assert_eq!(
                busy.stats.token_io, 0,
                "waiter entered token I/O before actual owner release"
            );
        }
    }
    harness.release(winner);
    let mut frames = Vec::new();
    for index in 0..4 {
        frames.push(harness.done(index));
    }
    harness.finish();
    let expected = persisted(&harness, &frames[0]);
    for frame in &frames {
        success(frame);
        assert_eq!(
            frame.digest, expected,
            "concurrent first use returned a different token"
        );
    }
    assert_eq!(
        frames.iter().map(|frame| frame.stats.decision).sum::<u32>(),
        1
    );
    assert_eq!(
        frames.iter().map(|frame| frame.stats.scratch).sum::<u32>(),
        1
    );
    let fifth = run(&mut harness, Operation::Ensure);
    assert_eq!(fifth.digest, expected);
    assert_eq!(fifth.stats.scratch, 0);
    assert_eq!(fifth.stats.writes, 0);
    permanent(&harness);
    harness.complete();
    println!("token-qualification:first-use:four-plus-fifth:complete");
}

#[test]
fn lifecycle_pairs_are_serialized_cross_process() {
    let _driver = support::driver();
    let operations = [Operation::Ensure, Operation::Regenerate, Operation::Remove];
    let mut cases = 0;
    for first in operations {
        for second in operations {
            let mut harness = Harness::new();
            seed(&harness, SEED);
            let mut a = harness.config(first);
            a.gates = vec![Event::Acquired];
            let mut b = harness.config(second);
            b.gates = vec![Event::Acquired];
            let a = harness.spawn(a);
            let b = harness.spawn(b);
            harness.ready_all();
            harness.go(&[a]);
            harness.wait(a, Event::Acquired, harness.interleave());
            harness.go(&[b]);
            let busy = harness.wait(b, Event::Busy, harness.interleave());
            assert_eq!(
                busy.stats.token_io, 0,
                "second transaction entered under the first lock"
            );
            harness.release(a);
            harness.wait(b, Event::Acquired, harness.interleave());
            let a_result = harness.done(a);
            success(&a_result);
            let intermediate = if first == Operation::Remove {
                assert!(
                    !super::token_path(&harness.paths).exists(),
                    "remove retained token"
                );
                None
            } else {
                Some(persisted(&harness, &a_result))
            };
            // All parent data readers have closed before the second real operation proceeds.
            harness.release(b);
            let b_result = harness.done(b);
            harness.finish();
            success(&b_result);
            if second == Operation::Remove {
                assert!(
                    !super::token_path(&harness.paths).exists(),
                    "serial remove retained token"
                );
            } else {
                let final_digest = persisted(&harness, &b_result);
                if second == Operation::Ensure && first != Operation::Remove {
                    assert_eq!(
                        Some(final_digest),
                        intermediate,
                        "serial ensure did not reuse intermediate state"
                    );
                } else if let Some(intermediate) = intermediate {
                    assert_ne!(
                        final_digest, intermediate,
                        "serial rotation did not replace the token"
                    );
                }
            }
            if first == Operation::Ensure {
                assert_eq!(a_result.digest, support::digest(SEED));
            }
            if first == Operation::Regenerate {
                assert_ne!(a_result.digest, support::digest(SEED));
            }
            permanent(&harness);
            harness.complete();
            cases += 1;
            println!("token-qualification:pair:{first:?}:{second:?}:complete");
        }
    }
    assert_eq!(cases, 9);
}

#[test]
fn contention_times_out_without_change_cross_process() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    seed(&harness, SEED);
    let holder = super::lock_token(&harness.paths, true).expect("actual parent token lock");
    for operation in [Operation::Ensure, Operation::Regenerate, Operation::Remove] {
        harness.spawn(harness.config(operation));
    }
    harness.ready_all();
    harness.go(&[0, 1, 2]);
    for index in 0..3 {
        harness.wait(index, Event::Busy, harness.deadline());
        let result = harness.done(index);
        refused(&result, ErrorKind::TimedOut);
        assert!(
            result.elapsed_ms >= 5000,
            "contended operation returned before original five seconds"
        );
        assert_eq!(result.stats.acquired, 0);
        assert_eq!(result.stats.token_io, 0);
        assert_eq!(result.stats.scratch, 0);
    }
    drop(holder);
    harness.finish();
    same_bytes(&super::token_path(&harness.paths), SEED);
    permanent(&harness);
    harness.complete();
    let mut late = Harness::new();
    seed(&late, SEED);
    let mut config = late.config(Operation::Ensure);
    config.delay = support::Delay::Lock;
    let index = late.spawn(config);
    late.ready_all();
    late.go(&[index]);
    let result = late.done(index);
    late.finish();
    refused(&result, ErrorKind::TimedOut);
    assert!(result.elapsed_ms >= 5000);
    assert_eq!(result.stats.attempts, 1);
    assert_eq!(result.stats.acquired, 0);
    assert_eq!(result.stats.token_io, 0);
    let next = run(&mut late, Operation::Ensure);
    assert_eq!(persisted(&late, &next), support::digest(SEED));
    late.complete();
    println!("token-qualification:timeout:E:R:D:actual-lock-late-return:complete");
}

#[test]
fn killed_lock_owner_releases_cross_process() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    seed(&harness, SEED);
    let mut owner = harness.config(Operation::Hold);
    owner.gates = vec![Event::Acquired];
    let owner = harness.spawn(owner);
    let waiter = harness.spawn(harness.config(Operation::Ensure));
    harness.ready_all();
    harness.go(&[owner]);
    harness.wait(owner, Event::Acquired, harness.interleave());
    harness.live(owner);
    harness.go(&[waiter]);
    harness.wait(waiter, Event::Busy, harness.interleave());
    // The control/gate stays open: this is an actual kill, not RELEASE or EOF unwind.
    harness.killed(owner);
    let stopped_reads = harness.stopped_reads(owner);
    let result = harness.done(waiter);
    let waiter_reads = harness.done_reads(waiter, &result);
    harness.finish();
    harness.assert_reads_retained(owner, stopped_reads);
    harness.assert_reads_retained(waiter, waiter_reads);
    assert_eq!(persisted(&harness, &result), support::digest(SEED));
    assert!(
        result.elapsed_ms < 5000,
        "same original-deadline waiter did not recover admission"
    );
    permanent(&harness);
    harness.complete();
    println!("token-qualification:actual-owner-kill:already-waiting-admission:complete");
}

#[test]
fn non_contention_errors_are_returned_once() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    seed(&harness, SEED);
    let mut config = harness.config(Operation::Ensure);
    config.fault = Some(Fault::Lock);
    let index = harness.spawn(config);
    harness.ready_all();
    harness.go(&[index]);
    let result = harness.done(index);
    harness.finish();
    refused(&result, ErrorKind::Other);
    assert_eq!(result.fault, Some(Fault::Lock));
    assert_eq!(result.stats.attempts, 1);
    assert_eq!(result.stats.sleeps, 0);
    assert_eq!(result.stats.token_io, 0);
    assert!(
        result.elapsed_ms < 5000,
        "non-contention fault entered the retry clock"
    );
    let next = run(&mut harness, Operation::Ensure);
    assert_eq!(persisted(&harness, &next), support::digest(SEED));
    harness.complete();
    let mut foreign = Harness::new();
    seed(&foreign, SEED);
    drop(
        DirectoryGuard::private(&foreign.paths.root.join(super::TOKEN_LOCK_FILE_NAME))
            .expect("foreign lock directory"),
    );
    let result = run(&mut foreign, Operation::Ensure);
    assert!(
        !result.ok && result.kind != "TimedOut",
        "actual directory refusal became contention"
    );
    assert_eq!(result.stats.attempts, 0);
    assert_eq!(result.stats.token_io, 0);
    assert!(
        foreign
            .paths
            .root
            .join(super::TOKEN_LOCK_FILE_NAME)
            .is_dir()
    );
    same_bytes(&super::token_path(&foreign.paths), SEED);
    foreign.complete();
    println!("token-qualification:non-contention:injected-lock:actual-directory:complete");
}

#[test]
fn exact_file_bounds_and_corrupt_inputs() {
    let _driver = support::driver();
    let mut payloads = Vec::new();
    for (spaces, trailing) in [
        (4032, None),
        (4033, None),
        (4032, Some(b'x')),
        (4031, Some(b'x')),
    ] {
        let mut bytes = SEED.to_vec();
        bytes.extend(std::iter::repeat_n(b' ', spaces));
        if let Some(trailing) = trailing {
            bytes.push(trailing);
        }
        payloads.push(bytes);
    }
    let mut invalid_utf8 = SEED.to_vec();
    invalid_utf8.push(0xff);
    payloads.push(invalid_utf8);
    let mut long_hex = SEED.to_vec();
    long_hex.push(b'0');
    payloads.push(long_hex);
    payloads.push(CANARY.to_vec());
    payloads.push(Vec::new());
    let lengths = [4096, 4097, 4097, 4096, 65, 65, CANARY.len(), 0];
    assert_eq!(payloads.len(), 8);
    for (index, bytes) in payloads.into_iter().enumerate() {
        assert_eq!(bytes.len(), lengths[index]);
        let mut harness = Harness::new();
        seed(&harness, &bytes);
        let result = run(&mut harness, Operation::Ensure);
        if index == 0 {
            success(&result);
            assert_eq!(result.digest, support::digest(SEED));
        } else {
            refused(&result, ErrorKind::InvalidData);
        }
        assert_eq!(result.stats.scratch, 0);
        same_bytes(&super::token_path(&harness.paths), &bytes);
        permanent(&harness);
        harness.complete();
        println!("token-qualification:bounds:{index}:complete");
    }
}

#[test]
fn atomic_failures_preserve_token_and_cleanup_owned_scratch() {
    let _driver = support::driver();
    let mut cases = 0;
    for fault in [Fault::Write, Fault::Sync, Fault::Rename] {
        for cleanup_fault in [false, true] {
            let mut harness = Harness::new();
            seed(&harness, SEED);
            sentinels(&harness);
            let mut config = harness.config(Operation::Regenerate);
            config.fault = Some(fault);
            config.cleanup_fault = cleanup_fault;
            let index = harness.spawn(config);
            harness.ready_all();
            harness.go(&[index]);
            let result = harness.done(index);
            let terminal_reads = harness.done_reads(index, &result);
            harness.poll_after_terminal(index, terminal_reads);
            harness.finish();
            assert!(!result.ok, "controlled atomic failure became success");
            assert_eq!(
                result.fault,
                Some(fault),
                "cleanup replaced the primary fault"
            );
            assert_eq!(
                result.kind,
                match fault {
                    Fault::Write => "WriteZero",
                    Fault::Sync => "Other",
                    _ => "PermissionDenied",
                }
            );
            assert_eq!(result.stats.scratch, 1);
            assert_eq!(result.stats.writes, 1);
            assert_eq!(result.stats.closed, 1);
            assert_eq!(result.stats.cleanup, 1);
            assert_eq!(
                result.stats.cleanup_fault,
                if cleanup_fault {
                    Some(Fault::Cleanup)
                } else {
                    None
                }
            );
            assert_eq!(result.stats.syncs, u32::from(fault != Fault::Write));
            assert_eq!(result.stats.renames, u32::from(fault == Fault::Rename));
            let scratch = scratch(&harness);
            if cleanup_fault {
                assert_eq!(scratch.len(), 1, "failed cleanup lost the owned scratch");
                let bytes = private_bytes(&scratch[0]);
                assert_eq!(bytes.len(), if fault == Fault::Write { 16 } else { 64 });
                assert!(
                    bytes.iter().all(u8::is_ascii_hexdigit),
                    "owned scratch is not actual generated hex"
                );
                assert!(
                    locron_core::filesystem::is_private(&scratch[0], false)
                        .expect("owned scratch privacy")
                );
            } else {
                assert!(scratch.is_empty(), "successful cleanup retained scratch");
            }
            same_bytes(&super::token_path(&harness.paths), SEED);
            check_sentinels(&harness);
            permanent(&harness);
            harness.complete();
            cases += 1;
            println!(
                "token-qualification:atomic-fault:{fault:?}:cleanup-fault:{cleanup_fault}:complete"
            );
        }
    }
    assert_eq!(cases, 6);
}

#[test]
fn failed_exclusive_create_preserves_foreign_scratch() {
    let _driver = support::driver();
    for directory in [false, true] {
        let mut harness = Harness::new();
        seed(&harness, SEED);
        sentinels(&harness);
        let foreign = harness.paths.root.join("dashboard.token.foreign.tmp");
        if directory {
            drop(DirectoryGuard::private(&foreign).expect("foreign scratch directory"));
            put(&foreign.join("leaf"), b"foreign directory bytes");
        } else {
            put(&foreign, b"foreign scratch bytes");
        }
        let mut config = harness.config(Operation::Regenerate);
        config.scratch_suffix = Some("foreign".to_owned());
        let index = harness.spawn(config);
        harness.ready_all();
        harness.go(&[index]);
        let result = harness.done(index);
        harness.finish();
        assert!(!result.ok, "exclusive foreign create became success");
        assert_eq!(result.stats.scratch, 0);
        assert_eq!(result.stats.writes, 0);
        assert_eq!(result.stats.syncs, 0);
        assert_eq!(result.stats.renames, 0);
        assert_eq!(result.stats.cleanup, 0);
        if directory {
            same_bytes(&foreign.join("leaf"), b"foreign directory bytes");
        } else {
            same_bytes(&foreign, b"foreign scratch bytes");
        }
        same_bytes(&super::token_path(&harness.paths), SEED);
        check_sentinels(&harness);
        permanent(&harness);
        harness.complete();
        println!("token-qualification:foreign-exclusive-create:directory:{directory}:complete");
    }
}

#[test]
fn missing_root_remove_does_not_create_state() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    let sibling = harness
        .paths
        .root
        .parent()
        .expect("fixture root parent")
        .join("sibling");
    fs::write(&sibling, b"sibling sentinel").expect("sibling sentinel");
    let result = run(&mut harness, Operation::Remove);
    success(&result);
    assert!(
        !harness.paths.root.exists(),
        "missing-root remove created state"
    );
    assert_eq!(result.stats.entered, 0);
    assert_eq!(result.stats.token_io, 0);
    assert_eq!(result.stats.scratch, 0);
    let mut sibling_bytes = Vec::new();
    fs::File::open(&sibling)
        .expect("sibling sentinel")
        .take(32)
        .read_to_end(&mut sibling_bytes)
        .expect("bounded sibling sentinel");
    let equal = sibling_bytes == b"sibling sentinel";
    assert!(equal, "missing-root remove changed a sibling");
    harness.complete();
    println!("token-qualification:missing-root-remove:complete");
}

#[test]
fn permanent_lock_identity_survives_remove() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    seed(&harness, SEED);
    let initial = run(&mut harness, Operation::Ensure);
    success(&initial);
    let initial_reads = harness.done_reads(0, &initial);
    let old = open_private(
        &harness.paths.root.join(super::TOKEN_LOCK_FILE_NAME),
        fs::OpenOptions::new().read(true).write(true),
    )
    .expect("retained actual old lock handle");
    let removed = run(&mut harness, Operation::Remove);
    success(&removed);
    let removed_reads = harness.done_reads(1, &removed);
    assert!(!super::token_path(&harness.paths).exists());
    assert!(
        harness
            .paths
            .root
            .join(super::TOKEN_LOCK_FILE_NAME)
            .is_file()
    );
    old.try_lock().expect("actual same old handle lock");
    let waiter = harness.spawn(harness.config(Operation::Ensure));
    harness.ready_all();
    harness.go(&[waiter]);
    let busy = harness.wait(waiter, Event::Busy, harness.interleave());
    assert_eq!(
        busy.stats.token_io, 0,
        "permanent lock was replaced under the retained old handle"
    );
    drop(old);
    let result = harness.done(waiter);
    let waiter_reads = harness.done_reads(waiter, &result);
    harness.finish();
    harness.assert_reads_retained(0, initial_reads);
    harness.assert_reads_retained(1, removed_reads);
    harness.assert_reads_retained(waiter, waiter_reads);
    persisted(&harness, &result);
    permanent(&harness);
    harness.complete();
    println!("token-qualification:permanent-old-handle-after-remove:complete");
}

#[test]
fn atomic_success_has_private_token_and_no_scratch() {
    let _driver = support::driver();
    let mut harness = Harness::new();
    seed(&harness, SEED);
    sentinels(&harness);
    let result = run(&mut harness, Operation::Regenerate);
    let actual = persisted(&harness, &result);
    assert_ne!(actual, support::digest(SEED));
    assert_eq!(result.stats.scratch, 1);
    assert_eq!(result.stats.writes, 1);
    assert_eq!(result.stats.syncs, 1);
    assert_eq!(result.stats.closed, 1);
    assert_eq!(result.stats.renames, 1);
    assert_eq!(result.stats.cleanup, 0);
    assert!(scratch(&harness).is_empty());
    check_sentinels(&harness);
    permanent(&harness);
    harness.complete();
    println!("token-qualification:actual-atomic-success:complete");
}

#[cfg(windows)]
fn sharing(frame: &Frame) {
    assert!(!frame.ok, "actual held handle did not refuse");
    assert!(
        matches!(frame.raw, Some(32 | 33)),
        "expected actual native sharing refusal"
    );
}

#[cfg(windows)]
#[test]
fn windows_actual_sharing_failures_are_owned() {
    let _driver = support::driver();
    let mut destination = Harness::new();
    seed(&destination, SEED);
    sentinels(&destination);
    let held = open_private(
        &super::token_path(&destination.paths),
        fs::OpenOptions::new().read(true),
    )
    .expect("actual no-delete destination handle");
    let result = run(&mut destination, Operation::Regenerate);
    sharing(&result);
    assert!(
        result.elapsed_ms >= 5000,
        "actual rename did not retain its existing retry clock"
    );
    assert_eq!(result.fault, None);
    assert_eq!(result.stats.renames, 1);
    assert_eq!(result.stats.cleanup, 1);
    drop(held);
    assert!(scratch(&destination).is_empty());
    same_bytes(&super::token_path(&destination.paths), SEED);
    check_sentinels(&destination);
    permanent(&destination);
    destination.complete();
    let mut cleanup = Harness::new();
    seed(&cleanup, SEED);
    sentinels(&cleanup);
    let mut config = cleanup.config(Operation::Regenerate);
    config.fault = Some(Fault::Write);
    config.gates = vec![Event::BeforeCleanup];
    let index = cleanup.spawn(config);
    cleanup.ready_all();
    cleanup.go(&[index]);
    let before = cleanup.wait(index, Event::BeforeCleanup, cleanup.interleave());
    assert_eq!(
        before.stats.closed, 1,
        "cleanup preceded actual producer leaf closure"
    );
    let scratch = scratch(&cleanup);
    assert_eq!(scratch.len(), 1);
    let held = open_private(&scratch[0], fs::OpenOptions::new().read(true))
        .expect("actual no-delete owned scratch handle");
    cleanup.release(index);
    let result = cleanup.done(index);
    cleanup.finish();
    refused(&result, ErrorKind::WriteZero);
    assert_eq!(result.fault, Some(Fault::Write));
    assert!(
        matches!(result.stats.cleanup_raw, Some(32 | 33)),
        "actual cleanup blocker did not refuse"
    );
    assert_eq!(private_bytes(&scratch[0]).len(), 16);
    same_bytes(&super::token_path(&cleanup.paths), SEED);
    check_sentinels(&cleanup);
    drop(held);
    locron_core::filesystem::remove_private_file(&scratch[0])
        .expect("remove only released owned scratch");
    permanent(&cleanup);
    cleanup.complete();
    println!("token-qualification:native-sharing:destination:owned-cleanup:complete");
}

#[cfg(windows)]
fn candidate(harness: &Harness, config: &support::Config) -> PathBuf {
    harness.paths.root.join(format!(
        "{}.{}.pending",
        super::TOKEN_LOCK_FILE_NAME,
        config.nonce
    ))
}

#[cfg(windows)]
fn final_identity(harness: &Harness) -> locron_core::filesystem::FileIdentity {
    let file = open_private(
        &harness.paths.root.join(super::TOKEN_LOCK_FILE_NAME),
        fs::OpenOptions::new().read(true),
    )
    .expect("actual retained winner identity handle");
    locron_core::filesystem::file_identity(&file).expect("actual full winner identity")
}

#[cfg(windows)]
#[test]
fn windows_final_lock_publication_hides_live_constructor() {
    let _driver = support::driver();
    // Assert the static constructor/ACL and Dispose slices against immutable Core source.
    let core = include_str!("../../../locron-core/src/windows/filesystem_worker.ps1")
        .replace("\r\n", "\n");
    let fixture = include_str!("constructor_fixture.ps1").replace("\r\n", "\n");
    let (_, body) = core
        .split_once("            'create_file' {\n")
        .expect("Core create slice");
    let (acl, _) = body
        .split_once("                    $file.Dispose()\n")
        .expect("Core constructor slice");
    assert!(
        fixture.contains(acl),
        "native constructor/ACL slice diverged from Core"
    );
    assert!(
        fixture.contains("                    $file.Dispose()\n"),
        "native Dispose slice diverged from Core"
    );
    let mut negative = Harness::new();
    drop(DirectoryGuard::private(&negative.paths.root).expect("private negative root"));
    let a = negative.spawn(negative.config(Operation::ConstructorFinal));
    let b = negative.spawn(negative.config(Operation::Ensure));
    negative.ready_all();
    negative.go(&[a]);
    negative.wait(a, Event::Constructor, negative.interleave());
    negative.live(a);
    negative.go(&[b]);
    let refusal = negative.done(b);
    sharing(&refusal);
    assert_eq!(refusal.raw, Some(32));
    assert_eq!(refusal.stats.attempts, 0);
    assert_eq!(refusal.stats.token_io, 0);
    assert!(
        std::time::Instant::now() < negative.interleave(),
        "sharing refusal was retried through the original clock"
    );
    negative.release_constructor(a);
    success(&negative.done(a));
    negative.finish();
    permanent(&negative);
    negative.complete();
    println!("token-qualification:publication:actual-final-constructor-raw32:complete");

    for hidden_constructor in [true, false] {
        let mut harness = Harness::new();
        drop(DirectoryGuard::private(&harness.paths.root).expect("private publication root"));
        let mut a_config = harness.config(Operation::Ensure);
        a_config.constructor = hidden_constructor;
        a_config.gates = vec![if hidden_constructor {
            Event::ClosedUnpublished
        } else {
            Event::BeforePublish
        }];
        let a_path = candidate(&harness, &a_config);
        let mut b_config = harness.config(Operation::Ensure);
        b_config.gates = if hidden_constructor {
            vec![Event::Acquired]
        } else {
            vec![Event::BeforePublish, Event::Acquired]
        };
        let a = harness.spawn(a_config);
        let b = harness.spawn(b_config);
        harness.ready_all();
        harness.go(&[a]);
        if hidden_constructor {
            let held = harness.wait(a, Event::Constructor, harness.interleave());
            assert_eq!(held.length, 0, "live constructor candidate was not empty");
            harness.live(a);
            let absent = open_private(
                &harness.paths.root.join(super::TOKEN_LOCK_FILE_NAME),
                fs::OpenOptions::new().read(true),
            )
            .expect_err("live hidden constructor exposed the final");
            assert_eq!(absent.kind(), ErrorKind::NotFound);
            harness.go(&[b]);
            harness.wait(b, Event::Acquired, harness.interleave());
            harness.release_constructor(a);
            harness.wait(a, Event::ClosedUnpublished, harness.interleave());
        } else {
            harness.wait(a, Event::BeforePublish, harness.interleave());
            harness.go(&[b]);
            harness.wait(b, Event::BeforePublish, harness.interleave());
            harness.release(b);
            harness.wait(b, Event::Acquired, harness.interleave());
        }
        same_bytes(&a_path, b"");
        let identity = final_identity(&harness);
        harness.release(a);
        let busy = harness.wait(a, Event::Busy, harness.interleave());
        assert_eq!(busy.stats.collision, 1);
        assert_eq!(busy.stats.candidate_cleanup, 1);
        assert!(
            !a_path.exists(),
            "timely loser cleanup retained its closed candidate"
        );
        assert_eq!(
            final_identity(&harness),
            identity,
            "collision replaced the permanent winner identity"
        );
        harness.release(b);
        let a_result = harness.done(a);
        let b_result = harness.done(b);
        harness.finish();
        assert_eq!(
            persisted(&harness, &a_result),
            persisted(&harness, &b_result)
        );
        permanent(&harness);
        harness.complete();
        println!(
            "token-qualification:publication:hidden-constructor:{hidden_constructor}:no-clobber:complete"
        );
    }

    for delay in [
        support::Delay::BeforePublish,
        support::Delay::AfterCreate,
        support::Delay::AfterPublish,
    ] {
        let mut harness = Harness::new();
        seed(&harness, SEED);
        sentinels(&harness);
        let mut config = harness.config(Operation::Ensure);
        config.delay = delay;
        let late_success = delay == support::Delay::AfterPublish;
        let path = candidate(&harness, &config);
        let index = harness.spawn(config);
        harness.ready_all();
        harness.go(&[index]);
        let result = harness.done(index);
        harness.finish();
        refused(&result, ErrorKind::TimedOut);
        assert!(result.elapsed_ms >= 5000);
        assert_eq!(result.stats.token_io, 0);
        assert_eq!(result.stats.candidate_attempts, 1);
        assert_eq!(result.stats.candidate_created, 1);
        assert_eq!(result.stats.candidate_closed, 1);
        assert_eq!(result.stats.publish, u32::from(late_success));
        assert_eq!(result.stats.candidate_cleanup, 0);
        if late_success {
            assert!(!path.exists());
            permanent(&harness);
            let next = run(&mut harness, Operation::Ensure);
            assert_eq!(persisted(&harness, &next), support::digest(SEED));
        } else {
            same_bytes(&path, b"");
            assert!(
                !harness
                    .paths
                    .root
                    .join(super::TOKEN_LOCK_FILE_NAME)
                    .exists(),
                "expired unpublished candidate became final"
            );
        }
        same_bytes(&super::token_path(&harness.paths), SEED);
        check_sentinels(&harness);
        harness.complete();
        println!("token-qualification:publication:controlled-expiry:{delay:?}:complete");
    }

    for directory in [false, true] {
        let mut harness = Harness::new();
        seed(&harness, SEED);
        sentinels(&harness);
        let config = harness.config(Operation::Ensure);
        let foreign = candidate(&harness, &config);
        if directory {
            drop(DirectoryGuard::private(&foreign).expect("foreign candidate directory"));
            put(&foreign.join("leaf"), b"foreign candidate directory bytes");
        } else {
            put(&foreign, b"foreign candidate bytes");
        }
        let index = harness.spawn(config);
        harness.ready_all();
        harness.go(&[index]);
        let result = harness.done(index);
        harness.finish();
        assert!(
            !result.ok,
            "foreign candidate exclusive create became success"
        );
        assert_eq!(result.stats.candidate_attempts, 1);
        assert_eq!(result.stats.candidate_created, 0);
        assert_eq!(result.stats.candidate_closed, 0);
        assert_eq!(result.stats.publish, 0);
        assert_eq!(result.stats.candidate_cleanup, 0);
        assert_eq!(result.stats.token_io, 0);
        if directory {
            same_bytes(&foreign.join("leaf"), b"foreign candidate directory bytes");
        } else {
            same_bytes(&foreign, b"foreign candidate bytes");
        }
        assert!(
            !harness
                .paths
                .root
                .join(super::TOKEN_LOCK_FILE_NAME)
                .exists()
        );
        same_bytes(&super::token_path(&harness.paths), SEED);
        check_sentinels(&harness);
        harness.complete();
        println!(
            "token-qualification:publication:foreign-candidate:directory:{directory}:complete"
        );
    }

    let mut collision = Harness::new();
    seed(&collision, SEED);
    sentinels(&collision);
    let mut a_config = collision.config(Operation::Ensure);
    a_config.gates = vec![Event::BeforePublish, Event::BeforeCleanup];
    let a_path = candidate(&collision, &a_config);
    let mut b_config = collision.config(Operation::Ensure);
    b_config.gates = vec![Event::BeforePublish, Event::Acquired];
    let a = collision.spawn(a_config);
    let b = collision.spawn(b_config);
    collision.ready_all();
    collision.go(&[a, b]);
    collision.wait(a, Event::BeforePublish, collision.interleave());
    collision.wait(b, Event::BeforePublish, collision.interleave());
    collision.release(b);
    collision.wait(b, Event::Acquired, collision.interleave());
    let identity = final_identity(&collision);
    collision.release(a);
    collision.wait(a, Event::BeforeCleanup, collision.interleave());
    let held = open_private(&a_path, fs::OpenOptions::new().read(true))
        .expect("actual no-delete candidate cleanup blocker");
    collision.release(a);
    let result = collision.done(a);
    sharing(&result);
    assert_eq!(result.stats.collision, 1);
    assert_eq!(result.stats.candidate_cleanup, 1);
    assert_eq!(result.stats.acquired, 0);
    assert_eq!(result.stats.token_io, 0);
    assert_eq!(result.fault, None);
    assert_eq!(final_identity(&collision), identity);
    collision.release(b);
    success(&collision.done(b));
    collision.finish();
    same_bytes(&a_path, b"");
    same_bytes(&super::token_path(&collision.paths), SEED);
    check_sentinels(&collision);
    drop(held);
    locron_core::filesystem::remove_private_file(&a_path)
        .expect("remove only released owned candidate");
    permanent(&collision);
    collision.complete();
    println!("token-qualification:publication:actual-collision-cleanup-refusal:complete");

    for cleanup_fault in [false, true] {
        let mut harness = Harness::new();
        seed(&harness, SEED);
        sentinels(&harness);
        let mut config = harness.config(Operation::Ensure);
        config.fault = Some(Fault::Publish);
        config.cleanup_fault = cleanup_fault;
        let path = candidate(&harness, &config);
        let index = harness.spawn(config);
        harness.ready_all();
        harness.go(&[index]);
        let result = harness.done(index);
        harness.finish();
        refused(&result, ErrorKind::PermissionDenied);
        assert_eq!(result.fault, Some(Fault::Publish));
        assert_eq!(result.stats.publish, 1);
        assert_eq!(result.stats.candidate_cleanup, 1);
        assert_eq!(
            result.stats.cleanup_fault,
            if cleanup_fault {
                Some(Fault::Cleanup)
            } else {
                None
            }
        );
        assert_eq!(result.stats.token_io, 0);
        if cleanup_fault {
            same_bytes(&path, b"");
        } else {
            assert!(!path.exists());
        }
        assert!(
            !harness
                .paths
                .root
                .join(super::TOKEN_LOCK_FILE_NAME)
                .exists()
        );
        same_bytes(&super::token_path(&harness.paths), SEED);
        check_sentinels(&harness);
        harness.complete();
        println!(
            "token-qualification:publication:injected-primary:cleanup-fault:{cleanup_fault}:complete"
        );
    }
    println!("token-qualification:native-publication:all-dispositions:complete");
}
