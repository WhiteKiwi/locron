# Capture the unexpected terminal native target cause before choosing a fix

Owner #172; PR #183. Source13705bd CI39003147356 x64stable job117310099259
fails the actual-dashboard SSE continuation: target exit101/failed instead of
running, finalized output559 payload/717 physical bytes, cause unavailable.
Lifecycle15PASS/1FAIL; HTTP31PASS; MSRV/ARM whole jobs pass. Raw output is absent
from uploaded receipts. Preserve the failure and do not infer raw5/persist/holder.
The existing required lint guard genuinely fails on this native failure.

1. **Review cause-capture scope before implementation.** Verify: SPEC, separate
   research/Findings and Implementation record known/unknown facts and privacy
   bounds. Update/read back #172 and parent-review this plan before development.
   Old-head macOS DNS-only retry stays suspended, actual retries0.
2. **Add only closed failure observations.** Verify: only windows_lifecycle.rs
   changes on the PR source. Existing failure formatter observes only terminal
   exit101/finalized canonical output through guarded access and explicit small
   physical/frame/payload caps. Existing validated frames reassemble STDERR;
   exactly one strict phase/counter/allowlisted-kind/raw-i32 record is rendered
   as typed safe fields. Missing/refused/malformed/duplicate/oversized/partial/
   live data stays unobserved; no raw payload/path/UUID/token/panic text emitted.
   Add inspect_err stage emission for create/write/flush, preserving original
   expects and existing persist log/operation. No retry or behavioral fix.
3. **Verify observation boundaries and original controls.** Verify: genuine
   FrameWriter controls cover fragmented positive records and wrong-channel,
   malformed/duplicate, corrupt/truncated/oversized, missing/refused negatives.
   Independent inverse preserves every original16 lifecycle body/assertion,
   clock, owner and cleanup plus held-destination/raw5/same-candidate/release
   control. Only the existing helper-call path argument and selected helper
   observation/emission change. Format/strict lint pass; native compile/tests
   must validate Windows-only code, not a Linux no-op.
4. **Run a finite real native probe in a separate branch.** Verify: parent creates
   isolated worktree/branch from reviewed observation source; separate developer
   changes only that branch's existing dispatchable CI workflow to real native
   CLI/lifecycle builds with exact head/tree/artifact receipts. Execute the full
   original lifecycle suite up to50 times, stop on first nonzero, retain original
   clocks/cleanup and35-minute job ceiling. Never publish reduced CI to the PR.
   No failure observed means non-reproduction, not proof of a fix.
5. **Decide from actual results, then qualify publication.** Verify: record exact
   observed stage/error or explicit unknown. Before any behavior change update
   the applicable plan/Findings/Implementation/#172 and review it. Eventually
   require final-head all17 ordinary CI/native3/Audit, fresh local artifact-bound
   suites and existing11/strict plus required native dependency. Merge only the
   verified head, check main tree and owning criteria; keep failed history intact.
