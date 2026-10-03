# Canonical indexed Windows source integration

Refs #35. Based on PR #120 at 91da51a46484f2783074dfd08810a3eda1d53fed.
This implements the accepted SPEC Windows ownership/integrity requirement without
changing product scope. The source remains under the existing Windows test-only
distribution boundary; no package, task, alias or registry mutation is exposed.

## Missing connection

The registered-index reader retains actual HKCU64 registration and its index,
but does not qualify the five package files. The paired archive verifier knows
canonical bytes but owns no local objects. Bind these two existing facts without
confusing a registry row, pathname, receipt assertion or static PE header with a
complete installed/activated package.

## Implementation order and verification

1. Qualify the selected Release/checksum/archive entirely in memory, including the
   final checksum-file API digest, exact complete checksum inventory, target and
   five-member paired PE contract. Verify: malformed or changed canonical bytes
   refuse before opening any installation or querying native registration.
2. Retain the registered index and all five fixed source leaves under the existing
   package-source read policy. Bound each read by its exact canonical length and
   the existing 64 MiB inventory ceiling, compare actual SHA-256, retain complete
   native IDs and refuse aliases between listed names. Verify: all five changed,
   missing, locked or reparse leaves refuse; accepted guards prevent writes and
   renames; a locally plausible but noncanonical companion is not accepted.
3. Revalidate the guarded source and index under the original absolute deadline.
   Never extend that clock during a later revalidation call. Verify: source
   identities, lengths, paths and digests remain exact, missing roots remain
   absent, and expiry refuses rather than reviving a proof. Keep checks on the
   actual production composition, alongside disposable source-file fixtures.

Use a separate child module beneath windows_payloads to avoid modifying #120's
reviewed registry/index implementation. Preserve all additive declarations from
#122/#124/#125 during later integration. Read guards overlap across validation;
no repair, creation, deletion or release of an admitted source is required.

## Explicit limits

The alias argument remains only the selected index's row expectation. This work
does not authenticate the actual symlink, execute native version/ABI probes,
authorize a maintenance helper, or establish task quiescence. It is deliberately
named indexed source evidence rather than complete IndexedPair authority.
Native positive registration tests require an actual selected-client installation;
disposable source fixtures are not passed off as that evidence. Native compile,
lint, x64/ARM64 fixtures and independent review remain merge gates. Synchronous
filesystem calls remain owned by their outer finite phase; clock checks cannot
cancel a stalled kernel call. No separate sub-session result is claimed here.
