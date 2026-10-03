# Registered WinGet index and paired payload composition

Execution: #35. Prerequisite: #120 at 91da51a46484f2783074dfd08810a3eda1d53fed.

## Existing contract

The accepted WinGet design separates directory/console-alias index metadata from
actual five-file package contents. A matching registration or SQLite row cannot
prove that the installed console/GUI images and documents match a release. Add
the missing guarded file composition without changing the package source policy,
index parser, v1 reader, manifest layout, public dispatch or lifecycle authority.

Consume the existing canonical Release/checksum/paired archive validators; the
supplied release metadata must come from the existing trusted Remote path in a
future caller. Verify the complete checksum inventory and API digest binding
before interpreting archive bytes. The constructor does not independently prove
the network provenance of caller-supplied Release data or Authenticode identity.

Read all five fixed payloads under existing current-SID package-source guards,
not stricter standalone ACLs or repaired descriptors. Check normalized paths,
actual full file IDs, byte lengths, every final digest and both PE subsystems;
reject duplicate admitted payload identities. Keep only one local read buffer
at a time and retain the root and all five file handles on success.

Compose that object with #120's opaque RegisteredIndex using the exact canonical
console child and expected console alias. The guarded root must have the exact
release-version/native-target directory name; RegisteredIndex independently
requires that same console to be the actual selected registration's nested child.
This binds the version-root and the live selected registration without exposing
its private fields or constructing a registration from serialized caller facts.
Revalidate retained file identities/bytes and the live registration/index before
returning. Later revalidation uses the earlier original deadline and accepts a
mutable object borrow to prevent overlapping cursor reads.

## Implementation and Verify

1. Qualify the guarded five-file inventory against the existing canonical byte
   validators. **Verify:** disposable x64/ARM64-appropriate PE files plus all
   documents retain five distinct IDs/read guards; changed/missing leaves, wrong
   version-root, corrupted release/checksum/archive inputs and hardlink aliases
   refuse without filesystem writes or unlisted-file reads.
2. Connect the opaque native RegisteredIndex. **Verify:** an actual unregistered
   temporary installation passes file qualification but fails the full native
   registration boundary; changed retained metadata and expired revalidation
   refuse. Do not fabricate a positive registry-backed result or edit host keys.
3. Qualify on the existing native lanes. **Verify:** exact-head formatting,
   compilation and selected Windows fixtures pass, with executed results recorded
   separately from review. Real selected-client installation/upgrade/removal is
   still the required positive complete WinGet acceptance gate.

## Limits retained

The expected alias remains an index-row expectation, not authentication of the
actual symlink or proof that no GUI alias exists. Identity uniqueness checks cover
five payloads, not arbitrary outside hardlinks or the private index ID. Native
image version/ABI execution, provider acceptance, source promotion and all task,
registry, PATH, helper, journal or package-mutation effects remain separate gates.
The composition inherits the Windows test-only distribution boundary. An original
absolute deadline is checked around synchronous operations; it does not cancel
stalled kernel I/O or replace the outer finite owned-worker requirement.
