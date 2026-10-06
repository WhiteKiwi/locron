# Read-only MCP cancellation decisions

Refs #161, stacked on #164 at `a930b708c3d61c891316ba44e3839f2fd8d18466`. Reviewed before source against the current root SPEC/IMPLEMENTATION/ISSUES workflow, docs/mcp/SPEC.md, core CancellationDecision and the actual Store cancellation transaction. Core CancellationDecision currently names the completed outcome only; the admission predicates and effects live in Store. Extract that existing admission once and share it with a read-only Store preview; do not create a competing MCP policy.

## Observable contract

A cancellation dry-run observes one durable state/reason/prior-request snapshot, describes the same prospective outcome or refusal as a live call with the same explicit acknowledgement, and never reserves the outcome or proves process death. The live transaction always rereads its own current row before admitting effects.

Retain dry_run, run_id, state and would_request_cancellation. Add decision, already_requested, would_cancel_before_execution, would_acknowledge_unconfirmed and resulting_state. Exact cases:

| Observed state / option | decision | would_request_cancellation | would_cancel_before_execution | would_acknowledge_unconfirmed | resulting_state |
| --- | --- | --- | --- | --- | --- |
| queued or retry_wait, no acknowledgement | cancelled_before_execution | true | true | false | cancelled |
| ordinary starting/running, no prior request | cancellation_requested | true | false | false | unchanged |
| ordinary starting/running, prior request | already_requested | false | false | false | unchanged |
| running + termination_unconfirmed, acknowledgement true | acknowledged_unconfirmed | false | false | true | interrupted_unknown |

The before-execution boolean describes immediate durable cancellation, not process death. A queued/retry-wait run would still be cancelled even if an old cancellation timestamp exists; do not mistake that case for an already-requested running cancellation. already_requested reports the observed timestamp only and is not proof of a live process or quarantine clearance.

Quarantine without acknowledgement, acknowledgement outside a running termination_unconfirmed quarantine, missing run and terminal state refuse using the existing live Store errors. In particular, an acknowledgement preview is not a new process-cancellation request. Descriptions and examples must not imply otherwise. Ordinary live result fields and quarantine policy do not change.

## Architecture and scope

Use the existing CancelOutcome as the typed prospective decision. A public read-only CancellationPreview record includes state, outcome and whether a durable request already exists; its constructor performs one SELECT through an already-open Store. The shared pure decision accepts only row facts and the explicit acknowledgement flag. Both preview and the existing IMMEDIATE cancellation transaction call it. Preserve live SQL, event payloads, retry removal, one-time completion action and commit boundaries. No schema, dependency, scheduler/OS process control, release or credential change.

Audit other cancellation surfaces before widening the patch. Current CLI Cancel takes run_id and acknowledge_unconfirmed without a dry-run flag; the dashboard cancellation request likewise has only acknowledgement. Their actual calls inherit the shared admission with unchanged output. The source audit is not a claim that every historical or external client was tested.

## Change order and Verify

1. Extract the pure Store admission and add the one-snapshot read-only preview, then switch the existing live transaction to the same decision. **Verify:** all active/terminal/quarantine states and both acknowledgement values preserve existing outcome/error text; a preview followed by a changed state does not authorize stale live effects. Compare every old/new SQL and event literal so no live effect semantics drift.
2. Replace only the MCP dry-run branch with the typed preview renderer and record the response clarification in docs/mcp/SPEC.md. **Verify:** ack preview reports would_acknowledge_unconfirmed=true and would_request_cancellation=false; ordinary already-requested calls do not invent a second request. Missing/terminal/refused states use the existing tool-error envelope. No wake is sent from dry-run, and native/runtime strings stay unreflected in new response fields.
3. Add pure decision tests, Store read-only/no-write checks and real MCP subprocess fixtures built through public Store lifecycle APIs without executing a target. **Verify:** queued, starting, running, retry_wait, terminal, quarantine and prior-request cases match independent live fixtures; previews leave file bytes, run/attempt facts, retry intents, events and completion actions unchanged. Real acknowledged cancellation preserves the audit event and interrupted_unknown result. Validate races by altering state between preview and live execution.
4. Format and execute focused and existing locked Rust/MCP/Store checks in an isolated hosted runner. **Verify:** no-zero-test filtering, warnings-denied Clippy, scoped diff and expected branch head; preserve other pending PRs. Record actual results and unexecuted native gates separately before handoff.

## Review and publication

The temporary preparation workflow stays outside the final feature tree. No main push, force push, merge, issue closure, release, installed job/service mutation or automatic risk acknowledgement is authorized. The current chat has no independent development/review sub-session; hosted execution is test evidence only. Keep Draft for independent review and exact-head x64/ARM64/Unix qualification. Merge #164 first and retarget this follow-up afterward.
