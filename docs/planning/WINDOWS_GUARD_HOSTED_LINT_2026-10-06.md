# Current guard hosted lint continuation — 2026-10-06

## Fixed contract, pins and measured failures

Continue privacy#27, CI#31, dashboard#162, repair#146, prune#149 and coordination#163
for diagnostic drafts PR141/147. This is a narrow compile/lint correction; frozen
SPEC/ARCHITECTURE, original trust/error/owner/privacy/cleanup and every acceptance gate
remain unchanged. Signing37 and wider delivery/account/logon/reboot work stay separate.

Exact combined Source heads are A346c08cb2977e772e0a27920c4c00de006c76419 and
Ba29a5f7900364ebe8462d200ee55172715713da6. Both already include the whole selected
common319a2a7aad21c9d00b3c7dc0f40dd49263b58821 and incoming
c39ebeb3d2c3784bf8a06f383fa8d490f79edf61/tree9cec9f922b141ab4d0fd03f1cbd657cb64995663
contributions, with main/base f90a9d5dc2e6bafbc163567b333c398777647402 pinned. Future
Docs-only descendants may be used only after their entire non-Docs modes/blobs are exact.
This plan selects no newer/floating Source, common main merge or old failure waiver.

Root's once-acquired A CI37365084437 Windows1.98 job111948190639 measures EIGHT
clippy::useless_conversion errors in Core filesystem.rs896/906/919/931 and
1196/1206/1220/1234. B CI37365095813 Linux1.98 job111948225647 measures
AdmissionRole::SharingControl never constructed at Server api_prune/qualification.rs283.
These are separate actual lint failures with narrow Source-backed corrections below.
Neither establishes a native guard cause, current whole-run success or main acceptance.

## Common exact ACE-index correction

