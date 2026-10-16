# Run one complete native library context

Owners #172/#31; related #162/#149. Base ea3d9861d9079c258046923a1cddc100469227b0,
tree092a105ef0f47f189fea112fb8f691efb0ca47e6. Frozen SPEC/product scope is unchanged.
This CI amendment explicitly retires the alternate A full-library runtime context
and first PR144 ledger/upload. It supersedes ONLY the two-context/six-ledger
requirements in WINDOWS_COMBINED_LIBRARY_EVIDENCE_2026-10-06.md and subsequent
foundation plans. All unique source-level tests and original native oracles remain.

## Completed evidence and remaining failure

Run38886903109 checks out same-tree e202e1bfdacd35f9add54799c8823eff168b5030.
Fourteen actual jobs pass; all3 foundations reach their original35-minute limit.
All5 Unix jobs pass; macOS x64 finishes26m54 within30minutes. GitGuardian passes.
All3 Core148, all6 full Engine75/Server75/Store134+3ignored contexts, all6 PR144195
(each156parser+39runtime/cleanup,80 documents), all3 prune88/API45+CLI43 and native
Engine43/Store42/caller6/output91 gates pass. Raw Store/caller capture hashes were
not independently rechecked: final copy40/upload41 are SKIPPEDall3. Prepare17 does
not qualify final artifact provenance. New readiness/withheld/publication passall3;
original CLI23PASS/0testFAIL/1INCOMPLETE (MSRV outputcapture26).

Stable/ARM pass service64 at193.38/194.93s, GUI12, lifecycle16, dashboard4,
guardedprune3, maintenance17 and MCPhelpers3. MCP18 actually begins, then job
cancellation interrupts it; no complete result or specific test failure is proved.
MSRV service and subsequent tail are unrun. Historical failed/skipped facts stay.

The MSRV A/B full-library steps take457/487s: Engine17.11/17.43s,
Server296.03/306.07s and Store140.57/114.54s actual runtime. Stable A/B take326/351s;
ARM375/418s. Scoped Engine+Server no-run builds cost at most63.40/70.83/60.16s
(MSRV/stable/ARM), so even removing those builds cannot close the MSRV gap. Step30
is a legitimate ordinary dev graph, completes33.63/24.12s, and stays unchanged.
MCP compile completes3.49/2.78s before actual test execution/cancellation; no MCP
deadlock or assertion failure is inferred. Exact changed-head timings may vary.

## Source/actual-ledger audit and coverage decision

A11 and B13 have the same working tree, shell, condition, three-package/default-
feature/lib/test-profile/locked command and test-threads1. Only Cargo target and
the fresh PR144 evidence sibling differ. Both inherit the SAME A BuildFacts and
B prune production/recovery inputs. PR144 does not read LOCRON_PRUNE_*; the
separate API-prune test uses those same B artifacts in both contexts.

All3 paired actual full-library runs have the same ordered287(package,test,status)
entries: Engine75, Server75, Store134PASS plus the same3ignored. All3 actual195-key
pairs bind the SAME A Store-helper fullID/SHA/features and
A controller fullID/SHA. Each B runtime admits and retains those ORIGINAL helpers,
places actual Server/Store/controller facts in tickets, verifies each real child's
current_exe/fullID/hash, uses the controller39 times and populated Store actor26
times (13 rows intentionally absent), and checks39 cleanups. B retains its own
real no-follow executable/parent admission and fresh private evidence leaf.
The original first and second Server fullIDs AND hashes differ; no byte or
physical-context equivalence is claimed. AEngine/AServer full suites will no
longer run; AStore remains an actual support actor. This consciously retires one
alternate compiled-image context, while retaining every unique test assertion in
one complete B run on EACH original x64 stable/MSRV and ARM64 native host.

Issue162 requires native privacy/ACL/sharing/token/negative controls, actual
Server library and remaining middleware/contract/browser acceptance. It does not
independently require two artifact namespaces. Its older two-context execution
plan is explicitly superseded here; other issue delivery criteria remain open.
Separate independent source/ledger/consumer reviews found no unique assertion
parameterized by A versus B namespace. Fresh native qualification remains required.

## Exact selected source lease

