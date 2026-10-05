# Locron execution Issues

Maintainer execution scope, tasks, Verify criteria, evidence and progress live in
[WhiteKiwi/locron Issues](https://github.com/WhiteKiwi/locron/issues), alongside contributor bug
reports and proposal discussions. Product/design contracts remain repository documents.
`TODO.md` is a pointer; the static migration map below records historical cutover state.

## Planning and execution

1. Review SPEC and the relevant product/design contracts. Update SPEC before changing product
   scope or behavior; update ARCHITECTURE before changing a durable boundary or invariant.
2. Resolve open questions in a separate research session and record evidence in FINDINGS.
3. Update IMPLEMENTATION with the approach, edge cases, change order and verification strategy.
4. Review the issue inventory and continue a matching task before creating another. Record scope,
   phase, dependencies and a concrete `Verify` criterion for every step in a plan with three or
   more steps. Include the intended observable outcome and exact commands or evidence needed.
5. Review the complete plan, then hand implementation to a separate development sub-session.
   Before an implementation deviation, update the applicable planning document and issue first.
   The parent reviews changes and handles repository publication.
6. Keep progress and exact-revision evidence in the issue body or comments. Close the task as
   completed only after its Verify criteria and every required PR merge/publication succeed.
   An opened PR completes a task only when that is its requested deliverable. Partial
   implementation keeps remaining native acceptance or release work open.

Maintainer tasks use the `task` label. An open task with `status: todo` is planned; an open task
with `status: in progress` is active. A completed task is closed with the completed reason.
Keep these labels and issue state consistent with recorded evidence; a label change is not proof
that verification passed. Read current progress in the
[task inventory](https://github.com/WhiteKiwi/locron/issues?q=is%3Aissue+label%3Atask).
Deferred ideas stay in BACKLOG until selected and planned.

Preserve migrated Phase, original Status, Legacy ID, source order, dependencies, PR links and
historical evidence. Existing `LOCRON-TODO-001`–`LOCRON-TODO-069` identities remain fixed; new tasks
do not reuse those legacy keys. The migration footer supersedes private-Project instructions in
otherwise verbatim historical issue bodies. Completed source work retains its completion history;
migration does not claim that its checks were rerun.

## 2026-10-03 static migration receipt

The exhausted Project source contains 83 unique unarchived tasks: 70 Done, eleven In Progress and
two Todo. Thirteen matching Windows tasks reuse existing issues #24–#36; each preserves its prior
public body in an explicitly historical comment. The other seventy tasks use native draft-to-issue
conversion, retaining their original Project item identities. Source scope, titles, checklist
state, Verify criteria and evidence are preserved in each destination issue.

CLI readback confirmed all 83 source bodies, metadata and destination states before the Project
was closed on 2026-10-03. The Project remains private with the same 83 item identities and fields:
seventy converted Issue items and thirteen retained historical Windows drafts. Its earlier README
is preserved in historical details. The closure retires live tracking without deleting history.

The following map records migration-time state: 70 closed/completed histories and thirteen open
tasks, comprising eleven active and two planned. It is static evidence; subsequent status changes
belong to Issues. Existing issue #4, Windows umbrella #23 and deferred signing proposal #37 are
separate records. Administrative migration creates no platform-support or release claim.

The earlier Project workflow and original legacy links are preserved in
[`planning/PROJECTS-2026-10-03.md`](planning/PROJECTS-2026-10-03.md), Git blob
`1665baa51c4baee912a211e55934d3531f54cab5`, SHA-256
`f93b4bebbbaefe31fdeccc18d515c51ffc33d7d419da9c3885b22dd15d575af9`.
The original TODO snapshot and completed archive remain unchanged; see [`PROJECTS.md`](PROJECTS.md).
Original Project links below remain historical private references; public issue bodies preserve
the complete execution context.

| Original source | Original Project item | Repository issue | Migration status |
| --- | --- | --- | --- |
| [LOCRON-TODO-001](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L12) | [item 260545617](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545617) | [#45](https://github.com/WhiteKiwi/locron/issues/45) | Done |
| [LOCRON-TODO-002](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L18) | [item 260545646](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545646) | [#46](https://github.com/WhiteKiwi/locron/issues/46) | Done |
| [LOCRON-TODO-003](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L26) | [item 260545690](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545690) | [#47](https://github.com/WhiteKiwi/locron/issues/47) | Done |
| [LOCRON-TODO-004](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L42) | [item 260545728](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545728) | [#48](https://github.com/WhiteKiwi/locron/issues/48) | Done |
| [LOCRON-TODO-005](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L53) | [item 260545750](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545750) | [#49](https://github.com/WhiteKiwi/locron/issues/49) | Done |
| [LOCRON-TODO-006](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L58) | [item 260545782](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545782) | [#50](https://github.com/WhiteKiwi/locron/issues/50) | Done |
| [LOCRON-TODO-007](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L66) | [item 260545814](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545814) | [#51](https://github.com/WhiteKiwi/locron/issues/51) | Done |
| [LOCRON-TODO-008](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L84) | [item 260545874](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545874) | [#52](https://github.com/WhiteKiwi/locron/issues/52) | Done |
| [LOCRON-TODO-009](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L94) | [item 260545911](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545911) | [#53](https://github.com/WhiteKiwi/locron/issues/53) | Done |
| [LOCRON-TODO-010](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L111) | [item 260545982](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260545982) | [#54](https://github.com/WhiteKiwi/locron/issues/54) | Done |
| [LOCRON-TODO-011](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L121) | [item 260546018](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546018) | [#55](https://github.com/WhiteKiwi/locron/issues/55) | Done |
| [LOCRON-TODO-012](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L140) | [item 260546055](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546055) | [#56](https://github.com/WhiteKiwi/locron/issues/56) | Done |
| [LOCRON-TODO-013](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L145) | [item 260546079](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546079) | [#57](https://github.com/WhiteKiwi/locron/issues/57) | Done |
| [LOCRON-TODO-014](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L152) | [item 260546124](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546124) | [#58](https://github.com/WhiteKiwi/locron/issues/58) | Done |
| [LOCRON-TODO-015](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L169) | [item 260546147](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546147) | [#59](https://github.com/WhiteKiwi/locron/issues/59) | Done |
| [LOCRON-TODO-016](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L176) | [item 260546171](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546171) | [#60](https://github.com/WhiteKiwi/locron/issues/60) | Done |
| [LOCRON-TODO-017](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L195) | [item 260546190](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546190) | [#61](https://github.com/WhiteKiwi/locron/issues/61) | Done |
| [LOCRON-TODO-018](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L202) | [item 260546218](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546218) | [#62](https://github.com/WhiteKiwi/locron/issues/62) | Done |
| [LOCRON-TODO-019](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L212) | [item 260546265](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546265) | [#63](https://github.com/WhiteKiwi/locron/issues/63) | Done |
| [LOCRON-TODO-020](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L221) | [item 260546309](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546309) | [#64](https://github.com/WhiteKiwi/locron/issues/64) | Done |
| [LOCRON-TODO-021](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L231) | [item 260546342](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546342) | [#65](https://github.com/WhiteKiwi/locron/issues/65) | Done |
| [LOCRON-TODO-022](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L248) | [item 260546377](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546377) | [#66](https://github.com/WhiteKiwi/locron/issues/66) | Done |
| [LOCRON-TODO-023](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L254) | [item 260546411](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546411) | [#67](https://github.com/WhiteKiwi/locron/issues/67) | Done |
| [LOCRON-TODO-024](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L263) | [item 260546441](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546441) | [#68](https://github.com/WhiteKiwi/locron/issues/68) | Done |
| [LOCRON-TODO-025](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L281) | [item 260546485](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546485) | [#69](https://github.com/WhiteKiwi/locron/issues/69) | Done |
| [LOCRON-TODO-026](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L289) | [item 260546519](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546519) | [#70](https://github.com/WhiteKiwi/locron/issues/70) | Done |
| [LOCRON-TODO-027](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L303) | [item 260546545](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546545) | [#71](https://github.com/WhiteKiwi/locron/issues/71) | Done |
| [LOCRON-TODO-028](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L309) | [item 260546570](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546570) | [#72](https://github.com/WhiteKiwi/locron/issues/72) | Done |
| [LOCRON-TODO-029](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L315) | [item 260546595](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546595) | [#73](https://github.com/WhiteKiwi/locron/issues/73) | Done |
| [LOCRON-TODO-030](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L328) | [item 260546627](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546627) | [#74](https://github.com/WhiteKiwi/locron/issues/74) | Done |
| [LOCRON-TODO-031](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L337) | [item 260546658](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546658) | [#75](https://github.com/WhiteKiwi/locron/issues/75) | Done |
| [LOCRON-TODO-032](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L348) | [item 260546695](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546695) | [#76](https://github.com/WhiteKiwi/locron/issues/76) | Done |
| [LOCRON-TODO-033](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L359) | [item 260546729](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546729) | [#77](https://github.com/WhiteKiwi/locron/issues/77) | Done |
| [LOCRON-TODO-034](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L371) | [item 260546763](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546763) | [#78](https://github.com/WhiteKiwi/locron/issues/78) | Done |
| [LOCRON-TODO-035](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L380) | [item 260546797](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546797) | [#79](https://github.com/WhiteKiwi/locron/issues/79) | Done |
| [LOCRON-TODO-036](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L387) | [item 260546835](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546835) | [#80](https://github.com/WhiteKiwi/locron/issues/80) | Done |
| [LOCRON-TODO-037](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L399) | [item 260546861](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546861) | [#81](https://github.com/WhiteKiwi/locron/issues/81) | Done |
| [LOCRON-TODO-038](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L406) | [item 260546900](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546900) | [#82](https://github.com/WhiteKiwi/locron/issues/82) | Done |
| [LOCRON-TODO-039](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L413) | [item 260546930](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546930) | [#83](https://github.com/WhiteKiwi/locron/issues/83) | Done |
| [LOCRON-TODO-040](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L422) | [item 260546968](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260546968) | [#84](https://github.com/WhiteKiwi/locron/issues/84) | Done |
| [LOCRON-TODO-041](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L433) | [item 260547006](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547006) | [#85](https://github.com/WhiteKiwi/locron/issues/85) | Todo |
| [LOCRON-TODO-042](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L442) | [item 260547042](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547042) | [#86](https://github.com/WhiteKiwi/locron/issues/86) | Done |
| [LOCRON-TODO-043](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L454) | [item 260547076](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547076) | [#87](https://github.com/WhiteKiwi/locron/issues/87) | Done |
| [LOCRON-TODO-044](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L465) | [item 260547117](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547117) | [#88](https://github.com/WhiteKiwi/locron/issues/88) | Done |
| [LOCRON-TODO-045](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L471) | [item 260547145](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547145) | [#89](https://github.com/WhiteKiwi/locron/issues/89) | Done |
| [LOCRON-TODO-046](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L482) | [item 260547178](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547178) | [#90](https://github.com/WhiteKiwi/locron/issues/90) | Done |
| [LOCRON-TODO-047](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L492) | [item 260547212](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547212) | [#91](https://github.com/WhiteKiwi/locron/issues/91) | Done |
| [LOCRON-TODO-048](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L503) | [item 260547249](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547249) | [#92](https://github.com/WhiteKiwi/locron/issues/92) | Done |
| [LOCRON-TODO-049](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L513) | [item 260547280](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547280) | [#93](https://github.com/WhiteKiwi/locron/issues/93) | Done |
| [LOCRON-TODO-050](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L520) | [item 260547319](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547319) | [#94](https://github.com/WhiteKiwi/locron/issues/94) | Done |
| [LOCRON-TODO-051](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L527) | [item 260547345](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547345) | [#95](https://github.com/WhiteKiwi/locron/issues/95) | Done |
| [LOCRON-TODO-052](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L534) | [item 260547377](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547377) | [#96](https://github.com/WhiteKiwi/locron/issues/96) | Done |
| [LOCRON-TODO-053](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L549) | [item 260547417](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547417) | [#97](https://github.com/WhiteKiwi/locron/issues/97) | Done |
| [LOCRON-TODO-054](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L561) | [item 260547454](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547454) | [#98](https://github.com/WhiteKiwi/locron/issues/98) | Done |
| [LOCRON-TODO-055](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L569) | [item 260547501](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547501) | [#99](https://github.com/WhiteKiwi/locron/issues/99) | Done |
| [LOCRON-TODO-056](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L579) | [item 260547530](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547530) | [#100](https://github.com/WhiteKiwi/locron/issues/100) | Done |
| [LOCRON-TODO-057](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L586) | [item 260547573](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547573) | [#101](https://github.com/WhiteKiwi/locron/issues/101) | Done |
| [LOCRON-TODO-058](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L592) | [item 260547604](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547604) | [#102](https://github.com/WhiteKiwi/locron/issues/102) | Done |
| [LOCRON-TODO-059](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L599) | [item 260547626](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547626) | [#103](https://github.com/WhiteKiwi/locron/issues/103) | Done |
| [LOCRON-TODO-060](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L616) | [item 260547657](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547657) | [#104](https://github.com/WhiteKiwi/locron/issues/104) | Done |
| [LOCRON-TODO-061](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L621) | [item 260547731](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547731) | [#105](https://github.com/WhiteKiwi/locron/issues/105) | Done |
| [LOCRON-TODO-062](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L633) | [item 260547799](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547799) | [#106](https://github.com/WhiteKiwi/locron/issues/106) | Done |
| [LOCRON-TODO-063](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L643) | [item 260547856](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547856) | [#107](https://github.com/WhiteKiwi/locron/issues/107) | Done |
| [LOCRON-TODO-064](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L656) | [item 260547924](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547924) | [#108](https://github.com/WhiteKiwi/locron/issues/108) | Done |
| [LOCRON-TODO-065](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L666) | [item 260547962](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260547962) | [#109](https://github.com/WhiteKiwi/locron/issues/109) | Done |
| [LOCRON-TODO-066](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L676) | [item 260548019](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548019) | [#110](https://github.com/WhiteKiwi/locron/issues/110) | Done |
| [LOCRON-TODO-067](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L686) | [item 260548068](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548068) | [#111](https://github.com/WhiteKiwi/locron/issues/111) | Done |
| [LOCRON-TODO-068](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L699) | [item 260548091](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548091) | [#112](https://github.com/WhiteKiwi/locron/issues/112) | Done |
| [LOCRON-TODO-069](https://github.com/WhiteKiwi/locron/blob/93b4144cc5f9d056609e3f88e37e9451ea2ad506/docs/TODO.md#L705) | [item 260548124](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260548124) | [#113](https://github.com/WhiteKiwi/locron/issues/113) | Done |
| [locron/windows-2026-10-02/24](https://github.com/WhiteKiwi/locron/issues/24) | [item 260621689](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621689) | [#24](https://github.com/WhiteKiwi/locron/issues/24) | Done |
| [locron/windows-2026-10-02/25](https://github.com/WhiteKiwi/locron/issues/25) | [item 260621724](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621724) | [#25](https://github.com/WhiteKiwi/locron/issues/25) | In Progress |
| [locron/windows-2026-10-02/26](https://github.com/WhiteKiwi/locron/issues/26) | [item 260621753](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621753) | [#26](https://github.com/WhiteKiwi/locron/issues/26) | In Progress |
| [locron/windows-2026-10-02/27](https://github.com/WhiteKiwi/locron/issues/27) | [item 260621783](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621783) | [#27](https://github.com/WhiteKiwi/locron/issues/27) | In Progress |
| [locron/windows-2026-10-02/28](https://github.com/WhiteKiwi/locron/issues/28) | [item 260621801](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621801) | [#28](https://github.com/WhiteKiwi/locron/issues/28) | In Progress |
| [locron/windows-2026-10-02/29](https://github.com/WhiteKiwi/locron/issues/29) | [item 260621817](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621817) | [#29](https://github.com/WhiteKiwi/locron/issues/29) | In Progress |
| [locron/windows-2026-10-02/30](https://github.com/WhiteKiwi/locron/issues/30) | [item 260621854](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621854) | [#30](https://github.com/WhiteKiwi/locron/issues/30) | In Progress |
| [locron/windows-2026-10-02/31](https://github.com/WhiteKiwi/locron/issues/31) | [item 260621868](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621868) | [#31](https://github.com/WhiteKiwi/locron/issues/31) | In Progress |
| [locron/windows-2026-10-02/32](https://github.com/WhiteKiwi/locron/issues/32) | [item 260621902](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621902) | [#32](https://github.com/WhiteKiwi/locron/issues/32) | In Progress |
| [locron/windows-2026-10-02/33](https://github.com/WhiteKiwi/locron/issues/33) | [item 260621938](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621938) | [#33](https://github.com/WhiteKiwi/locron/issues/33) | In Progress |
| [locron/windows-2026-10-02/34](https://github.com/WhiteKiwi/locron/issues/34) | [item 260621974](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260621974) | [#34](https://github.com/WhiteKiwi/locron/issues/34) | In Progress |
| [locron/windows-2026-10-02/35](https://github.com/WhiteKiwi/locron/issues/35) | [item 260622009](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260622009) | [#35](https://github.com/WhiteKiwi/locron/issues/35) | In Progress |
| [locron/windows-2026-10-02/36](https://github.com/WhiteKiwi/locron/issues/36) | [item 260622031](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260622031) | [#36](https://github.com/WhiteKiwi/locron/issues/36) | Todo |
| [pnpm integration (PR38)](https://github.com/WhiteKiwi/locron/pull/38) | [item 260689470](https://github.com/users/WhiteKiwi/projects/4?pane=issue&itemId=260689470) | [#114](https://github.com/WhiteKiwi/locron/issues/114) | Done |

## PR136 reviewed observation continuation (2026-10-04)

Issue31 remains open:7343/run37196440237 has one actual ARM History expiry and unknown cleanup completion. The helper-only current-call observation continuation is documented before Source; it does not close Windows native acceptance or change the original clocks/oracles. planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md carries the four-step Verify handoff and immutable raw/proposal hashes.


## Current PR136 observer and concurrent publication integration

Root selects the exact disjoint observer4639 and concurrent6e98 contributions in [the completed integration plan](planning/WINDOWS_NATIVE_OBSERVER_CONCURRENT_2026-10-04.md). Preserve current main9de and all original gates; actual new observer qualification is pending and the concurrent x64 ChildExited/Run failure remains unexplained.

## Native SID common repair Verify handoff (2026-10-05)

Existing privacy#27, CI#31 and coordination#163 own the four ordered concrete Verify in
planning/WINDOWS_NATIVE_TOKEN_SID_2026-10-05.md. Root whole-plan exact issue GET ALL and
final committed-plan/new-Docs reread precede separate two-file implementation. Preserve
original issue bodies/history and every native clock, guard, creation, counter-stop and
cleanup requirement. Common main-based publication and each representative's qualification
are distinct phases; source/static/API success cannot close broad Windows acceptance.

The same plan's before-Source scalar permit-return refinement preserves on-time once-sharing.
All three owning exact amended-plan GETs and Root whole final reread precede the paused
developer; no Source draft was applied before this corrected ownership selection.

### Native SID cache Verify precision (2026-10-06)

For #27/#31/#163, no-late-cache Verify concerns an initially empty cache before verified
publication. Preserve refusal without clearing a prior verified entry, and report the
existing post-publication/outer-return expiry limit without promising rollback or atomic
clock/publication. The complete precise plan and owning exact readbacks/final reread
precede the Source commit; all genuine hosted identity/ownership/native gates remain open.

### Native SID hosted lint Verify handoff (2026-10-06)

#27/#31/#163 continue the same common repair: b4c422's actual Windows1.98 lint is FAIL,
not completed native acceptance. The complete plan now selects one explicit scalar
Receiver drop while preserving the owned lifetime, early-error RAII, SAME returned permit,
all original gates/clocks. Actual all-three Core136P/1F additionally requires the existing
sid-deadline coupling repair: native API first, explicit genuine stock SID under that SAME
deadline, then original elapsed/PID/marker. All six new SID selectors passed, but the cold
gate failed and CLI downstream/Cancel acceptance is unrun. Exact whole-plan POST/GET and
Root whole committed-plan/new-Docs reread precede all selected scoped corrections; full
selected-region inverses and fresh lint/native Verify follow. No closure or prior
PASS transfer is implied; all other Source and filesystem-worker bytes remain unchanged.
The completed run has13 ordinary metadataPASS/4FAIL/2optionalSKIP; package/coherence
metadata lacks Root ZIP proof and original8x3 CLI/Wake/Cancel acceptance is unrun.

The complete amended plan also covers the unrun parent-exit-host's direct native-SID-to-
stock-PID coupling. Capture original30s at fixed-thread entry, native bounded API/grammar
first, same-deadline genuine stock SID/Boolean identity, then old positive PID/publication;
no SID output or deadline reset. Generic deadline/Child/Job/handles/heartbeat/capture/
disposal/guard-release assertions and45/30/3s stay exact. This latest amendment supersedes
the three-file boundary: cumulative four paths, follow-up three, filesystem_worker.rs
unchanged. Verify actual316-entry Docs parent and313 protected modes/blobs, all original
selectors and three full-file inverses. Fresh hosted parent-exit remains required; an
unrun gate is not PASS. The same owning whole-plan POST/GET/final reread precedes Source.

### Native SID lib-test syntax Verify (2026-10-06)

Continue #27/#31/#163 under the complete latest plan: actual d89cd3e1 Windows lib-test lint
has two errors, with native/package jobs pending at snapshot2. Select only integer
assert_eq! for zero PID and the test-helper's single-input lifetime elision; preserve the
returned SAME borrowed permit/body, all original clocks/owners/positive PID/privacy and
parent-exit controls. Complete owning whole-plan POST/GET and Root actual final reread
precede Source; two full-file inverses and314 protected modes/blobs of316 precede fresh
lint/native/downstream qualification. No allowances/new tests/skips/old PASS transfer or
issue closure is implied. Other Source, including fixed worker/crash host, stays d89cd3e1.
Root's later metadata-only snapshot3 ARM cancellation step failure is cause UNKNOWN;
completed raw native counts/Cancel outcomes remain unqualified. This syntax selection
makes no runtime repair or all-gates-PASS claim.

### Joined native SID/Group D Verify order (2026-10-06)

Existing privacy#27, CI#31 and integration#163 own the complete latest
WINDOWS_NATIVE_TOKEN_SID_2026-10-05.md ordering amendment. Exact whole-plan POST/GET ALL
and Root actual whole committed-plan/new-Docs reread precede the normal exact reviewed
D17133 merge into the common line. No new issue, public API, diagnostic seam, test or
clock/security/oracle change is selected; other guard groups remain later.

Preserve actual d89 Core137/new six/stock-parent proofs and23PASS/1FAIL original CLI
matrix: ARM ChildExited/Run then cleanup application5 has no captured daemon cause;
its16/4/3/11/4/21 downstreams are skipped. Original first work/private owners and the
already reviewed D bounded closed capture remain exact. d89 package168 each/provenance/
Guardian cannot transfer. Root qualifies one genuinely joined changed head with full
required actual native/lint/downstream/package evidence and source/ordered-tree binding.
Keep166/167 open until required Verify plus actual main contribution prove verified
subsumption; no ancestry-only closure. Broader privacy/CI/integration/release/account/
logon/reboot/signing criteria remain at their actual state. Five concrete Verify steps
in the complete plan govern this integration and publication handoff.


## Group D representative166 integration and private observation (2026-10-05)

Group D continues existing35/31/coordination163 with five explicit Verify steps in planning/GROUP_D_WINGET_NATIVE_INTEGRATION_2026-10-05.md. Integration, private observation and hosted acceptance are separate phases. Preserve broader release/WinGet/Windows requirements and actual unknown causes; no premature issue/member closure.


### PR166 current-main output-recovery continuation (2026-10-05)

Issue35 owns the exact b5f/f90 Git-only integration and current-head Verify in planning/WINGET_OUTPUT_MAIN_F90_INTEGRATION_2026-10-05.md. Preserve old Source and actual evidence separately, record exact plan GET and final reread before the separate developer, and keep native/public-release/installed-channel acceptance open. Signing37 remains deferred; Project-only historical instructions stay superseded.


## Issue31 producer/attempt observation continuation, 2026-10-05

Issue31 remains active/open after e9d MSRV Wake and1f5 ARM Cancel failures. Actual HTTP235 PASS belongs to Issue143/1f5, not native completion. Follow the four concrete Verify in planning/WINDOWS_PRODUCER_ATTEMPT_DIAGNOSTICS_2026-10-05.md; Root committed plan/exact Issue31 GET/final reread must precede separate five-path Source. Record actual changed-head controls/records and unknowns in Issue31. Signing37 and broad public release/catalog/installation/account/task/logon/reboot remain separately open.


## Windows producer Run stderr boundary (2026-10-05)

Issue31 / draftPR167 owns the before-Source narrow initial Run stderr refinement. Preserve the four incomplete Source drafts during this Docs-only amendment. Record the complete amended plan and its Verify, exact Issue GET/body/state readback, then Root whole final committed-plan reread before the separate developer resumes. Native delivery/types/control fit and first work/cleanup acceptance remain pending.


## Issue31 hosted producer lint continuation, 2026-10-05

Issue31 and draftPR167 remain open. Record the complete four-step Verify from planning/WINDOWS_PRODUCER_HOSTED_LINT_2026-10-05.md with exact comment GET and unchanged original Issue state/body; parent whole final committed-plan reread then separate development. Only two Source paths/four literal syntax sites are selected. Current71a10 actual queued/cancellation controls PASSall3, old x64/ARM Wake and Windows lint FAIL; changed-head acceptance is pending. Preserve signing37 deferred and separate release/account/task/logon/reboot/WinGet work.


## Before-Source producer lint formatter reconciliation, 2026-10-05

Scoped Rustfmt1.94 returned0 but expanded only the two new iterator heads by22 bytes/three lines each. Separate development paused before staging/commit and returned its mutation lease. Root preserved both dirty Source drafts, reviewed both complete drift patches and independently inverted both wraps plus the complete research change. The whole WINDOWS_PRODUCER_HOSTED_LINT_2026-10-05.md now selects those exact formatted19554/f25da689 and267889/0b0c2ebc bytes before separate continuation, with unchanged four concrete Verify/grammar/owners/clocks, exact Issue31 GET and Root whole final amended-plan reread. Root changes only Docs; no native/type/cause PASS or workflow/assertion repair is implied.


## Group D strict helper lint Verify continuation — 2026-10-05

Owning35/31 and coordination163 retain native/publication acceptance, including unresolved actual22 ARM Wake. planning/GROUP_D_WINDOWS_HELPER_STRICT_LINT_2026-10-05.md selects only the closed21 private lint repairs after Docs/owning readback. Verify unchanged Source/old plans/full inverses and all owning exact GETs before Root entire final reread; then separate helper-only clean development, full Root review and genuine changed-head required lint/native/package/paired evidence. Positive producer scalars or static proofs do not qualify failed native selectors or unknown allocation/fault classes.

## Group D cancellation initialization Verify continuation — 2026-10-05

Continue owning35/31 and coordination163 with the four concrete Verify steps in planning/GROUP_D_CANCEL_INITIALIZATION_WITNESS_2026-10-05.md. Actual9078 lint/paired success and3 distinct native failures remain revision-bound. Verify whole-plan exact owning GET ALL and Root final complete new-plan/Docs review before one-call separate Source; then clean inverse/protected/clock/selector evidence, Root ordinary publication and all exact changed-head native/producer/paired/required gates. Both cancellation owners must obtain genuine same-daemon initialization witness before first Add and retain every original deadline and state/cleanup assertion. No issue/member closure or wider acceptance follows from static proof or this stronger private readiness gate.

## Group D role-before-probe Verify continuation — 2026-10-05

Owning35/31 and coordination163 continue the four concrete Verify steps in planning/GROUP_D_ROLE_BEFORE_INITIAL_PROBE_2026-10-05.md. Exact full-plan owning GET ALL and Root entire final new-plan/Docs review precede helper-only separate Source. Preserve one-call inverse, protected bytes, actual PID/Held/metadata owner, original 8s/200ms/30s and every selector/capture/cleanup assertion. All current-head ordinary/native/GitGuardian/fullpaired gates remain required; role ownership is not Store/bind readiness and unknown native/raw32/capture causes remain failed/open. Latest user direction defers141/Group A; eligible remaining groups proceed against current main with full member contribution verification.

## Group D first-signal Verify continuation — 2026-10-05

Owning35/31 and coordination163 continue four concrete Verify steps in planning/GROUP_D_FIRST_CANCELLATION_SIGNAL_2026-10-05.md: exact entire plan POST/GET ALL then Root final full reread; separate helper-only metadata with full inverse/protected/clock/grammar evidence; independent current-head publication/ordered synthetic tree; all17 ordinary/native/GitGuardian/complete paired gates. Daemon-local first signal is an observation, not native termination or completed acceptance. Actual MSRV Queued, separate late-positive capture refusal and all native failures remain open. #141 is deferred; other eligible groups proceed independently against fresh main.


## c39 test-only observation and independent A/B Verify (2026-10-06)

Continue existing privacy27/CI31/integration163 with the complete amended native SID plan and its five concrete Verify steps. Root commits four Docs, publishes whole-plan exact POST/GET ALL with original Issue body/state preserved, then actually rereads the full final committed plan before separate helper-only Source. Actual c39 original controls22PASS/2FAIL and MSRV downstream skip remain failed/unqualified; no causal runtime repair or completed Windows/support claim. First-only matched cleanup facts and complete first positive scalar retention do not waive the original5s/8s/200ms/30s/25ms/5sgrace or CLEANED assertions.

Permit independent DIAGNOSTIC exact-c39+A d24a2a65c05ef52eea644723d8f8ea5a11c0286a/B b6c9d1ecbf04db7e4dfd5b71a26de48d501ea4a0 qualification before common main merge, with Docs-only descendants and each full own required Verify. Root retains commits/API/publication/memory. Preserve failed/current-head evidence, all existing oracles/main-inclusion/member-contribution and closure gates; no Source conflict decision, new diagnostics import, old PASS transfer or ancestry-only closure. C/E stay later and wider release/account/logon/reboot work remains open. This overrides only the earlier A/B schedule, not acceptance or security.


## Native3905 hosted lint-only Verify continuation (2026-10-06)

Continue privacy27/CI31/distribution35/integration163 and PR171/166/167 with the complete native SID plan's three newest Verify steps. Root commits only four prefix-preserving Docs and obtains exact WHOLE-plan POST/GET ALL with original bodies/state/history retained, then actually rereads the entire final committed plan/new blocks before separate helper-only Source. Select only patch73c6e1e's six equivalent Boolean/cleanup-claim spans; full inverse/protected tree and pinned static checks precede Root review/publication and fresh required strict-lint/all-three native plus complete member/main contribution.

Actual3905 ARM Wake remains Expired/History despite later same-owner Drop27331us, child-root absent and post-gated CLEANED; RootReturn is observed before its unchanged post-gate. Current x64 foundations and genuine queued/cancel positives do not waive that failure, lint failure or pending/cancelled/skipped/unrun gates. No owner-PC runtime, Source-first reconciliation, unchanged-head rerun, allowance, clock growth, production repair or ancestry-only PR/Issue closure. Wider privacy/Windows/release/account/logon/reboot scope remains at its actual acceptance.
