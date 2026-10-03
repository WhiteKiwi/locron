# Locron execution Project

Maintainer execution scope, tasks, verification criteria, evidence and progress live in the
private [Locron GitHub Project](https://github.com/users/WhiteKiwi/projects/4), owned by WhiteKiwi and linked to WhiteKiwi/locron.
The repository is public; access to the Project requires permission. Product requirements,
architecture, implementation rationale and research remain public repository documents.
`TODO.md` is a pointer, not a live checklist. External contributors can report bugs or discuss
proposals through the [public issue tracker](https://github.com/WhiteKiwi/locron/issues).

## Planning and execution

1. Review SPEC and the relevant product/design contracts. Update SPEC before a product scope or
   behavior change; update ARCHITECTURE before changing durable boundaries or invariants.
2. Use a separate research session when questions remain and record evidence in FINDINGS.
3. Review IMPLEMENTATION's approach, edge cases, change order and verification strategy.
4. Create or update Project-only draft tickets for execution scope and phased tasks. Every step
   in a plan with three or more steps needs concrete Verify criteria. Include context/source links,
   the intended result, verification commands or observable checks, and relevant evidence.
5. Review the complete plan before a separate development-session handoff. Before an implementation
   deviation, update/review the applicable design document and Project drafts, then change code.
6. Keep Status and evidence current. Mark Done only after Verify criteria succeed, evidence is
   recorded, and any ticket-required PR merge or publication is complete. When the ticket requests
   an opened PR, that deliverable defines completion. The parent reviews the implementation and
   handles repository-level publication.

Use **Todo** for planned unfinished work, **In Progress** for work being executed, and **Done** for
verified completion. All maintainer execution tickets remain DraftIssue content inside this
Project. Link the Project itself to the repository; do not convert internal drafts to repository
issues. Public contributor bug reports remain valid repository issues, and PRs may refer to them.
When a PR links a private draft, include enough public context and verification for its reviewers.

**Phase** records the work's phase; **Legacy ID** and **Source order** identify migrated tasks.
Existing `LOCRON-TODO-001`–`LOCRON-TODO-069` keys and order values are fixed historical identifiers.
Do not renumber or reuse them for future tasks. New drafts can use Phase and Status without
pretending to belong to this source migration. Draft and Project item Node IDs are different:
content updates use the draft ID, and status/custom-field updates use the Project item ID.
Read IDs and option IDs from the API; do not derive them from names or substitute a legacy key.

The three table views are [Active](https://github.com/users/WhiteKiwi/projects/4/views/1) (`-status:Done`),
[History](https://github.com/users/WhiteKiwi/projects/4/views/2) (`status:Done`), and [All tasks](https://github.com/users/WhiteKiwi/projects/4/views/3) (no filter).
Read current status in these views rather than maintaining another repository checklist.
If you lack Project access, use public requirements/design/history and request maintainer context
through the normal contribution discussion; do not reconstruct a competing execution tracker.
Deferred ideas stay in BACKLOG until selected and planned for execution.

## Migration receipt and preserved source

The 2026-10-02 migration read back all 69 original items as unique, unarchived Project drafts:
68 Done, one Todo, zero In Progress, across 15 phases. The 68 historical Done states preserve
the original completion records; this migration did not rerun their past verification. Exact
source blocks, shared phase context, Verify criteria, evidence and source links were retained. Five display titles use word-boundary
abbreviation to at most 240 characters for readability; complete original wording remains in
their bodies. The only unfinished source item is `LOCRON-TODO-041`, the hosted cache-restoration
verification follow-up. Source configuration alone did not complete that task.

The snapshot [`planning/TODO-2026-10-02.md`](planning/TODO-2026-10-02.md) is byte-identical to
`docs/TODO.md` at commit `93b4144cc5f9d056609e3f88e37e9451ea2ad506`, SHA-256
`aeaf61423326384d4e38709da731e764029777c6bf808c4d06403372b6ee9d52`. Its old live-list language is historical source text.
`TODO-archive.md` preserves the earlier archive, and BACKLOG retains inactive ideas. Repository
issue #4 remained closed with its baseline identity/state/update timestamp unchanged; no repository
issues were created, changed, deleted or converted by this migration.

The following map is a **static migration receipt**, not a second live status tracker. “Migration
status” records acceptance-time state; later task updates belong only to the Project. Item links
use the Project item's full database identifier. Their private destination requires Project access.

### Acceptance startup observer correction before v0.9.6 tagging (2026-09-30)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 1 | [LOCRON-TODO-001](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L12) | [Establish the failing hosted evidence and harness interference scope.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545617) | Done |
| 2 | [LOCRON-TODO-002](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L18) | [Make the acceptance startup observer passive and diagnostic.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545646) | Done |
| 3 | [LOCRON-TODO-003](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L26) | [Pass scoped local and exact-revision hosted verification before tagging.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545690) | Done |

### Second feedback and v0.9.6 signed macOS release (2026-09-30)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 4 | [LOCRON-TODO-004](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L42) | [Review research, host evidence, release preconditions, and the complete plan.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545728) | Done |
| 5 | [LOCRON-TODO-005](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L53) | [Distinguish all human target summaries.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545750) | Done |
| 6 | [LOCRON-TODO-006](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L58) | [Preserve identical macOS registration files during refresh.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545782) | Done |
| 7 | [LOCRON-TODO-007](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L66) | [Add verified signing/notarization and signed-only release inputs.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545814) | Done |
| 8 | [LOCRON-TODO-008](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L84) | [Add the isolated published-old-updater smoke.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545874) | Done |
| 9 | [LOCRON-TODO-009](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L94) | [Prepare v0.9.6 metadata/docs and pass local candidate gates.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545911) | Done |
| 10 | [LOCRON-TODO-010](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L111) | [Publish the reviewed exact revision through a pull request and immutable tag.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545982) | Done |
| 11 | [LOCRON-TODO-011](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L121) | [Verify final distribution, real update compatibility, and host preservation.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546018) | Done |

### v0.9.5 feedback correction release (2026-09-30)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 12 | [LOCRON-TODO-012](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L140) | [Review release scope, preconditions, and publication plan.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546055) | Done |
| 13 | [LOCRON-TODO-013](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L145) | [Prepare consistent v0.9.5 metadata and curated release notes.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546079) | Done |
| 14 | [LOCRON-TODO-014](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L152) | [Pass local candidate and exact-revision hosted release gates.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546124) | Done |
| 15 | [LOCRON-TODO-015](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L169) | [Merge the verified candidate and trigger its immutable release tag.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546147) | Done |
| 16 | [LOCRON-TODO-016](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L176) | [Confirm distribution and old-updater compatibility.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546171) | Done |

### First feedback triage (2026-09-30)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 17 | [LOCRON-TODO-017](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L195) | [Confirm feedback against current code and release artifacts; review the complete plan.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546190) | Done |
| 18 | [LOCRON-TODO-018](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L202) | [Fix checksum parsing and future release checksum filenames.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546218) | Done |
| 19 | [LOCRON-TODO-019](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L212) | [Add advisory process-resolution warnings and the narrow HTTP-selector hint.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546265) | Done |
| 20 | [LOCRON-TODO-020](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L221) | [Expose latest retained run identity/state in list JSON and human table.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546309) | Done |
| 21 | [LOCRON-TODO-021](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L231) | [Complete documentation and regression verification; review the scoped diff.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546342) | Done |

### v0.9.4 security patch release (2026-09-29)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 22 | [LOCRON-TODO-022](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L248) | [Define the release scope and review the distribution preconditions before changing metadata.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546377) | Done |
| 23 | [LOCRON-TODO-023](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L254) | [Prepare the exact v0.9.4 release candidate and curated security note.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546411) | Done |
| 24 | [LOCRON-TODO-024](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L263) | [Pass local release gates and hosted pull-request validation.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546441) | Done |
| 25 | [LOCRON-TODO-025](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L281) | [Merge the verified candidate and create the immutable release tag.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546485) | Done |
| 26 | [LOCRON-TODO-026](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L289) | [Confirm v0.9.4 distribution across all release channels.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546519) | Done |

### Rustls advisory remediation (2026-09-28)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 27 | [LOCRON-TODO-027](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L303) | [Reproduce the failed audit and review the upstream fix boundary and existing product scope.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546545) | Done |
| 28 | [LOCRON-TODO-028](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L309) | [Update the lockfile to the patched Rustls release with no advisory exemption.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546570) | Done |
| 29 | [LOCRON-TODO-029](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L315) | [Verify the corrected graph locally and in hosted pull-request checks.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546595) | Done |

### v0.9.3 patch release (2026-08-28)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 30 | [LOCRON-TODO-030](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L328) | [Prepare the lockstep v0.9.3 workspace version and curated changelog for the completed active run-detail loading and live-follow correction.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546627) | Done |
| 31 | [LOCRON-TODO-031](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L337) | [Run the complete local release-candidate verification without uploading or publishing.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546658) | Done |
| 32 | [LOCRON-TODO-032](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L348) | [Hand the verified release candidate to the parent session for publication.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546695) | Done |

### Active dashboard run detail live following (2026-08-28)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 33 | [LOCRON-TODO-033](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L359) | [Cover automatic active following, terminal no-stream behavior, run and attempt transitions, base64 output, replay deduplication, pause/resume, reconnect feedback, and guarded terminal reconciliation with focused frontend tests.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546729) | Done |
| 34 | [LOCRON-TODO-034](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L371) | [Implement the run-detail live state lifecycle without changing server or durable semantics.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546763) | Done |
| 35 | [LOCRON-TODO-035](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L380) | [Rebuild the committed frontend distribution.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546797) | Done |
| 36 | [LOCRON-TODO-036](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L387) | [Complete proportionate verification and record evidence.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546835) | Done |

### CI toolchain and cache optimization (2026-08-26)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 37 | [LOCRON-TODO-037](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L399) | [Replace the two `clippy::map_unwrap_or` violations with the direct `Result::is_ok_and` predicate without changing service detection behavior.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546861) | Done |
| 38 | [LOCRON-TODO-038](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L406) | [Separate the pinned development/lint toolchain from the package MSRV and document the policy.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546900) | Done |
| 39 | [LOCRON-TODO-039](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L413) | [Prove the correction locally and on hosted runners without adding CI fan-out.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546930) | Done |
| 40 | [LOCRON-TODO-040](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L422) | [Correct toolchain selection and reduce the test/lint matrix from fourteen jobs to nine while preserving stable coverage on all four supported hosts, lint on both operating systems, and one Linux x86_64 Rust 1.94 MSRV gate.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546968) | Done |
| 41 | [LOCRON-TODO-041](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L433) | [Restrict Rust cache creation to the default branch without disabling useful dependency and target restoration.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547006) | Todo |
| 42 | [LOCRON-TODO-042](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L442) | [Complete local and hosted verification and record measured job count, wall time, runner time, toolchain versions, and cache behavior.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547042) | Done |

### Deterministic dashboard port-policy verification (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 43 | [LOCRON-TODO-043](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L454) | [Replace global-default-port conflict tests with test-owned server policy contracts and pure CLI policy-selection coverage while preserving product behavior.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547076) | Done |
| 44 | [LOCRON-TODO-044](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L465) | [Stress the corrected dashboard/server seams under parallel execution.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547117) | Done |
| 45 | [LOCRON-TODO-045](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L471) | [Complete the repository verification gate and inspect the final scoped diff.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547145) | Done |

### README information architecture refresh (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 46 | [LOCRON-TODO-046](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L482) | [Reorganize the README opening and practical workflow so the first screen identifies locron as a local, explainable scheduler for developers and agents on macOS and Linux, while preserving the accepted non-marketing voice and shipped…](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547178) | Done |
| 47 | [LOCRON-TODO-047](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L492) | [Add concise local-failure, human/agent feedback-loop, scheduler-scope, and architecture explanations without overstating sleep detection, exactly-once execution, MCP, or OS-service integration.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547212) | Done |
| 48 | [LOCRON-TODO-048](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L503) | [Preserve and verify installation, dashboard, target/schedule, documentation, contribution, and license guidance after the reorganization.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547249) | Done |

### v0.9.2 patch release (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 49 | [LOCRON-TODO-049](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L513) | [Prepare the lockstep v0.9.2 workspace version and curated changelog entry for the completed lifecycle human-output and stale dashboard-route fixes.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547280) | Done |
| 50 | [LOCRON-TODO-050](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L520) | [Run the release-version contract and focused local release checks on the exact release tree.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547319) | Done |
| 51 | [LOCRON-TODO-051](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L527) | [Commit and push the reviewed release revision, then create and push immutable annotated tag `v0.9.2`.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547345) | Done |
| 52 | [LOCRON-TODO-052](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L534) | [Confirm publication and update this machine's managed installations and services.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547377) | Done |

### Dashboard lifecycle human output and stale detail recovery (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 53 | [LOCRON-TODO-053](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L549) | [Replace raw serialized default output across daemon service, dashboard lifecycle, and successful self-update commands with labeled human reports while preserving every `--json` envelope and token secrecy boundary.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547417) | Done |
| 54 | [LOCRON-TODO-054](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L561) | [Render complete job and run not-found states for stale authenticated detail routes while preserving valid deep links and ordinary non-404 failure feedback.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547454) | Done |
| 55 | [LOCRON-TODO-055](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L569) | [Run the focused and repository-level verification gates and record their evidence here.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547501) | Done |

### v0.9.1 patch release (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 56 | [LOCRON-TODO-056](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L579) | [Prepare the lockstep v0.9.1 workspace version and curated changelog entry for the completed human `history` terminal-width improvement.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547530) | Done |
| 57 | [LOCRON-TODO-057](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L586) | [Run the release-version contract and focused local release checks on the exact release tree.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547573) | Done |
| 58 | [LOCRON-TODO-058](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L592) | [Commit and push the reviewed release revision, then create and push immutable annotated tag `v0.9.1`.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547604) | Done |
| 59 | [LOCRON-TODO-059](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L599) | [Confirm the complete automated publication across crates.io, GitHub Release, and Homebrew.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547626) | Done |

### crates.io source installation and trusted publication (2026-08-25)

| Source order | Legacy source | Draft ticket | Migration status |
| ---: | --- | --- | --- |
| 60 | [LOCRON-TODO-060](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L616) | [Complete and review the product contract, ecosystem research, accepted package graph, installation-ownership boundary, CI/CD design, and this verified checklist before implementation.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547657) | Done |
| 61 | [LOCRON-TODO-061](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L621) | [Rename the user-facing Cargo package to `locron`, centralize the four internal path-plus-exact version dependencies, add complete crates.io metadata/publication restrictions to all five packages, and update every package-name consumer…](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547731) | Done |
| 62 | [LOCRON-TODO-062](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L633) | [Add the standalone installation receipt and require it for `locron self-update`, with Homebrew-, Cargo-, older-installer-, and generic-channel guidance while keeping daemon/dashboard registration available to Cargo installations.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547799) | Done |
| 63 | [LOCRON-TODO-063](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L643) | [Add source-package validation to push/PR CI and a least-privilege, protected-environment crates.io publication gate to the tag workflow using the official OIDC action, exact-version preflight inventory, native ordered workspace…](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547856) | Done |
| 64 | [LOCRON-TODO-064](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L656) | [Update README, INSTALL, RELEASE, CLI/operator guidance, architecture/package naming, release checks, and usage measurement wording for the Cargo channel, source-build requirement, update/removal commands, service behavior, standalone…](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547924) | Done |
| 65 | [LOCRON-TODO-065](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L666) | [Compact the live checklist by moving fully completed sections verbatim to `docs/TODO-archive.md`, archiving completed evidence from mixed sections, and retaining every open item with its context and verification method in this file.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547962) | Done |
| 66 | [LOCRON-TODO-066](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L676) | [Run the complete local gate and source-package rehearsal without uploading to crates.io.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548019) | Done |
| 67 | [LOCRON-TODO-067](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L686) | [At the first crates.io-backed release, bootstrap all five packages manually from the exact clean release commit with a narrow temporary token, revoke it, configure each trusted publisher for the `crates-io` environment, then push the…](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548068) | Done |
| 68 | [LOCRON-TODO-068](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L699) | [Fix the hosted source-package archive assertion to inspect the already materialized server package listing instead of piping `tar -tf` into `grep -q` under `pipefail`.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548091) | Done |
| 69 | [LOCRON-TODO-069](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L705) | [Confirm the complete hosted CI workflow on the corrective commit reports the source-package job and every other job green.](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548124) | Done |
