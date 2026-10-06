# JSON boolean flags at the dashboard API boundary

Base: main `46445881e320f440c02d94d7e9e5838033933fea`. This is a correction to the existing dashboard dry-run contract, not a new command or mutation policy.

## Source-established mismatch

`JobCreateRequest`, `JobUpdateRequest` and `SettingsPutRequest` all deserialize `dry_run` using the query-flag helper. That helper requests `Option<String>`, so JSON boolean values are rejected before the handlers can choose their existing read-only/live Store branch. The same helper correctly handles textual query flags. No production incident or executed HTTP regression is claimed from this source inspection.

## Approach and Verify

1. Extend the shared flag deserializer with an explicit untagged boolean-or-string representation. Native booleans return their exact value; textual and nullable inputs retain the current grammar. Verify JSON true/false, missing/default false, legacy empty/true/false/1/0 strings and explicit null; reject numbers, arrays, objects and unsupported strings rather than introducing truthiness.
2. Preserve the existing three request structs, query shapes, handlers and Store selection. Verify real authenticated create/update/settings requests with dry_run=true return the normal dry-run envelope and leave job revisions/settings/history and absent state untouched. Verify false/missing use the existing live path, and the old query flag forms still work.
3. Keep this an implementation Draft with explicit reviewer checks. Verify `cargo fmt --all --check`, server library/contract tests and exact-head CI before merge. Existing Windows/native, security and lifecycle criteria remain separate; no test-only success is inferred.

The implementation is limited to the shared deserializer, with no dependency, route, response-schema, CLI, scheduling, database or release change. Compatibility intentionally retains explicit null and empty textual flags as true, as before. It does not reinterpret dry-run query parameters on endpoints whose contract uses JSON bodies.

The current environment has no Rust compiler or independent development/review sub-session. Source review and exact diff checks are possible here; compilation, new regression execution and independent review remain handoff gates. No live service, token, user jobs, installed software, release, merge or issue closure is authorized by this slice.
