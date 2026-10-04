# Complete active-run observation

Base: main `814e689bb0717aba183dbd084e6151ba11ac4e99`. Continue SPEC's observable scheduling facts and non-mutating previews; no new overlap, retry, quarantine, retention or admission policy is proposed. Existing #156/#157 history pagination work is independent.

## Source-established gap

CLI, MCP and dashboard manual-run previews infer active work by filtering the latest 100 history rows. Their job `why` views do the same; CLI `explain` counts active work in the latest 1000 rows. An older running/queued/retry-wait occurrence can fall outside those prefixes after enough newer terminal or skipped occurrences. The scheduler still queries durable active states directly, but previews and explanations can incorrectly report no active work.

## Implementation and Verify

1. Add one read-only Store operation over an already-resolved canonical job UUID. Count all `queued`, `starting`, `running` and `retry_wait` rows, and optionally return the newest at-most-100 active rows, filtering state before LIMIT. Count and sample share one read transaction. A zero sample limit returns the count without decoding snapshots. **Verify:** older active rows remain visible behind more than 100 and 1000 terminal rows; exact total exceeds the sample cap when necessary; all four active states and running termination-unconfirmed rows count, terminal states do not; order uses requested_at_us/id. Invalid UUIDs refuse, unknown canonical IDs return zero, and callers retain their existing live-job lookup semantics.
2. Route CLI/MCP/HTTP manual-run dry runs through the count-only path and their `why` views through the active-only sample. CLI `explain` uses the uncapped total rather than counting its sample. **Verify:** equivalent fixtures yield matching overlap observations across surfaces, with preserved JSON fields, human text, redaction and not-found behavior. No enqueue, wake, cancellation, task or filesystem effects occur in preview.
3. Keep legacy history and latest/anomalous-run readers, real execution paths, state transitions and all existing test/workflow coverage unchanged. **Verify:** real Store/CLI/MCP/HTTP regressions, count/sample consistency with two WAL connections, read-error propagation, formatting, warnings-denied Clippy and exact-head platform CI pass before merge.

## Limits and publication

This reports a durable snapshot, not process liveness, reserved capacity, or a transaction spanning job-definition lookup and later execution. Existing preview decision vocabulary remains unchanged; this does not promise complete admission simulation. The `why` list remains bounded to 100 active records; `explain`'s count is not inferred from that list. Counting may scan active rows and is not an elapsed-time bound.

The owner requests implementation Drafts with verification handed off. The current container cannot resolve GitHub and has no Rust toolchain or independent development/review sub-session. Large source files may be patched in a temporary feature-branch-only runner using exact Git blobs, unique reviewed literals and full inverse equality. It must prepare an unreferenced candidate that removes its own workflow and preserves all unrelated blobs/modes. Parent reviews and publishes by non-force advancement of this feature branch only. Mechanical preparation or SQL-model checks are not Rust/native tests. Keep review and qualification explicitly pending; no main write, merge, release or user-state changes.
