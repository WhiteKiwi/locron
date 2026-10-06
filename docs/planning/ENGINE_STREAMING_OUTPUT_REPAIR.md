# Streaming engine output repair

Base: main `46445881e320f440c02d94d7e9e5838033933fea`. Continue the existing SPEC/IMPLEMENTATION captured-output and recovery contract, with STORAGE.md's versioned frame format, payload bound and stale/terminal-attempt eligibility unchanged. This is independent of the Store retained-file repair correction and active dashboard/release Drafts.

## Source-established gaps

Engine repair calls the same vector-producing parser as read_frames, retaining every decoded payload merely to sum its bytes. Memory grows with the entire log although repair only needs counters and a valid-prefix offset. The payload read uses `is_err()` and treats every I/O error as an incomplete tail, allowing the later truncation even when the read failed for another reason. Header/magic/seek errors already have stricter propagation; preserve that behavior.

## Implementation and Verify

1. Factor a private generic Read + Seek scanner with a per-frame callback, valid-prefix offset and payload counters. Keep read_valid_frames/read_frames collecting exactly their old Vec<Frame> results; repair discards each validated frame immediately and retains at most one bounded payload. **Verify:** byte-for-byte frame/result compatibility for empty/one/many-frame files and all three channels; large multi-frame repair has memory proportional to MAX_FRAME_LEN rather than file length.
2. Classify payload errors explicitly: only UnexpectedEof ends an incomplete tail; every other read error propagates without reaching truncation. Keep bad magic/channel refusal and existing over-limit/CRC/sequence tail handling unchanged. **Verify:** an injected non-EOF payload error after valid frames reaches the caller, does not become a successful prefix result and never authorizes a repair write; ordinary corrupt/incomplete tails retain the same prefix as before.
3. Repair and synchronize through the existing single guarded read/write file, checking its observed length for drift before set_len. Keep public stats, framing bytes, limits, writer/finalization code and output-path APIs unchanged. **Verify:** observed grow/shrink refuses without a repair write; normal repaired counts agree with decoded bytes and no run/attempt state or lifecycle decision changes. Run Engine output tests and cross-platform recovery consumers before merge.

This is a bounded-memory scanner, not an elapsed-time bound, an atomic snapshot against concurrent writers, or a new output-retention policy. Caller lifetime/quiescence requirements remain mandatory. read_frames deliberately still returns an in-memory vector. A filesystem error remains visible and no unknown execution outcome is relabeled.

The requested deliverable is an implementation Draft with Verify handoff, not a testing-only PR. No Rust compiler or independent development/review sub-session is available here; compilation, regression execution and native qualification remain explicit pending gates. No user logs, live jobs, installations, releases, merges or issue states are changed by this work.
