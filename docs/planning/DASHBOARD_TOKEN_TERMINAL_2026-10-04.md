# PR141 measured failure and selected terminal peer repair

Root selection, 2026-10-04: preserve the frozen product SPEC and existing token production/ownership contract. This selection repairs the measured lint diagnostics and the Source-established extra-read terminal contract gap; it does not establish the historical RST phase/cause or fix the independent x64 supervisor gate failure. No Source change has occurred in this selection commit. All fresh runtime/native/strict-gate qualification remains pending. Follow the four ordered steps and their concrete Verify criteria below. The original research observations and explicit UNKNOWNs are retained in full. The only selected Source paths are token.rs, token/test_support.rs and token/qualification.rs; no clock, producer, cleanup, workflow, dependency, service or operating-system change is selected.

Read-only research at `4fa326b8f0e35ba41cc49c3517b8475b2f09728a`, tree `b1508c0f83760533ed30be16506ad410f11a13b6`, against merged main `9de596f8ffe48e8036f775ad1fddbea9302915a1`. Root selects the exact three-Source-path repair and C1/C2/C3 assertion groups below. Before separate Source implementation, Root must record Issue162, GET the exact comment, and reread this complete committed plan. No Source, Docs, Issue, Git ref, CI, memory, fixture, compiler, PowerShell or live owner-PC operation was performed in this research.

## Actual result, Source inference and unknowns

Ordinary run `37204722580/1` completed FAILURE on the exact 4fa head. Completed job metadata has eleven SUCCESS, six FAILURE and two optional SKIP rows. The saved five raw/clean log pairs are hashed in `inventory.json` and individually connected to actual observations in `actual-evidence-matrix.json`. A successful package or Unix job is not evidence that the new Windows protocol correction ran. Package archives were not inspected in this research.

| Evidence class | Observed result | Exact reference |
| --- | --- | --- |
| Actual Linux warnings-denied Clippy | `unchecked_time_subtraction` at support342 and `needless_pass_by_value` at support750; lib-test compile fails with two diagnostics | lint-linux clean438..467; raw SHA256 `894763345fc8a57f79d487905b889806d4280d42d1ae8fd1413b0f0aadf5d966` |
| Actual Windows warnings-denied Clippy | `question_mark` at token248 and255; non-test library compile fails with two diagnostics | lint-windows clean407..433; raw SHA256 `dfbafcfffddca853085a194c6d9c6f4cb57adbbb8ee9bd91af3e633097f1afe3` |
| Actual Windows x64/ARM/MSRV token tests | First independent failure is `atomic_failures_preserve_token_and_cleanup_owned_scratch`: `fixture control read failed: kind=ConnectionReset raw=Some(10054)` at support737. Each row then has twelve `DRIVERS` poison failures at support30. Each reports43PASS13FAIL; original token3PASS | x64 clean869..870/948; ARM873..874/952; MSRV777..778/856 |
| Actual x64 service test | `native_io_expiry_quarantines_activation_child_and_all_later_admission` fails at supervisor1687, `the actual native I/O boundary was not entered`; separate service suite63PASS1FAIL | x64 clean1237..1247; raw SHA256 `114d8de7d875797f0cc05adb9c8c13f58e7edf603ed875c0b8fb7cc0076a3bba` |
| Actual ARM/MSRV service test | The same service case passes; each separate service suite64PASS0FAIL | ARM clean1233/1239; MSRV1081/1087 |
| Source-established harness gap | After validating and storing terminal `Done`, the owner loop immediately performs another socket read. Future polls also keep reading that completed owner | support932..949; next `while let Some(frame) = channel.poll(deadline)` iteration |
| Unobserved current-failure facts | Last validated frame, actual fixture Child PID, actual pre-cleanup exit status/code, capture contents and which of the six atomic-fault subcases was reached are not reported. Done-before-reset versus reset-before-Done is unknown | read panic737..741 contains only ErrorKind/raw; first subcase marker appears only after complete at qualification519..522 |

The thirteen Windows failures are not thirteen independently observed token failures. The twelve later cases stop at the driver mutex before their fixture work. The original three sequential successes do not qualify the thirteen new drivers. This research establishes a test-protocol contract defect and four exact lint diagnostics; it does not establish a production token/ACL/native constructor failure.

The first atomic-fault configuration does not select the PowerShell constructor (`Config.constructor=false`, support807); the constructor fixture is selected by separate Windows drivers. Therefore raw10054 cannot be attributed to the FullControl constructor from this evidence. A successful returned-error token `Done(ok=false)` is also not a failing child entry: normal fixture tests must still exit0.

## Call and lifetime boundaries

