# Windows SID fixture and obsolete PR CI correction

Owning issues: #172 and #31. Base product source536671f80b9ea4b17f4fc0362b3fa0ba60e43607.
This is the user-requested follow-up to PR182's Windows foundation failure. SPEC's
identity clarification preserves the existing timeout/error/cache contract. Product
identity, SQLite, IPC, cleanup and privacy behavior are not changed by this phase.

## Evidence and selected correction

FINDINGS records the complete research and the two exact CI/CD guideline revisions.
Windows MSRV job116810653241 runs Core with test-threads=1 and returns136PASS/1FAIL:
the owner's injected panic ultimately exceeds its original one-second deadline,
while the fixture unconditionally expects Other. Production correctly gives timeout
precedence after expiry. Buffered logs do not identify the host's exact delay site.
Independent exact-code probes show a gated panic hook is sufficient to produce this
mismatch, while hookless real unwind preserves the existing classification, join,
cache and permit assertions. Keep the real worker and all original deadlines.

The whole CI run remains30m31s with independent existing foundation failures.
Superseded PR work overlapped the latest head for4m51s/875 summed job-wall-seconds.
Remove obsolete execution without promising a measured latest-run speedup.

## Ordered changes and Verify

1. **Freeze evidence and plan before source.** Keep SPEC/Findings/implementation and
   owning issue records consistent; parent reviews all before separate development.
   **Verify:** actual log IDs, exact-code probe hashes, guideline revisions and primary
   references distinguish demonstrated mechanism from unknown native delay location.
2. **Isolate the SID unwind fixture.** Replace only its panic expression with
   resume_unwind carrying the same static payload, with a brief intent comment.
   **Verify:** production bytes, real thread/query/join, three error oracles, empty
   cache, permit release and original one-second deadlines remain unchanged. Both
   toolchains' portable controls exercise slow-hook timeout and hookless unwind.
3. **Bound obsolete PR work.** Add a workflow/event/PR cancellation group; non-PR
   events use unique run IDs and do not cancel each other. Select verified v1 for
   the six rust-toolchain master references, retaining compiler inputs and every
   existing job/command. Other historical action pins remain outside this phase.
   **Verify:** same-PR revisions share a group; different PRs/workflows/events and
   main/manual runs do not. YAML/actionlint and exact inverse checks pass; job names,
   conditions, matrix entries, tests, artifacts, permissions and cache policy remain
   unchanged. Do not add preflight duplication, retries or broader serialization.
4. **Qualify changed source and publish exact results.** Run formatting, strict
   affected Clippy and the existing native Core command on x64 stable/MSRV and ARM64.
   **Verify:** all137 native Core controls execute with no failure, including existing
   expired-cache/late-owner cases. Record native job/source/toolchain and subsequent
   foundation results separately. Historical failures and unrelated acceptance stay
   open. Read back PR/issue records; do not close #172/#31 before their own Verify.

## Boundaries and review

The Linux extraction is a mechanism probe, not Windows runtime acceptance. Core
already uses one test worker, so higher parallelism or a changed timeout is not a
repair. PR-only cancellation is an efficiency correction independent of SID behavior;
static group checks establish configuration, not observed cancellation or a latency
benchmark. Existing lightweight SQLite/permission regressions remain in CI. Public
GitHub-hosted runners, current native matrix, cache paths and required gates remain.
Separate investigation may plan remaining PR144/prune/daemon failures after their
actual causes are established; they are not silently waived by this phase.
