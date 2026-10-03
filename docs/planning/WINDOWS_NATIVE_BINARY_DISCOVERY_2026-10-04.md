# Native per-binary Windows discovery

Frozen SPEC and existing #31/#32 requirements remain unchanged. Full runs37135594179 and
37135898657 reached actual CLI unit testing but stopped before the positive token integration
cases and later workspace binaries. Ordinary run37135898238 has a retained x64 cold Core
failure. This plan collects missing native facts; it cannot turn those failures into success.

1. Create a retained verification-only branch from reviewed PR130 head
   543e82392754c2e2a1286ae7ae8e22c9c3f89f95. Record measured findings and this plan before Source.
   **Verify:** branch has no existing unrelated changes; docs-only planning commit and exact
   CLI #31 Verify readback precede the separate development session. No frozen SPEC change.
2. In this temporary branch only, replace ci.yml with a contents-read manual-only caller.
   Use native windows-2025 x64 stable, windows-11-arm ARM64 stable, and windows-2025 x64 Rust1.94.
   Explicit RUSTUP_TOOLCHAIN and compiler/host checks reject the wrong architecture/release.
   Use independently verified immutable checkout, Rust-toolchain and cache action revisions.
   Preserve toolchain assertions before any test; keep the normal PR130/main workflow unchanged.
   **Verify:** actionlint and parent full-source review pass; the only Source blob changed from
   543e823 is .github/workflows/ci.yml, with every app/test/lockfile/mode preserved.
3. Execute each unchanged positive token case separately with locked Cargo, exact selection:
   dashboard::doctor_reports_the_dashboard_exposure_facts,
   service::lifecycle_human_modes_render_labeled_reports_instead_of_json,
   service::dashboard_status_reports_service_state_url_and_token_facts. Integration test targets
   are selected with --test and names with --exact; do not include a module prefix in the names.
   Follow with cargo test --workspace --all-targets --locked --no-fail-fast, default features.
   **Verify:** all four test steps are attempted after successful compiler verification even if
   an earlier test failed, unless cancelled. No continue-on-error, retry, test serialization,
   skip, warning allowance, deadline/assertion/fixture change. Failed exits keep the job failed.
4. Review, commit and push only this temporary branch, then dispatch its ci.yml using CLI.
   **Verify:** run head equals reviewed Source commit, each native compiler/host is recorded,
   all nine focused test selections report one actual test rather than zero matches, and full
   no-fail-fast output records each later binary's result. Retain full failed logs and hashes.
5. Record coverage and failures in #31, preserving its original body/state and unfinished scope.
   **Verify:** exact CLI readback matches reviewed evidence; no full-suite or support completion
   claim unless all original gates genuinely pass. Switch the clean worktree back to PR130;
   do not merge/cherry-pick this verification-only workflow or waive its existing failed gate.

Only the temporary branch's .github/workflows/ci.yml is authorized for Source in this handoff.
No application/test/lockfile changes, owner-PC effectful acceptance, infrastructure/resource
changes, release/catalog publication or normal CI/ruleset changes are authorized by this plan.
