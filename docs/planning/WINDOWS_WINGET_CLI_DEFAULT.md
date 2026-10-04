# Pair-aware WinGet command entrypoint

Refs #35 and #32. Base: main 46445881e320f440c02d94d7e9e5838033933fea.

## Existing contract and defect

The frozen SPEC requires the console and internal GUI launcher together. The merged release path now builds and admits five-file paired Windows ZIPs for v0.10+. However, the command in RELEASE.md omits --paired, while main() passes argparse's false default to render(). The documented command therefore selects the historical four-file verifier and rejects a valid current release. The console-only alias and complete checksum inventory remain required.

This corrects the maintainer entrypoint rather than changing product scope. Retain the explicit internal render(..., paired=False) fixture route and its existing callers; do not expose a new legacy CLI mode. Preserve --paired as a compatibility spelling for existing automation. Both public spellings select paired inspection and never execute the ZIP images. --validate still invokes only the installed WinGet structural validator after generation.

## Change order and Verify

1. Route main() to paired inspection regardless of whether the compatibility --paired flag is present; clarify its help. Verify the default and explicit spellings pass paired=True, including --validate, and that rejected input never invokes winget validate.
2. Add entrypoint regressions and a real-archive integration regression using the existing paired fixture builder. Verify the documented no-flag command generates byte-identical manifests to --paired; both reject legacy ZIPs with no output. Preserve the explicit internal legacy fixture behavior.
3. Correct RELEASE.md's obsolete four-file inventory sentence and document the paired CLI default. Verify the public names, two architectures, sole locron alias, complete checksum requirement and unsigned-first policy are unchanged. Run Python tests and retain exact-head CI separately from local results.

## Handoff boundaries

No tag, release, package installation, catalog submission, task/registry/PATH mutation, or issue closure. Clean native WinGet lifecycle and catalog acceptance remain open. This chat environment has no separate development/review sub-session; do not claim independent review. Keep the result Draft for the maintainer's separate review and native qualification.
