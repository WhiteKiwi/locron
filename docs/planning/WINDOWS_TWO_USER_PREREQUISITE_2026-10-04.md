# First native two-user prerequisite, before privacy/IPC acceptance

Reviewed implementation plan; 2026-10-04. The frozen product specification is unchanged.
The exact application base is `55859361892a422c7d3868b741431480ec435503`. Research `03ee16d` remains historical evidence.
Root owns this plan, Issue #27/#29 Verify and publication; the separate development session
owns the five Source paths below after exact Issue readback. Hosted native account and
fixture effects are confined to the manual disposable GitHub job. Owner-PC account, ACL,
policy, task, installer and reboot effects are outside this plan. The ordinary/full CI
workflow remains conserved on the application branch.

The deliverable is one explicitly invoked hosted prerequisite: two real Users-only credential identities, their same-token child trees owned through existing public `OwnedChild`, and confirmed cleanup of the exact newly created accounts. It does not execute locron jobs, open application state, register tasks, test private file/pipe authorization, qualify profiles for permanent installation, or complete Issues #27/#29. These limitations are acceptance boundaries, not skipped test cases.

## Frozen source and workflow boundary

Use a separate verification branch based on the exact reviewed application revision. On that branch only, replace the **existing** `.github/workflows/ci.yml` with a manual-only `workflow_dispatch` workflow. Keep the canonical branch's ordinary/full CI byte-for-byte intact, do not create a required-check alias, and do not import this replacement into main. This follows the already used per-binary verification-branch route and avoids a new workflow filename that is absent from the default branch. GitHub requires a dispatch workflow to exist on the default branch and supports selecting a branch/tag ref when dispatching. [Manual workflow dispatch](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)

The allowed Source paths are exactly:

| Path | Allowed change |
| --- | --- |
| `crates/locron-engine/examples/windows_user_prerequisite.rs` | New documented example, strict explicit role arguments; actual prerequisite only when directly invoked. |
| `crates/locron-engine/Cargo.toml` | Explicit example target with `test=false`, `bench=false`; Windows-only **dev** edges `windows-permissions = "=0.2.4"` and `sha2.workspace = true` (locked workspace 0.10.9/std). |
| `Cargo.lock` | Only engine dependency mappings for already locked `windows-permissions` and `sha2 0.10.9`; no package/version/checksum change. |
| `scripts/test-windows-user-prerequisite.ps1` | New fixed stock PowerShell 5.1/.NET orchestrator, all effectful code confined to the disposable hosted job. |
| `.github/workflows/ci.yml` | Manual-only verification-branch replacement, two native rows and bounded sanitized receipt upload. |

