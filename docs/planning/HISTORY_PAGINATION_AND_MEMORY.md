# Complete history pagination with bounded result memory

Base: main `814e689bb0717aba183dbd084e6151ba11ac4e99`. Continue the frozen SPEC's observable execution history, STORAGE.md's read-transaction boundary, and the dashboard's existing 1..100 page-size / offset / literal-search contract. This is a correctness and implementation-cost correction, not a new history API or retention policy. Existing dashboard, pruning, output-repair and Windows diagnostic Drafts remain separate.

## Source-established gaps

The job-filtered HTTP endpoint counts all live-job runs but fetches `limit + offset` through `Store::history`, which caps the prefix at 1000 before the handler skips the requested offset. Pages crossing 1000 are shortened and later pages are empty despite a larger total. The count and page are also read separately.

`Store::search_history` eagerly collects every run and its full snapshot JSON before filtering and keeping a page. Even an empty query loads the entire retained history, although the response contains at most 100 runs.

## Implementation and Verify

1. Add a read-only Store page operation with an optional live-job reference. Resolve the reference, count and select the bounded page in one read transaction. Apply SQL LIMIT/OFFSET directly; distinguish huge/after-end offsets from integer overflow and return the exact total plus an empty page. Preserve newest-first requested_at_us/id order and current live-job-only filtering semantics. **Verify:** over 1200 runs produce complete pages at offsets 990, 1000 and 1100, correct final/empty pages and exact totals; ties, unknown/removed job references and concurrent writes do not mix count/page snapshots. Existing Store::history remains capped and unchanged for its current callers.
2. Route empty searches through the same global SQL page operation. For nonempty literal Unicode-lowercased search, iterate database rows and retain only the requested matching page instead of collecting all rows. Count every match under the same read transaction. **Verify:** empty and literal-search pages retain all current matching, trimming, Unicode, order and total semantics; late read failures remain failures; Rust-owned results hold at most one current row plus the capped page. This does not claim bounded SQLite cache/sort memory or sublinear nonempty-search time.
3. Replace only the job-filtered HTTP handler's count/prefix/skip branch with the new Store page result. Keep response fields, validation, redaction and attempt enrichment unchanged. **Verify:** real authenticated HTTP cases cross the previous 1000-row boundary without truncation and preserve 400/404 behavior, q/job exclusivity and 1..100 limits; focused Store/server tests, formatting, Clippy and exact-head platform CI pass before merge.

## Publication and handoff

Use an exact, hash-pinned transformation for the large source files rather than reconstructing unseen text. A temporary feature-branch-only GitHub runner may prepare an unreferenced candidate commit, proving unique anchors, inverse equality and unchanged other file blobs/modes; remove that temporary workflow from the candidate tree. The parent reviews the diff and publishes only this feature branch without force. No main write, release, installation, live job execution or user-data mutation.

The owner requests implementation Drafts with verification recorded for handoff. No independent development/review sub-session is available. Source/SQL-model checks are not Rust or native execution evidence; leave regression, concurrency, memory measurement and independent review explicitly pending. Keep the tracking issue open until Verify and merge requirements are satisfied.
