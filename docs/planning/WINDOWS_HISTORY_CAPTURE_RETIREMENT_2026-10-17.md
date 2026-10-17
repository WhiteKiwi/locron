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
existing status-zero/root-exit/read checks and reader/child closure. The reusable
Command still owns its configured stdout duplicate after spawn; for successful
History only, reset that stdout to null after the body returns and before unlink.
Select retirement in the Default-Command wrapper, exclude held-duplicate controls,
and keep InitialRunOwner and the original capture body unchanged. Use the existing
normal guarded `remove_private_file`, with the original Control check before and
after. Split that leaf's original guard into its file and parent guard, close the
file, and retain the parent guard through removal. Keep the owning private root
and parent-chain protection, and preserve removal errors.
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
   child/read and parent Command stdout closure precede retirement; the original
   parent guard survives removal, and original errors/clocks and all other
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

## Reviewed phase amendment after b317 native results

b317's CI38990074641 completes16/17 actual jobs. Both History retirement
regressions and the original combined native capture contract pass on all three
Windows rows. MSRV standalone cancellation instead expires during History: actual
daemon spawn returns at365291us, readiness is first observed at4333446us, leaving
less than4.032s of the inherited8s, before the required5s production force-stop
grace. Durable Cancelled is unobserved; later cleanup takes8973us. This failed
result remains evidence. Run's847010us producer timestamp has its own local origin;
it cannot isolate setup/Add cost in the Control clock. Add-only extraction is
therefore not selected as an unmeasured sufficient fix.

The SPEC phase amendment explicitly supersedes the earlier plan's post-spawn8s
total Windows cancellation requirement. Setup runs under the same entry-born30s
outer horizon. After actual Running plus at least two strictly increasing counters
and daemon-live proof, immediately before actual Cancel dispatch, monotonically
shorten the current case ceiling to min(current, now+8s). Near the outer horizon
only its remaining time is available; never grant30+8s. This change measures the
actual cancellation contract while retaining every real readiness and termination
oracle. It is a fixture clock-scope correction, not a proven setup optimization.

1. **Review amended contract and failure evidence.** Verify: SPEC, Findings and
   Implementation record the new origin, unchanged total ceiling/product grace,
   preserved b317 failure and unmeasured OS cause. Update/read back #172 and parent
   review before development source changes.
2. **Implement monotonic phase admission in the separate development session.**
   Verify: cancellation_daemon publishes the existing outer ceiling at real spawn;
   cancel_work shortens it before constructing/dispatching Cancel after real
   readiness. A fixture-only primitive requires a nonzero case, prechecks admission
   and expiry, rejects extension, CASes only to an equal/earlier encoded deadline,
   then postchecks. Never reset case/probe/admitted or promote a late result.
   Both standalone/combined share this path. Other cases and product source stay
   unchanged, including the authenticated probe's original seal and deadlines.
3. **Verify actual limits and final source.** Verify: focused native controls prove
   shortening takes effect, extension/absent-case/expired/refused admission cannot
   be accepted or reopened, and outer cap remains. Execute these serially in the
   existing cancellation step, keeping original exact cancel and combined capture
   controls. All17 actual jobs, all three native rows, strict lint and Audit must
   pass on one final source. Existing paired-manifest CI coherence runs; separate
   path-filtered workflow is not described as a current-head run.
4. **Publish and restore protection.** Verify: complete diff review, restore/read
   back the three native required contexts alongside the existing11, ready/merge
   the follow-up PR at the verified head, check main tree equality, record receipts
   and close only #172/#180/#181 after their final criteria succeed.
