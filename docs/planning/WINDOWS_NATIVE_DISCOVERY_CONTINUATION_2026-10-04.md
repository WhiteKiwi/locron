# Native per-binary discovery after reviewed fixture corrections

Continue Issue #31's existing manual-only discovery plan. Earlier Source543
discovery37140952254 reached23 binaries,17 nonempty,784 actual cases per row,
with680/104 x64 stable and679/105 ARM64/MSRV passed/failed. Preserve that failure
history. Ordinary/full CI currently stops or fails before qualifying all later
binaries, so new fixture Source requires another complete measured inventory.

This isolated verification branch starts at reviewed private-fixture Source03
and docs ce641392. Before publication integrate the parent's newly reviewed
stock/configuration Source without changing it. Actual measured Source is the
complete immutable integrated tree recorded by the parent; do not label a future
or cancelled run as completed. Frozen SPEC and normal PR/main CI stay unchanged.

1. Preserve a clean isolated verification worktree and reviewed docs before
   Source. **Verify:** docs-only plan commit, exact Issue #31 readback and separate
   development handoff; no owner-PC test/account/task/installer execution.
2. The separate developer copies only the complete reviewed manual-only ci.yml
   from c374176b68740a1c073724536b26b844168c62ec into this branch. **Verify:** exact
   old workflow blob, immutable actions, contents-read/manual trigger,35min job
   limit, original x64 stable/ARM64 stable/x641.94 native/compiler assertions and
   fail-fast:false. No application/test/dependency or main workflow change.
3. Retain all3 exact private-token selectors before locked workspace all-target
   --no-fail-fast. **Verify:** each selection reports exactly1 actual passing
   named test, all9 across rows; original nonzero failures remain failures while
   later binaries run. No skip/ignore/allow/serialization/retry/timeout change.
4. After Source review integrate the exact newly reviewed parent head, then
   publish only this verification branch without a PR and dispatch via CLI.
   **Verify:** every non-workflow Source blob/mode equals that parent; full
   workflow remains the original blob. Actual head/compiler/host and every
   started binary's complete raw result are retained. Cancellation is incomplete.
5. Record all original and new test counts and failures in Issue #31 and relevant
   acceptance Issues. **Verify:** zero-case binaries are reported as zero cases;
   nested helper results cannot substitute for top-level binary results. Exact
   comment readback, no import of this temporary workflow into PR/main and no
   partial discovery/full-suite/support claim. Required red rows still block merge.
