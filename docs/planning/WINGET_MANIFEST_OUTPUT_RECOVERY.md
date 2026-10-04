# Recoverable WinGet manifest output

Refs #35. Base: main `46445881e320f440c02d94d7e9e5838033933fea`. Independent of #137's CLI paired default and #138's release upload snapshots.

## Existing behavior and selected correction

The renderer validates its inputs, creates the requested output directory, then writes three YAML files sequentially with overwrite mode. An encoding/write/close failure can leave a partial directory that prevents a straightforward retry. A file appearing between directory creation and a later write can also be truncated by that write. This change makes ordinary failed generation recoverable without changing manifest schema, names, contents, paired/legacy selection or structural-validator behavior.

## Implementation and Verify handoff

1. Encode all document contents before creating output, reject non-leaf output names, and create each file exclusively. Verify successful output remains identical UTF-8/LF bytes; encoding failure has no output effect; an existing output directory or concurrently appearing file is never overwritten.
2. Record the identities of the newly created directory and files. On generation exceptions, best-effort remove only still-matching created files, then remove the directory only if still matching and empty. Never recurse, delete pre-existing entries, repair permissions or remove created parent directories. Verify second/third-file open/write/close failure permits retry when cleanup succeeds; substituted files, a replaced directory and unrelated entries are retained. Cleanup problems annotate the original exception when supported, rather than claim success or hide the original failure.
3. Route generation through the new writer while preserving post-generation `winget validate`. Verify structural-validator failure still leaves all three complete manifests for review and does not print success; #137's default/explicit paired entrypoint behavior composes unchanged. Run the existing manifest/entrypoint suites plus Windows file-I/O controls before merge.

The owner requested implementation-first Drafts. Repository regression-test additions and native validation remain explicit PR/issue handoff items; small local writer smoke checks may be recorded separately. This is best-effort recovery from ordinary exceptions, not atomic visibility, crash recovery, durability after power loss, or a security sandbox against hostile same-user path races. Ambiguous cleanup leaves output for manual inspection. No new dependency, package installation, catalog submission, release, merge or issue closure. Independent development/review sessions are unavailable here; maintainer review remains required.


## Maintainer review and control handoff (2026-10-04)

The source diagnostic gap for a renamed-away directory and the missing Verify controls are selected in WINGET_OUTPUT_RECOVERY_CONTROLS_2026-10-04.md. That plan supersedes the earlier unexecuted verification proposal using available exact Source and Issue35, without claiming to read missing Docs9038. Normal reviewed main814 composition preserves the paired CLI default. Separate developer owns the three-file correction/controls/workflow; parent reviews/publishes and requires hosted32/38 control evidence, unchanged regressions and all required exact-head gates before merge. Broader WinGet publication/installation acceptance stays open.
