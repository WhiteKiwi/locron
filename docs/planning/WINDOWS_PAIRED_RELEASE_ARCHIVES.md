# Publish the paired Windows executable archive

Refs #32 and #33. Windows CI already builds and probes the fixed console/GUI executable pair, while the tag release workflow still emits the historical console-only ZIP. This plan aligns the actual v0.10+ release artifact with the frozen two-executable Windows product contract without changing public asset names or signing policy.

## Scope

For Windows feature releases, build both `locron.exe` and the internal `locron-service-launcher.exe` with the existing static-CRT/native-target policy. Package the exact five-file ZIP through the existing paired archive path and run its native version/launcher identity probes on the architecture-specific Windows runner before upload. The public ZIP filename, target matrix, checksums, release asset count, unsigned policy and non-Windows release paths remain unchanged.

The Ubuntu publication boundary cannot execute Windows images. It must statically re-inspect the downloaded final paired ZIP bytes using the same exact inventory/PE/import parser and reject the old four-file layout. Native runtime evidence stays owned by the Windows build leg.

## Implementation and Verify

1. Change the Windows release build to compile both bins with `windows-service-launcher` enabled and package with `--launcher`. **Verify:** x64 and ARM64 tag-build steps require both native binaries; the resulting ZIP contains exactly console, GUI launcher, README and both licenses and passes the existing paired version/identity probes under system-only PATH.
2. Change release publication inventory validation from the historical single-image archive check to static paired inspection. **Verify:** a valid paired x64/ARM64 artifact set is accepted on a non-Windows host, while either legacy four-file ZIP, wrong subsystem/architecture, extra/missing member or malformed paired ZIP refuses before publication.
3. Extend hermetic distribution fixtures to exercise the actual paired publication boundary and preserve the exact ten public archive/package names for Windows releases. **Verify:** existing legacy pre-v0.10 inventory behavior remains unchanged; v0.10+ retains two ZIP asset names rather than adding launcher assets, and no test invokes a real Windows executable.

## Boundaries

This does not create a tag or release, submit WinGet manifests, sign Windows code, claim clean-machine support, or activate installer/update/service effects. The initial Windows release remains explicitly unsigned. Publication still requires the ordinary release workflow and its independent gates; this change only makes its Windows ZIP bytes match the already-defined paired product artifact.
