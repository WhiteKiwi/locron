# Environment setting preview correction

Companion to DASHBOARD_JSON_BOOLEAN_FLAGS.md and issue #143; reviewed before source on main `46445881e320f440c02d94d7e9e5838033933fea`.

The environment.NAME branch in settings_put checks only whether a Store exists before calling set_environment and send_wake. with_store_for returns Some(read-only Store) for an existing database in dry-run mode. Consequently an existing-state preview attempts a write and fails, even when the textual dry_run spelling is used. The absent-state dry-run case avoids this accidentally because its Store is None. This is not evidence of a successful unauthorized write: the selected Store is read-only.

Gate the existing setter and wake call on !body.dry_run, then require the already-guaranteed live Store in that branch. Keep name/value validation, current-state-based created/replaced reporting, value redaction and result shape unchanged. An actual set must still use the same existing setter and emit its wake only after success.

Verify handoff: preview both new and existing environment keys on populated state using native JSON true and compatible textual true; response must succeed with value_redacted=true, preserve the complete persisted settings and avoid setter/wake calls. An absent-state preview creates no DB. Invalid names/values still refuse. False/missing flags must persist actual changes and use the existing wake path. Compare no-reset body/query parsing and all server library/contract tests on the exact PR head.

This small second correction belongs in the same API dry-run PR, not a separate security or Windows lifecycle PR. No new test execution, compiler result, native runtime success or independent review is claimed here. The owner requested implementation Drafts with concrete uncompleted Verify criteria for the next review session.
