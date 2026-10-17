# Native Windows CI ownership research

Research scope: the user's request to repair all current Windows foundation failures.
Base inspected: f8d77077967b9aab1a87b0397a84dfd0204bbf54. This document proposes
read-only measurements; it does not authorize token, artifact, product or workflow
changes. Parent review and owning issue updates precede implementation.

## Established facts and open cause

PR144 has reported the typed OwnedOwnerMismatch predicate for an actual CargoJSON
artifact, and separate runs have refused the initial tempfile-created parent. The
exact owner class behind those native failures is not measured. An administrator
token whose TokenOwner differs from TokenUser is a plausible shared creation cause,
not a demonstrated host fact.

Source confirms two different policies. Core's private descriptor and executable
checks require the current account's TokenUser as owner. Explicit private file and
directory creators set that owner and their protected current-user/SYSTEM DACL.
The guarded exclusive empty-file creator sets/readbacks the descriptor on that
exact CreateNew handle before caller bytes. In contrast, PR144 fresh_root first
uses tempfile::tempdir_in, keeps its result and then validates it; the CI producer
uses ordinary mkdir and Cargo/compiler file creation. Those default creations do
not explicitly request the Core owner descriptor.

The existing compiler-artifact selection binds package, kind, profile, features,
absolute path and content hash. Its guarded consumer independently admits the
original file before canonicalization and checks the hash and physical identity.
These are required boundaries; diagnostic facts must not replace any admission,
rename/copy/rebind an executable or reinterpret administrator ownership as user
ownership.

## Primary Windows contracts

Microsoft documents that the [owner of a newly created object](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object)
comes from the creating effective token's default owner unless an explicit security
descriptor supplies one. [TOKEN_OWNER](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_owner)
may name an eligible user/group already in the token, so it need not equal TokenUser.
The managed [WindowsIdentity.Owner](https://learn.microsoft.com/en-us/dotnet/api/system.security.principal.windowsidentity.owner)
exposes that default owner; [User](https://learn.microsoft.com/en-us/dotnet/api/system.security.principal.windowsidentity.user)
exposes the account. [ObjectSecurity.GetOwner](https://learn.microsoft.com/en-us/dotnet/api/system.security.accesscontrol.objectsecurity.getowner)
can return the descriptor's SecurityIdentifier without resolving account names.
Compare those objects internally; never serialize their strings or ACL contents.

[SetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-settokeninformation)
supports changing default-owner information with appropriate token access;
[token access rights](https://learn.microsoft.com/en-us/windows/win32/secauthz/access-rights-for-access-token-objects)
distinguish TOKEN_QUERY from TOKEN_ADJUST_DEFAULT. This is separate from modifying
groups or privileges. A [duplicated token](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-duplicatetokenex)
can isolate a later experiment, but [CreateProcessAsUser](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw)
may require or temporarily enable privileges. It is not an automatically permitted
launch substitute under the no-privilege-change boundary. Ordinary CreateProcess
also does not adopt an impersonating thread's token as the child's primary token;
thread impersonation alone is not a supported compiler-owner repair.

## Minimal read-only observation proposal

1. Extend the existing native-toolchain reporting step with one effective-token
   observation before Core/PR144 work. Record only owner-equals-user, owner-is-System,
   owner-is-BuiltinAdministrators and a closed impersonation class. **Verify:** no
   identity/path/ACL text or token mutation appears in output; unavailable queries
   explicitly remain unobserved. No new build or duplicate test is needed.
2. In the existing producer, after the original CargoJSON selection and hash reads,
   inspect the already-created namespace and the selected controller/Store files.
   Record file-owner-present, owner-equals-user, owner-equals-observed-token-owner,
   owner-is-System, owner-is-BuiltinAdministrators and DACL-protected booleans.
   **Verify:** the original selected paths, hashes and facts remain unchanged; no
   copy, ownership repair, extra build or broad filesystem search occurs. Observe
   the actual fresh tempfile parent separately at its existing construction point
   if its exact owner remains needed; namespace similarity is not that measurement.
3. Review native rows before selecting a repair. **Verify:** distinguish an
   administrator default owner from another owner, descriptor refusal, reparse or
   unreadable metadata; retain all original failed receipts and guarded oracles.
   A diagnostic Get-Acl path read is not a retained-handle identity proof and cannot
   authorize the file. Existing guarded admission remains authoritative.

Only fixed schema/role/status/error/impersonation classes, booleans and nulls may be
logged. Actual request paths travel privately over stdin, not displayed argv/source.
Never relay a caught exception or child's raw stdout/stderr: accept only the bounded
closed JSON result. Proposed limits are at most16 KiB UTF-8 input, four object roles,
8 KiB output and one five-second diagnostic-child cap with kill/wait on expiry.
These govern independent observation, not a replacement or extension of an existing
Store/SID/PR144 deadline. A refused observation remains unobserved and never turns a
required test failure into success. The early token query can run inline in its
existing PowerShell process; producer file observations should be one small batch.

The scratch PowerShell proposal uses query-only WindowsIdentity and Get-Acl. It
emits only the fixed fields above, disposes its identity handle and catches errors
into fixed classes. Its producer observer token is labelled separately: a later
observer cannot directly prove a historical rustc child's token. Native PowerShell
execution, exception redaction and effective/primary-token equivalence remain
unverified. The proposal has not been installed in CI or run against Windows.

## Conditional repair boundaries

If native facts confirm TokenOwner differs from TokenUser and the actual artifact
owner equals that default owner, creating compiler outputs with the same TokenUser
as default owner is consistent with the existing executable policy in principle.
It changes newly created objects rather than taking ownership of an existing file.
Select a concrete launch recipe only after proving an independently owned child
token, unchanged parent token, same TokenUser/groups/privileges/default DACL and no
implicit privilege enabling. Read back only TokenOwner=TokenUser before creation.
If these properties or access rights cannot be established, keep the refusal;
do not alter the runner/job token, account, credentials or group membership.

A compiler-only adjustment cannot fix a tempfile parent later created by an ordinary
test process. That case needs its own confirmed measurement and an exclusive private
creation recipe, or a separately isolated test-child token with equivalent proofs.
Do not chmod/SetOwner an already refused root, adopt an existing random path or widen
Core's strict owner/private-DACL checks. No repair is selected by this research.

Required native validation after any later reviewed change: x64 stable/MSRV and
ARM64 run original Core ownership controls, exact CargoJSON/physical/hash artifact
binding, all PR144 keys and root/file cleanup oracles, both original library passes,
and every other required foundation gate. Verify parent/security-state conservation,
foreign-file refusal and no raw SID/path/ACL leakage. Metadata observations alone
cannot qualify those runtime contracts or advertise Windows acceptance.

## Reproducible handoff

Scratch proposal and source hashes are in
`/workspace/scratch/locron-contention-evidence/windows-owner-research/`:
owner-observation-proposal.ps1 and proposal-receipt.json. These are local research
hints, not repository runtime dependencies. Static inspection confirms no security
setter, raw SID formatting, raw ACL output, new process creation or file mutation in
the proposal. The proposal SHA256 is
`a01e47e752ed4af7bb61af303a3a9467a431f9f3156deb3a92054c9308ea6948`.
PowerShell/native execution is explicitly pending. Only this research document is
added to the repository; shared FINDINGS and product/workflow/helpers remain
untouched by this sub-session.
