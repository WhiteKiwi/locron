# AGENTS.md

## Core Workflow

Product and design documentation remains the source of truth for this repository. Execution scope, tasks, verification evidence, and progress live in the private Locron GitHub Project as Project-only draft tickets. Before changing code, read `docs/SPEC.md`, `docs/IMPLEMENTATION.md`, `docs/PROJECTS.md`, and the relevant Project drafts. For a new or materially changed body of work, update the relevant planning document before implementation begins.

The required planning order is:

1. Draft or update `docs/SPEC.md`.
2. Resolve its open questions through research and record supporting evidence in `docs/FINDINGS.md` when research is needed.
3. Complete or update `docs/IMPLEMENTATION.md`.
4. Create or update the relevant Project draft tickets. Include a concrete `Verify` criterion for every step in a plan with three or more steps; use Phase and Status to track execution.
5. Review the completed plan once more before implementation.
6. Hand implementation to a separate development sub-session. The parent planning session does not implement code after drafting the specification.

If a decision changes during implementation, update the applicable planning document first and only then change code. Never implement first and reconcile the documents afterward.

## Planning Documents

### `docs/SPEC.md` — What and Why

- Define the goal, observable completion criteria, scope, and open product questions.
- Do not describe filenames, modules, classes, database tables, or implementation steps.
- Freeze the specification after agreement. A later specification edit represents a product-scope or behavior change.

### `docs/IMPLEMENTATION.md` — How and Why This Approach

- Describe architecture, data flow, design decisions, trade-offs, edge cases, change order, and verification strategy.
- Make the approach reviewable without opening source code.
- Keep the change plan limited to this repository.
- Record why an approach was selected, not only what will be changed.

### Project Draft Tickets — Execution and Verification

- Use the private [Locron Project](https://github.com/users/WhiteKiwi/projects/4) for maintainer execution scope, phased tasks, verification criteria, evidence, and progress.
- Use Project-only drafts; do not convert internal tasks to repository issues. Public contributor bug reports remain on the repository issue tracker.
- Record concrete `Verify` criteria for every step of a plan with three or more steps.
- Set Status to Done only after Verify criteria succeed, evidence is recorded, and any PR merge or publication required by that ticket is complete. A ticket requesting an opened PR can complete at that deliverable.
- Keep task status current throughout the work. Review the relevant drafts before implementation deviations.
- `docs/PROJECTS.md` owns workflow and the static migration map. `docs/TODO.md` is a pointer, not a second live checklist.
- The Project is private. Without access, use public product/design documents and ask a maintainer for task context; do not create a parallel execution checklist.

## Sub-session Handoff

- Immediately after the initial specification draft, continue research or development in a separate sub-session.
- When specification questions remain, the research sub-session produces `docs/FINDINGS.md` before the planning documents and relevant Project draft tasks are finalized.
- The development sub-session owns documentation updates caused by implementation decisions and reports its changes and verification results back to the parent session.
- The parent session reviews the report and handles repository-level publication work.

## Git

- Commit messages must use `{type}: {message}`.
- Keep the `type` lowercase and concise.
- Write the `message` as an imperative, specific summary.
- Inspect staged and unstaged changes before every commit.
- Stage only the files that belong to the current change.