Separate diagnostic_surfaces development owns ONLY .github/workflows/ci.yml:
remove the first A-target full-library step and its immediately following first
PR144 upload (16 contiguous lines/952bytes in the reviewed base). Keep the B
full-library consumer and existing second upload BYTE-IDENTICAL, including the
-second artifact name, fresh evidence override, always/missing-file policy and
retention. Do not silently leave the first upload pointing at an absent ledger.
Both original producers, A controller/Store-helper/facts, original first/second
absence checks/exports and B production/recovery bindings remain literal. The
unused first evidence path remains absent; no copy/adoption/fabricated first ledger.

Preserve all Cargo queries, features/profiles/cache, matrix, all other steps and
jobs, every test/thread count/selector/oracle, native35/Unix30 limits and final
binding/copy verifier. No concurrent commands, background service, timeout growth,
warmup, retry or product/fixture/security/dependency change. Native-tail helper
does not consume the removed PR144 ledger or A binary paths: it still requires
original B producer/test_target/binding, fresh tested GLOBAL cold Core and all
B source roles/identities/hashes. Do not relabel BRecovery console provenance.

## Ordered work and concrete Verify

1. Parent records completed evidence/research, this amendment and Implementation;
   posts WHOLE final plan to all4 owners, exact-GET verifies and rereads after
   independent decision review, BEFORE separate source GO.
   **Verify:** SPEC frozen; lost alternate A runtime/first ledger explicit;
   source/env/actual pair and consumer audit complete; no source edit first.
2. Separate development removes ONLY the reviewed two contiguous workflow steps.
   **Verify:** whole-workflow inverse restores exact ea3 bytes; original producers
   and complete B consumer/upload byte-identical; all other step objects, job
   conditions/matrix/time limits/commands/oracles/bounds/roles literally conserved.
3. Parent and independent contention_development review the complete patch.
   **Verify:** actionlint, YAML and all embedded Python syntax, diffcheck, exact
   two-step semantic deletion, original whole-file inverse and clean scoped source;
   no extra mirrored tests or portable inference of native behavior.
4. Publish ONE composed Docs+source head and run ordinary full CI.
   **Verify:** same-head/tree all17 actual jobs PASS within unchanged limits; all3
   original cold Core148, ONE complete B Engine75/Server75/Store134+3ignored and
   ONE PR144195/39cleanup/80-bound-document ledger on each native host (3 ledgers,
   NOT6), with actual original A controller39 and Store26/absent13 binding intact.
   All3 native prune88/FI07/N07/N06/full producer conservation; all24 actual CLI
   executions/new controls; native Engine43/Store42/caller6/output91 with raw final
   copied captures rehashed; complete service/GUI/lifecycle/maintenance/MCP/HTTP;
   original final binding/GLOBAL cold-Core freshness/source fullIDs/all copy roles/
   hashes/manifest and actual native upload must RUN/PASS. All5 Unix suites,
   prune/state/PR144 and other lint/package/coherence jobs stay required.
5. Record exact current-head results in PR/Issues without transferring old passes.
   **Verify:** distinguish executed/failed/incomplete/skipped, any new failure
   returns to research/Docs/Issues BEFORE another change; remaining delivery and
   other Issue162/browser criteria stay open. No unchanged-head rerun, merge,
   release, installed binary replacement, production state or service change.

Root owns planning/review/commits/publication, never implementation. The retirement
is a CI coverage decision, not a claim that the removed physical image was tested
on a future head or that any pending final provenance is already qualified.

## References and retained evidence

- [CI/CD optimization](https://app.obsdog.ai/wiki/?document=doc_01a100b93ce57543afc8e0e282624b26&space=spc_e642627646453ea633d4b0a6da31410c),
  current rev_fde9dd96cde270be7c997dfd18ee8b69; [verification selection](https://app.obsdog.ai/wiki/?document=doc_cbb83df8d13683e772063a2a6b8d356c&space=spc_e642627646453ea633d4b0a6da31410c),
  rev_787d6b89e8a3bf6d544f599c96c06190. Both complete documents were independently
  read; preserve repository-specific core/failures and default one test worker.
- https://doc.rust-lang.org/cargo/commands/cargo-test.html
- https://doc.rust-lang.org/cargo/reference/features.html#feature-unification
- https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepsenv
- https://docs.github.com/en/actions/tutorials/store-and-share-data

Read-only reports: phase6-native-38886903109/A-vs-B-independent-decision-audit.md,
A-vs-B-actual-native-ledger-comparison.json and all-five-Unix/all-six-native-PR144/
all-three-native-prune/native-output-core-libraries receipts; phase7 research
proposal/inverse and guideline audit. These scratch paths are handoff hints, not
repository runtime dependencies. Source/hosted receipts remain authoritative.
