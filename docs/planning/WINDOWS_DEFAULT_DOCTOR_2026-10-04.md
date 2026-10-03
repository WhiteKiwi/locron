# Default Windows doctor diagnostics, 2026-10-04

Status: selected plan before Source, Issues29/31 in progress. Root owns product
decision/review/publication; a separate developer owns Source after exact Issue
Verify readback. Baseeae776ff2bd2a3a9b1fa591139615b9a3139e48e is whole-tree
equal to Root's reviewed latest-main integrationd948286. It contains reviewed
mainaa9cf6c plus the unchanged native CLI input slice. No dependency change.

## Selected observable contract

On Windows with LOCRON_SERVICE_BACKEND absent only, doctor returns the existing
CLI envelope/schema, actual store/integrity/resolution/token facts and fixed
dashboard access URL. Dashboard registered/loaded are null, with additive
service_status="unprobed". Neither registration nor listener availability was
queried; the URL is configuration rather than proof of a live listener.
Exactly these three human lines express the known unprobed facts:

```
info local wake: named pipe (availability is unprobed)
info dashboard service: registration is unprobed
info dashboard listener: availability is unprobed
```

All existing known boolean true/false Unix/explicit-fake shapes and output lines
remain exact, including false. A present selector (including empty, non-Unicode,
auto or another explicit value) does not enter the default diagnostic branch;
existing forced-backend/error handling remains authoritative. Keep genuine
state/store/token ACL/type/I/O errors. No synthetic UID, arbitrary-error catch,
fake default, token creation, Task Scheduler/listener/wake probe or staged
service/installer/updater effects are introduced by the facts branch.

The whole doctor keeps its existing store initialization and diagnostic lock
behavior; it is not newly described as an effect-free command. This documents
the explicit Windows unknown-fact output before Source, under the accepted
diagnostic/permission contract; it does not advertise released Windows support.

## Measured evidence and attribution

The separate research report01d3ae05/candidatecf37e5ca/receiptbb85cdc7 pins37
committed snapshots and complete Rust/TS consumers. service.rs1238 constructs
ServiceContext, unconditionally invokes id -u before port selection, and default
Windows select_port refuses. main.rs3568 requires those facts before rendering.
CLI main is the only caller; HTTP/MCP diagnostics and browser DTO are independent.

Correction to the immutable report's path inference: actual495f full-native
37157212227 ARM and x64MSRV successfully register the two existing test inputs
and reach human_doctor2941/human_forms3181 doctor assertions. Their first doctor
exit/code/cause remains unprinted; do not call this an observed add rejection or
infer the exact command error from empty output. A genuine absolute native CLI
target is still selected for meaningful resolution coverage in the two cases.
Those rows execute71 cases67PASS/4FAIL, including ten actual corrected input
cases each. x64stable instead has CLIunit300PASS/1FAIL at native_io_expiry and
never reaches the integration binary. Both Clippy rows repeat the41 existing
diagnostics. Every later all-target binary remains unreached in this run.

## Source lease and ordered Verify

Only crates/locron-cli/src/service.rs, src/main.rs and tests/cli.rs may change.
No Cargo/lock/workflow/backend/production wake/service admission changes.

1. Accept the caller StatePaths in the crate-private doctor facts helper. Default
   Windows obtains the existing actual token observer and fixed URL directly,
   without ServiceContext/UID/port/registration discovery. Existing Unix and
   forced-backend paths retain the same Some(paths.root.clone()) context and
   original status behavior. **Verify:** default Windows needs no Unix id/fake
   selector; known Unix/fake true/false shapes stay exact; genuine token/state
   failures propagate. Only the documented null+unprobed result is added.
2. Update the single doctor call and human rendering for exactly that coherent
   unprobed result. Preserve all known ok/warn/fail facts/values and existing wake
   line. **Verify:** successful native JSON has null registered/loaded plus exact
   marker; human has each of the three exact info lines once, and actual token,
   process-resolution and integrity facts. Unknown service facts never become
   false/not-running or a successful registration claim.
3. Port only human_doctor_prints_one_level_line_per_check and human_forms_leave_
   the_json_envelope_untouched input to the existing success_process_args helper;
   preserve Unix argv and every old assertion. Add actual command-success checks
   before interpreting doctor output and allow only the three literal info lines,
   never arbitrary info prefixes. Add focused Windows default diagnostic
   coverage under stock-system-only child PATH with all three service fake
   variables removed, for empty/default and meaningful native-target state,
   actual missing/private token posture and genuine invalid token leaf refusal.
   Known explicit fake true/false shape coverage must not be weakened or made
   stock-PATH support. **Verify:** all original71 Windows selectors remain,
   unrelated test bodies/blobs/clocks/owned children stay exact; each new named
   native case actually executes. Existing dashboard fake exposure assertions
   remain verbatim, with default JSON/human correspondence and no token bytes.
4. Return the full three-path Source patch, formatting/metadata, narrow Unix
   inverse and protected mode/blob/selector/error/ownership map. Root reviews
   every file/context and dispatches fresh changed-Source native gates only.
   **Verify:** Rust1.94/1.98format and current static checks, genuine default
   x64/ARM/MSRV doctor results and unchanged Unix/fake regressions are recorded;
   red required checks block merge. Full warnings-denied lint, actual wake,
   cancellation readiness and x64 supervisor expiry failure remain separate open
   gates. No zero-case/skip/late synthetic success or repeated unchanged full run.

Production wake API/finite-owner development is NOT authorized by this lease.
The separate wake proposal requires docs-first original-deadline/same-connection
server-PID/ACK/child+Job ownership and capture/cleanup review. No new dependency,
deadline enlargement, worker reset, whole-suite serialization or protection
override. Actual two-user prerequisite, standard-user Windows11/reboot,
public unsigned release/install/update/remove/WinGet remain unqualified;
signing37 is deferred. Issues remain open until all their criteria are met.


## Native expectation correction before Source (2026-10-04)

Continue the original four Verify criteria after measured full37160909694 x64
301unitPASS/75integration70PASS5FAIL. Product, source behavior, privacy and error
classification remain fixed. Only the two new native test expectations may change
after this plan/Issue Verify readback and separate development handoff.

1. Correct the native resolved-path oracle. **Verify:** independently canonicalize
   the known cargo-built executable with std::fs::canonicalize, require UTF-8, and
   compare its complete value to resolved_executable. Keep requested_executable
   equality to the original argv and effective/execution stock PATH equality.
   No prefix stripping, basename/suffix-only comparison or production resolver use.
2. Correct the complete typed-error oracle. **Verify:** invalid token directory
   still refuses with exit5, service_io, no data and the unchanged directory.
   Require the full fixed Display prefix "service management failed: cannot inspect
   token ACL:" and include the observed message when that assertion fails. Do not
   match any service_io generically, catch an arbitrary error or return success.
3. Conserve/review/qualify the two test changes. **Verify:** independent whole-file
   inverse restores both previous functions, all75 native selectors/clocks/assertions
   remain outside the specified oracles, and production main/service blobs and every
   other tracked Source remain exact. Rust1.94/1.98 format/diff checks and parent
   review pass. Fresh changed-head ordinary required CI and all four real native
   doctor cases plus the two existing doctor cases must execute on x64/ARM/MSRV.
   Retain original failed logs and all later CLI/full lint/acceptance failures.
