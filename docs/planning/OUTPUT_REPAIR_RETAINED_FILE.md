# Retained-file output repair

Base: main `46445881e320f440c02d94d7e9e5838033933fea`. This is an implementation correction under the frozen SPEC's captured-output/recovery guarantees and STORAGE.md's output file protocol, not a wire-format or retention-policy change. IMPLEMENTATION's existing output-recovery work and Issues-only workflow remain authoritative.

## Source-established failure paths

The Store output repair routine reads the original length, scans frames, and truncates through three separate pathname opens. Its `while let Ok(Some(frame))` exits on every error, then truncates anyway. A non-tail read failure can therefore authorize truncation without proving that the remaining bytes are damaged. Separate scan/truncate opens can also target different filesystem objects after path substitution.

## Implementation and Verify

1. Add a private FrameReader constructor that consumes an already guarded file. Keep the public read-only open path unchanged; repair opens one guarded read/write file and keeps that same file and directory guards through header validation, scanning, truncation and sync. **Verify:** substitute or unlink/recreate the pathname between scan and truncation; the replacement is never opened or truncated. Native Windows sharing/ACL/reparse behavior must remain enforced by the existing filesystem adapter.
2. Match scan outcomes explicitly. Keep the existing complete-frame and recognized invalid/incomplete-tail repair policy; propagate every other I/O error before any set_len or sync call. **Verify:** inject PermissionDenied/Other read failures after at least one valid frame and require the original error with no truncation; incomplete header/payload and bad CRC/sequence retain only the last complete valid prefix; invalid magic remains non-mutating refusal.
3. Use metadata from the retained handle for the original size and refuse observed size drift or a valid prefix beyond that original size before truncation. **Verify:** grow/shrink during inspection and require refusal without a repair write; normal successful repairs preserve counts/bytes and remove exactly the measured tail. Run Store output tests and native maintenance recovery gates before merge.

The size observation is an additional consistency check, not an atomic lock against an active or hostile same-account writer. The caller must still satisfy STORAGE.md's terminal/stale-lifetime eligibility and exclusion of live attempts. No advisory-lock, ownership, selection, repair-on-permission-error, pathname cleanup or new retry policy is added. A sync failure after truncation remains an error, not rollback or a durability claim.

This is a code Draft with regression/native verification handed off in the PR and issue. No Rust compiler or separate development/review sub-session is available in this chat runtime; do not claim either compilation or independent review. No user output is repaired here, and no merge, release or issue closure is authorized by this slice.
