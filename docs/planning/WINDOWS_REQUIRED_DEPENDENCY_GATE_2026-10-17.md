# Enforce native Windows qualification through the existing required check

Issues #172/#180/#181; PR #183. Source517b0cc CI39000739527 passes all17
actual jobs/native3 and Audit. Registering native3 as additional required contexts
fails403 Resource not accessible by integration. The existing11/strict rules
remain unchanged. Use authorized CI configuration to strengthen the already
required Windows lint result instead; do not change administrative permissions.

This explicitly supersedes earlier14-context restoration criteria with equivalent
ordinary-PR native enforcement through one existing required context. Report the
actual registered count11 truthfully. Preserve all previous failed/native evidence.

1. **Review the fallback before implementation.** Verify: SPEC, Findings and
   Implementation record the real403, primary GitHub needs/always semantics,
   acyclic fixed-three-row DAG and changed enforcement mechanism. Update/read
   back #172/#180/#181; parent-review before separate development.
2. **Change only the existing required lint job dependency and guard.** Verify:
   needs windows-foundation; jobif always() AND original ordinary predicate;
   first conditional guard throws when needs.windows-foundation.result !=success.
   All-success runs every original six lint commands. Failure, cancellation,
   skipped, empty and unknown result fail ordinary required lint instead of
   silently skipping it. Preserve original exploratory-dispatch exclusion,
   matrix3/fail-fast,35-minute execution limits, permissions and all other bytes.
3. **Qualify the final workflow and source.** Verify: independent whole-workflow
   inverse/DAG/result truth-table review and YAML parse pass; original product
   files unchanged. Refresh clean-head stable/MSRV maintenance/fullCLI artifacts,
   fmt/strict lint and new same-source all17 actual CI jobs/native3/Audit pass.
   Confirm required lint starts only after all three native jobs complete and
   its original commands execute. Failed history is not promoted or retried.
4. **Publish and complete actual acceptance.** Verify: existing11 registered
   contexts/strict/bypass/other rules read back unchanged; required Windows lint
   now enforces native3 plus lint. No14-context claim or administration mutation.
   Merge the qualified head, verify exact main tree, complete each owning issue's
   actual Verify and confirm no remaining open PRs.
