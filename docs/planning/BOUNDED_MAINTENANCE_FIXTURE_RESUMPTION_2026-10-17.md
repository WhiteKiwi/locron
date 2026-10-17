# Acknowledge portable maintenance completion across bounded passes

Issues #172/#181; PR #183. Actual source678b7cf92f8fe937d874c9441c354af5062ffc18
CI38995426850/LinuxMSRV job117286813154 returns successful resumed outputs_pruned0
against expected1 at maintenance.rs1109. This is the same successful-partial-pass
contract documented in Findings6635; its exact native deferral cause is unknown.

1. **Review contract and evidence before implementation.** Verify: SPEC, Findings
   and Implementation retain250ms/100-action production passes, fixed5s helper
   progress allowance and exact final safety assertions; #172/#181 plan/readback
   and parent review precede separate development.
2. **Correct the finite portable positive fixtures.** Verify: only the maintenance
   test module changes, reusing unchanged maintain_until for:
   - repairs_and_finalizes_a_referenced_partial;
   - recovered/missing branches of reconciles_an_already_renamed_final_and_a_missing_artifact;
   - finalized_fixture setup, with recovered>=1/exact1 before real candidate query;
   - foreground_after_prune_intent_preserves_file_and_next_pass_resumes;
   - only later completion in expired_pass_preserves_pending_work_and_a_later_pass_completes_it;
   - setup and eventual output+run completion in resumes_pending_output_prune_before_deleting_run_metadata;
   - setup and completion in completes_pending_prune_when_the_output_directory_is_already_missing.
   Foreground initial setup must acknowledge the actual held intent handle before
   original pruned0/file-present/pending1 assertions. Release the same handle before
   bounded resume. All exact final counts/files/frames/rows remain. Do not query
   Store inside completion predicates; actual recovery counter follows successful
   durable reconciliation. Real Err still fails; no production/helper/limit changes.
3. **Verify safety and real completion.** Verify: explicit initial expired pass,
   action100 control, Windows held-reader/time/refusal/unsafe controls and frozen
   qualification rows are unchanged. Maintenance and complete CLI unit tests pass
   locally on stable/MSRV, fmt/strict lint/diff and independent review pass. New
   final source must pass all17 actual CI jobs, Audit and all original native rows;
   preserve the failed678 result and do not retry unchanged source or skip cases.
4. **Publish verified result.** Verify: restore/read back native3 required checks
   while preserving old11/strict rules; merge reviewed head, verify exact main tree
   and close only #172/#180/#181 after all their actual criteria succeed.

## Referenced-run orphan isolation amendment

f475 source CI38996053842 finishes16success/1MSRVfailure. Maintenance
job117288877733 observes exact recovery1/orphan1, then referenced1.log absent.
HTTP31/31 passes afterward; x64 stable and ARM jobs pass completely. The fixture
finishes its synthetic run at epoch9 but maintains at real2026+2h, legitimately
meeting default90-day run retention. A later bounded pass may therefore prune the
reference before orphan scanning completes. Actual per-pass native timing is not
instrumented. This is a fixture isolation correction, not a production policy fix.

1. **Review isolation before implementation.** Verify: SPEC, Findings and
   Implementation record the incompatible clocks and source-backed retention
   mechanism. Update/read back #172/#181 and parent-review this amendment before
   separate development. Preserve f475's failure and passing HTTP observations.
2. **Isolate only the existing-run orphan control.** Verify: capture one
   orphan_pass_time value; set run_retention_age_us=none through Store before the
   first maintenance call and pass that same time to unchanged maintain_until.
   Preserve the original exact recovered1/orphan1 and referenced-present/orphan-
   absent assertions literally. No production policy/helper/budget/other test
   changes; existing orphan-cursor isolation is the precedent.
3. **Qualify and publish the new source.** Verify: independent minimal-diff
   review, stable/MSRV maintenance and full CLI with actual clean-head artifact
   bindings, fmt/strict lint, then all17 actual CI jobs/Audit/native3 pass on the
   new source. Restore/read back native3 required contexts preserving old11/strict,
   merge verified head, verify main tree and complete #172/#180/#181 criteria.

## Windows post-reader-release completion amendment

50e1 source CI38998271829 completes16success/1MSRVfailure. The maintenance
suite17PASS/1FAIL reaches the post-release recovered1 assertion in
reader_sharing_timeout_keeps_recovery_pending_until_a_later_pass with actual0.
Its earlier real sharing refusal/4500ms-to8s Err/pending1 checks have passed,
as have the orphan fixture and later HTTP31/31. Native x64stable/ARM full jobs pass.
One successful250ms pass is not proof of eventual recovery after releasing a
reader. Actual native deferral boundary remains unknown.

1. **Review this explicit scope amendment.** Verify: SPEC, Findings and
   Implementation distinguish initial real refusal from later completion. Update
   and read back #172/#181 and parent-review before separate source development.
   Preserve50e failure and the other passing controls as their actual observations.
2. **Change only later positive completion.** Verify: replace maintain(...,11)
   with unchanged maintain_until and recovered>=1 in this named Windows test.
   Exact inverse preserves the whole initial sharing/timeout/pending/reader-drop
   block, all final recovered1/file/frame assertions, helper, production and other
   fixtures. Existing5s progress/250ms/100 actions and real-error refusal stay.
   This supersedes the previous blanket Windows exclusion only for this call.
3. **Qualify and publish.** Verify: independent review, stable/MSRV maintenance
   and artifact-bound fullCLI, fmt/strict lint, then new same-source all17CI/Audit
   and native3 pass, including this actual Windows-only path. No unchanged rerun,
   test skip or timeout enlargement. Restore/read back native3 required checks
   preserving old11/strict, merge verified head, verify main tree and close only
   #172/#180/#181 after their real criteria succeed.

## Finite expansion to the other two Windows positive premises

Separate source audit finds the same successful-partial premise in the owned-reader
worker and unsafe-object run25 setup. Neither is reported as a failed50e control.
This explicitly expands the previous single-call amendment to three named positive
call sites only; initial refusal/timing and frozen qualification remain unchanged.

1. **Review before these additional edits.** Verify: SPEC/Findings/Implementation
   record both actual source assumptions and distinguish them from observed
   failure. Update/read back #172/#181 and parent-review before separate development.
2. **Acknowledge the real positive premises.** Verify: owned-reader worker uses
   unchanged maintain_until recovered>=1, sends the report directly, and removes
   only the redundant Result-envelope unwrap. Keep original150ms no-completion,
   5s receiver, join and every final exact report/file/frame assertion. Helper5s
   starts in the worker and includes held-reader work; realErr immediately fails.
   Unsafe-object run25 setup uses the same helper plus exact recovered1 before
   finalfile/candidate reads and actualACL mutation. Preserve every subsequent
   unsafe/refusal assertion. Inverse restores every other byte, including all
   negative/timeout/orphan/frozen controls, production and helper.
3. **Qualify and publish all three Windows positive corrections together.**
   Verify: same stable/MSRV fullCLI/maintenance/fmt/strictlint/independent review
   and new final-source17CI/Audit/native3, then unchanged protection/merge/tree/
   issue-closure criteria. Existing limits stay; successful0 never counts asErr.