No engine/core production module, public API, CLI/GUI binary target, dependency version, release asset, existing test or lint policy changes. Cargo examples use public library and dev dependencies. **`test=false` controls default selection, not all explicit CLI selection:** default `cargo test` builds an untested example for compilation checking; `cargo test --example windows_user_prerequisite`, `--examples`, or `--all-targets` can instead select it as a test. `--all-targets` includes `--examples`. Keep the existing default `harness=true`; no `harness=false` is proposed. An explicitly selected libtest build replaces example `main`, so this example with no `#[test]` functions can produce zero-case output without running the effectful native entry point. [Cargo test target selection](https://doc.rust-lang.org/cargo/commands/cargo-test.html#target-selection), [Cargo example/harness defaults](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#examples)

The Rust 1.94 Cargo implementation confirms the interaction: `new_all_targets` creates a specific filter including examples, and the example's test-to-build conversion only occurs for a nonspecific filter with `test=false`. [Pinned filter](https://github.com/rust-lang/cargo/blob/rust-1.94.0/src/cargo/ops/cargo_compile/compile_filter.rs#L147-L156), [Pinned unit mode](https://github.com/rust-lang/cargo/blob/rust-1.94.0/src/cargo/ops/cargo_compile/unit_generator.rs#L71-L80). No `#[test]`, `#[ignore]`, missing-env success/return or feature-based silent target removal is added. Missing/unknown role arguments and non-Windows execution of the normal example return nonzero. **Compilation, target selection, or zero-case libtest output contributes zero prerequisite acceptance cases.** Only the explicit orchestrator invocation of the normal native example, with two actual credential executions and the full token/Job/cleanup receipt, qualifies this prerequisite; ordinary/full summaries never substitute for that manual evidence.

Orchestrator and example refuse before effects unless the explicit hosted-prerequisite mode and GitHub-hosted dispatch context are present (`GITHUB_ACTIONS=true` and the workflow-provided `runner.environment=github-hosted`). These are accidental-local-execution guards, not fabricated token evidence or an unforgeable proof of VM origin; actual run metadata is the hosted provenance. There is no absent-environment successful fixture branch.

## Concrete actor and channel ABI

The orchestrator keeps credentials, created account records and actual `.NET Process` handles in a finite owner. It launches only the exact copied example image using `ProcessStartInfo`: `UseShellExecute=false`, local credential `UserName`/`Domain`, `Password` SecureString, explicit trusted working directory, `LoadUserProfile=true`, stdout/stderr redirection and no interactive shell. Profile load is explicit, but runner USERPROFILE/USERNAME/PSModulePath must not be accepted as actor identity or used to discover application state. Environment is a fixed system allowlist and fixture TEMP/TMP; paths are exact new job paths, not arbitrary operator input. [Credential launch](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.username?view=netframework-4.8.1), [Password property](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.password?view=netframework-4.8.1), [Profile load](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.loaduserprofile?view=netframework-4.8.1)

The roles are `outer`, `inner` and `grandchild`, with fixed actor labels A/B and a single fixed root-first scenario. The credential outer captures its entry `Instant` before input or native work and constructs its same-image `tokio::process::Command`; its first child is exclusively `OwnedChild::spawn(command, ChildWindow::Hidden)`. Before that successful spawn it performs only argument validation, exact image/cwd selection, local deadline/runtime/owner setup: no SID adapter, state access, PowerShell, arbitrary command or helper child. After admission, it may query its own process SID in the same retained native owner for comparison. All group/adaptor probes and grandchild behavior occur inside the admitted inner tree, not in the outer.

Use these existing APIs only:

- `locron_engine::windows_child::{OwnedChild, ChildWindow, SpawnFailure}`: `spawn`, `id` for internal observation, `try_wait`, `tree_empty`, `confirm_exit_until`, `terminate_until`. No raw creation flags, PID-based termination or private stdio accessor.
- Safe `windows_permissions::utilities::current_process_sid()` and fallible `wrappers::ConvertSidToStringSid(&sid)` query the **Rust process's primary token**, not a supplied SID or a cache. These are already-locked 0.2.4 APIs; the inspected local source opens the actual current process token and reads TokenUser. Use the fallible conversion rather than Display's internal expect. [Public utilities](https://docs.rs/windows-permissions/latest/windows_permissions/utilities/index.html)
- Only after inner containment, fixed `locron_core::windows::run_script_json_until` with the original local actor deadline obtains actual stock `[WindowsIdentity]::GetCurrent()` SID, token Groups and `[WindowsPrincipal].IsInRole` booleans. Probe SID must equal the Rust primary SID. The script has no account/file/task operation and receives no credentials. Enabled Users membership is positive; Administrators and privileged built-in group presence must be negative. This helper is an actual same-token descendant, not proof inferred from a username.
- Inner spawns only the exact same-image grandchild via ordinary `std::process::Command`, no breakaway/WMI/credential override; the retained outer Job must contain it by real inheritance. Inner retains its actual native child handle until root exit. No metadata-only descendant authority.

The chosen stdio is feasible in the actual pinned implementation: `OwnedChild::spawn` preserves the supplied command; its final callback changes creation flags only. The pinned process-wrap `CreationFlags`/`JobObject` hooks change flags/Job assignment and `KillOnDrop` changes kill-on-drop. They do not replace stdio. Freeze **inner stdin=null, stdout=inherit, stderr=null**, grandchild stdin=null/stdout=inherit/stderr=null. The outer writes ownership frames only to its separately redirected stderr; inner/grandchild write identity frames only to inherited stdout. Inner finishes its one stdout frame before spawning grandchild, so their records cannot overlap. The outer never calls `take_stdout/take_stderr`, which are `pub(crate)` at `windows_child.rs:243–249`.

Controller reads both streams concurrently with bounded .NET byte `ReadAsync`, never sequential `ReadToEnd`, unbounded line accumulation or an unlimited `WaitForExit`. Stdout must contain exactly the inner and grandchild identity records; stderr exactly the owner-start, root-reaped/tree-live and cleanup-confirmed records. Each UTF-8 newline JSON frame is at most 1,024 bytes, each stream at most 4 KiB, aggregate at most 8 KiB per actor, no unknown fields/extra bytes/trailing records. Native EOF of both streams and exact outer exit status are required after confirmed tree cleanup. These are private inherited control channels: actual SIDs/PIDs exist only in memory, never logs/artifacts. Native redirected-handle survival across credential logon and `ChildWindow::Hidden` is a required positive Verify, not assumed from the source audit. [Redirected stream behavior and deadlock](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.redirectstandardoutput?view=netframework-4.8.1)

Freeze exact private frame keys before source: identity `{schema:"locron.windows-user-identity/v1", actor:"A"|"B", role:"inner"|"grandchild", pid:u32, sid:string, probe_sid:string, users_enabled:bool, administrators_enabled:bool, forbidden_builtin_group_present:bool}`. Owner-start `{schema:"locron.windows-user-owner/v1", actor, phase:"owner_started", pid:u32, sid:string, root_pid:u32}`; root-live `{schema, actor, phase:"root_reaped_tree_live_negative_confirm", root_exit_zero:bool, tree_empty:bool, negative_timed_out:bool}`; cleanup `{schema, actor, phase:"cleanup_confirmed", root_exit_zero:bool, tree_empty:bool}`. PIDs must be positive actual observations, SID strings canonical native values bounded to 256 ASCII bytes. These frame values are checked against the retained actual credential Process and newly created account records, not accepted as independent product authority. The token probe accepts well-known built-in SID prefix `S-1-5-32-` only for Users (`S-1-5-32-545`), requires Users enabled, and refuses any other built-in group present; actual local-membership enumeration separately requires Users as the only assigned local group. This prevents a filtered privileged token being mistaken for an independent standard account merely because `IsInRole(Administrators)` is false.

Test control marker files use only the new per-actor fixture directory and fixed nonsecret nonce/ready data. They are not application private-state roots. The orchestrator creates a new protected trusted-admin/System job directory and separate A/B control directories with mutation only for the corresponding actor plus trusted administrators/System; copied example image permits A/B read/execute and only trusted writers. A runner-profile directory or shared A+B mutation root is not adopted. Marker/source image removal is possible under the explicit trusted-admin fixture ACL; no production owner-only root is repaired for cleanup.

## Four ordered steps and Verify

1. **Freeze/build the explicit manual substrate before accounts.** Two rows: native `x86_64-pc-windows-msvc` on `windows-2025`, native `aarch64-pc-windows-msvc` on `windows-11-arm`, Rust **1.94.0**, fail-fast false. Build the exact example with `cargo +1.94.0 build -p locron-engine --example windows_user_prerequisite --locked --target <native-target>`. Checkout the exact dispatch SHA and check rustc host matches target. Run orchestrator from `shell: cmd` using absolute stock Windows PowerShell 5.1 `-NoLogo -NoProfile -NonInteractive -File`; no explicit or implicit custom `ExecutionPolicy Bypass`, policy setting, module installation or runtime C# compiler. **Verify:** manifest metadata shows one non-test example, no new package identities/checksums; ordinary/full app Source/tests/CI are conserved outside the isolated branch replacement; actual OS/native architecture/module/current policy preflight succeeds or this prerequisite fails. Check names are new manual-prerequisite names, never ordinary/full required check names.

2. **Create exact Users-only A/B identities and obtain actual token evidence.** Generate unique short names and CSPRNG passwords in a dedicated finite stock runspace owner. Keep exact successful new-account `{name,SID,created_this_run,SecureString}` records only in that owner; add only those accounts to the well-known Users group and inspect all actual local memberships. Verify they are Users-only, not Administrators/Backup Operators or any additional privileged/local group. Also query the actual runner token: administrative true and SID distinct from both. The credential outer/inner/grandchild actual primary SIDs must match the recorded user and each other; A/B/runner differ, stock probe SID matches, enabled Users true and privileged groups false. **Verify:** two actual account creations and two native credential executions succeed; neither supplied SID vectors, filtered administrator token nor network-only logon is substituted. Bad membership, failed native module/logon, missing sidecar frame or identity mismatch is FAIL with no raw user data emitted.

3. **Prove root exit is insufficient, then confirm the actual tree stop.** For each A/B actor the admitted inner obtains its token evidence, starts the same-token grandchild, waits for the exact fixture ready marker within its existing budget and exits zero while the grandchild remains alive. Outer polls actual root reaping and authoritative Job nonemptiness. `confirm_exit_until(min(original_deadline, now+100ms))` must time out while that descendant remains live; this expected negative probe is distinct from an unconfirmed final cleanup. Then `terminate_until(original_deadline)` must confirm root status and empty Job, with no new cleanup clock. **Verify:** actual root exit zero + tree nonempty + expected short negative + final authoritative tree empty for both users, correct two stdout identity frames and three owner stderr frames, and genuine terminal EOF/outer exit zero. Failed enrollment or native Job nesting, early grandchild exit, query error or final timeout never becomes PASS or a retry. A `SpawnFailure::ExecutionMayHaveStarted` retains containment and is final unknown cleanup, not NotStarted.

4. **Remove only exact recorded accounts after confirmed cleanup and emit bounded evidence.** After actual outer Process exit and both inner root/tree confirmations, re-read each account by exact name, require the same created SID, remove that account and verify its absence/membership removal. Close confirmed completed Process/channel objects, remove only exact known job directories/image/markers, preserve unrelated objects. **Verify:** created=2, executed=2, removed=2, no actor ownership pending, both streams closed, no unknown file/account entry removed. Partial setup may remove only successfully recorded accounts with confirmed no live/unknown launch. An unknown spawn/actor/runspace prevents a clean-account claim; it fails with explicit VM-disposal fallback.

## Deadline, secret and receipt contract

Use an original monotonic controller deadline of **180 seconds**, born before native preflight/account effects. Setup and cleanup each use their original capped 30-second phase, each credential-start return is capped at 15 seconds, and each actor owner has a 30-second local deadline born at outer entry before spawn/SID/native work. The controller caps each complete actor launch/exchange/exit at **45 seconds from before credential Start**, always within the original 180 seconds. These are distinct explicit nested caps, not a claim that a Rust Instant is serializable across processes. Child remaining-duration metadata is floored, refuses sub-1ms, is established before spawn, and cannot renew owner/controller authority; late child records are refused. Every `OwnedChild` poll/query/confirmation uses the original outer Instant. The helper keeps its documented cleanup allowance, but any return beyond the enclosing deadline is failure.

The example's outer owner runs the actual spawn/query/SID/stream work in one retained worker; the entry driver never indefinitely joins it. On expiry it permits no further success/admission and retains that owner/Job/runtime/streams pending the administrative controller's bounded failure handling. The controller's original actor deadline remains authoritative even if a child starts late or reaches its advisory local deadline later; late frames are not accepted and no new actor is admitted behind unknown ownership. This first stage does not add or exercise a synthetic global owner reset.

Potentially blocking Start/account/group/pipe/file/native work belongs to the one finite admitted runspace/actor owner, with the original clock already set. The driver polls `BeginInvoke` completion under that clock; closes any invocation input collection before launch; calls `EndInvoke` only after confirmed completion. Never synchronously join, Stop/Dispose an unfinished runspace or block on a worker-held lock after timeout. A late owner retains actual Process/Child/Job/stream/account records until confirmed cleanup or the failed disposable job is destroyed. The driver can report unknown and request emergency termination through the exact retained Process object, but outer kill, Job-handle close, process EOF or VM disposal alone cannot be relabelled authoritative tree cleanup. Job timeout is 10 minutes including compilation; it is a disposal backstop, not a successful 180-second proof. [Async PowerShell invocation](https://learn.microsoft.com/dotnet/api/system.management.automation.powershell.begininvoke), [EndInvoke wait](https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.powershell.endinvoke?view=powershellsdk-7.4.0)

Passwords are built by appending individual CSPRNG-derived characters to a SecureString and retained only in memory. Do not assemble a complete plaintext .NET string or use `GetNetworkCredential().Password`, plaintext conversion, stdout/argv/env/JSON/file/sidecar/password transcript. Clear transient random buffers and dispose SecureStrings after their confirmed launches/cleanup. Be precise: the framework/native logon implementation necessarily materializes credential data in process/native memory; this plan promises no deliberate plaintext serialization or persistence, not that plaintext can never exist internally. Normal Windows account creation stores OS credential state until exact account removal; that is not a secret artifact.

Account removal, profile-directory deletion and HKCU/HKEY_USERS hive unload are different facts. This first plan asserts exact account removal only; it does **not** delete profiles or promise hive unload. Profiles/hives remaining from `LoadUserProfile=true` are explicitly disposed with the fresh hosted VM. If no-live-actor proof is unknown, even account cleanup is unknown and the row fails. Do not call profile removal APIs, grant restore privileges or touch pre-existing profile keys to manufacture a green receipt.

Final receipt is strict `locron.windows-user-prerequisite/v1`, max **4 KiB**, with exact source/base SHA, workflow/run/attempt, native target, OS SKU/build/image, actual compiler version, built example SHA256, actor labels A/B, boolean token/distinctness/Users-only evidence, root/tree/negative-probe/EOF/exit confirmations, created/executed/removed counts, cleanup enum, profile disposition `disposable-vm`, elapsed milliseconds and fixed failure category. No username, actual SID/PID, password, profile/scratch path, raw actor output/error, private sidecar bytes or hashes of private identity frames. Expected counts are 2/2/2 and all confirmations true; missing receipt, extra/malformed data, zero executions, unknown cleanup or any late result is FAIL. Upload only the sanitized receipt and exact built example identity/evidence; do not upload actor scratch, account records, streams, SecureStrings or profile contents.

Exact receipt top-level keys are `schema`, `source_sha`, `base_sha`, `workflow`, `run_id`, `run_attempt`, `target`, `os`, `compiler`, `example_sha256`, `runner_administrative`, `identities_distinct`, `actors`, `counts`, `cleanup`, `profile_disposition`, `elapsed_ms`, `failure`. `os` contains only `{sku,build,image}`; `actors` is ordered A then B, each `{label,primary_tokens_match,users_only,root_exit_zero,tree_live_negative_confirmed,tree_empty_confirmed,stdout_eof,stderr_eof,outer_exit_zero}`; `counts` is `{created,executed,removed}`. `cleanup` is `confirmed` or `unknown`; `failure` is null only for confirmed full success, otherwise a fixed nonprivate enum (`preflight`, `account`, `credential_start`, `token`, `protocol`, `containment`, `deadline`, `cleanup`, `native_owner_unknown`). Receipt source/run/OS strings have explicit small length limits, not arbitrary environment or error dumps; all bool/count claims derive from actual retained observations. A partial failure receipt can report its known counts and unknown cleanup, but cannot satisfy the expected success contract.

The bounded budgets, dependency edge and runspace ABI above are selected. Root reviews this completed plan and records Issues #27/#29 prerequisite Verify before handing separate development the exact five allowed paths. The two manual native rows qualify only this prerequisite at their exact SHA. Hosted x64 Server and tool-rich Win11 ARM results remain distinct from clean standard-user Win11 application acceptance. The remaining 12-case privacy/IPC design stays a later phase.


### Existing servicing ancestry versus new fixture writers (2026-10-04)

Before accepting an existing servicing ancestor in Source, distinguish its trust from
the new fixture DACL. Reviewed application5585936 core filesystem.rs Windows constants
and stock.rs TRUSTED/verify_descriptor already recognize SYSTEM, Administrators and the
fixed Windows TrustedInstaller SID
S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464. Select that same fixed
servicing trust for read-only validation of existing local Program Files/Windows ancestry:
owner and actual mutation ACEs may belong only to those trusted identities. Reparse,
unknown owner/ACE or other actual object/retained-child mutation refuses. Directory
sibling creation is distinct from mutation of the retained existing object; preserve
the reviewed directory mask distinction and no-delete ancestry protection.

This does not authorize modifying an existing ancestor or adopting a shared writable
image/root. New protected job/image writers remain exactly Administrators/SYSTEM;
new A/B controls add only their corresponding actual actor SID. Do not add a servicing
writer to those new descriptors. Verify existing ancestry and exact handles without
repair; create only the new job namespace and conserve application ACL policy. The
original finite owner/deadline/secret/real-token/cleanup receipt contract remains.


### Selected administrative guard-owner amendment (2026-10-04)

The following complete candidate is now selected by Root after full plan/ABI review;
its previous outside-repository candidate status is historical. It becomes dependent
Source authorization only after this Docs commit and exact Issue27/29 Verify readback.
The five-path lease remains in its existing worktree; no main merge/rebase may occur
during active Source WIP. Root will integrate4ab7b9cfcef0b53d21814721126fe59d96442230
(whole-tree equal10fe) after Source is clean and reviewed, before any manual publication.
Keep all original contracts except the explicit additive guard role/receipt/failure enum
selected below. No native acceptance is claimed. Source unsupported bootstrap may fail.

# Administrative retained-ancestry owner: design candidate, 2026-10-04

Research pin: `6dcd88d0a5b633a62259242ec6fefb5453d67572`. Its earlier prerequisite plan selects application base `55859361892a422c7d3868b741431480ec435503`, now historical. The next Source base transitions to integrated main `4ab` (tree equal to `10fe`) only after Root supplies its exact clean Source lease; neither research pin nor earlier558 is the next development base. This document is outside the repository and is not an implementation authorization or native acceptance receipt.

## Selected approach and the bootstrap boundary

Add one explicit **administrative `guard-admin` role to the existing planned engine example**, retained by the orchestrator's finite credential-free .NET Process owner. Keep the same five Source paths, existing Windows-only dev edge on windows-permissions0.2.4, and all production APIs/packages unchanged. This role holds trusted directory ancestry; it is separate from A/B's credential `outer`, whose first child remains exclusively public `OwnedChild::spawn` before SID/adaptor/state work. The administrative role itself does not first spawn an OwnedChild. Its permitted native guard calls and bounded adapter descendants remain in its one retained worker; its Process and worker are never abandoned on unknown completion.

There is a necessary bootstrap distinction: no code can have retained directory handles before that first native guard process has run. The **compiled guard substrate launch precedes fixture image/account setup**. Before Start, the controller keeps the exact build-output regular file open read-only with sharing Read only, verifies the expected build hash and its handle-bound owner/DACL, and walks every existing local source/cwd ancestor from the fixed trusted drive root downward against fixed SYSTEM/Administrators/TrustedInstaller owner and actual mutation policy. At each accepted level the already accepted parent must deny nontrusted delete-child/object mutation; a descriptor read alone from an arbitrary mutable chain is insufficient. It refuses an untrusted/reparse/unknown component, does not repair or copy to an unguarded replacement directory, and has not created A/B accounts or the fixture namespace. The initial descriptor walk is a bootstrap trust precondition, **not retained ancestry evidence**. The actual guard must then acquire source-parent/cwd/anchor DirectoryGuards and recheck the same regular file and expected bytes before `anchor_held`; only that point admits fixture setup.

This avoids an A/B path-replacement race: those accounts are absent, the accepted existing chain denies nontrusted object/child mutation, and the exact leaf lease denies write/delete replacement. Trust in the already selected privileged SYSTEM/Administrators/TrustedInstaller servicing identities remains the boundary. **A GitHub checkout/source/cwd owned by an arbitrary current runner SID or granting that SID mutation may honestly refuse**, even if the runner is administrative. Public ancestors' current-SID exception must not be used to bypass the frozen fixed-principal bootstrap rule. No fallback trusts that SID, stages/copies an image before guarded setup, repairs checkout ACLs or asserts hosted PASS. Do not claim that a regular-file lease alone prevents an ancestor rename, that arbitrary mutable build ancestry is safe, or that this supplies mapped-image authentication. If Root instead requires retained ancestry **before the guard substrate's own first Start**, this five-path design cannot satisfy that stronger condition: stock Framework cannot open those directory handles, and adding native entry before native entry does not solve it. That narrower interpretation needs an explicitly reviewed bootstrap boundary or an existing trusted directory-handle launcher/API; do not silently weaken it in Source.

## Public API evidence and handle policy

At the research pin, core `filesystem.rs:60–64` calls `guard_directory(path,false,false)`: `DirectoryGuard::ancestors` is existing-only and creates/repairs nothing. `841–870` opens root to leaf, checks directory type/reparse and trusted ownership/mutation, retains every `File` in `_handles`, and returns canonical path only after validation. `645–656` requests READ_CONTROL|FILE_READ_ATTRIBUTES|FILE_LIST_DIRECTORY and share READ|WRITE, omitting DELETE sharing, with BACKUP_SEMANTICS|OPEN_REPARSE_POINT. The guard's live lifetime, rather than a saved normalized path, preserves the object boundary. `674–703` permits current SID plus fixed SYSTEM/Admin/TrustedInstaller; the example must additionally enforce the plan's fixed privileged existing-ancestor policy, rather than treating current-SID acceptance alone as administrative provenance.

An admin call to `ancestors(control_A)` must refuse A's mutation ACE because A is neither that current admin SID nor a trusted servicing principal. Therefore keep `DirectoryGuard` for the trusted anchor, source/image parent and job root, and hold **each actor control leaf with a separate safe std File** after its parent chain is held. Use `OpenOptionsExt::access_mode(0x00020000|0x80|1)`, `share_mode(1|2)`, and `custom_flags(0x02000000|0x00200000)`, existing-only, no create/truncate/delete flags. Check `File::metadata` is a directory and has no reparse bit, then safe `windows_permissions::wrappers::GetSecurityInfo(&file,SE_FILE_OBJECT,Owner|Dacl)` on that retained handle. Protected control DACL and owner must match the frozen exact Administrators/SYSTEM plus corresponding actual A or B rule; no other principal or unknown ACE is accepted. Preserve the existing sibling-creation versus retained-object mutation distinction when validating old ancestry. New job/image descriptors never add TrustedInstaller.

Safe direct opens are example-local fixture composition, not a new filesystem API. Native opens/metadata/security/reads may block: perform them in the finite retained owner; a timeout is not cancellation or permission to drop a still-running owner. A control path string/descriptor DTO cannot substitute for its retained leaf File. The already locked safe GetSecurityInfo/SetSecurityInfo wrappers accept std File via AsRawHandle; no unsafe/reflection/PInvoke/runtime compiler is needed in this repository.

## Exact role, environment and private control ABI

Guardian argv is exactly `--hosted-prerequisite guard-admin`; no path, SID, account name, password or request JSON appears in argv. `.NET ProcessStartInfo` uses the exact held build-output executable, `UseShellExecute=false`, `CreateNoWindow=true`, explicit trusted existing cwd, no UserName/Domain/Password, no LoadUserProfile, and redirected stdin/stdout/stderr. Keep the actual Process returned by Start, not a reconstructed PID. There is one guardian, no replacement launch after unknown ownership.

Clear inherited environment and supply the existing fixed system allowlist: SystemRoot/WINDIR, PATH containing only the stock system directories, validated existing TEMP/TMP, `GITHUB_ACTIONS=true`, `LOCRON_HOSTED_PREREQUISITE=1`, `LOCRON_RUNNER_ENVIRONMENT=github-hosted`. These final two names are proposed private example/orchestrator mode metadata, not application configuration. Do not supply runner USERPROFILE/USERNAME/PSModulePath as token evidence. Root may align these two private names with its frozen Source naming before handoff, but may not omit explicit mode/provenance refusal.

Use private inherited stdin/stdout channels, with stderr required to be empty plus actual EOF. UTF-8 newline JSON; each frame including newline <=4096 bytes, each direction <=32768 bytes, exactly eight requests/eight corresponding acknowledgments in success, no unknown keys/type coercion/trailing bytes/extra records. Each request has exactly `schema:"locron.windows-user-guard/v1",nonce:<32 lowercase hex>,seq:<0..7>,op:<fixed enum>,remaining_ms:<positive floored u32>,payload:<exact op shape>`. Each success acknowledgment has exactly `schema,nonce,seq,op:<fixed corresponding enum>,ok:true,payload:<exact shape>` below. A failure acknowledgment has exactly `{schema:"locron.windows-user-guard/v1",nonce,seq,op:"failed",ok:false,payload:{failure:<fixed enum>}}`, only after a valid header/correlation was received. Freeze the exact nonprivate failure enum as `preflight|account|credential_start|token|protocol|containment|deadline|cleanup|native_owner_unknown|guard_setup`: the original nine values plus guard_setup for refused ancestry/inheritance/create/descriptor/image setup. At most one terminal failure ACK, no subsequent success/retry; raw native messages/private paths/SIDs are forbidden. Invalid UTF-8/JSON or missing/unvalidated correlation receives no invented nonce/seq ACK and is controller protocol FAIL with actual cleanup facts retained. Paths are private input fields, canonical local absolute drive paths with no ambiguous components, each <=512 UTF-16 units; actual encoded frame ceiling is checked before dispatch. Actual SIDs are canonical ASCII <=256 bytes. All private values remain memory-only.

| seq | request / exact payload | acknowledgment / exact payload | retained state |
| --- | --- | --- | --- |
| 0 | `acquire_anchor` / `{anchor,source_image,source_sha256,cwd,controller_remaining_ms,setup_remaining_ms}` | `anchor_held` / `{pid,runner_sid,source_sha256}` | Existing anchor, source-parent and cwd guards; exact source leaf lease. SID/PID compared privately with actual runner and retained Process. |
| 1 | `create_job` / `{name}` | `job_held` / `{created:true}` | Exact new child under anchor, initialized trusted job guard. Name is a single fixed-prefix random component, never a path. |
| 2 | `create_controls` / `{actors:[{label:"A",sid},{label:"B",sid}]}` | `controls_held` / `{created:2}` | Exact new A/B controls and no-delete actor leaf Files. Uses only successfully created account SIDs already checked by controller. |
| 3 | `hold_image` / `{sha256}` | `image_held` / `{sha256}` | Exact freshly copied fixed image leaf, handle-bound final DACL/hash; root/controls remain held. |
| 4 | `release_image` / `{}` | `image_released` / `{released:true}` | Allowed only when controller's actual A/B Process/tree/EOF and exact account-absence state is confirmed. Parent-chain guards remain. |
| 5 | `release_controls` / `{}` | `controls_released` / `{released:2}` | Controller has confirmed exact image and fixed marker leaves removed; trusted root stays held. |
| 6 | `release_job` / `{}` | `job_released` / `{released:true}` | Controller has removed only the exact now-empty controls; retained existing anchor stays held. |
| 7 | `finish` / `{}` followed by actual stdin EOF | `all_released` / `{released:true}` then stdout EOF | Controller has removed only the exact empty job root. Close remaining source/cwd/anchor and source leaf; worker completes; Process exits0 and both output EOFs are required. |

The controller admits each next request only from its actual typed private Process/account/creation/channel state. Guardian acknowledgments prove its own live guard actions, not account/Job cleanup. It never accepts arbitrary paths, arbitrary cleanup targets, supplied privilege results or release success from raw metadata. Drop of EOF or a killed Process is not a successful release acknowledgment. Guardian channel terminal reading and output work belong to the retained owner under the original deadline; no unbounded main-thread EOF/join wait.

Keep the original sanitized receipt and add exactly `guard:{bootstrap_fixed_trust,anchor_held,job_created,controls_held,image_held,continuous,image_released,controls_released,job_released,all_released,stdout_eof,stderr_eof,exit_zero}`, all actual booleans. `continuous` requires the actual retained guardian owner and its required handles remain live across admitted fixture/account setup, both actor launches and actor/account cleanup until the corresponding authorized releases. Time expiry, early process/EOF, unknown worker or missing checkpoint cannot produce continuous:true. Full success requires **all thirteen true**, the original ordered A/B facts and expected original counts2/2/2, cleanup confirmed, failure:null, and total UTF-8 receipt <=4096 bytes. No new private fields, path/SID/PID/frame dump or supplied descriptor is persisted; guard_setup extends the overall failure enum consistently. This is additive prerequisite proof, never production filesystem/IPC acceptance.

## Creation and phased cleanup order

`ancestors` cannot guard a missing leaf. Acquire an existing trusted anchor first and refuse missing source/cwd/anchor. Under that live chain, the guardian uses **safe `std::fs::create_dir` for each exact new namespace/control component**, not create_dir_all or DirectoryInfo.Create's existing-or-new result as creation evidence. AlreadyExists refuses without adoption/repair/removal. Its one successful create is recorded in the same retained owner before subsequent native work. Open that exact new directory with the no-delete flags above plus only the necessary WRITE_DAC/WRITE_OWNER initialization rights (no DELETE), initialize owner/protected DACL through safe SetSecurityInfo, read it back on the same handle, and retain it. New trusted namespace ends with owner Administrators or SYSTEM and protected Admin/SYSTEM DACL; new controls add only their recorded actor SID. Root must approve this transfer of *already planned fixture directory effects* to the native guard owner before Source.

This is **exclusive creation followed by guarded initialization**, not an atomic create-with-final-SDDL claim. There is no CreateDirectory wrapper in the inspected windows-permissions0.2.4 public API; std create_dir has no security-descriptor parameter. DirectoryInfo.Create(sd) can return an existing directory and cannot populate created-this-run authority. Select Root's supported no-foreign-writer alternative instead of inferring atomic protected creation.

Before create, open/read the exact held parent's DACL and evaluate **all** standard inheritable allow ACEs, including INHERIT_ONLY and NO_PROPAGATE effects, not just ACEs effective on the parent. Conservatively examine both OBJECT_INHERIT and CONTAINER_INHERIT entries; use full mutation mask `0x500d0156`, **including** add-file/add-subdirectory `0x06` on the proposed child. Reject any such grant to a principal other than Administrators/SYSTEM or the actual positively administrative creator token's default owner. Do not rely on an opposing deny ACE to excuse a forbidden allow. Reject absent DACL, unknown/object/callback ACE, unresolved generic SID and CREATOR_GROUP. CREATOR_OWNER maps to the **actual guardian token's default owner**, obtained by one fixed bounded `run_script_json_until` native `[WindowsIdentity]::GetCurrent()` probe `{sid,owner_sid,administrative}` within the same original setup deadline; its SID must equal safe Rust primary SID, administrative must be true, and default owner must be Administrators/SYSTEM or that actual administrative SID. Unknown owner refuses; supplied usernames/SIDs or runner environment do not substitute. An effective inherited mapped ACE and any still inherit-only generic ACE are both validated in the resulting exact new handle readback. Reject inheritable TrustedInstaller mutation for the new child even though its existing-ancestor mutation is trusted. This is deliberately stricter than public ancestors' current-object mask.

The actual inherited new-object descriptor must thus deny every foreign mutation throughout the initialization interval. Native create Ok is recorded before open; a partial or failed open/initialization remains a created object with unknown cleanup, never an adopted existing leaf. Check exact new directory type/no-reparse, initial owner/DACL and empty contents **before** a propagating DACL write, then set/read back final protected owner/ACL on the same retained handle and check empty contents again. No actor/image/account launch is admitted in that interval. If the hosted anchor cannot meet the inheritance precondition, fail before creation; do not use broadly writable Temp or add another package/API as a fallback. The final new descriptor writers remain exactly Administrators/SYSTEM; only final controls add their corresponding actual actor. No fresh directory becomes an actor path before exact final DACL, empty contents and live no-delete handle are confirmed. This rule supplies a concrete feasible API sequence; its actual accepted anchor and token-owner facts still need hosted Verify.

The exact setup order is **existing anchor/source/cwd ACK → create_job Ok/held ACK → create exact accounts and verify Users-only memberships/SIDs → create_controls Ok/held ACK using those recorded SIDs → copy+hold_image ACK → credential A/B actors**. No account is created until the new job namespace has native create-only and held-handle confirmation. Only after job/control guards are held does the controller copy the fixed image with the frozen create-new, explicit trusted-writer/read-execute actor descriptor and verify final bytes; guardian `hold_image` retains its exact read/no-write/no-delete leaf and trusted parents before any credential Start. Marker writes occur only inside the respective guarded control leaf. A/B outer/inner/grandchild containment and token assertions remain unchanged.

After both actual actor trees and Process/EOF are confirmed, remove the exact recorded accounts and verify SID-bound absence. Release only the image leaf gate so the controller can remove that fixed image under retained root ancestry. Remove only fixed verified marker leaves under retained controls, then release controls and remove each exact empty directory nonrecursively. Release the job-root guard only after both controls are confirmed absent; remove the exact empty job root while anchor remains held. Finally finish/close stdin and require all-released acknowledgment, actual guardian exit0 and stdout+stderr EOF before closing its confirmed Process/runspace and claiming overall cleanup. Never recursively remove unknown entries. Successful cleanup contains all account, actor-tree, leaf, empty-directory, staged-release and terminal Process/EOF facts.

Resource inventory is explicit: any initial directory initialization File or duplicate DirectoryGuard containing a released leaf must close at that same release boundary. Keep exactly one trusted job-root chain referenced by both raw control leaves where possible; release_controls drops control leaves, release_job drops all job-root data handles, and finish drops only the separately retained existing source/cwd/anchor chains and source leaf. A surviving duplicate must not be hidden behind a false release ACK. Child-directory removal happens only after its own gate is released and while its parent remains retained. Source/cwd are existing and are never removed or repaired.

## Original clocks and unknown ownership

Keep controller180s born before native preflight; original setup30s and cleanup30s; each Start-return15s; A/B actor controller45s and native outer30s. Guardian Start/acquisition/creation/image setup are included in **the same original setup30s**, not added after it. Guardian captures entry Instant before input/native work and receives only floored remaining caps established **before Start**; the controller's actual original deadline remains authoritative. Guard residency through actor/cleanup is a resource lifetime within original controller180s, not another 30-second acquisition budget. Every later release/readback is bounded by the original cleanup/controller clock. A requested remaining duration can shorten but never extend a previously accepted local horizon; a late Start/ACK/frame cannot renew authority.

DirectoryGuard's internal SID/native calls do not expose cancellation or an `until` parameter. Its newly born internal helper clock is not authority to exceed this outer phase. The driver stops accepting success at the original enclosing deadline and retains the still-running worker/Process/guards. Native query/file/ProcessStart is not asserted preemptible. Do not EndInvoke/Stop/Dispose/join unfinished work, reacquire a worker-held lock or admit another guardian/actor/account cleanup behind unknown ownership. Emergency Process kill or VM disposal is failure cleanup only. This addition preserves the original account/profile/hive distinction and secret constraints.

## Four ordered Verify additions

1. **Source and bootstrap conservation:** only five frozen paths; no unsafe/reflection/PInvoke/compiler/new package or production API change; exact build image lease/hash and trusted existing descriptor policy precede guardian Start; actual native anchor acquisition must happen before any fixture/account mutation. Verify negative missing/untrusted/reparse bootstrap is refusal with zero setup effects, fixed frame failures only, and no claim of pre-Start retained ancestry.
2. **Handle-bound setup:** native x64/ARM guardian acknowledges actual PID/current runner SID, existing-chain/no-delete guards, exclusive new directories and exact final DACLs. A/B mutation controls are handled by separate actor-specific Files. Verify active guards make exact fixture directory rename/delete fail, while allowed marker creation succeeds; an already-existing or unknown namespace is never adopted/removed. No Source acceptance is inferred until these actual hosted checks execute.
3. **Actors under continuous ownership:** existing two genuine Users-only token executions and root-reaped/tree-live/negative-confirm/final-empty facts remain exact. Guardian must stay live with all required chain/leaf handles during setup, A/B starts and account/actor cleanup. Verify original caps and native redirected-channel/EOF proof, no warm-up/new clock/global reset, no success from guardian PID/DTO alone.
4. **Ordered final cleanup:** require exact SID-bound account removal, actor Process/tree/EOF confirmations, bounded release ACKs in sequence, verified fixed leaf removal, exact nonrecursive empty-directory removals under retained parents, then final guardian release/exit0/EOF. Unexpected entry, stale/wrong ACK, extra frame, timeout, unfinished native owner or missing receipt is FAIL with cleanup unknown. Only explicit manual native harness runs qualify; compilation/zero-case and hosted Server/ARM success do not complete Issues27/29 or clean standard-user Win11 acceptance.

## Feasibility sources

- [Pinned core guard source](https://github.com/WhiteKiwi/locron/blob/6dcd88d0a5b633a62259242ec6fefb5453d67572/crates/locron-core/src/filesystem.rs#L60), retained chain at L841, flags L645, policy L674. Local snapshots and SHA256 are in source-inventory.json.
- [Microsoft Framework FileStream source](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/filestream.cs#L605): its public FileOptions whitelist excludes directory BACKUP_SEMANTICS/OPEN_REPARSE_POINT. A handle constructor does not manufacture a missing directory handle.
- [Rust OpenOptionsExt](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html) and [pinned Rust1.94 Windows extension source](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/os/windows/fs.rs): safe access/share/custom flags are available without exposing a raw handle constructor.
- [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): directory data opens require BACKUP_SEMANTICS; sharing remains effective for the live handle. Omitting delete sharing participates in delete/rename exclusion; OPEN_REPARSE_POINT permits exact-object reparse inspection.
- [Microsoft GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo), [SetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setsecurityinfo) and locally inspected locked safe windows-permissions wrappers: descriptors are read/initialized on retained handles.
- [Rust create_dir](https://doc.rust-lang.org/std/fs/fn.create_dir.html) and [Microsoft CreateDirectoryW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createdirectoryw): one-component exclusive create fails if the directory exists; default descriptor inheritance must be considered before later protected initialization.
- [Microsoft ACE inheritance rules](https://learn.microsoft.com/en-us/windows/win32/secauthz/ace-inheritance-rules), [automatic propagation](https://learn.microsoft.com/en-us/windows/win32/secauthz/automatic-propagation-of-inheritable-aces), and [WindowsIdentity.Owner](https://learn.microsoft.com/en-us/dotnet/api/system.security.principal.windowsidentity.owner?view=netframework-4.8.1): inherit-only and generic SID effects need explicit child evaluation; the token's real default owner is distinct from User and is used for creator-owner mapping.
- [.NET redirected Process streams](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.redirectstandardoutput?view=netframework-4.8.1): private concurrent bounded reads avoid sequential pipe deadlock; actual completion and EOF still require measurement.

No native behavior, fixture/account action, PowerShell parser or test was executed during this research. PrivacyDev direct message delivery was unavailable in the current visible agent inventory; Root supplied its exact Framework/API boundary and will review this amendment before dependent Source.


### Selected public sanitized evidence boundary before dependent Source (2026-10-04)

This documents the final publication creation rule before its dependent Source.
It adds no product scope, private-state/credential authority or fixture cleanup
claim. The private job/control namespace continues to use only the guard-admin
native create_dir Ok-only ledger and the exact eight-request protocol above.

Publish only prerequisite-evidence/{built-example.json,receipt.json} beneath the
exact already bootstrap-validated existing cwd; recheck that trusted parent
without repairing it. Publication remains in the same finite native runspace
and its original enclosing cleanup/controller deadline, with pre/post gates.
DirectoryInfo.Create with a supplied protected Administrators/SYSTEM descriptor
may return existing-or-new: it supplies NO created-this-run, adoption or
directory-removal authority. Before either file open, verify actual directory
type/no reparse, fixed Admin/SYSTEM owner, protected DACL and exactly two
inheritable FullControl allow ACEs for Admin/SYSTEM; refuse every other/unknown
descriptor and never repair an existing directory. Open each fixed sanitized
output with FileMode.CreateNew plus an explicit protected Admin/SYSTEM file
descriptor; verify the exact new handle descriptor, write bounded UTF-8 <=4KiB,
flush, close and check deadline. Existing files refuse without overwrite/retry.
No public artifact directory/file cleanup is performed. This public evidence
path is not retained fixture ancestry and is not credential/state authority.
Publish a success receipt only after all actor/account/guardian facts and
original clocks are confirmed; failed/late publication or any unknown cleanup
cannot produce workflow success. Failure evidence may say cleanup unknown but
cannot satisfy the success contract.

Verify this boundary with the existing plan gates: the same original finite
owner/deadline encloses publication; only the two fixed sanitized new files are
admitted with actual protected descriptors; existing/unsafe/reparse or late
outputs refuse without overwrite, repair, removal or success. A partial file,
failed upload or a syntactically valid failure receipt is not native acceptance.
Source remains five paths and separate-session owned; actual hosted evidence
and Root complete-source review remain required before any qualification.


### Selected locked SHA-256 and individual native-call gates (2026-10-04)

Root reviewed the separate held-Source audit before this dependent amendment.
The example's source/copied-image identity must use the workspace's already
selected RustCrypto sha2 0.10.9, with default-features=false and std, through a
Windows-only engine dev sha2.workspace=true edge. Core/CLI/Server already use
this locked package. Add only the disambiguated engine lock mapping sha2 0.10.9;
retain windows-permissions 0.2.4. No new package/version/checksum, production
dependency, asm/compress feature or handwritten SHA-256 implementation is
selected. The locked checksum remains
a7507d819769d01a365ab707794a4084392c824f54a7a6a7862f8c3d0892b283.

1. Reuse sha2::{Digest,Sha256} with incremental new/update/finalize. Verify:
   compare the entire locked graph/package identities and resolved features;
   only these two existing Windows engine dev mappings change, and custom
   compression/constants/padding code is absent. An offline metadata/format
   result is not example type checking or native identity acceptance.
2. Hash the exact already held read-only regular leaf through its duplicated
   handle, explicitly seek that duplicate to zero, then read bounded chunks.
   Verify: clone, seek and each read have checked_native pre/post gates under
   the same original deadline; update/finalize/final response refuse lateness.
   File::try_clone shares the cursor: these fresh source/image handles are
   hashed once with no concurrent reader. Keep the original handle and actual
   ancestry alive, never reopen by pathname or add/replay an owner/clock.
   Native late returns remain parked with the same retained result/stack.
   Hosted actual .NET source/copy SHA-256 and Rust source/copy SHA-256 must
   agree before image authority is acknowledged; no mocked hash substitutes.
3. Conserve the existing every-native-call deadline contract for actual SID
   retrieval and conversion, including ACL-owner/ACE SID conversion. Verify:
   checked_native gates each separate OS call using the original caller
   deadline; a late first query admits no conversion/next query. Retain the
   actual returned owner/full stack on unknown lateness and preserve Token
   versus Native error categories while propagating Deadline refusal.

The product specification, five-path Source boundary, fixed account/Job/ACL
protocol, original clocks, public receipt rules and strict native Verify remain
unchanged. Local link.exe is unavailable: the earlier attempted cargo check
stopped before example type checking; do not infer compiler/native acceptance
or repeat owner-PC fixture execution. Root records exact Issues27/29 readback
before releasing the held development lease. Both actual manual hosted rows
and complete Root Source review are still required.

Research: pinned [sha2 API](https://docs.rs/sha2/0.10.9/sha2/),
[RustCrypto source](https://github.com/RustCrypto/hashes/blob/82c36a428f8d6f05f3bfccdedb243e9d1f85359d/sha2/src/lib.rs),
[File clone cursor contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_clone).

### Reviewed current-main integration before hosted publication (2026-10-04)

The first actual hosted qualification uses application main
e6eb8879e927a631cce707cb1e6aacb3c08ebb3b, whose whole tree equals the independently
reviewed PR134456524b. Original5585936 remains the research starting point;
2f7cea6 is the frozen Source planning parent, not a claim about the final application
revision. Root reviewed all five prerequisite Source files at620712d and the
excluded lockfile, independently verified287 protected blob/modes, full patch
inverse and the354-package/feature graph. Static results contribute zero actual
token, Job, account or cleanup acceptance.

Integration preserves all five reviewed prerequisite Source blobs exactly.
Every other non-document blob/mode equals reviewed main, including release
workflow, fixture portability and the two existing cfg(debug_assertions) braces.
Conflicting append-only planning documents retain both sides in order. This
isolated branch still has the reviewed manual-only ci.yml and must never create
a PR or replace main CI. Both actual native rows, normal-example type checking,
strict fixed-principal bootstrap and all original four Verify criteria remain
pending; no bootstrap fallback, owner-PC effect or changed deadline is selected.

### Named Read trait binding before changed-Source qualification (2026-10-04)

Actual normal-example builds at a89d1b2/run37163445014 fail on both Rust1.94
native rows with E0405 at example1080: guard_frame names impl Read, while the
grouped import only brings Read as _ into method resolution. No actual actor,
guard, account or cleanup acceptance was reached; the verification steps were
skipped and artifacts are absent. Root's initial static review missed this
binding. The separate readonly report preserves both full logs and the unapplied
candidate. This evidence does not prove that no later type error can exist.

1. Bind the existing trait name. Verify: separate development changes only the
   grouped std::io import Read as _ to Read; keep Seek as _, the impl Read bound,
   every operation, protocol, ownership/clock and guard unchanged. No wrapper,
   dependency, workflow, diagnostic allowance or fixture fallback is selected.
2. Conserve and review. Verify: reversing that single import token restores the
   entire example; every other tracked blob/mode stays exact to the Docs parent.
   Rust1.94 formatting/diff and Root full one-file review pass. Preserve the failed
   run and issue evidence before Source; no local native build/host effect retry.
3. Qualify changed Source. Verify: both explicit normal examples actually build,
   then all original four ordered native credential/token/Job/EOF/account/guard
   criteria execute successfully on x64 and ARM. Any later compile, strict
   bootstrap, logon or unknown-owner failure remains a failure needing review.
   Keep missing-evidence upload refusal and the original clocks. Issues stay open.

### Fixed failure diagnostics before another changed-Source run (2026-10-04)

Actual e4e5db9/run37164778853 builds both normal Rust1.94 examples successfully.
Both actual verification steps exit1 with the same native_owner_unknown line;
artifacts=0. PS879 emits it for unfinished shared-unknown/phase-expiry, PS886
for completed EndInvoke exception. The raw logs cannot distinguish the branch,
last setup operation, hidden exception, token/Job/account counts or cleanup.
The failure Write-Evidence can itself fail its existing checkout Bootstrap-Chain.
No bootstrap/ACL/module/token fix is justified by those logs. Frozen SPEC and all
original native acceptance remain unchanged. Select observation only.

Only scripts/test-windows-user-prerequisite.ps1 may change. Every other tracked
blob/mode, Rust example, five-path original prerequisite graph, manual workflow,
metadata/frame/public artifact schema and existing operation/condition remain
exact. Root does not implement Source. No local native/PS/parser/account/ACL/
policy operation; separate development follows exact Issues27/29 Verify readback.

The owner publishes bounded scalar diagnostics only. Checkpoint is dispatch
intent immediately before the already selected operation, never its completion,
authority, account count or cleanup. Fixed checkpoint set:

entry, metadata, stock_preflight, stock_utility_open, stock_utility_import,
stock_accounts_open, stock_accounts_import, stock_exports, runner_identity,
os_metadata, checkout_bootstrap, image_open, image_hash, anchor_bootstrap,
guardian_start, guard_acquire_anchor, guard_create_job, account_a_create,
account_b_create, guard_create_controls, image_copy, guard_hold_image,
actor_start, actor_read, cleanup_accounts, cleanup_release_image,
cleanup_remove_image, cleanup_remove_markers, cleanup_release_controls,
cleanup_remove_controls, cleanup_release_job, cleanup_remove_job,
cleanup_finish_guard, cleanup_guard_eof, cleanup_dispose, evidence_publish,
confirmed. Missing/unrecognized values render unknown. actor_start/read repeats
for A/B without rendering any actor identity; the checkpoint is not a token proof.

Before failure-evidence publication, freeze the first owner's failure_checkpoint,
existing selected category and fixed exception kind. category remains exactly
the already existing allowlist preflight, account, credential_start, token,
protocol, containment, deadline, cleanup, guard_setup, native_owner_unknown.
No exception message is printed or copied into shared state. Use only safe
managed -is type checks and at most four InnerException links (five nodes), no
reflection/native inspector. Fixed kind set: unauthorized, security, io, win32,
argument, invalid_operation, timeout, method_invocation, runtime, pipeline,
unknown. Prefer the deepest recognized specific kind; wrapper kinds do not
overwrite a specific kind. Unknown types render unknown. Never publish a type
name, FullyQualifiedErrorId, Message/ToString, native error text or stack.

Keep independent failure_evidence scalar not_attempted/attempting/written/refused
and evidence_kind from the same kind allowlist. Failure-evidence's exception may
only set evidence_kind/refused; it cannot overwrite the original failure facts.
No retry/new artifact/parent repair is added. All diagnostic defaults are fixed
unknown/entry/not_attempted. The synchronized table supplies scalar snapshots,
not an atomic cross-field transaction or inferred causal/cleanup proof.

Distinguish these existing driver refusal paths with fixed branch literals:
shared_unknown, phase_expired_pending, phase_expired_completed,
endinvoke_exception, result_rejected, phase_expired_after_dispose,
phase_expired_after_success_print. Keep each original decision expression and
exit1 exactly; shared_unknown takes priority only when the existing combined
condition already refused. For a completed EndInvoke exception, classify that
driver exception locally without reading the invocation's Error/Output stream.
No unfinished EndInvoke/Stop/Dispose/join or resource-list access is added.

Each existing failure exit may emit ONE fixed ASCII stderr line at most512 bytes,
preserving the native_owner_unknown prefix and appending only validated fixed
checkpoint/failure_checkpoint/branch/category/kind/evidence/evidence_kind values.
Validate every field against its frozen set immediately before rendering; any
unrecognized value becomes unknown. No PID/SID/path/account/UUID/channel/raw
frame/credential/hash/environment/argv/descriptor or counter is rendered.
The line is refusal evidence and supplies ZERO native prerequisite acceptance.
No diagnostic helper can reopen admission or turn a late failure into success.

1. Add fixed owner checkpoints and failure scalars. Verify: text-only whole-file
   inverse strips only those inert assignments/helpers, leaving every existing
   native operation/order/conditions and same owner/resources/unknown parking.
   Original180/30/15/45/30/100ms and publication limits stay exact; all other
   tracked blob/modes and workflow stay exact. No new effectful native calls.
2. Add bounded refusal rendering only. Verify: each existing driver refusal uses
   only the closed sets above and <=512-byte ASCII; first owner failure survives
   a second evidence failure; existing success/exit decisions and original clocks
   remain exact. Source/type/text checks disclose no private values or extra
   streams. Root reviews the complete one-file patch and preserved-source inverse.
3. Qualify changed observation on the two native rows. Verify: the normal examples
   still actually build; each failure distinguishes a branch/checkpoint/fixed
   kind without a raw secret/path/identity. Preserve e4e failed logs and absent
   artifacts. A diagnostic failure remains FAIL with cleanup unconfirmed and
   Issues27/29 open; only all original token/Job/account/guard/EOF criteria can
   qualify. Any newly measured cause needs another Docs-first plan before repair.

### Fixed substage observation after actual stock opening refusal (2026-10-04)

Actual changed Source28449fd/run37167319135 passes both Rust1.94 normal builds,
but real verification fails on x64/ARM with first checkpoint stock_utility_open,
kind unknown, then separate evidence_publish/refused (x64 kind runtime, ARM
unknown). Raw logs and Root receipt are pinned in FINDINGS and Issues27/29.
Open-FixedFile's exact lower operation is unobserved; this is not a measured
path/ACL/module/token cause or permission to repair it. Original counts/token/
Job/13guard/EOF/cleanup remain unqualified, never inferred0/PASS.

Keep frozen SPEC and every original prerequisite/previous diagnostic contract.
Select only scripts/test-windows-user-prerequisite.ps1 observation refinement.
All other297 tracked blobs/modes, example/graph/manual workflow, fixed Utility
path/trust/descriptor/categories/classifiers/guards/evidence schema/limits and
180/30/15/45/30/100ms clocks remain exact. No added native call, reflection,
stream inspection, policy change, fallback, ACL/staging repair, replay/retry,
current-SID authority or local owner-PC native/PS/parser/AST/effect.

Only three new fixed scalars: substage, failure_substage, evidence_substage.
Current substage is dispatch intent before an existing operation/predicate,
not completion, object identity or authority. Freeze failure_substage beside
existing first-failure fields BEFORE evidence admission. Freeze evidence_substage
only in the existing secondary catch; it cannot overwrite first failure. Keep
defaults entry/unknown/unknown, synchronized scalar snapshots not an atomic
cross-field transaction. Driver reads these fixed strings only, never resources.

Closed set: entry, path_validation, attributes, directory_security,
security_owner, security_sddl, security_raw_acl, security_rules, owner_check,
owner_untrusted, acl_presence_check, null_acl, ace_shape_check, ace_shape,
foreign_mutation_check, foreign_mutation, file_open, file_security,
evidence_serialize, evidence_parent, evidence_directory_create,
evidence_directory_security, evidence_identity_write, evidence_receipt_write.
Unknown/missing values render unknown. Repeated ancestor/leaf operations use
the same labels; no path/component/index/SID/principal/ACL/native error appears.

Add inert assignment markers immediately before existing Local-Path validation,
File.GetAttributes, DirectoryInfo.GetAccessControl, ObjectSecurity.GetOwner,
SDDL retrieval/RawSecurityDescriptor construction, ACE validation, FileStream
Open/Read/ShareRead and retained stream GetAccessControl. Evidence serialization,
checkout bootstrap, existing directory creation/readback and the two existing
exclusive CreateNew writer calls get the corresponding fixed intent labels.
Lower shared helpers may replace the current label with their actual lower
operation; this cannot alter first-failure/evidence snapshots or admission.

For the FOUR existing Check-FixedSecurity predicates, assign owner_check,
acl_presence_check, ace_shape_check or foreign_mutation_check before evaluation;
only inside its already-taken rejection body assign owner_untrusted, null_acl,
ace_shape or foreign_mutation immediately before the unchanged throw. Preserve
every predicate, evaluation count/order and throw category. A fault during a
predicate and its explicit policy refusal stay distinguishable without exposing
raw facts or adding a native query. These labels do not authorize a trust change.

Append individually validated substage/failure_substage/evidence_substage to
the existing ONE ASCII refusal line. Longest fixed label27 bytes, conservative
complete-line maximum413 including CRLF; retain the <=512-byte limit and fixed
constant fallback. Preserve every original driver branch/condition/exit1 and
success decision. No diagnostic can reopen admission or qualify late output.

1. Implement fixed substage assignments/snapshots only. Verify: full-script inverse
   restores28449fd exactly; native calls, security predicates/throws, existing
   first/evidence fields, accounts/guards/operation order/clocks stay exact and
   all other297 modes/blobs match. Separate developer implements after Root
   plan and exact Issues27/29 Verify readback; Root never implements Source.
2. Review bounded renderer and whole patch. Verify: all three fields use only
   the24 closed labels or unknown, every path stays <=512 ASCII bytes including
   CRLF, first failure survives secondary publication failure; no private value,
   native call, unfinished EndInvoke/Stop/Dispose/join or stream access. Root
   full Source/context/inverse review plus static diff/rules precedes publication.
3. Qualify ONE changed-head hosted run. Verify: both actual Rust1.94 normal builds
   still pass, then actual fixed substage refusal or complete original genuine
   receipt is observed on both rows. Preserve28449fd failed raw logs/artifact0.
   Later/unknown failure stays FAIL; diagnostic observation contributes ZERO
   native acceptance. All original credential/token/Job/account/guard/EOF Verify
   remains required and issues stay open. A measured cause needs Docs before fix.
