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


## Root-reviewed actual CLI discovery controls before Source (2026-10-04)

Owner-authorized continuation after independent research. Frozen SPEC and the
existing storage contract remain unchanged. Own source head is
6a26d1256e509be51df0cafb6f3d6c0b88bf8962; select actual merged main
9de596f8ffe48e8036f775ad1fddbea9302915a1, tree51c7643e6d5e648f48822a89909337d5c9231736.
Root104-artifact/42-snapshot proof15d9ba7a validates the whole Source inverse,
Windows function and old tests, independent base/head/main-c4 trees and disjoint
composition. New main138 is an explicit incoming Git integration, not a new
runtime result for this PR. Matching execution record is Issue152.

### Selected minimum boundary

Adapt only crates/locron-cli/tests/cli.rs within the existing cfg(unix)
state_discovery module. Add genuine isolated child-command cases and narrowly
needed private test helpers. Preserve the entire existing three test bodies,
without_default/returned-error guidance helpers, every other test/helper/Unix
literal and Windows section. Keep paths.rs byte-exact head6a26 and all other
production/dependency/lock/workflow Source exact selected-head/main contribution.
Do not change configuration/path policy or use discovery as the expected oracle.

Use actual cargo-built locron with each fresh temporary parent/cwd and only child
Command.env_remove/env/args. Parent process environment never changes. Candidates
are distinct test-owned paths, with spaces/Unicode and byte-preserving OsString.
Actual Linux matrix is HOME absent/empty/present crossed with XDG absent/empty/
present. Reuse the original absent/absent human and JSON/stream error tests for
that one existing pair; separately qualify empty-XDG with absent HOME. Nonempty
XDG chooses XDG/locron; absent/empty XDG chooses HOME/.local/state/locron when
HOME is present, including empty HOME's existing relative-to-cwd behavior.
Do not add HOME validation or change relative-XDG/empty-override policy.

For accepted selections first run actual prune --dry-run. Assert no database,
managed root or sidecars created at any selected/nonselected candidate and no
unexpected child-cwd entries before any live command. Then run actual config get
global_concurrency, require the unchanged success output and state.db at the
independently expected actual path; all nonselected roots remain absent. Path
selection itself is nonmutating; a later live config read intentionally opens
state, so those phases and assertions must remain ordered.

Add a genuine nonempty relative-XDG case to preserve its existing cwd-relative
selection. Actual Unix invalid-UTF8 components use OsStringExt::from_vec without
NUL, not lossy String conversion. On Linux independently qualify nonempty XDG
and HOME fallback; on Unix qualify explicit CLI override over a different actual
environment override, and environment override alone. Expected paths retain raw
bytes; validate actual DB placement and nonselected-root absence. No mirror of
StatePaths::discover or parent env mutation may supply the expected result.

On macOS, a real HOME default selects Library/Application Support/locron even
with competing XDG, including nonempty non-UTF8 HOME. HOME absence with XDG
present retains original state_error/guidance and no state effects. Empty HOME
retains its original relative fallback; do not claim XDG fixes Unix service
commands whose separate ServiceContext still requires HOME. Preserve the old
explicit override tests and error exit5/human/CLI JSON/stream envelopes exactly.

These ordinary finite state-discovery commands reuse the existing subprocess
mechanism. The normal CI job bound is not per-call native preemption. A new
timeout/cleanup wrapper, production seam, dependency or workflow choice returns
to Root Docs/Issue before Source. No owner-PC executable/compiler/parser/fixture
execution is selected; actual Linux/macOS/Windows results come from hosted CI.

### Four ordered Verify steps

1. Freeze Docs, matching Issue152 and final Root review. Verify: all research
   hashes/exact pins/unchanged SPEC, complete new Doc bytes and Issue four-step
   comment readback precede the separate one-test-file development lease.
2. Integrate exact main and add genuine Unix child cases. Verify: ordinary Git
   ancestry includes exact6a26 and main9de; original Docs plus incoming Docs
   suffixes remain complete once. All three original discovery bodies, every
   other Source mode/blob and Windows branch remain exact; only selected tests
   adapt. Matrix/relative/invalid-byte/macOS/override cases use independent
   expected paths, dry-before-live checks and no global environment mutation.
3. Complete Root conservation and static review. Verify: the full new Source
   delta and module are reviewed; removing selected additions recovers entire
   original cli.rs. Exact paths.rs/platform branches, old test selectors and
   all incoming release/dashboard/WinGet Source are bound. Rust1.94/1.98 fmt,
   diff check and full locked offline metadata pass; no native acceptance is
   inferred from these checks or old18-success metadata.
4. Fresh actual head qualification and merge. Verify: hosted Linux and both
   macOS architectures execute every selected discovery case with zero ignored,
   failure or missing cases and original error/override controls intact. Existing
   native Windows x64stable/x64Rust1.94/ARMstable and strict lint/full CI gates
   pass at that exact reviewed head/tree, including snapshot/paired bindings.
   Root retains actual raw named outcomes and package provenance before exact
   head publication/merge and Issue152 completion. Any unexpected state effect,
   failure or unrun check remains visible; no unchanged retry/gate suppression.

This limited path selection issue can close after its actual Verify and merge;
broader Windows installation/account/task/logon/reboot/public-release acceptance
continues in its owning Issues. No other open PR Source is silently imported.
