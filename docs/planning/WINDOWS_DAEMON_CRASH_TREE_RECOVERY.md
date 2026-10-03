# Windows registered-daemon crash tree recovery acceptance

Refs #28 and #30. Existing native tests prove Job Object containment inside Runner and prove registered-daemon ownership/control separately, but they do not compose an actual registered daemon crash with a durable running attempt whose target has a live descendant.

## Scope

Add a disposable Windows-only integration fixture that starts the real CLI in registered daemon mode, manually queues one disabled job with retries configured, and runs a target fixture that remains active behind a gate while spawning a second descendant process. Kill the registered activation process without cooperative shutdown, observe that both target layers stop making progress, restart the same registered daemon, and verify the one durable run recovers as `interrupted_unknown` without retry or duplicate side effect.

No production process-supervision, shutdown, scheduling or storage semantics are changed.

## Fixture correction (2026-10-04)

CI run `37134032103` failed the new acceptance test during descendant job creation
on all three native lanes (jobs `111234766809`, `111234766831`, `111234766850`).
The fixture supplies `--retry-delay 200ms`, but the existing duration parser only
accepts an integer followed by `s`, `m`, `h`, or `d`. A same-head macOS CLI build
reproduces exit 2 with empty stderr and an `invalid_request` JSON error on stdout;
`1s` succeeds and records a 1,000,000-microsecond retry delay. This is a fixture
input correction, not a parser or process-security policy change.

Both heartbeat writers also use `std::fs::write`, whose create/truncate step
precedes the complete counter write. A separate real Rust counter process on
macOS was stopped in that window and killed; the published counter remained
empty after the owned process was reaped. This reproduces a portable publication
race, not a Windows Job Object failure.

1. Use the supported `1s` retry delay and observe recovery for 1,500 milliseconds,
   longer than that delay. Include status, stdout and stderr in descendant add/run
   failures. **Verify:** the exact unsupported input refuses with the documented
   JSON error, the corrected input registers the intended retry policy, and native
   CI reaches the existing live-tree and durable-recovery assertions.
2. Write and flush each complete heartbeat into a same-directory temporary file,
   then atomically replace its published path with `NamedTempFile::persist`.
   Preserve the previous complete counter until replacement; publication failures
   remain explicit. **Verify:** a real fixture child holding an unpublished next
   snapshot is forcibly stopped and reaped while the previous counter remains
   readable, and a subsequent complete snapshot replaces it successfully. Apply
   this publication helper to both root and descendant heartbeats.
3. Apply standard Rust formatting. Keep every original 30-second ownership,
   termination and recovery deadline, live-tree assertion, absent completion
   marker, exact one-run `interrupted_unknown` history, no-retry check and
   authenticated shutdown assertion. **Verify:** Rust 1.94/1.98 format checks,
   relevant portable regressions and whitespace checks pass; exact-head Windows
   x64/ARM64/MSRV CI must qualify the actual native lifecycle. Local macOS checks
   do not substitute for that Windows execution.

## Verify

1. Establish real durable active work. **Verify:** the registered activation and owned daemon roles are live, the run is `running`, and both the direct target and its child descendant publish independent progressing heartbeats before the crash.
2. Crash the actual registered daemon owner. **Verify:** activation/worker/daemon ownership is released within the existing native bound, neither heartbeat advances after release, and the gated completion side effect remains absent.
3. Restart against the same state. **Verify:** the same run id becomes `interrupted_unknown`; history contains exactly that one occurrence after a retry window; no completion marker appears and neither stopped heartbeat resumes. The restarted registered role remains functional and can be shut down through the existing authenticated lifetime control.

## Boundaries

This is acceptance composition, not a new kill mechanism. It does not claim forced-kernel-operation failure coverage, replacement admission, configured grace timing, login/reboot behavior, installer activation or public Windows support. Existing Unix crash-boundary tests remain unchanged.


## PR134 failed-continuation observation before Source (2026-10-04)

Continue Issues28/30 after the exact eae776 required failure recorded in FINDINGS.
The product specification and all lifecycle success predicates remain frozen. Select
only one test file's failure diagnostics, without assuming the cause or retrying a
failed operation. The separate developer owns implementation after this plan and the
exact Issue30 Verify readback; the parent reviews and publishes.

1. Preserve actual durable failure facts. **Verify:** only when the already-read run
   is not running, include its bounded reason and the first owned attempt's state,
   outcome, exit code and artifact byte/truncation counts in the existing failing
   assertion. Do not print resolved paths, argv, tokens, configured values or output
   payloads. At most one attempt summary is rendered; querying failures remain explicit.
   No extra Store/native I/O occurs on the successful path, and no clock is restarted.
2. Identify real heartbeat publication failures. **Verify:** retain the same one
   write/flush/persist sequence and immediate failure, but use fixed phase plus counter,
   ErrorKind and raw OS code instead of path-bearing error Debug. A deterministic
   disposable held-destination native control must reach the actual persist failure,
   show its fixed facts, preserve the previous complete counter, and publish the next
   complete counter successfully only after releasing its own blocking handle. No
   publication retry, synthetic success, release of production guards, warmup, suite
   serialization or deadline enlargement is selected.
3. Review and qualify the changed file. **Verify:** all original sixteen lifecycle
   selectors, their predicates, observation/ownership/cleanup clocks and production
   blobs remain conserved outside the explicit diagnostics/control. Rust1.94/1.98
   formatting and whitespace checks pass; the parent reviews every changed hunk and
   separately reviews excluded documents. Fresh ordinary pull_request CI at the new
   exact head/main must pass every required native row. Preserve the eae failure and
   its unknown cause even if fresh diagnostics/control pass; broader CLI4/lint41 and
   acceptance work remain open. A passing rerun alone is not causal evidence.