1. Actual `Command::spawn` creates a fixture test-binary child; `Owned` is stored immediately at support842..855. stdout/stderr are private `NamedTempFile::new` captures. The retained Child, nonce and channels are the ownership references. Parent lib-test thread IDs in panic lines do not identify these Child PIDs.
2. `Channel::poll` checks the unchanged absolute deadline, reads one actual TCP byte at a time, limits the entire LF-terminated frame to1024, parses JSON, checks sequence and max64 events, and returns one frame at support714..747. EOF/WouldBlock return None; all other read errors panic. PID/nonce and native-completion linkage are checked in the shared Harness path at support933..948 before a frame is admitted to `Owned.frames`.
3. The actual child finishes the real token API operation, creates one `Done` result, checks errors for token/corrupt canaries, and sends Done at support648..693. Scope destruction then releases its owned TCP control. The parent's existing `done()` waits for that frame, not EOF. The parent loop nevertheless reads again before returning Done to the driver. Source alone does not reveal which native close outcome the hosted failure encountered.
4. `finish()` retains the actual Child until confirmed `try_wait`, requires exit success for normal Done entries, reads bounded stdout/stderr after reap and checks SEED/CANARY/current persisted token privacy (support1101..1177). `complete()` performs checked teardown only after every status is confirmed (1179..1204). These assertions must remain exact.
5. The explicit hard-stop test keeps the Acquired gate/control open and performs actual Child kill + nonzero reap under3s at support1072..1098. Its waiter continues using the original five-second token clock (qualification330..356). This intentional stop is distinct from an unexpected active-peer reset or an incomplete normal child.
6. Panic cleanup uses the original common3s kill-all then reap-all Drop (support1208..1242). A status first returned after that cleanup kill is not evidence of the child's pre-reset outcome. Known cleanup returns permit capture deletion; unknown ownership disables cleanup and retains them. The provided logs expose no actual child capture bytes or pre-cleanup exit status. REST artifact metadata exposes only four package artifacts, and workflow152..164 names binaries/native metadata, not token captures. Archive contents remain uninspected.

## Proposed finite Source scope

Only the three Source paths listed below are selected. No Cargo, Cargo.lock, Core, CLI, Store, router, PowerShell, workflow, producer wire format, public API, clock or new native gate change is needed for the token harness repair.

| Source path at4fa | Expected whole-line patch region | Proposed change |
| --- | --- | --- |
| `crates/locron-server/src/token.rs` |248..252 and255..259 only | Use `lock_remaining(deadline).inspect_err(...) ?` at each existing check; cfg(test) closure retains exactly the current observe-Expired statement and the same returned original error |
| `crates/locron-server/src/token/test_support.rs` | Instruction derive172; begin342; Channel697..753; Owned755..766/845..855; owner polling930..951; killed1085..1091; bounded test-only terminal receipt/assertion helpers beside existing Harness methods | Add Copy/Clone to payload-free Instruction; checked_sub derives the same original production origin; retain validated terminal Done or confirmed intentional kill, freeze the actual read-call count, and stop subsequent reads of only that terminal owner |
| `crates/locron-server/src/token/qualification.rs` | Add assertions in original atomic-fault454..526, killed-owner330..356 and permanent-identity600..638 bodies | Verify terminal read counts and bindings using the actual existing producers/retained Children. Keep every pre-existing assertion, outcome, cleanup and marker; no new functional driver or support child selector |

The first two lint normalizers are literal and bounded:

```rust
#[derive(Clone, Copy, Serialize, Deserialize)]
enum Instruction { Go, Release }

control.production = Some(
    deadline.checked_sub(super::TOKEN_LOCK_TIMEOUT)
        .expect("fixed token fixture production origin"),
);
```

At both token expiry checks, preserve the test-only observation with the returned-error `inspect_err` form selected by Root:

```rust
lock_remaining(deadline).inspect_err(|_error| {
    #[cfg(test)]
    test_support::observe(root_path, test_support::Event::Expired);
})?;
```

Do not reset the time origin with another Instant::now, use saturating_sub as a replacement, suppress a lint, or change error precedence. Current token5s, WouldBlock-only10ms retry and every candidate close/publication/disposal check remain exact. Future warning/type acceptance is pending until the changed head actually runs.

## Terminal owner semantics

Use one private finite owner state: Active; Done with the admitted terminal sequence/read receipt; Stopped with the confirmed nonzero status/read receipt. A terminal state is never overwritten by another transition. This is a test-harness protocol state, not a token admission state. Initialize Active on actual Child ownership. Preserve the actual Child, channel Some, native channel/PID, nonce, frames and status. Dropping/taking the terminal channel would break `ready_all` when the same Harness later spawns another child, so that is not selected.

