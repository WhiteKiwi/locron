# Windows registered-daemon crash tree recovery acceptance

Refs #28 and #30. Existing native tests prove Job Object containment inside Runner and prove registered-daemon ownership/control separately, but they do not compose an actual registered daemon crash with a durable running attempt whose target has a live descendant.

## Scope

Add a disposable Windows-only integration fixture that starts the real CLI in registered daemon mode, manually queues one disabled job with retries configured, and runs a target fixture that remains active behind a gate while spawning a second descendant process. Kill the registered activation process without cooperative shutdown, observe that both target layers stop making progress, restart the same registered daemon, and verify the one durable run recovers as `interrupted_unknown` without retry or duplicate side effect.

No production process-supervision, shutdown, scheduling or storage semantics are changed.

## Verify

1. Establish real durable active work. **Verify:** the registered activation and owned daemon roles are live, the run is `running`, and both the direct target and its child descendant publish independent progressing heartbeats before the crash.
2. Crash the actual registered daemon owner. **Verify:** activation/worker/daemon ownership is released within the existing native bound, neither heartbeat advances after release, and the gated completion side effect remains absent.
3. Restart against the same state. **Verify:** the same run id becomes `interrupted_unknown`; history contains exactly that one occurrence after a retry window; no completion marker appears and neither stopped heartbeat resumes. The restarted registered role remains functional and can be shut down through the existing authenticated lifetime control.

## Boundaries

This is acceptance composition, not a new kill mechanism. It does not claim forced-kernel-operation failure coverage, replacement admission, configured grace timing, login/reboot behavior, installer activation or public Windows support. Existing Unix crash-boundary tests remain unchanged.
