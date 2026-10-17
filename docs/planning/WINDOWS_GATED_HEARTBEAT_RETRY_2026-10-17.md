# Preserve gated heartbeat progress through a finite raw5 retry

Owner #172; PR #183. Actual diagnostic39008090099/job117324576383 iteration5
reports the failed child's phase=persist/counter855/PermissionDenied/raw5,
failed101/finalized565/723. Four full19PASS suites precede18PASS/1FAIL. Native
subcall and holder remain unknown; the control's counter8 is distinct.

1. **Review the evidence-conditioned scope.** Verify SPEC, separate research/
   Findings and Implementation, #172 exact readback and parent entire final
   plan review before separate development. Preserve all failed probe/CI facts.
2. **Retry only the observed gated fixture refusal.** Verify only the lifecycle
   test file changes: one new gated publisher creates one snapshot and performs
   the original first low-level persist. A helper consumes a real PersistError
   and retries only raw5 with its same file. Use one immutable additional-retry
   horizon min(original gate deadline, publisher entry+100ms), never reset;
   callback and <=1ms remaining-time sleeps are charged. Check admission around
   callbacks/sleeps and before every added native call. Unsupported errors or
   exhausted admission return the last actual refusal/candidate. Native calls
   are non-preemptible: do not claim strict100ms wall completion. The original
   first operation remains unchanged; expired retry admission never invents
   an OS error. Emit one closed terminal record and panic on terminal refusal.
   Existing one-shot publisher/persist/log/held-control bytes stay literal.
   Change exactly the gated target's one publish call to pass its ORIGINAL
   absolute60s deadline. All other15 bodies, original3s progress/other clocks,
   assertions, ownership/cleanup, product/dependencies remain exact.
3. **Prove real native safety and completion.** Verify five meaningful controls:
   actual share1 refusal then synchronous exact-handle release publishes8
   from the same candidate after proving old7; permanent held100ms returns
   actualraw5/samecandidate/old7 and only explicit release permits recovery;
   pre-expired retry receives a genuine refusal and admits no callback/attempt;
   absent destination parent gives genuine non5 and admits no retry; genuine
   default-sharing reader records success or actualraw5 while proving exact
   complete-counter/candidate/release invariants. No guessed holder claim.
   Release control may use its existing30s owner horizon; permanent-refusal
   control separately qualifies exact100ms admission. Independent source
   inverse, fmt/strict local lint and native24 inventory must pass.
4. **Qualify publication on the final source.** Verify a fresh isolated harness
   binds real native CLI/test artifacts and unchanged1.98 strictlint/stable
   execution, full24/defaultparallel/max50/firstfailure/35min. This is a finite
   qualification of changed behavior, not repetition until success. All17
   ordinaryCI/native3/Audit, fresh local bound fullCLI/maintenance, no open review
   blockers and existing11/strict plus required native dependency must pass.
   Merge only that verified head, verify main tree and owning criteria before
   closure. Historical passes never qualify a changed head.

## Active-connection publication and isolated trigger

The shell credential is expired401; the repository connector remains active.
Parent may publish exact blobs/tree/commit/ref using that connection and verify
the readback, then fetch public objects locally without the stale credential.
Because connector workflow dispatch is unavailable, the separate qualification
workflow adds push ONLY for diagnostic/windows-gated-heartbeat-qualification,
with an exact event/ref guard, while retaining explicit dispatch. No main/PR
trigger or reduced PR workflow is allowed. Update/read back #172 and reread this
whole amendment before separate CI development; all24/50/35min/final gates stay.
