# Guarded paired update admission

Refs #33 and #34. Based on #124's actual CanonicalPair and #122's canonical paired Payloads. This continues the frozen no-downgrade, exact-release and preserve-existing-installation contracts; it introduces no public command or new wire schema.

## Scope

Select an unchanged installation or a strictly newer complete replacement BEFORE releasing either source image read guard. Versions are canonical numeric triples, not strings. Same-version content/source divergence refuses instead of silently reinstalling changed immutable release bytes. Architecture, exact destination and raw PATH restoration ownership stay unchanged. A newer target creates one validated v2 receipt from canonical payloads at the existing canonical directory. Installation options, task snapshots, helper/request authorization, native version/ABI execution and durable effects remain separate requirements.

## Implementation and Verify

1. Add a private pure comparison of validated current/desired v2 receipts. Verify numeric ordering, downgrade refusal, target/directory/account/channel/ABI invariants, exact same-version archive and all seven hashes, and literal PATH record preservation. No receipt comparison alone grants live authority.
2. Compose this decision with the actual retained CanonicalPair and canonical target Payloads. Keep the old guards and desired payload borrow in one move-only plan, without writable-handle escape. Verify a real private-file fixture keeps both images immutable during preparation and produces only the selected receipt bytes; same-version equality must not require exclusive opens.
3. Permit only a replacement plan to consume the existing dual-image exclusive gate. Carry the selected target and receipt bytes alongside that live result. Verify contention on either image refuses without installed-file writes and no-op cannot be misrepresented as replacement. Check expiry against the original deadline before and after preparation/transition; never substitute a new thirty-second window.

This is a Windows-test-only integration slice. It creates no operation directory, journal, backup, task, PATH value or installed leaf. A successful plan or exclusive observation is NOT updated=true, quiescence, final activation, rollback or installation success. Preserve all existing core ACL/sharing predicates and legacy single-image routes.

## Review and execution boundary

The plan precedes source. The available chat environment can publish through the GitHub connector and run local Python, but has no native Windows or Rust toolchain and no separate development sub-session. Record that limitation rather than claiming independent review or unexecuted tests. Exact-head hosted Rust/native CI and maintainer review remain merge gates. Keep #33/#34 open.
