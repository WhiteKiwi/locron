# Bounded package probe capture

Refs #32 and #31. The existing package verifier checks a 4 KiB output threshold only AFTER subprocess.run has captured all output. Preserve the accepted version/identity formats while enforcing the cap during capture. This is CI/package tooling, not the product's native installer or Job Object runtime.

Python's primary documentation notes that communicate buffers data in memory and that process creation cannot be interrupted on many platform APIs: https://docs.python.org/3/library/subprocess.html#subprocess.Popen.communicate and https://docs.python.org/3/library/subprocess.html#subprocess.run . Do not claim a hard bound on synchronous native process creation/termination.

1. Add a dependency-free subprocess adapter with exactly one active probe per Python process, two binary readers and at most 4097 retained bytes per stream (4096 accepted plus one refusal byte). Both streams drain concurrently. Verify real disposable child processes producing exact-limit output, over-limit stdout/stderr, concurrent output, nonzero exit, invalid text and delayed exit.
2. Start the monotonic operation deadline before spawn and require child exit PLUS both stream EOFs before success. On failure, terminate/reap only the owned child, with one separate finite cleanup deadline and finite reader joins. If cleanup is uncertain, retain the owner/readers/pipes and leave admission closed until process exit; never close a stream concurrently with its reader. Verify child exit without EOF, EOF without child exit, startup exceptions and uncertain cleanup in isolated processes. This is not descendant-tree supervision.
3. Use this adapter by default for the existing package version/identity probes while keeping explicit controlled executors available for fixtures. Retain stock-only PATH, no-shell argv, all exact output checks, ZIP generation and release publication boundaries. Add an inexpensive Python workflow for Linux and Windows; native package CI remains the actual image gate. Verify all existing distribution/manifest fixtures and unchanged accepted outputs, plus the real-child adapter suite.

No dependencies, installed software, registry/PATH values, tasks, releases or catalog entries are changed. Windows runtime behavior and x64/ARM64 packaged binaries still require exact-head CI.

The initial slice had no separate development/review session; that records its
historical verification limits. The reader-start interruption repair below was
implemented in a separate development session and reviewed by the parent
session. Describe executed checks and remaining exact-head Windows CI
requirements accurately.

## Reader-start interruption safety

The native reader can exist before `Thread.start()` returns or publishes its
`ident`. An interruption during that interval makes startup uncertain; a missing
identifier is not evidence that the reader was never started. Preserve the
existing never-close-under-reader contract by recording each start attempt
before calling `Thread.start()`. Only a reader with no start attempt can be
skipped during cleanup. Every attempted reader requires a finite join and
confirmed termination; an exception or uncertain completion retains the owner,
both pipes and closed admission. Do not infer safe closure from exception type
or create a new operation budget.

Verify in a separate Python process using an actual native reader held before
bootstrap publishes its identifier/start event. Interrupt the caller's real
`Thread.start()` wait and prove that the owned child is reaped while the reader's
pipe stays open, cleanup is unconfirmed and a later probe is refused. Release
the fixture reader, confirm its eventual exit and keep the intentional quarantine
isolated from other tests. Distinguish this uncertain-start case from failures
before any reader-start attempt, which can still close pipes and release
admission after the owned child is confirmed reaped. Preserve successful capture,
ordinary timeout cleanup, exactly two readers, the 4 KiB + one refusal-byte cap,
root-only termination and the separate three-second cleanup deadline. This is a
repair of the existing contract, not a product-scope or public runtime change.

## Integration with newer main

Main advanced to `0757ed92505a20930daa3f6e657227da721f6f83` while this slice was published, adding #123's raw ZIP admission. Merge that snapshot normally with both parents preserved; do not rewrite main or either upstream PR. Retain its complete tree, `windows_zip.py`, raw-catalog workflow/fixtures and every new archive/CRC/decoded-size predicate. Apply only the already-reviewed bounded-executor import/default selections to its exact `windows_release.py` blob `13d4d5f727ca56b94aeb69f924e0f59f13270715`.

Verify the fetched main baseline by Git blob hash, compare the integrated module against it, rerun the 18 real-child/integration tests with its actual raw ZIP reader, and run the existing manifest/catalog suites in fresh hosted CI. The final diff against newer main must contain only this slice's five files; no upstream payload/ownership/runtime change belongs in the resolution. This integration does not alter the output/lifecycle policy above.
