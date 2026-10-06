<!--
Thanks for contributing to locron.

Please read CONTRIBUTING.md first if you have not. The short version: for anything beyond a small
fix, the planning documents change before the code does.
-->

## TL;DR
<!-- Summarize the outcome in one or two sentences. -->

<!-- Link the relevant repository issue and summarize its scope. Close an issue only when this PR fulfills all of its Verify and delivery criteria; otherwise report the remaining acceptance work. -->

## Why

<!-- The problem being solved. Link the repository issue or planning document where it was agreed. -->

## How / What
<!-- Describe the key changes and approach, including trade-offs worth reviewing. -->

## Verification
<!--
List the exact commands or manual steps you actually ran and their results.
Separate passed, failed, and unrun checks; explain why any relevant checks were not run and note limitations.
For bug fixes, include evidence that the issue reproduces before and passes after the change, or explain what could not be verified.
Include the tested commit and CI links when relevant.
-->

```
```

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace --all-targets` passes
- [ ] Commits follow `{type}: {message}`
- [ ] Only files belonging to this change are staged

**If this changes behavior, scope, or architecture:**

- [ ] `docs/SPEC.md` updated, or this change is within the existing frozen scope
- [ ] `docs/IMPLEMENTATION.md` reflects the approach actually taken
- [ ] Relevant issue progress/evidence is current, with concrete `Verify` criteria for each step
- [ ] Remaining acceptance is recorded before closing any partially implemented task
- [ ] User-facing docs updated (`docs/CLI.md`, `docs/OPERATOR.md`, `README.md`) if the surface moved

## Additional Context
<!-- Optional: include UI screenshots, risks, migration or rollback steps, dependent PRs, or follow-ups only when relevant. Remove this section if not needed. -->

<!--
Anything that would be easy to miss: unhappy-path behavior on daemon crash, clock jumps or DST,
overlapping runs, restart recovery, or a deliberate trade-off you want a second opinion on.
-->
