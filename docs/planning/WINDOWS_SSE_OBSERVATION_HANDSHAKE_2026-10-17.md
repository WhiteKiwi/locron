# Observe each real SSE stage before advancing the fixture

Issue #172; follow-up PR #183. Baseline source
d0c06293e194890b3f3f7a16a0527d9e99dea4e4. Its three native cancellation/capture
steps pass; final ARM HTTP contract instead fails actual event ordering in
CI38993049610/job117279470281. Preserve both results independently.

The live fixture advances Store state with fixed300/600ms sleeps and reads its
real response after all mutations. The actual200ms poll can first discover frames
after Running, yielding run/attempt before output. The polling product contract
does not promise to reconstruct unobserved intermediate snapshots. The test must
prove its preceding observation before publishing the next mutation.

1. **Review evidence and causal premise.** Verify: SPEC, Findings and Implementation
   explicitly distinguish this event-order failure from passed cancellation and
   identify original10s as per-chunk, not global. Record/read back #172 and review
   the completed plan before development.
2. **Implement the test-only handshake in a separate session.** Verify: only
   crates/locron-server/tests/contract.rs changes. The same actual cookie-authenticated
   response is owned by a small incremental collector with retained partial buffer
   and all events. Observe/validate queued, then starting run+attempt, then two
   outputs/seq0+1, then running run+attempt before each next Store mutation. Complete
   and drain through actual terminal EOF. Preserve every original final assertion
   literally and all other read_sse callers. No task/channel/dependency/product hook.
3. **Verify real streaming and complete final source.** Verify: existing live,
   reconnect, disconnect and output-sequence tests and the full HTTP contract suite
   pass locally; fmt/strict Clippy pass. Existing10s per awaited chunk and200ms
   production polling stay unchanged; missing acknowledgment, premature EOF or read
   failure refuses. No unchanged rerun/skip/timeout increase. New final-source CI
   must pass all17 actual jobs, Audit and native x64/ARM/MSRV original commands.
4. **Finish publication and issue acceptance.** Verify: parent complete diff review,
   restore/read back native3 required contexts preserving existing11/strict rules,
   ready/merge verified PR head, confirm main tree equals source tree, then close
   only verified #172/#180/#181 with exact evidence and preserved failed history.
