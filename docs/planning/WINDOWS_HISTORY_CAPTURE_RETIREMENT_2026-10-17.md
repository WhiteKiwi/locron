# Dispose completed Windows History captures

Issue: #172. Baseline main `d4d2c12e1994f417b6ef0eea4cf035e146c1b360`, tree
`27019b0635cd6f59456cc2fecb822166b1342f16`. Scope follows the user's explicit
instruction to resolve the remaining Windows failure after PR182 was merged.

## Evidence and selected behavior

CI38986232389/job117258276483 confirms durable cancellation and stopped progress
before cleanup. DropState takes 1.902814s and crosses the original eight-second
case deadline; subsequent confirmed cleanup is late and remains a failed result.
The 30-second outer capture horizon did not expire. Findings records exact clocks,
returned facts, proper startup-reader release and the unknown lower-level OS cause.

Each actual History CLI creates a distinct guarded stdout file, reads it through
an independent identity-checked bounded reader after actual root exit, then leaves
the original file guard and leaf until final cleanup. At h84/epoch87 the source
therefore retains 84 History files plus Add/Run/Cancel carriers. No later observer
uses those earlier files; the returned bytes already have separate ownership.

Retire only the current successfully captured History leaf immediately after the
existing status-zero/root-exit/read checks and reader/child closure. Use the existing
normal guarded `remove_private_file`, with the original Control check before and
after. Close that leaf's retained original guard before its guarded removal, keep
the owning private root and parent-chain protection, and preserve removal errors.
Do not use the strict filesystem-worker remover's separate adapter deadline.

The lifetime capture count and generated name continue increasing and retain the
existing maximum. Retirement removes only the current last Vec entry; no earlier
slot, pathname or object is reused. Preserve create-new collision refusal and full
original/reader identity during each carrier's lifetime. The OS may recycle a file
ID after deletion; never use a deleted ID as evidence of a current object's identity.

All other phases and ownership paths remain: simultaneous Version identities and
held duplicates, producer/paired stderr, nonzero exit, live child, oversized or failed
read and their original final cleanup/quarantine. Caller-owned returned bytes survive
retirement, including subsequent JSON validation. No product source or API changes.

The normal guarded remover preserves the repository's existing security semantics;
it does not claim atomic file-ID-conditioned unlink against a hostile same-user
replacement. An already-started OS call can still return late. Any late/error return
remains failed under the same caller clock and existing owned-state cleanup.

## Boundaries and verification

This reduces unnecessary final batch resources, not the cancellation work required
for success. Keep original 25ms polling and stopped-progress proof, eight-second
case including cleanup, 30-second capture horizon, five-second production stopping
grace, 35-minute job and every original assertion. Existing successful cleanup
summaries provide actual native interval evidence. Total performance and historical
OS causation remain unknown until native qualification, and a pass does not prove
the old OS cause. Do not retry an unchanged source until it turns green.

1. **Review the evidence and plan.** Verify: SPEC and Findings separate late cleanup
   from cancellation failure; exact owning issue has this plan, and parent reviews
   it before a separate development session changes source.
2. **Implement the narrow fixture lifetime change.** Verify: only the Windows CLI
   support module and existing CI capture step change; actual successful History
   child/read precede retirement, original guards/errors/clocks and all other
   capture paths remain. No product code, dependencies or oracle weakening.
3. **Qualify ownership and real native behavior.** Verify: focused real CLI History
   captures return valid owned bytes and remove only completed carriers, keep private
   root/sibling canary, never reuse a name, refuse a pre-existing next leaf unchanged,
   and retain non-History/held-duplicate/live/error carriers. Run these regressions
   in the native step before its unchanged original exact capture selector. Final
   fmt/strict lint and all 17 actual CI jobs, Audit and paired manifests pass on one
   exact source, including all three native rows and original cleanup assertions.
4. **Publish and close only verified work.** Verify: parent reviews the complete
   diff/results, merges the follow-up PR and checks main tree equality. Update #172
   with measured native intervals, source/tree/CI/merge receipts and actual remaining
   acceptance. Do not close broader clean-machine/release tasks from this result.
