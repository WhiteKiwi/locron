# Windows doctor effective PATH lookup parity

Refs #26. Base main `15820ed96cc6da2bd8c37d42ec8788fd2180114b`.

## Scope

Windows environment keys are case-insensitive and runtime target construction already uses `locron_core::execution::environment_value`. The dashboard diagnostic path also uses that helper, but the CLI and MCP doctor renderers directly call `process.env.get("PATH")`. A valid attempt environment whose retained spelling is `Path` or `path` can therefore resolve an executable correctly while those two diagnostic surfaces report a missing effective path.

Use the same shared lookup helper for CLI and MCP doctor output. Do not change environment merging, executable resolution, persistence, redaction or output schema.

## Verify handoff

1. Replace only the two diagnostic map lookups. **Verify:** Windows fixture with mixed-case PATH spelling reports the same effective value through CLI doctor, MCP doctor and dashboard diagnostics while resolved executable remains unchanged.
2. Preserve Unix exact-key behavior and missing-PATH output. **Verify:** existing doctor JSON/human/MCP contracts remain byte/field compatible apart from correcting the Windows value.
3. Run formatting, focused CLI/MCP doctor tests, core execution tests, warnings-denied Clippy and current native x64/ARM64 CI before merge.

No service, state, user environment, release or installation mutation belongs to this change.
