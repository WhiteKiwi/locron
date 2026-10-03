# Windows ZIP catalog admission parity

Related execution issues: #32 and #35. Prerequisite: #121, based on #119.

## Contract and scope

The frozen Windows release contract already requires exact inventories, bounded
bytes, matching raw local/central records and no ambiguous payload data. The
Rust implementation of that policy is `windows_package.rs::catalog` at main
b6b98ebcb262117c70bab54dcb4ebcf491b59bc0. The Python release/WinGet inspection
path at #121 checks the library's normalized catalog but not all raw records.
This follow-up applies the existing admission policy before Python decompression;
it does not change SPEC, the four/five-member layouts, executable probes, native
installation authority, release publication or catalog submission.

## Implementation order and verification

1. Add one dependency-free raw ZIP structure validator matching the selected
   Rust catalog policy: final uncommented single-disk EOCD, exact member count,
   exact raw names, permitted flags/methods/attributes, matching local metadata,
   and contiguous nonoverlapping local-record coverage before the central table.
   Retain the 64 MiB archive/aggregate expanded bounds. **Verify:** canonical
   stored/deflated x64 and ARM64 archives pass; every independently changed
   record field, prefix/gap/trailer, duplicate, metadata and size-bound case
   refuses before decompression or any executable probe.
2. Call it from the common Python snapshot inspector only after final checksum
   comparison. Read and CRC-check every listed payload, including documentation
   in the legacy four-file mode, using the same bounded snapshot. **Verify:**
   checksum mismatch still precedes parsing, corrupt non-executable members
   refuse, existing valid result dictionaries and package output bytes remain
   unchanged, and native paired validation still invokes all required probes.
3. Extend the existing path-filtered offline workflow to run focused raw-record
   regressions alongside the paired-manifest suite. **Verify:** local Python
   regression suites, AST/YAML parsing and scoped diff checks pass; read back the
   published head and the new hosted job. Native x64/ARM64 package jobs remain
   separate acceptance gates, not inferred from synthetic PE fixtures.

## Boundaries and review

No network, extraction, registry, PATH, task, installed-file or publication effect
is added by the parser. Allowed member names are supplied by the existing fixed
version/architecture inventory. Test fixtures use synthetic images and temporary
files only. New-format support requires reviewing both implementations rather
than loosening one side silently. This session does not claim a separate
independent development/review session; independent review remains required.