For Active, execute the unchanged shared Channel parser/read path and all existing Harness PID/nonce/native-completion/event limits. Only after those checks and storing the full Done frame, freeze the actual Rust read-call receipt, set Done and break immediately before another read. Future Harness polls skip this terminal owner while continuing to poll other active owners and pending peers. Do not catch ConnectionReset and turn it into None, Ready, Done or success. A terminal transition does not certify Child exit.

For Stopped, transition only after the existing actual kill, actual nonzero try_wait result and timely3s check. Do not synthesize Done or infer success from a kill request, PID string or elapsed timer. The existing same-deadline waiter still executes its actual lock attempt and token operation. Unexpected reset/EOF of an Active child stays an incomplete protocol failure; it cannot enter this intentional state.

Keep all old `finish()` checks exactly. A valid Done followed by a nonzero normal Child still FAILs. Missing Done still fails the original expected-event/deadline path. Done encountered before a requested intermediate event still fails the existing wait/wait_any assertion. Invalid JSON, partial pre-Done framing, >1024 frame, >64 events, wrong sequence/PID/nonce and invalid native-completion linkage remain FAIL. Before-Done partial EOF is not promoted to terminal: the current wait reaches its original absolute failure clock. Pending-peer reset also remains a failure.

At the admitted Done boundary the current one-byte parser has drained through LF and its user buffer is empty. Stopping reads cannot inspect hypothetical bytes remaining in the kernel after Done. Qualification relies on the owned producer's last-Done protocol plus actual exit0/capture/privacy checks. Do not claim arbitrary post-Done trailing bytes were observed or validated; testing them would require a different protocol selection, not a silent change in this lease.

Increment a test-private optional-u64 read-call receipt immediately before each actual `TcpStream::read` in the shared Channel. Do not count parser iterations or model predictions. Start Some(0); checked_add overflow makes the observation None and cannot change token work, ownership, completion or admission. Terminal verification must require observed Some values and equality to the frozen transition receipt; None is unqualified, never a fabricated zero. No new lock, socket, process, timer or event is introduced for the receipt.

Retain `frame.sequence`, actual Child PID/nonce validation, empty bounded user buffer and `next_sequence==terminal_sequence+1` as coherent terminal facts. Stopped retains no fabricated terminal frame. No secret/path/capture bytes need be printed for these assertions.

## Three existing actual control groups

| Fixed group | Actual existing path and additional oracle | Cardinality / evidence boundary |
| --- | --- | --- |
| C1 Done from an actual failed token operation | Original write/sync/rename × cleanup success/refusal6 cases. After `done`, verify validated terminal binding and read count frozen at transition; make exactly one extra shared Harness poll and assert no terminal owner read. Preserve original finish exit0, primary-fault, scratch ownership/privacy, old-token, sentinel, permanent lock and checked teardown assertions |6 actual subcases inside the existing one functional driver; no new producer mode. This measures no post-Done Rust read, not the historical reset cause |
| C2 retained completed peers during new admission | Original permanent-identity flow has actual Ensure, Remove and later waiting Ensure. Snapshot terminal receipts for the first two actual children before later active-child poll; after that child completes, require old counts unchanged. Preserve original retained old lock/Busy/token-I/O0, drop-old-handle, persisted token and permanent identity assertions |3 actual owned children in the existing driver; shared GO+15s retained. Confirms completed channels stay Some while old peers are not reread |
| C3 intentional hard stop while a real waiter survives | Original held-Acquired actual Child, already-Busy waiter, actual kill/nonzero reap. Freeze killed-peer read receipt only after timely confirmed reap; after waiter's Done/finish require killed receipt unchanged and waiter Done/exit0. Preserve actual owned Child, no RELEASE/EOF unwind and original elapsed<5000 assertion |2 actual children in the existing driver. Before-stop reads remain Active; unknown reap cannot certify terminal ownership |

The original count stays11 portable functional drivers and2 additional Windows functional drivers; one fixture support entry remains separately counted. Added assertion groups do not become three new drivers. C1 has6 actual subcases, C2 has3 retained normal child entries, C3 has2 children. Windows typed frame/exit/capture facts must come from fresh x64/ARM/MSRV raw logs after Source changes; these assertions are currently proposals, not measured successes.

Pre-Done rejection and nonzero-child preservation are Source inverse obligations in this minimal slice. No new injected RST producer, fake child, arbitrary socket linger/FFI, ignored mutex poison or fabricated negative native result is selected. The active-path read error condition, fixed ErrorKind/raw panic and all existing frame/sequence checks must be inverse-equal. Therefore the proposal does not add a measured negative RST or malformed-child result; if Root requires that additional runtime evidence, it must select a separate bounded producer/control plan before Source.