The entire current Core filesystem.rs blob is identical in A and B. Each of the eight
diagnostic indices is the original index in 0..acl.len() used by acl.get_ace(index):
four in trusted_directory and four in verify_owned_executable. The pinned existing
windows-permissions0.2.4 Acl::len returns u32 and get_ace takes u32. Thus these indices
are already u32; their original u32::try_from(index).ok() cannot fail or change the value.
Rust's reflexive TryFrom contract is infallible, so Some(index) preserves the exact
Option<u32> diagnostic value without a cast, new query or error/guard decision.
[TryFrom primary contract](https://doc.rust-lang.org/std/convert/trait.TryFrom.html).
The versioned1.94 web page was inaccessible; actual pinned1.94/1.98 type/lint fit still
requires hosted qualification, not this current documentation or byte research.

Select ONLY those eight u32::try_from(index).ok() expressions to become Some(index)
in both representatives' Core file. Keep the root-first usize conversion in
ancestor_refusal and the distinct existing chain-index test conversion checked/literal.
This refines the earlier generic checked-conversion wording ONLY for these eight proven
u32 ACE slots; unrepresentable usize ancestry indices still project none. No diagnostic
mask/label/optional value, custom Error/native object/raw/Display or renderer changes.
The original A214/B192/generic158 CRLF models remain <=256 with the same exact values.

## B-only platform role correction

The Server enum variant and label arm compile on Unix while the ONLY actual
AdmissionRole::SharingControl constructor is N07 inside existing cfg(windows)
native_case. Add #[cfg(windows)] ONLY to the SharingControl variant AND its matching
label match arm. Gating only the variant would leave a Unix arm referencing a missing
variant. Windows retains all seven names/order/labels and genuine held-sharing control;
Unix retains all six actual cross-platform roles. Preserve the complete original N07
calls/full-ID/error/raw32-or33/refusal/holder/release assertions and every Unix branch.

The Rust Reference permits cfg on enum variants and match arms and removes only a
false-predicate form. Use that existing syntax, with no dummy constructor or warning
allow/expect/dead-code suppression.
[cfg](https://doc.rust-lang.org/reference/conditional-compilation.html#the-cfg-attribute),
[enum syntax](https://doc.rust-lang.org/reference/items/enumerations.html),
[match-arm attributes](https://doc.rust-lang.org/reference/expressions/match-expr.html#attributes-on-match-arms).
Language syntax is not current-head compilation or actual native acceptance.

## Separate original CLI timeout remains unknown

Root's B Linux ARMstable job111948225655 measures89P/1F/60.22s: first SEL02-cli
hits the original check at CLI explicit_prune/qualification.rs173 then owned_row755
reports false. This is the distinct CLI file, not the Server guarded-open tag/role.
Its60s operation horizon starts before Owner preparation; outer90s result ownership is
unchanged. The shared check reveals no current call/path/holder/errno/child status.
No cause-backed timeout repair is selected. Preserve its first failure, failed-worker
retention and all original60s/90s checks; no retry, warming, reordering, serialization,
clock extension, success promotion or replacement by a passed lint/diagnostic receipt.

## Exact separate development and retained authority

Source stays PAUSED until Root commits both complete identical plans and append-only
new Docs, completes every owning whole-plan POST/exact GET and rereads the whole
committed plan plus all new Docs AFTER ALL GETs. Root owns #27/#31/#162/#146/#149/#163
and PR141/147 readback/publication. A later separate developer handoff may change ONLY
Core filesystem.rs in A/B at the eight stated expressions, plus B Server
api_prune/qualification.rs at the two stated attributes. Any shared Source commit or
normal cherry-pick choreography must be explicitly assigned by Root; no Source/Git
implementation follows from this Docs-only research lease.

Preserve every original predicate/skip/mask/condition/evaluation/call/order/return/error/
owner/lifetime/full-ID/descriptor/absence/restoration/case/assertion/oracle/cleanup region.
No common SID, private_directory/stock, Store/SQL/Engine/production request, CLI timeout,
root path/ACL/trust repair, public API, Cargo/dependency, workflow/selector or test change.
Keep all5s Wake/8s Cancel/30s owners/25ms stop/200ms probe/production5s grace and every
native clock literal. Unknown/unreturned facts remain refused; diagnostics are not repair.

Whole Source inverses must restore each exact parent after reversing only the eight
expressions and two B inserted attribute lines. Pinned formatter wrapping confined to
these selected expressions may be recorded as exact layout-only inverse components;
no surrounding clause or other Source adaptation is selected. Strict lint stays enabled.
Actual type, native behavior, current whole-run and every required Verify remain pending.

## Ordered work and concrete Verify

1. Root freezes Docs before Source. **Verify:** four Docs-only paths per exact A346/Ba29,
   three existing literal prefixes and one identical complete new plan; all368/355 other
   tracked modes/blobs AND before/after physical working bytes preserved, unchanged HEAD/
   index/SPEC/Source. Bind all three actual retained logs and all eight proven u32 slots;
   preserve old plans/failed evidence. All six owning Issues and both representative PRs
   receive the whole selected plan by POST/exact GET with body/state/history unchanged,
   then Root full committed-plan/new-Docs reread AFTER ALL GETs precedes a separate lease.
2. Separate development implements only the selected common eight and B two sites.
   **Verify:** complete staged/unstaged/committed Source and layout inverses, whole parent
   restoration and all other modes/blobs/working bytes exact. Same Core contribution in
   A/B; checked usize conversions and all existing native mutation/positive controls
   literal; Windows seven/Unix six role names/callers retained. Permit standalone
   rustfmt1.94 and1.98 checks/full locked OFFLINE metadata only locally. No compiler,
   Clippy/tests/native/PowerShell/parser/security/host/fixture effects; no allowance.
   A broader required decision stops for Docs FIRST. Static proof supplies no acceptance.
3. Root reviews/publishes genuinely changed heads and qualifies fresh completed CI.
   **Verify:** exact head/base/ref/ordered synthetic parents and whole equal Source tree;
   acquire each completed raw job once, strict Windows/Unix1.98 lint and all unchanged
   1.94/x64/ARM/MSRV/native/complete representative matrices pass. Original measured guard
   predicates/full-ID/private/absence/restoration/owner/cleanup controls, old cases and
   all current member Verify remain required. Keep the independent original SEL02-cli
   timeout and c39 MSRV failure at their actual failed scope until their unchanged actual
   criteria pass; no previous count/metadata/late output/static proof waives them. Complete
   owning/member Verify plus required main contribution precedes closure. No signing,
   release/install/account/logon/reboot or broad acceptance follows from this correction.
