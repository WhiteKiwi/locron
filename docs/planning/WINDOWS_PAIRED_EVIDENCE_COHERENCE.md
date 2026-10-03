# Windows paired package evidence coherence gate

Refs #31 and #32. This continues the existing paired Windows draft-package CI without changing product behavior, release inventory, signing policy, or installer/update semantics.

## Scope

Each native package matrix leg already emits a strict paired ZIP plus `verification.json` containing repository, workflow, run, source revision/head, target, runtime probe facts and final archive digest. Today those two artifacts are preserved independently. A later consumer must not accidentally combine x64 and ARM64 evidence from different commits, reruns, workflow refs, versions, or repositories.

Add one post-matrix CI gate that downloads both artifacts from the current run, revalidates each static paired ZIP on a non-Windows host, and requires the two verification documents to describe one coherent build event. This gate proves artifact-pair provenance consistency only; it does not replace either architecture's native version/ABI probes or establish release publication.

## Implementation and Verify

1. Add a strict evidence reader for the existing `locron.windows-paired-package-verification/v1` shape. Reject duplicate/unknown/missing keys, noncanonical SHA-256 values, wrong target sets, malformed runtime probe facts, invalid launcher identity, and mismatched archive filenames/digests. **Verify:** fixture pairs pass only when both complete documents and archive bytes agree.
2. Bind both architecture records to one build event: repository, merge/source revision, PR head revision, ref, workflow, run id/attempt, version, unsigned policy, toolchain policy and probe policy must match. Require exactly x64 plus ARM64 and require each rustc host/launcher target to match its target. **Verify:** independently mutate every shared provenance field and architecture-specific identity; each mutation refuses.
3. Add a post-`windows-package` Ubuntu job using current-run artifact downloads and explicit current GitHub context expectations. Re-run static paired ZIP inspection there, without executing either image. Add the offline unit suite to the existing installer checks. **Verify:** the focused Python suite and YAML parse pass; hosted CI must show both native package legs succeeded before the coherence gate can succeed.

## Boundaries

This gate does not publish artifacts, merge evidence across runs, execute Windows binaries on Linux, submit WinGet manifests, sign code, or claim clean-machine Windows acceptance. Native x64/ARM64 jobs remain the source of runtime probe evidence. The new job only consumes artifacts produced by the same current workflow run and fails closed on ambiguity.
