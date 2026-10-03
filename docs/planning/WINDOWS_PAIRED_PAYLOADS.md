# Canonical paired payload assembly

Continuation of the accepted Windows paired distribution contracts in SPEC, IMPLEMENTATION and issues #33/#34. This extends the existing canonical release source, five-member archive verifier and strict v2 receipt rather than changing installation policy. Plan recorded before implementation.

## Goal and authority

The current canonical payload builder accepts the earlier four-file ZIP and produces a v1 six-payload receipt. The accepted pair instead needs both executable images, all five ZIP members, the canonical installer and uninstaller, and a v2 receipt containing exactly seven payload hashes. Add a separate paired builder; do not widen the legacy builder or silently migrate its transaction engine.

The new result holds verified in-memory release bytes, not filesystem ownership or runtime activation authority. Receipt metadata, even when matched against canonical bytes, never authorizes installing, deleting, replacing or starting anything. Native executable version/ABI probes, actual file guards, mapped-holder exclusion, journals, task state restoration and every-effect crash tests remain downstream gates. Do not expose a CLI command or remove the existing Windows test-only staging boundary.

## Change order and Verify

1. Reuse the canonical Release/Remote/checksum and paired archive APIs to assemble all seven payloads. Hash bootstraps and the checksum document against final API digests, and validate the complete checksum inventory before archive interpretation. **Verify:** both architecture fixtures pass; changed checksum/ZIP/bootstrap bytes, mismatched API hashes and missing native assets fail without filesystem or network effects in the pure verifier. The optional downloader uses only the existing canonical source and performs all network work before any later lifecycle action.
2. Generate the existing strict v2 receipt with ordered console/GUI bindings and complete seven-file hashes. Validate every future fixed destination path and the exact optional raw PATH restoration record before serialization; keep private immutable payload metadata. Provide a canonical-receipt comparison that revalidates the receipt and compares every release field/hash, without granting live ownership. **Verify:** round trips, Unicode paths and legitimate PATH state survive; swapped binaries, modified bootstrap/document hashes, wrong version/source/channel/ABI and ambiguous paths refuse.
3. Add focused Rust fixtures under the existing `self_update::windows_` native test boundary. **Verify:** inspect the complete diff and run the existing Windows x64/ARM64/MSRV gates on the Draft's exact head. Report unavailable local Rust/Windows execution honestly, with no passing native or independent-review claim.

No new dependencies, release version, public archive publication, WinGet catalog update, service/registry/PATH mutation or installation dispatch is included. This change does not close #33 or #34. The old v1 paths remain unchanged so another implementation can integrate the paired effect engine explicitly after its remaining safety gates.
