<!--
Thanks for contributing to locron.

Please read CONTRIBUTING.md first if you have not. The short version: for anything beyond a small
fix, the planning documents change before the code does.
-->

## What this changes

<!-- One or two sentences. What behavior is different after this pull request? -->

Closes #

## Why

<!-- The problem being solved. Link the issue or the planning document where it was agreed. -->

## Verification

<!--
How you know this works — not just that it compiles. Name the test that covers new behavior, or
describe how you reproduced the bug before fixing it.
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
- [ ] `docs/TODO.md` status is current, with a `Verify` entry for each step
- [ ] User-facing docs updated (`docs/CLI.md`, `docs/OPERATOR.md`, `README.md`) if the surface moved

## Notes for the reviewer

<!--
Anything that would be easy to miss: unhappy-path behavior on daemon crash, clock jumps or DST,
overlapping runs, restart recovery, or a deliberate trade-off you want a second opinion on.
-->
