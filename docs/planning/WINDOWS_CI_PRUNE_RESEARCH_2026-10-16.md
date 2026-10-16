# Windows foundation prune research

Owning execution scope: #172 and #31; related output-prune qualification remains
owned by its existing issues. Research source: f8d7707, after the independent SID
fixture and PR cancellation correction. Actual observations below are from source
536671f80b9ea4b17f4fc0362b3fa0ba60e43607, CI38850575751, Windows x64 stable
job116810653593. This document authorizes no product, fixture, workflow, assertion,
deadline, row-ledger or permission change. Parent planning/Issues review must select
each subsequent source phase. PR144 ownership research belongs to a separate session.

## Observed failure boundaries

The first three-library context reports Server73PASS/2FAIL. Its prune failure is
`windows-store-open operation=12 stage=sqlite-migrate category=store`, followed by
`actual-store-creator` and failed row EL07-api. The second context again reports
Server73PASS/2FAIL; it passes EL07-api and native N07-api, then fails N08-api with
`actual API error object missing`. The exact HTTP status, parsed `ok`, response code,
leaf absence and resulting SQL state were not emitted. A missing error object alone
does not prove HTTP200, successful deletion or completed pruning.

The separate CLI qualification driver reports `actual-nonzero-summary`; that receipt
does not identify its first failed row. Do not label that failure N08-cli without
the bounded child evidence. Both native N08 rows retain their original refusal
oracle: a real readonly managed leaf, refusal, first prune intent pending, later
candidate untouched, no success/completion, exact saved attribute restoration by
the disposable owner. The current complete public envelope and all full SQL/file
oracles remain authoritative.

Root independently compared main a3f43fc's CI37419780729, Windows x64
job112126332446: the same N08-api missing-error-object and CLI nonzero-summary
failures predate the SQLite changes. This comparison establishes ancestry, not the
unobserved response or a successful current acceptance result.

## N08 source mechanism and primary evidence

The API and CLI native fixtures save actual attributes, set exactly the readonly
bit, and verify `actual_attributes == saved_attributes | 1`. Their production
removers both call Core `remove_private_file`. Core verifies a private managed
regular leaf, separates its file and retained ancestry guard, closes the leaf
handle, then calls Rust `fs::remove_file` while retaining ancestry. Closing that
leaf handle is necessary because its sharing mode denies deletion.

Both supported Rust implementations first call DeleteFileW. On ACCESS_DENIED they
open the file with DELETE and call `posix_delete`, explicitly ignoring readonly;
successful fallback returns Ok. Thus readonly by itself does not require Rust's
current removal to fail. This is a source-supported explanation for N08, pending
the actual closed response/poststate observation.

Primary references checked on 2026-10-16:

