# Canonical paired inventory and exclusive handoff gates

Execution: #33 and #34; prerequisite Drafts #118 and #122.

## Existing contract and scope

SPEC already requires both fixed Windows executables and all seven standalone
payloads to agree with the canonical immutable release. IMPLEMENTATION requires
retained object identities and private ancestry before replacement, and refusal
of mapped or changed leaves without a path-based replacement fallback. This
slice connects those existing contracts; it does not change product behavior.

The integration branch combines the exact #118 and #122 source trees with a
normal two-parent commit. Neither upstream PR nor main is modified. Review and
merge those prerequisites before retargeting the follow-up to main.

A local receipt, even one whose hashes match the installed files, is not a
canonical release proof. Consume the existing paired inventory together with
the existing immutable canonical Payloads value, repeat actual account/target
and every receipt/source/hash binding, and compare retained file lengths. Keep
both proofs together in an opaque move-only value with the original absolute
deadline. No constructor accepts JSON, arbitrary file facts or a replacement
clock as equivalent ownership.

For the no-write exclusive transition, retain a private directory guard and
reacquire receipt/non-executable read guards while the complete original
inventory is still held. Compare every reacquired full identity, path and digest
before dropping the old inventory. Then acquire both existing private share-zero
executable gates in fixed console/launcher order and recheck the exact original
full IDs, paths, lengths and hashes on those retained handles. The window between
read and exclusive guards is not atomic: any substitution, including identical
bytes at a different object, must refuse. A second-gate failure drops the first
gate without any file/task/PATH/journal effect. Never delete, repair, replace,
kill a mapped holder, adopt an unknown object, or infer task quiescence here.

Retaining six companion guards involves bounded extra reads; use existing
filesystem helpers rather than expose private inventory internals or weaken
sharing/ACL rules. Synchronous native calls still belong to an outer owned phase;
clock checks do not cancel stalled kernel I/O. Expiration before/after an
observation refuses and never creates a new budget.

## Implementation and Verify

1. Connect live inventory to canonical payloads. **Verify:** a genuine disposable
   seven-file/v2 installation passes, while an intrinsically valid forged receipt
   and matching modified local bytes fail canonical comparison. Version, archive,
   account, path, target and all seven digests remain bound under read guards.
2. Add the paired no-write exclusive transition. **Verify:** both held gates
   exclude opens/rename, a locked second executable leaves both files unchanged,
   a same-byte different-ID replacement refuses, and readonly/multiply-linked
   images fail the unchanged core predicate. Receipt/companion guards remain held.
3. Qualify the integration. **Verify:** actual private disposable-file fixtures
   run on the repository's x64/ARM64 Windows lanes; scoped diff/format and normal
   checks pass. Record exact-head executed evidence separately from source review.
   Fixtures use structural PE bytes, not fabricated claims of executing Locron.

## Explicitly not completed by this slice

Native version/ABI execution, actual task quiescence, protected request/helper
acceptance, paired journal/backup/replacement/recovery effects, task restoration,
CLI install/uninstall/update dispatch and clean Windows 11 acceptance remain
required. The new code inherits the existing Windows test-only distribution
boundary. The exclusive result is a no-write observation, not a durable operation
or permission to replace a running installation. Both issues remain open.
