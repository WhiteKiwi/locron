# Contributing to locron

Thanks for your interest in `locron`. This document describes how the project is developed and what
a reviewable contribution looks like.

`locron` supervises real processes on a user's machine and owns durable state on their disk.
Scheduling correctness, state durability, and predictable process supervision matter more than
feature velocity, so the workflow below is deliberately documentation-first.

---

## Documentation comes before code

This is the most important rule in the repository, and it is unusual enough to read before you open
an editor. The authoritative statement of the workflow is [`AGENTS.md`](AGENTS.md).

**Planning documents are the source of truth. Code is the consequence.**

For a new feature or a materially changed behavior, update the planning documents in this order, and
only then write code:

| Order | Document | Answers | Must not contain |
| --- | --- | --- | --- |
| 1 | [`docs/SPEC.md`](docs/SPEC.md) | Goal, observable completion criteria, scope, open product questions | Filenames, modules, tables, implementation steps |
| 2 | [`docs/FINDINGS.md`](docs/FINDINGS.md) | Research evidence that resolves the spec's open questions | Conclusions without supporting evidence |
| 3 | [`docs/IMPLEMENTATION.md`](docs/IMPLEMENTATION.md) | Architecture, data flow, trade-offs, edge cases, change order, verification strategy | — |
| 4 | [`docs/TODO.md`](docs/TODO.md) | Phased checklist; every step of a three-or-more-step plan carries a concrete `Verify` entry | Steps marked done before their verification passed |

Three rules follow from this:

- If a decision changes while you are implementing, **update the planning document first**, then
  change the code. Never implement first and reconcile the documents afterward.
- `docs/SPEC.md` is frozen for the current milestone. Editing it means you are proposing a
  product-scope or behavior change — say so explicitly in the issue or pull request.
- A `docs/TODO.md` step is complete only after its `Verify` entry actually succeeded.

**Small changes do not need a planning cycle.** A typo, a broken link, a build fix for a new
platform, a dependency bump, or a one-line bug fix that does not change documented behavior — open
the pull request directly.

If you are unsure which side of that line your change falls on, **open an issue first and ask**.
That costs one round trip; a rejected 800-line pull request costs an afternoon.

---

## Getting started

### Prerequisites

- **Rust 1.94.0.** The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml), so
  `rustup` selects the right version — including `clippy` and `rustfmt` — automatically. 1.94 is also
  the project's MSRV, and CI builds against both `1.94.0` and `stable`.
- **macOS or Linux.** Windows is not a supported target.
- Nothing else. SQLite is compiled in through `rusqlite`'s bundled feature, so there is no system
  library to install and no external service to run.

### Build and run

```sh
git clone https://github.com/WhiteKiwi/locron.git
cd locron
cargo build
cargo run -- --help
```

To exercise a build without touching your real state, point it at a scratch directory:

```sh
cargo run -- --state-dir /tmp/locron-dev doctor
```

---

## Project layout

`locron` is a Cargo workspace producing one binary, `locron`, from five packages.

| Crate | Owns | Must not contain |
| --- | --- | --- |
| `locron-core` | Domain identities and values, schedules and policies, validation, state transitions, commands/results, and persistence/clock/executor ports | SQLite, CLI parsing or rendering, OS service setup |
| `locron-store` | SQLite connections and migrations, repositories, transactions, durable uniqueness, retention records, and implementations of core persistence ports | CLI presentation, process spawning, HTTP execution |
| `locron-engine` | The daemon runtime: lifetime and lock ownership, reconciliation, overlap/concurrency admission, retry timing, process/shell/HTTP runners, cancellation, recovery, signals, shutdown | SQLite implementation details, CLI presentation |
| `locron-server` | Loopback dashboard/API transport and embedded viewer | Scheduler ownership and direct SQLite table access |
| `locron` | Composition root and command entrypoint: parsing, human/machine rendering, bootstrap, wiring store to engine, `locron daemon run` | Reimplemented domain policy, scheduler loops, runner lifecycle |

The dependency direction is an invariant, not a convention:

```
                    locron
       /          /      \          \
locron-server locron-engine locron-store locron-core
```

