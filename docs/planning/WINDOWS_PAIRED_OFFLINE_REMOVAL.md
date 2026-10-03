# Retained v2 offline removal qualification

Execution: #33; reuses #27 private-file policy and the existing v2 receipt contract.
Base: main abf93b470f7e3abb3910f64cde63c550e9f30be6.

## Contract and boundary

The existing removal contract requires positive receipt ownership, offline
operation, exact helper/bootstrap verification and preservation of modified or
unverifiable nonessential payloads. The existing v1 reader does not understand
the two-executable/seven-file v2 inventory. Add a separate v2 reader without
changing the v1 reader, repair policy, public dispatch or receipt schema.

Keep the existing private directory and strict raw receipt under read guards.
Bind the actual current SID and native target, not caller-supplied equivalents.
Both fixed executable images and the retained .locron-installer.ps1 bootstrap
are mandatory exact inputs. Recheck ordered paths/hashes and console/GUI PE
subsystems before returning a removal observation. A changed/missing/unreadable
mandatory file refuses the whole observation. The offline script bootstrap is
not safe to load merely because a different optional companion passed.

For the remaining fixed documents/licenses/uninstall script, retain immutable
read guards only for unchanged receipt-listed bytes. Return Changed or
Unverifiable retention facts for the others; do not read or enumerate unlisted
files, discover arbitrary companions, follow reparse points, repair permissions,
create missing paths or touch durable state. Duplicate full file identities
among admitted receipt/payload names refuse rather than imply independent files.

No network or native executable/script execution is needed for this observation.
It does not prove canonical remote release identity, quiescence, exclusive
mapped-holder exclusion or permission to delete. The caller must retain these
guards through its later protected operation, backup and exact-object gates.
The caller's original absolute deadline is checked before/after observations;
expiration is a whole-phase error, never an Unverifiable permission. Synchronous
I/O still requires an outer owned phase and is not cancelled by a clock check.

## Implementation and Verify

1. Add the opaque v2 removal reader inside the existing Windows distribution
   staging modules. **Verify:** exact raw receipt, both images, bootstrap and
   unchanged optional files remain under immutable guards with actual full IDs;
   no request, journal, status, tasks or PATH effects occur.
2. Apply preservation/refusal rules. **Verify:** changed/missing/locked optional
   entries are retained with accurate reasons; mandatory tampering or missing
   images/bootstrap refuses; malformed receipt, invalid role PE and repeated IDs
   refuse without repairs, deletion or unrelated-file adoption.
3. Qualify native behavior. **Verify:** disposable private x64/ARM64 file fixtures
   exercise the rules, expired admission leaves absent roots absent, and exact
   head formatting/native tests pass. Record source review separately from
   unexecuted Windows acceptance; no independent-review result is invented.

The reader is not public uninstall wiring and does not close #33. Actual offline
helper handoff, both-image quiescence, precise deletion, conditional raw PATH
rollback, task cleanup and interruption recovery remain required.
