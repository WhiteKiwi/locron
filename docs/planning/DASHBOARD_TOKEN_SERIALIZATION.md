# Serialized dashboard token lifecycle

Refs #30 (token rotation/authentication and state-preserving dashboard lifecycle).
Base: main `46445881e320f440c02d94d7e9e5838033933fea`.

## Existing contract and source-established gaps

The frozen dashboard contract generates one private token on first use, reuses it until explicit reset, and removes it on disable. In token.rs, concurrent ensure calls can both observe NotFound and each atomically rename a different generated token over the destination. Atomic replacement alone does not serialize the read/decide/write operation. The current 4096-byte read also accepts a valid prefix without checking whether the file continues, and write/sync failures before rename leave the created temporary file behind.

## Approach and Verify handoff

1. Serialize ensure, regenerate and remove through a permanent, empty private `dashboard.token.lock` file using the existing guarded read/write open-or-create primitive and std File::try_lock. Acquire before reading or deciding to create. Retry only WouldBlock under one five-second acquisition clock; propagate all other errors and check expiry again after acquisition. Never unlink the lock file, including on disable, because unlinking permits different lock identities. Missing-root remove remains a no-op without creating state. **Verify:** simultaneous first-use callers that succeed return the same persisted token; later ensure reuses it; reset/remove/ensure do not overlap their read/write windows. Contention expires without replacing the token, other lock errors are not retried, and a hard-stopped owner releases admission. Run real process/Windows tests before merge.
2. Read at most 4097 bytes and reject above the existing 4096-byte allowance before token validation. Preserve the existing trim and 64-hex token contract. **Verify:** a valid token followed by enough whitespace to exceed the allowance and a valid prefix with later garbage both refuse, exactly bounded valid text is accepted, corrupt/non-UTF-8 input still refuses and error messages contain no token bytes.
3. Close and clean the attempt-created private temporary file when write_all or sync_all fails, just as the existing rename failure does. Preserve the original I/O error, old token and successful atomic-replace behavior. **Verify:** injected write/sync/rename failures leave the previous token unchanged and normally remove scratch; ambiguous cleanup remains a failure, never a success. No cleanup of unrelated paths.

Rust primary reference: https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock (stable since 1.89, before this repository's 1.94 minimum). TryLockError distinguishes WouldBlock from I/O errors; locks release when their unduplicated handle closes. The existing core filesystem API supplies validated file/parent guards; no dependency or native FFI is added.

This coordinates updated Locron callers, not old binaries or hostile same-account code bypassing the advisory lock. The acquisition deadline bounds retries, not interruptibility of synchronous kernel I/O. The permanent empty lock contains no secret; it is intentionally retained when the token is removed. A running server still uses its startup token until the existing reset/restart procedure runs.

The user requested implementation Drafts with verification handed off. Preserve existing tests; list unexecuted concurrency/failure/native checks in the PR rather than claiming completion. This environment has no independent development/review sub-session; independent maintainer review remains required. No live token, installed service, registry, task, tag, release, merge or issue closure is changed.