- `locron-core` depends on no other workspace crate.
- `locron-store` and `locron-engine` each depend on `locron-core`.
- **`locron-engine` does not depend on `locron-store`.** It receives persistence through core ports.
  A pull request that adds that dependency will be asked to invert it.
- `locron-server` depends only on `locron-core` and `locron-store`.
- `locron` is the composition root and depends on the other four. No library depends on it.

[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) is the full reference.

---

## Before you push

Run exactly what CI runs. All three must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Note that CI escalates warnings to errors, so **any new lint warning fails the build**. The
workspace also enforces:

- `unsafe_code = "forbid"` — there is no `unsafe` in this repository, and contributions may not
  introduce it.
- `missing_docs = "warn"` — new public items need doc comments.
- `clippy::all` and `clippy::pedantic` — the allow-list in the root `Cargo.toml` is deliberately
  small. Prefer fixing the lint over extending the list; if an allow is genuinely warranted, add it
  at the narrowest scope and explain why in the pull request.

Edition is 2024 across the workspace.

---

## Commit messages

Use `{type}: {message}`.

- Keep `type` lowercase and concise.
- Write `message` as an imperative, specific summary — `fix: bound daemon admission latency`, not
  `fix: bug fix`.
- Inspect staged and unstaged changes before every commit, and stage only the files belonging to
  the current change.

Types in active use: `feat`, `fix`, `docs`, `test`, `ci`, `release`. Others are fine when they are
lowercase and obvious.

```
feat: add locron mcp stdio server
fix: exclude live-lifetime attempts from output recovery
docs: define CI/CD and release policy
test: complete lifecycle fault boundaries
ci: migrate node20 actions to node24 majors
```

### Breaking changes

Mark a breaking change with `!` after the type — `feat!:` or `fix!:` — or add a `BREAKING CHANGE:`
footer. During pre-1.0 this bumps the **minor** version, per [`docs/RELEASE.md`](docs/RELEASE.md).

---

## Changelog

You do not need to edit [`CHANGELOG.md`](CHANGELOG.md) in your pull request. Release notes are
generated from commit messages with [git-cliff](https://git-cliff.org) using
[`cliff.toml`](cliff.toml), which is why the commit convention above matters more than it looks:

- `feat:` → **Added**, `fix:` → **Fixed**, `perf:`/`refactor:`/`revert:` → **Changed**,
  `docs:` → **Documentation**.
- `ci:`, `test:`, `chore:`, and `release:` commits are deliberately omitted — real work, but not
  user-visible change.

So write the commit subject as the line you would want a user to read in the release notes.

The maintainer curates the generated output before tagging:

```sh
cargo install git-cliff --locked
git cliff --unreleased --prepend CHANGELOG.md
```

---

## Pull requests

- **One change per pull request.** Unrelated refactors make review disproportionately expensive.
- **Include the documentation updates in the same pull request** as the code they describe, when
  the change required a planning cycle.
- **Explain the verification you ran**, not just the change you made. If you added behavior, say
  which test covers it; if you fixed a bug, say how you reproduced it first.
- Draft pull requests are welcome for early feedback — mark them as such.
- Scheduling, durability, and process-supervision changes should say what happens on the unhappy
  path: daemon crash mid-run, clock jumps and DST transitions, overlapping executions, and restart
  recovery.

CI runs `fmt`, `clippy`, and the test suite on macOS and Linux, on both `x86_64` and `aarch64`,
against MSRV and stable. A green local run on one platform is a good signal, not a guarantee.

---

## Reporting bugs and requesting features

Open an issue using the templates in the
[issue tracker](https://github.com/WhiteKiwi/locron/issues/new/choose). For bugs, the output of
`locron doctor` and your `locron --version` are the two things that most often decide whether the
report is actionable.

**Do not report security vulnerabilities in public issues.** See [`SECURITY.md`](SECURITY.md) for
the private reporting path.

---

## Licensing of contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as MIT OR Apache-2.0,
without any additional terms or conditions.

---

## Code of conduct

Participation in this project is governed by the
[Contributor Covenant](CODE_OF_CONDUCT.md). By participating, you are expected to uphold it.
