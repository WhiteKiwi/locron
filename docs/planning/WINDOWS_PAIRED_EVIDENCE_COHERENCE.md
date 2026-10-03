# Windows paired package evidence coherence gate

Refs #31 and #32. This continues the existing paired Windows draft-package CI without changing product behavior, release inventory, signing policy, or installer/update semantics.

## Scope

Each native package matrix leg already emits a strict paired ZIP plus `verification.json` containing repository, workflow, run, source revision/head, target, runtime probe facts and final archive digest. Today those two artifacts are preserved independently. A later consumer must not accidentally combine x64 and ARM64 evidence from different commits, reruns, workflow refs, versions, or repositories.

Add one post-matrix CI gate that downloads both artifacts from the current run, revalidates each static paired ZIP on a non-Windows host, and requires the two verification documents to describe one coherent build event. This gate proves artifact-pair provenance consistency only; it does not replace either architecture's native version/ABI probes or establish release publication.

## Implementation and Verify

1. Add a strict evidence reader for the existing `locron.windows-paired-package-verification/v1` shape. Reject duplicate/unknown/missing keys, noncanonical SHA-256 values, wrong target sets, malformed runtime probe facts, invalid launcher identity, and mismatched archive filenames/digests. **Verify:** fixture pairs pass only when both complete documents and archive bytes agree.
2. Bind both architecture records to one build event: repository, merge/source revision, PR head revision, ref, workflow, run id/attempt, version, unsigned policy, toolchain policy and probe policy must match. Require exactly x64 plus ARM64 and require each rustc host/launcher target to match its target. **Verify:** independently mutate every shared provenance field and architecture-specific identity; each mutation refuses.
3. Add a post-`windows-package` Ubuntu job using current-run artifact downloads and explicit current GitHub context expectations. Re-run static paired ZIP inspection there, without executing either image. Add the offline unit suite to the existing installer checks. **Verify:** the focused Python suite and YAML parse pass; hosted CI must show both native package legs succeeded before the coherence gate can succeed.

## Bounded artifact reads

Enforce the existing 128 KiB document limit while reading one open JSON stream:
retain at most 128 KiB plus one refusal byte and reject overflow before UTF-8
decoding or JSON parsing. A size observation does not replace the read bound.
**Verify:** an actual 131,073-byte document refuses without a whole-file read or
parse even when its observed metadata size is stale; an exact-limit valid
document remains accepted.

Use the existing `windows_release.inspect_archive` as the sole ZIP snapshot
reader and digest verifier. Its bounded 64 MiB plus one-byte read, expected
SHA-256 comparison and static ZIP inspection operate on the same captured
bytes. Compare its returned archive digest and binary facts with the native
verification document; do not perform a separate whole-file hash first.
Preserve raw ZIP structure, decoded-size, CRC, PE/import predicates and every
provenance, runtime-evidence, current-context and architecture requirement.
**Verify:** actual over-limit sparse ZIPs refuse without whole-file reads,
including a stale metadata size that requires the bounded read to detect
overflow; valid artifact pairs use one bounded snapshot per archive and all
existing evidence/catalog/manifest/distribution/release regressions pass.

## Boundaries

This gate does not publish artifacts, merge evidence across runs, execute Windows binaries on Linux, submit WinGet manifests, sign code, or claim clean-machine Windows acceptance. Native x64/ARM64 jobs remain the source of runtime probe evidence. The new job only consumes artifacts produced by the same current workflow run and fails closed on ambiguity.
