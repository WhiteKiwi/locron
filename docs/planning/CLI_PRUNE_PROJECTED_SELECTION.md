# Bounded explicit CLI prune selection

Refs #149. Base main `15820ed96cc6da2bd8c37d42ec8788fd2180114b`; CLI source blob `4e975d509029c94f277cd43a3adcf1d38308acfa`.

## Existing contract and scope

Continue the frozen SPEC's bounded log retention and non-mutating dry runs, STORAGE.md's terminal-output eligibility and intent/effect/completion ordering, and the explicit CLI follow-up recorded in #149. Dashboard #150 already carries its separate implementation. No new retention policy or change to the output format is proposed.

The current explicit CLI collects every candidate against the original retained byte total, then decreases that total only during removal. If usage is above the cap, the entire fetched batch can be selected unnecessarily. The final byte sum is also computed only after live effects and uses unchecked arithmetic.

## Implementation and Verify handoff

1. Scan the existing oldest-first, at-most-100 candidate batch once. Decrease a projected retained total whenever selecting an artifact, clamp projection at zero, and stop selecting fresh outputs when projection meets the existing byte cap. Continue selecting age-expired outputs under the same 30-day cutoff. **Verify:** 110 retained / 100 cap / three fresh 10-byte candidates selects only the first; test exact/below cap, age-only, mixed age, zero-size, large-size and empty/bounded batches with exact identity/order expectations.
2. Reject negative byte accounting and checked selected-byte overflow before any pending intent or filesystem effect. Use the already-computed byte total in both dry-run and live reporting; remove the obsolete execution-time retained update. **Verify:** invalid accounting refuses before mutation; dry/live against equivalent isolated state produce the same IDs, count, run count and bytes; no aggregate error can first occur after removal.
3. Preserve the existing missing-state dry-run return, read-only/live Store paths, Windows canonical path and guarded deletion, Unix refusal behavior, pending/removal/completion sequence, human strings and JSON fields. **Verify:** existing CLI prune/maintenance contracts, refusal and failure recovery cases, formatting and warnings-denied Clippy plus native x64/ARM64/Unix checks succeed before merge. #150's stronger dashboard path checks are not silently backported or claimed here.

## Publication and qualification

The source edit is a narrow, exact-match transformation of the pinned CLI blob, not a replacement reconstructed from excerpts. A temporary workflow on this feature branch may apply the literal patch in a disposable GitHub runner. It must verify the source Git blob, the unique function/patch anchors, reverse-patch equality and unchanged branch head; publish only the corrected source while deleting its own workflow in the same commit. No main write, force push, tag, release, dependency installation or user-state operation is authorized. The final PR diff must contain only this plan and the CLI source; no new permanent workflow is requested.

The owner requested implementation-first Drafts and explicit unexecuted Verify criteria. A runner performing a mechanical patch is not an independent development/review sub-session. No separate sub-session is available in this chat. Independent review and actual Rust/native behavioral qualification remain required; never describe source checks or an arithmetic model as a passing CLI regression. Do not merge or close #149.