## Failure diagnostics and remaining native unknowns

No diagnostic-message extension, error-arm status query, capture read, new producer mode or cleanup change is selected in the minimal three-path proposal. The existing read error prints only already-returned ErrorKind/raw. Before-Done read failure still lacks pre-cleanup exit status and capture cause. If future changed-head logs expose another active-peer failure, Root may separately select bounded already-observed channel/Child diagnostics after Docs/Issue readback; that is not authorization in this proposal. Do not print arbitrary error Debug/Display, frames, token/hash/canary values, nonce, root paths or captured bytes. Preserve common3s Drop byte-for-byte; its later killed/reaped status must never masquerade as a pre-reset status. Missing historical capture contents cannot be reconstructed from parent test output.

The separate supervisor failure is inherited byte-for-byte from main9de (`windows_supervisor.rs` SHA256 `4b944fb1a4a0e421323c05b0c1c9c0b64d8b582499044d6fd64bcde668e2cabc`). Its loop tries BeforeMetadataRead, MetadataRead, FactSync and FactRename; the current fixed failure message does not name the selected iteration. The100ms request and250ms return assertions precede the failed entered assertion, so the observed request expired and returned within the existing bound while its selected gate had not been observed. Neither exact iteration nor CPU/native/worker-queue cause is measured. Budget pre/post checks, Worker dispatch/response and pre-gate actual child/filesystem calls permit several histories; Source does not choose one.

Do not change supervisor clocks, gate placement, global serialization or production code as part of the token lease. The same unresolved gate failure already appears in preserved PR135 FINDINGS5393..5407 / WINDOWS_DEFAULT_DOCTOR plan131..138; this is contextual history, not a new cause proof. Route its actual new observation to the relevant existing lifecycle Issue through Root, with #30 linkage. If Source must be changed later, first select an observation-only failure message naming the existing finite Boundary and actual elapsed/gate flags; native call entry/return observations need a separate reviewed finite plan. Keeping this failure recorded is required; a token harness repair cannot close it.

## Proposed Docs-first ordered plan and Verify

1. Root records the exact measured4fa failures, terminal contract gap, unknown Child/capture/reset phase, selected three-path repair and C1/C2/C3 finite assertion groups in FINDINGS/IMPLEMENTATION/DASHBOARD_TOKEN_SERIALIZATION and Issue162; link the independent supervisor observation to its existing lifecycle task. **Verify:** exact Issue comment GET body equals the created request, complete committed plan reread, clean exact4fa Source parent, frozen SPEC and protected main139/138 behavior; no speculative native cause.
2. Separate developer applies only the four lint normalizers and the Active/verified-Done/actually-killed-and-reaped-Stopped peer/read receipt changes in token.rs and test_support.rs. Preserve producer/framing/ownership/cleanup/locks/clocks and all existing assertions. **Verify:** whole-file inverse restores4fa outside listed whole-line hunks; both `inspect_err` checks execute the same original lock_remaining call and return its same error; the non-test observer closure is empty; test Expired observations remain literal; Unix token code remains exact. No allow, retry/reset swallow, deadline reset or new native gate.
3. Separate developer adds only C1/C2/C3 extra terminal assertions to the three existing qualification bodies. **Verify:** original body statements/assertions/markers are retained in order; table cardinalities are6/3/2 with11portable+2Windows functional drivers and one support entry unchanged; actual shared read counter is advanced only at Rust TCP read admission and transition receipts are checked immediately and after later active polls. Original frame/capture/event/child bounds and5s/10ms/10s/4s/15s/3s constants remain exact.
4. Root independently reads all three changed files and diff/inverse/protected receipts, then performs ordinary publication and fresh changed-head qualification under unchanged workflows. **Verify:** static rustfmt1.94/1.98, locked offline metadata and diff/mode-blob checks pass; hosted warnings-denied Clippy succeeds on its actual1.98 targets, server lib/contract and all original ordinary/native x64/ARM/MSRV jobs execute. New C1/C2/C3 assertions must actually pass with each original13 driver/old3 test and bound/privacy/ownership cases. Preserve actual supervisor failure until resolved under its own selected plan, and require original full gates/Guardian/paired provenance before merge. Do not rerun an unchanged failed head or infer fresh acceptance from old package/job metadata.

Static checks are not native/type/Clippy acceptance. This research ran no compiler, fixture, native token API, app import, service, PowerShell, live token or CI dispatch. The Source proposal has not been compiled or executed; capture/child pre-reset cause and the exact supervisor boundary/cause remain incomplete. The selected complete plan is subject to exact Issue162 GET readback and Root final reread before any separate Source mutation.
