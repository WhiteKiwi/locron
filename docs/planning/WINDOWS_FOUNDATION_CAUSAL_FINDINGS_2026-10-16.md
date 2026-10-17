# Causal Windows foundation findings, 2026-10-16

Reviewed source c231dceee2bf52b2fe6d7126552efeea16115600 and completed ordinary
[run38860620196](https://github.com/WhiteKiwi/locron/actions/runs/38860620196).
Jobs x64 MSRV116841908205, ARM116841908273, x64 stable116841908586 all fail their
library/PR144/explicit-prune gates. Core137 and all8 original CLI selectors succeed
in every row; Windows native warnings-denied lint and all14 non-foundation jobs pass.

## Observed cause boundaries

Both x64 early native-toolchain PowerShell tokens have Owner=Administrators,
Owner!=User, no impersonation. Bash producer observation has Owner=User; actual
selected namespace/controller/Store objects have User owner, protected-DACL=false.
ARM query helpers time out at5s, cause UNKNOWN; do not call a timeout a token class.
Fresh root and second producer artifact owner classes were not directly measured.
The actual guarded owned-owner/private-parent refusals retain authority.

Microsoft defines default creation owner through the effective token's TokenOwner,
which can differ from TokenUser. GitHub explicitly uses Git for Windows for a
Windows shell:bash. Git for Windows runtime source at
81d9bd3d1c3680aa611ff2e5d1950787ffc95c6a opens its CURRENT process token, reads
TokenUser and sets TokenOwner to it; ordinary native children use CreateProcessW.
The same initialization also changes default/process DACL and attempts to enable
available backup/restore/debug privileges. Existing Bash is a standard CI shell
context, not a pure owner-only wrapper. Source revision is not bound to the runner's
installed DLL. Actual unchanged guards on all3 rows qualify the selected correction.

- [Microsoft default owner](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object)
- [GitHub shell contract](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepsshell)
- [MSYS owner and DACL initialization](https://github.com/git-for-windows/msys2-runtime/blob/81d9bd3d1c3680aa611ff2e5d1950787ffc95c6a/winsup/cygwin/uinfo.cc#L72)
- [MSYS privilege initialization](https://github.com/git-for-windows/msys2-runtime/blob/81d9bd3d1c3680aa611ff2e5d1950787ffc95c6a/winsup/cygwin/sec/helper.cc#L528)

All6 observed API N08 executions report HTTP200, ok=true, no error/code, absent
readonly leaf, both selected rows started+pruned. This confirms successful removal
and completion conflicting with the original native refusal oracle. Rust1.94/1.98
DeleteFileW ACCESS_DENIED fallback ignores readonly. Framework File.Delete directly
uses DeleteFile and accepts missing-file absence. Mapping only exact known managed
I/O/access exception types plus consistent Win32-facility HResult preserves real
numeric refusals without inventing LastError or attributing unrelated managed codes.
No new EL07 creator failure was observed in this run; no migration repair is selected.

- [Rust1.94 Windows removal](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs)
- [DeleteFileW readonly refusal](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew)
- [Framework File.Delete](https://github.com/microsoft/referencesource/blob/ec9fa9ae770d522a5b5f0607898044b7478574a3/mscorlib/system/io/file.cs)
- [Framework actual error mapping](https://github.com/microsoft/referencesource/blob/ec9fa9ae770d522a5b5f0607898044b7478574a3/mscorlib/system/io/__error.cs)

## Fresh private creation and no-clobber decision

Separate research checked tempfile3.27, file-id0.2.3 and current Core. TempDir is a
genuine exclusive mkdir receipt, but Windows inherited owner/DACL do not guarantee
Core's protected User/SYSTEM-only descriptor. DirectoryGuard::private creates or
verifies; Framework DirectoryInfo.Create(acl) does not refuse an existing directory.
It cannot replace the exclusive receipt. DACL-only restrict_owned is permitted for
fresh disposable setup before private admission, but requires an initially private
namespace; an ordinary trusted ancestor can have inherit-only foreign mutation ACEs.

An EMPTY exclusive staging directory under the already private evidence guard meets
that premise. Move it outside evidence before any bytes. Rust rename is unsuitable:
REPLACE_EXISTING and POSIX fallback can replace an existing empty destination.
Framework DirectoryInfo.MoveTo uses no-replace MoveFile. Retained original physical
handle plus full128 ID at private destination prevents ID-reuse substitution, while
both parents remain guarded. This is path-based mutation with a subsequent same-object
observation, not an atomic handle rename. Collision controls must avoid no-DELETE
target pins that would hide clobbering behavior. Incomplete/late postbinding retains
one owned quarantine and authorizes neither bytes nor saved-path cleanup.

file-id high-resolution identity is path-based, safe only while its source/destination
guard pins the entire no-reparse chain. No from-handle full-ID API is claimed. Existing
dependency suffices; no new unsafe Rust, FFI, token setter or dependency is selected.

- [Rust Windows rename contract](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/fs.rs#L2672)
- [Framework DirectoryInfo.MoveTo](https://github.com/microsoft/referencesource/blob/ec9fa9ae770d522a5b5f0607898044b7478574a3/mscorlib/system/io/directoryinfo.cs)
- [MoveFileW destination absence](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefilew)
- [CreateFileW sharing semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)

The [selected complete plan](WINDOWS_FOUNDATION_CAUSAL_REPAIR_2026-10-16.md)
owns implementation/Verify. Native qualification of the new selectors, preparation,
collision/ownership/late-result controls and original rows is still pending.
