# Indexed WinGet pair exclusion

Refs #35 and the accepted package-source exclusive proof gate. This follows
#126's canonical five-source/index connection at dc6fc28c626c64f779f80d1bf674cead7138c048.
It is separate from #124's stricter standalone replacement boundary.
The SPEC and selected ACL policy are unchanged; no package mutation is authorized.

## Approach

1. Consume the original indexed-source observation and its unchanged deadline.
   Preserve its registration/index, normalized ancestry and all three document
   read guards while capturing both source image identities/lengths/digests.
   Verify: only the two executable read handles are released; no serialized
   record or pathname alone constructs the returned proof.
2. Reacquire both fixed images through the existing package-source exclusive
   primitive, not the standalone-private primitive. Compare the actual complete
   IDs, paths, lengths and bounded streaming hashes after the read-to-exclusive
   gap. Verify: a blocked second image releases the first without writes, and
   either same-byte replacement at an unrecorded native ID is refused unchanged.
3. Revalidate the retained actual registration/index before returning. Preserve
   the original deadline; a later observation cannot revive an expired owner.
   Verify: native disposable fixtures retain both exclusive handles and document
   protection, reject readonly/multiple-link and expired admissions, preserve
   unrelated files, and release all acquired handles on refusal.

Keep the complete result opaque. Do not expose write/delete handles, infer task
quiescence, terminate mapped holders, create files or enable maintenance dispatch.
The later package manager needs its own mutation admission after this proof is
released; this transient lock is not authorization to overwrite a package.

## Verification boundaries

Native gate fixtures use actual disposable private files, a subset of the accepted
package-source ACL policy. They do not fabricate current-user registration or
claim an actual installed WinGet package, mapped-process acceptance, native
version/ABI execution or broad-ACL coverage beyond the reused core primitive.
The full composition retains #126's real registry reader. Native x64/ARM64/MSRV
CI and independent review remain required. The finite outer native worker must
retain ownership on uncertain I/O; synchronous clock checks are not cancellation.
No public CLI, release, catalog, PATH or task effect is added. No independent
sub-session result is claimed when none was available.
