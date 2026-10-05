# Canonical guarded removal for explicit CLI pruning

Refs #149. Stack on #151 at `3918e099f6f875ccd622a635308dea234534c59a` (main.rs blob `2f14edf8a3e05ea1420f96259ca709616b253d6d`). #150 implements the independent dashboard path; do not import the server crate into this CLI helper or modify its pending source.

## Existing contract and source finding

STORAGE.md requires canonical identifier-derived paths, no symlink traversal during pruning, and durable intent -> file removal -> completion. #151 fixes selection/accounting but intentionally preserves the old platform removal code. That code validates the run/attempt/path only on Windows and only immediately before each individual effect. Unix joins the persisted relative_path and checks only the final leaf, leaving parent symlinks and malformed stored paths outside the admission check.

This patch applies the existing path/deletion contract to the explicit CLI, without changing retention selection or enabling additional cleanup.

## Implementation and Verify

1. Build every selected final output path with StatePaths after positive u16 attempt conversion and exact `{run_id}/{attempt}.log` comparison. Perform the complete pure preflight before entering the live loop, including during a dry run. **Verify:** malformed UUID, attempt zero/overflow, absolute/traversal/partial/backslash/mismatched path and a malformed last candidate all refuse before any pending intent or file effect; valid path identities/order are unchanged and dry-run creates nothing.
2. Keep existing private state/output/run-directory guards alive through each removal. Missing output directories/leaves stay missing; unsafe parents or symlink/non-file leaves refuse. Use the existing Windows private-file removal adapter and Unix no-follow checks plus directory synchronization. **Verify:** parent/leaf symlink and Windows junction/ACL/sharing cases preserve external canaries; missing directories are not created/repaired; ordinary canonical files remain removable, and Unix directory sync precedes durable completion.
3. Keep the caller's pending -> remove/sync -> completion order and stop at the first error. Preserve #151's projected selection, byte totals, batch/age bounds, output text and response fields. **Verify:** failure injection never reports success or removes a later candidate; pending state remains recoverable. Run existing CLI prune/maintenance and native Windows output cases plus format/Clippy before independent review/merge.

## Publication and limits

Use a private CLI module and a narrow main.rs call-site edit. The large existing source may be transformed on an isolated feature-branch runner, tied to the exact input Git blob with unique anchor/inverse checks. The runner creates an unreferenced candidate only; parent connector review and non-force advancement publish it. Its temporary workflow must be deleted from the candidate tree. Final parent-relative diff is only this plan, the new helper and the CLI call site; no permanent CI job, unrelated source, parent/main write or dependency change.

Retained guards preserve the existing platform implementation, not a new hostile same-account writer sandbox or SQLite/filesystem atomic transaction. All user-state cleanup and native/runtime tests remain unexecuted in this code-first Draft. No separate development/review sub-session is available here; independent review and explicit Verify handoff remain required. No merge, release or issue completion.