- [Rust1.94 Windows unlink](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs)
  and [Rust1.98 Windows unlink](https://github.com/rust-lang/rust/blob/1.98.0/library/std/src/sys/fs/windows.rs):
  the same readonly-ignore fallback is present in both.
- [Microsoft DeleteFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew):
  readonly files fail with actual ERROR_ACCESS_DENIED; existing handles must permit
  delete sharing. Native success may remain pending until existing handles close.
- [.NET Framework File.Delete at ec9fa9a](https://github.com/microsoft/referencesource/blob/ec9fa9ae770d522a5b5f0607898044b7478574a3/mscorlib/system/io/file.cs):
  InternalDelete calls Win32Native.DeleteFile, accepts missing-file status and maps
  other actual native errors. It contains no Rust-style readonly-ignore fallback.
  The repository selects guarded WindowsPowerShell, so this Framework source is
  relevant; the precise hosted Framework build still needs native evidence.
- [.NET9 Windows FileSystem.DeleteFile](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.Private.CoreLib/src/System/IO/FileSystem.Windows.cs)
  corroborates direct DeleteFile, without establishing the hosted Framework build.
- [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
  documents delete-on-close sharing and deferred deletion. A DELETE_ON_CLOSE reopen
  is not selected: opening with that flag can schedule deletion before the reopened
  object's validation, and changes the point at which removal is observed.

The exact windows-permissions0.2.4 and winapi-util0.1.11 crates were downloaded and
their SHA256 checked against Cargo.lock. Neither exposes a safe DeleteFile or
file-disposition removal wrapper. Workspace unsafe Rust remains forbidden. Do not
add unsafe FFI or an unrelated filesystem dependency to bypass this boundary.

## First proposed source phase: closed observations only

Use the existing N08-api `Outcome` immediately after the actual router request and
before the unchanged `rejected` call. Emit only fixed ASCII field names and closed
values from the existing response: surface, row, numeric HTTP status, `ok` class
(true/false/missing/other), error class (object/missing/other), and error-code class
(state_error/invalid_request/durable_conflict/missing/other). An error string, raw
response/body, path, SID, SQL, headers, credential or job definition is never output.
An optional exact-success-envelope equality is a boolean observation, not an oracle
change or a substitute for the original rejection assertion.

Emit the response projection before any further I/O. Then, under the owner's same
absolute deadline, observe the selected first leaf using no-follow metadata and
project only present-file/absent/other/error plus readonly true/false/unobserved.
Read the already-owned fixture database without writing: project selected artifact
states only as finalized/prune_pending/pruned/other/missing and whether pending/
completion timestamps are present. Bound observations to the existing two N08
candidates and never print their keys, timestamps, bytes or rows. The later
candidate's state distinguishes a completed two-candidate operation from the required
first-pending/later-untouched refusal. Preserve every original assertion and its
expected envelope/category/status, SQL row bytes, file identity/contents/attributes,
cleanup and 60s operation/90s owner horizons.

For any failed owned Store creation, project only its existing safe
`StoreError::diagnostic` fields after the existing original-deadline check and
before unchanged `need(created, "actual-store-creator")`. This supplies operation,
stage, kind, elapsed/wait/transaction/commit durations, actual numeric codes, retries,
retryability, durable outcome and unknown owner without formatting the raw error.
If no structured record exists, emit a fixed unobserved category. Ordinary error
messages can contain fixture-private data and must not be printed.

Run the existing native API qualification on the exact built artifact with its
current binding and original command. This is an ordinary existing gate, not a new
manual/nightly lane. Record source, job, compiler and the closed observations before
choosing a fix. The CLI driver's private child captures can later be read through a
closed projection; printing all captures or treating summary failure as a row result
is not authorized by this research.

## Conditional strict-removal design

If the native observation confirms readonly was removed through Rust's fallback,
the candidate preserving the existing native-refusal row meaning is one fixed
delete selector in the existing guarded filesystem worker. Fixed `[IO.File]::Delete`
uses the actual native strict deletion through the stock Framework; input remains
validated data, never shell source. Do not clear attributes, widen owner/DACL policy,
add an ACL/share denial to the fixture, manufacture an OS code from metadata, or
accept success under the old refusal oracle. A metadata-only application refusal
would require an explicit policy/row-meaning decision and is not this candidate.

The selector needs a separately reviewed complete protocol and ownership design:

1. Retain all existing parent/no-follow/private leaf checks. Transfer the retained
   parent guard into the active request, then close the validated leaf before native
   removal. Do not retain a SQLite transaction, connection mutex or writer admission
   across worker/native I/O. Existing explicit/automatic prune already records and
   releases its durable intent before filesystem work.
2. Keep request/version/id/pid validation and finite queue/worker admission. A valid
   ordinary native refusal is a closed result carrying the actual Win32 error, not
   malformed protocol and not worker failure. Convert it to `io::Error` only after
   validating the frame; a healthy worker can remain warm. Distinguish a completed
   removed-or-already-absent result from refused, not-started and unknown outcomes.
   File.Delete accepts missing files, so it cannot honestly distinguish deleted from
   already absent without a separate native observation. Do not claim it does.
   Map only the actual native Win32-facility exception code; an unrelated adapter
   exception must use a fixed safe failure with no invented OS code. Neither the
   reply nor child stderr may include path-bearing Exception.Message, raw request
   data or arbitrary exception content from this new selector.
3. Preserve the established adapter/cleanup deadlines and caller's original finite
   deadline where one exists. A timeout or cleanup refusal never becomes success,
   even if native deletion completed late. Native I/O may already have taken effect;
   do not infer absence, unchanged bytes or replay safety from the returned error.
4. Current requests retain only path bytes. The caller waits its existing bounded
   cleanup interval after expiry; an unconfirmed child is quarantined, but the active
   request is subsequently dropped. A destructive selector therefore additionally
   needs request-owned ancestry retained until confirmed native/child disposal. On
   unconfirmed cleanup, explicitly transfer that guard into the parked quarantine
   owner. The synchronous caller's stack alone is insufficient. Queued requests
   never issued to the child can release guards after refusal. Late completed work
   may release them only after actual disposal is confirmed.
5. On refusal/timeout keep the SQL intent pending and stop later candidates. Do not
   finish SQL pruning or add automatic blind retries. Existing explicit recovery
   can recheck the canonical managed leaf: verified absence is idempotent and allows
   the same pending identity to complete; a remaining readonly leaf still refuses.

Required native Verify before accepting that design: actual deletion positive;
already-missing noncreation; readonly native OS5 with exact identity/bytes/attributes
and pending intent; original deny-delete sharing OS32/33 and release positive;
unchanged ACL/reparse/foreign-object refusals; bounded queue refusal before issue;
late deletion/reply with failure and no SQL completion; confirmed cleanup versus
unconfirmed quarantine retaining ancestry and refusing replacement work. Run original
API/CLI row counts and envelopes unchanged on x64 stable, x64 MSRV and native ARM64.

## EL07 is a separate initial-creator failure

`Owner::initialize` calls real Store::open before seeding EL07's interrupted_unknown
run or invoking the API. The failing `sqlite-migrate category=store` is therefore
before row behavior, output removal or N08's attribute mutation. The current Windows
trace recognizes raw Io/Sqlite and reports other StoreError variants coarsely; the
new observed wrapper can hide its already-available safe classification in this
legacy debug line. It does not show that the wrapper caused a failure.

Store::open starts its existing five-second operation before layout, writer admission,
connection/configuration and schema migrations. A fresh database can run five schema
migration transactions; later BEGIN checks the same original deadline after prior
uncancellable native work/commits. A delayed migration can consequently fail before
a later BEGIN without an external writer. This is a mechanism, not the measured
reason for this EL07 occurrence. Its actual kind/codes/durations/durable outcome
remain unknown until the closed creator record is captured. The second context's
EL07 pass also prevents calling this an unconditional eligibility bug.

Do not warm, preinitialize, retry or substitute a prebuilt DB merely to bypass the
real fresh Store creator, and do not enlarge its five-second budget. If the record
shows deadline consumption by repeated initial commits, a separate Store-owned
research phase may evaluate fewer initial migration commits while preserving the
complete fresh schema/settings/checksums/version, upgrade/crash behavior and original
deadline. If it shows actual SQLite/native refusal, investigate that actual code
instead. Neither source change is selected here.

## Research completion and next decision

Source/reference review and a closed diagnostic proposal are complete. Exact current
native response/poststate and creator details are pending; this Linux session cannot
supply Windows runtime evidence. Parent must review this research, finalize the
owning plan/Issues, then authorize only the observation phase. Only after exact-head
native observations can a causal implementation plan be selected. All ordinary
foundation failures remain required; none is waived by baseline ancestry or static
source reasoning.
