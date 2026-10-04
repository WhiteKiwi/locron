# Honor a selected XDG state directory without HOME

Base: main `15820ed96cc6da2bd8c37d42ec8788fd2180114b`; original `crates/locron-store/src/paths.rs` blob `5d33721e32e6b4bb9618dcca6011b5d8e6b7d4b5`.

## Existing contract and source finding

STORAGE.md selects `$XDG_STATE_HOME/locron` on Linux and falls back to `$HOME/.local/state/locron`. Explicit `--state-dir` and `LOCRON_STATE_DIR` remain higher priority. The implementation currently requires HOME before inspecting XDG_STATE_HOME, so a non-empty selected state directory cannot be used in a minimal environment without HOME. This corrects selection order under the existing contract; it does not introduce a new state layout or Windows behavior.

## Implementation and Verify

1. Move the HOME lookup into the branches that need it: the unchanged macOS default and the non-macOS fallback after XDG_STATE_HOME is absent or empty. **Verify:** in isolated Linux subprocesses with HOME removed and a non-empty absolute XDG_STATE_HOME, discovery selects exactly that directory's locron child and never consults a home fallback.
2. Preserve all remaining selection semantics, including OsString paths, empty-XDG fallback, explicit overrides, existing state-discovery error type/guidance, and the Windows KnownFolder adapter. **Verify:** supplied HOME and XDG prefer XDG; absent/empty XDG with HOME selects the old fallback; both absent return the same actionable state error; both explicit override forms still win. Include spaces, Unicode and Unix non-UTF-8 path bytes without changing the parent test environment.
3. Review and qualify the implementation Draft. **Verify:** the final source diff only moves HOME acquisition, all old path/identity tests remain intact, `cargo fmt --all --check`, Store path tests and the CLI discovery contracts pass on the affected Unix platform, and current Windows/macOS regression gates remain unchanged.

## Boundaries and handoff

Do not broaden this patch into relative-path policy, HOME validation, directory creation, permission repair, process-global environment mutation or dependency/workflow changes. State discovery itself stays non-mutating. This chat has no Rust compiler or independent development sub-session; source publication is an implementation Draft, not executed behavioral qualification. Record concrete unexecuted checks in its PR/issue, keep them open for independent review, and do not merge, release or touch user state.
