# AGENTS.md

## Core Workflow

Documentation is the source of truth for this repository. Always read `docs/SPEC.md`, `docs/IMPLEMENTATION.md`, and `docs/TODO.md` before changing code. For a new or materially changed body of work, update the relevant planning document before implementation begins.

The required planning order is:

1. Draft or update `docs/SPEC.md`.
2. Resolve its open questions through research and record supporting evidence in `docs/FINDINGS.md` when research is needed.
3. Complete or update `docs/IMPLEMENTATION.md`.
4. Complete or update `docs/TODO.md`, including a verification method for every step in a plan with three or more steps.
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

### `docs/TODO.md` — Progress and Verification

- Track implementation as phased checklists.
- For plans with three or more steps, every step must have a concrete `Verify` entry.
- Mark a step complete only after its verification succeeds.
- Keep status current throughout the work rather than reconstructing it at the end.

## Sub-session Handoff

- Immediately after the initial specification draft, continue research or development in a separate sub-session.
- When specification questions remain, the research sub-session produces `docs/FINDINGS.md` before the three planning documents are finalized.
- The development sub-session owns documentation updates caused by implementation decisions and reports its changes and verification results back to the parent session.
- The parent session reviews the report and handles repository-level publication work.

## Git

- Commit messages must use `{type}: {message}`.
- Keep the `type` lowercase and concise.
- Write the `message` as an imperative, specific summary.
- Inspect staged and unstaged changes before every commit.
- Stage only the files that belong to the current change.
