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
