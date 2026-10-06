# Combined Windows library evidence leaves — 2026-10-06

## Selected scope and evidence

Freeze SPEC/architecture. This CI-only fixture correction is planned at clean unpublished
Native+A+main7635cef4c77e3fd98621253995ec10cd1a437685, treeaa6b933e4e77b255219a7a5f573f965be87fb590.
A BuildFacts (dashboard_boolean_qualification.rs1940–1952) and producer facts (ci178–181)
do not bind an evidence path. Each Server binds its own current executable/fullID/SHA
(2984–2993). The evidence guard requires NotFound before private creation (2999–3005),
and completed.json is retained (3129–3130). Reusing the first environment for the second
full-lib therefore collides with the original absence requirement. This is a Source
contract finding, not an observed hosted cause or new PASS. Root's independent B audit
of per-row fresh TempDir/captures/CREATE_NEW found no matching fixed global collision.
Every historical failed/unexecuted result and remaining native acceptance stays intact.

## Exact future workflow-only change

Future Source changes ONLY .github/workflows/ci.yml's Windows-foundation composition.
Place BOTH original complete A/B artifact producers before the first full-library run;
preserve their distinct namespaces, admitted parents, Cargo targets, facts and hashes.
In A's producer, alongside the original evidence variable, compute a SECONDABSENT sibling
as parent / "redacted-evidence-second"; check it is distinct from the first path and
actually absent using the original absence-check policy. Do not create/adopt/repair it.
Export only the new scalar LOCRON_PR144_SECOND_EVIDENCE_DIR through GITHUB_ENV, leaving
original facts fields/16KiB cap/hash checks and first evidence export unchanged.

Keep the first A-target full-library consumer and first upload literally unchanged.
For the second B-target full-library step, override ONLY LOCRON_PR144_EVIDENCE_DIR with
${{ env.LOCRON_PR144_SECOND_EVIDENCE_DIR }} in that step's env. Preserve its B-target
selection and all validation/argv/conditions; BOTH literal Native -- --test-threads=2
suffixes remain. Do not rewrite the job-wide first path or change application inputs.
Add one independent second upload after that consumer: same original condition
${{ always() && steps.pr144_helpers.outcome == 'success' }}, actions/upload-artifact@v7,
retention-days14, if-no-files-found:error and original bounded *.json evidence policy;
path ${{ env.LOCRON_PR144_SECOND_EVIDENCE_DIR }}/*.json and distinct name
pr144-windows-${{ matrix.platform }}-rust-${{ matrix.rust }}-second. Original first
name/path/upload remain literal; missing second evidence must fail rather than pass.

All test/production Source, guards/fullID/descriptor checks, facts/targets/hashes, Unix
blocks, old commands/oracles/selectors/owners, caps and original clocks stay literal.
No retry, old evidence removal, new fixture creation in Python, warming, timeout growth,
security repair, skipped test or assertion relaxation is selected. This narrowly
supersedes ONLY d2's no-additional-A-env/upload adaptation for this absence collision.
The one finalPR171 route, all20 contribution conservation, same-final-head11 contexts,
Guardian/package/paired reviews, GitHub squash proof and residual172/owners remain.

## Ordered steps and concrete Verify

1. Root reviews/commits only this plan and literal FINDINGS/IMPLEMENTATION/ISSUES tails.
   **Verify:** complete four-path diff/prefix inverses/protected modes/blobs/physical
   bytes and empty Source/index; WHOLE plan/Verify POST/exact GET ALL #172/#31/#163/
   #162/#146/#149, then ACTUAL entire committed plan/tails reread AFTER ALL GETs.
2. Separate development composes only the selected Windows CI steps at that Docs parent.
   **Verify:** whole-workflow inverse, complete A/B producers/targets/facts/hash and
   all original Unix/first-consumer/upload bytes, both Native thread2 suffixes; all
   other tracked modes/blobs/physical bytes protected, actionlint/diff and complete
   staged/unstaged review. Any changed decision returns to Docs FIRST before Source.
3. Root publishes a genuinely changed final head and reads all three native rows.
   **Verify:** exact head/base/synthetic tree/toolchain/facts/targets bind both runs;
   original A/B selected controls and own Server fullID/SHA/guards/completion/cleanup
   are actually reached. Two distinct fresh leaves and independent bounded artifacts
   must bind each consumer. Record refusal/failure/skip/unreached/unknown, never old PASS.
4. After final combined Source publication, select ONE NEW final-production Wake/Cancel
   pair only after genuine current-head x64 package Source/tree/ZIP/PE/hash verification.
   **Verify:** later external receipt binds exact final Source/artifact IDs/archive/EXE
   SHA; new task/one-shot marker, literal fb8 controller/progress/profile parent+validator/
   commands/oracles/first-work/5s/8s/30s/25ms/caps/owned cleanup; only explicit source and
   package/archive/EXE bindings change. Root reads whole script/diff/full inverse BEFORE
   separate execution lease; no floating/current-installed substitute, warming or growth.
   Old bd86 PASS and earlier FAIL remain unchanged: this new pair is not their retry.
   This supersedes only d2's no-current-binary-substitution clause for the NEW final pair.
   Final product failure stays failed and needs diagnosis before merge, never CI-only.
5. Root records outcomes and completes the existing final delivery plan at actual scope.
   **Verify:** all20 contributions/Docs and one finalPR171, eleven genuine same-head
   requirements plus Guardian/package/paired reviews and exact squash main tree/parent;
   retain residual #172/matching owners and all unsatisfied Verify. No closure from
   fixture wiring, ancestry alone, synthetic facts or local/metadata success.

GitHub documents [step env precedence](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepsenv)
and [artifact names/paths and retention](https://docs.github.com/en/actions/tutorials/store-and-share-data).
These references establish mechanics only, not actual workflow/native PASS. Root owns
commits/Issues/API/publication/memory; this Docs phase runs no Source/native/compiler/test.
