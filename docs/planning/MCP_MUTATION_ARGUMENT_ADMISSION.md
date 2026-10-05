# MCP mutation argument admission

Refs #160. Reviewed against main `814e689bb0717aba183dbd084e6151ba11ac4e99`, MCP blob `d012a4e9778cb378219ec4974d0ef3f6f0848b1f`, the root SPEC/IMPLEMENTATION/ISSUES workflow and docs/mcp/SPEC.md. This implements the existing boolean, integer, array and non-mutating preview contract. No new maximum wait duration, scheduler policy or transport is introduced.

## Contract and approach

Validate each of the seven mutating tool entrypoints before it opens any Store. The server-owned tools/list schema is the source for required fields, primitive types, integer bounds, enums and string-array members; do not maintain a second drifting list of optional metadata fields. Only declared properties are validated, preserving the existing schema's allowance for unrelated extension fields. Unsupported schema shapes fail closed. Error text names only server-defined fields and requirements, never supplied values.

Omitted optional fields keep existing defaults. Explicit null is not omission for booleans, integers or arrays. Preserve the existing description-null behavior (absent description on creation, explicit clearing on update) and make that exception explicit in discovery. No truthiness conversion or partially filtered tag/argument array is accepted. Read-only tools and unknown-tool JSON-RPC errors remain unchanged; malformed mutation arguments use the existing tool result isError envelope.

Build the run wait deadline with checked Instant arithmetic before writable Store opening, enqueue or wake. Validate a supplied wait duration even when wait=false or dry_run=true. Keep the positive integer/default-30 contract and use platform representability rather than a new arbitrary product cap. The original deadline includes request preparation; expiration stops observation, never cancels the durable run. Update reads its current definition through a read-only Store and opens a writable Store only after metadata/definition validation; optimistic revision admission remains authoritative.

## Change order and Verify

1. Add the schema-backed private admission helper and checked wait-deadline helper. Wire every mutation entrypoint and move the update write-open after validation. **Verify:** all seven named mutation tools refuse malformed optional values and whole non-object arguments before Store access; allowed description null and omitted defaults remain valid. New errors do not reflect token-like canaries.
2. Add real newline-delimited MCP subprocess regressions using the existing test client and disposable state. **Verify:** malformed dry_run/wait/acknowledgement/enabled, metadata/string arrays, retry/wait integers and overflow leave absent state absent and existing DB/WAL/file bytes unchanged. Valid true previews and false/live calls retain expected behavior; wait expiry leaves a queued durable run rather than cancelling it.
3. Format and run focused Rust helper plus MCP tests in an isolated hosted runner. **Verify:** locked compilation, nonzero test selection, fmt/diff checks, unchanged unrelated blobs and exact candidate publication. Keep any unavailable native/independent review gate explicit in the Draft; no merge, release or issue closure.

## Publication and composition

Use a unique preparation branch to apply exact, single-match replacements to the pinned full source rather than reconstructing a large file from snippets. Its temporary workflow must not enter the final feature tree. Preserve #155 doctor and #159 active-run changes as independent pending integration; this patch does not touch those regions. #161 cancellation decision/preview is separate follow-up work.

No user state, daemon, token, installed binary, package registry or operating-system security setting is touched. The current chat has no separate development/review sub-session; the hosted runner is execution/verification, not an independent code reviewer. Keep Draft for independent review and exact-head platform qualification.
