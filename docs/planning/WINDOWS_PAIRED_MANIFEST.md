# Paired WinGet generation follow-up

Continuation of IMPLEMENTATION's Windows distribution work and issue #35, based on Draft #119. The frozen SPEC, five-file ZIP layout, existing package identifier, canonical asset names, and console-only alias are unchanged. This document records the implementation refinement before source changes.

## Boundary

A single manifest-generation host must inspect both native architectures without executing a foreign-architecture image. Separate the existing static ZIP/PE/import checks from native version/identity probes. Keep `validate --paired` and paired package production requiring their native probes. Manifest generation is static evidence only; it does not attest runtime version/ABI or installation.

The optional `--paired` renderer accepts exactly the existing five-file inventory, while its default remains the historical four-file path. Verify the final release checksum inventory and selected archive digest before inspection. Preserve the existing sole nested `locron.exe` alias: WinGet owns the extracted version directory, so neither a launcher command alias nor additional portable file rows are added. Do not emit the unsupported `Scope` field, lifecycle hooks, or `RequireExplicitUpgrade`.

## Change order and verification

1. Split static inspection from native probing without changing existing validation and package outputs. **Verify:** static inspection never calls an executor; native paired validation still runs both version probes and the launcher identity probe.
2. Add explicit paired manifest rendering using the static API and bounded checksum input. **Verify:** x64 and ARM64 manifests have canonical URLs/final hashes, one console alias each, and no launcher alias; default legacy generation remains available. Reject missing/extra files, wrong subsystem/architecture and final-digest mismatches before creating output.
3. Add an offline fixture suite and focused Ubuntu CI gate. **Verify:** execute the suite, inspect the full diff, and record exact revision results in the Draft PR. Existing native package CI remains authoritative for actual executable probes.

No tag, release asset, external catalog PR, package installation, registry/PATH mutation, or service action belongs to this change. Issue #35 remains open for native selected-client lifecycle, final public bytes, official validation and provider acceptance.

Source contracts: issue #35's accepted directory/symlink ownership refinement, `scripts/windows_release.py` from #119, and the existing `scripts/render-winget-manifest.py`. No new product behavior or dependency is selected. Separate development/review and native execution evidence must not be claimed unless actually performed.
