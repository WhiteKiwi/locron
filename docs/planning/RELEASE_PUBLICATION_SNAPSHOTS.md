# Stable publication snapshots

Refs #32 and #35. Base main 46445881e320f440c02d94d7e9e5838033933fea. This implements the existing final-byte/checksum and immutable-publication contract; it changes no release names, versions, channel ownership or signing policy.

## Source-established gap

release-assets.py validates paths and returns hashes, then runs gh api and later passes the original mutable paths to gh release create. A source artifact, checksum file, installer or notes file can change during that interval, so uploaded bytes need not be the validated ones. The Windows inventory path also discards inspect_archive's same-snapshot digest and hashes a second path read. The checksum document is parsed and hashed in separate reads.

## Approach and Verify

1. Before any network subprocess, copy the exact public inventory and active installers into a task-owned temporary directory. Copy using bounded chunks, a regular-file-only retained descriptor and an initial-size ceiling; reject symlinks, special files, changed native identity/size/timestamps or source-name substitution. Never change source bytes/permissions. Snapshot optional notes when present, keeping existing-release no-op behavior when notes are absent. Verify a changed source after admission cannot change any uploaded artifact, installer, checksum or notes; copying failure cleans only the owned temporary tree and performs no gh operation.
2. Validate and upload exclusively from that retained snapshot. Preserve the exact public basenames and existing-release digest/no-write check. Bind Windows validation to inspect_archive's returned digest, and parse/hash one bounded checksum document read. Verify both architectures, legacy version inventories, wrong digests and same-version remote drift refusal; publication errors and no-op completion release the snapshot without changing originals.
3. Add hermetic tests and a small path-filtered Linux Python workflow. Mock only gh, not copying/hashing/ZIP inspection. Verify source mutation at the actual API-to-upload boundary, failed copy/validation/upload cleanup, input path safety, exact uploaded bytes/names and no live network/install/release writes. Record local and hosted results separately.

The snapshot isolates this process's publication from changes to caller input paths. It is not a sandbox against malicious same-account code, a kernel-I/O cancellation guarantee or a replacement for signing/canonical source provenance. No fixed new package-size policy is introduced: copying cannot exceed each opened file's initial length, while existing ZIP and checksum limits still apply. A failed remote create may have created a partial GitHub Release; this change propagates failure and never deletes/replaces a remote release or claims rollback.

No real tag, release, registry upload, catalog submission, service mutation, merge or issue closure. A separate development/review session is unavailable in this chat; retain Draft and require independent maintainer review plus exact-head CI.
