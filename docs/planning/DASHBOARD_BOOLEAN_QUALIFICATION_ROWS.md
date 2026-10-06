# PR144 행별 qualification ledger — 193 portable + 2 native Windows

대상은 PR144 exact Source `a5e79b14cbae1375f9726fcb3563b3a5aa55fe1b`이다. 모든 행은 **UNMEASURED**이며 Root의 Docs/Issue 선택 전 proposal이다. 193은 parser156 + runtime37이고 Windows에는 native2를 추가하여195가 된다. 행/제품 case의 수이며 함수·assertion 수 또는 PASS 수가 아니다.

기존 JSON/QUERY/PREVIEW/LIVE/ENV-REFUSE/BODY-REFUSE/WIN-ENV key를 유지했다. 소실된 private combined report를 byte-identical 복원했다고 주장하지 않는다. PR145는 별도 범위다.

## 공통 oracle와 clock

- **J**: 실제 complete request struct를 serde_json으로 역직렬화; dry_run의 exact bool/actual reject 및 다른 필드 기본값 확인. helper의 복사 구현은 금지.
- **Q**: 기존 axum::extract::Query::<actual query struct>::try_from_uri를 호출하여 내부 actual serde_urlencoded 경로로 역직렬화; kebab-case field와 exact bool/actual reject 및 companion 기본값 확인. 새 serde_urlencoded dependency를 추가하지 않는다. route 실행이나 durable 효과의 증거로 올리지 않음.
- **O**: 실제 mkdtemp 자식 private root/부모/leaf no-follow identity ledger. Windows existing_private admission, ownership/ACL을 고치지 않고 검증. 모든 child/task/socket의 종료를 확인한 뒤 identity-guarded cleanup.
- **L0**: 같은 후보 revision의 Store::open_read_only connection에서 한 read transaction으로 main schema와 모든 typed row를 before/after 수집. commit된 logical state 및 schema/sequence/timestamps exact equality.
- **L+**: route별 필수 durable delta를 독립 oracle로 확인; generated UUID/time은 형식·요청 시각 bounds·필수 관계 검증. 모든 비대상 row/table/schema/sequence exact equality.
- **A0**: DB/state.db-wal/state.db-shm/state.db-journal 실제 absence를 handler 완료 직후, 어떤 live Store/GET/jobs보다 먼저 확인. root 자체 absence를 요구하지 않음.
- **F**: SQLite 파일은 단계별 inventory/identity/size/hash 진단만. SHM/readmark와 SQLite-owned sidecar lifecycle 변화는 자동 실패가 아님. 비SQLite sentinel/token/output regular files는 no-follow identity+bytes exact, socket은 typed identity.
- **W0**: per-owned-root cfg(test) API send_wake observer에 actual call attempt=0. 준비 단계의 실제 endpoint positive+drain 뒤 measurement window에 수신=0. delegate는 원래 real core send_wake 그대로.
- **W+**: per-root actual API wake attempt=1, delegated real send_wake result=Ok, 독립 Unix datagram 또는 Windows secured engine listener의 실제 delivery 확인. daemon 완료로 해석하지 않음.
- **E**: 실제 loopback HTTP+valid token Authorization+JSON content-type. OK envelope schema/ok/data/warnings와 independent expected data/action/redaction. nonce canary plaintext가 public response/log summary에 없어야 함.
- **R400**: 실제 400 envelope schema locron.api/v1, ok=false, error.code=invalid_request, Source-backed message. whole logical state/wake는 별도 확인.
- **R422**: actual Axum 0.8.9 JsonDataError 422. framework rejection body를 bounded 관측; Locron API JSON envelope를 강요하지 않음. JSON body에 malformed syntax를 섞지 않음.
- **N**: 실제 native Windows host+candidate source/compiler/artifact pin+private root/file ownership. Windows 환경명 canonical equivalence/reserved casing을 actual caller로 검증. Unix PASS로 대체 금지.

- **P clock**: 동기 serde156행, 새 hosted parser-phase absolute30s fail guard 선택안. I/O/runtime proof가 아니다.
- **H clock**: owned row 첫 preparation부터 absolute90s, ready≤30s, 새 operations<87s, cleanup≤원래90s 안의 마지막3s. runtime phase10min fail guard 선택안. 기존 생산 timeout/CI30·35min은 그대로다. Source/native가 이 전체 bound를 자동 보장한다고 주장하지 않는다.
- 응답만 도착하거나 outer timeout이 끝난 것을 spawned blocking worker/child의 종료로 간주하지 않는다. join/reap/observer complete 전에 행을 완료하지 않는다. 끝나지 않은 행은 FAIL/UNMEASURED이고 root를 보존한다.

## 실제 target와 독립 expected input

- JSON create는 원래 contract의 valid JobDefinition schema를 직접 사용하되 실제 candidate artifact executable과 owned cwd를 넣는다. API flag helper나 응답에서 expected bool을 만들지 않는다. update는 description만 변경하고 settings는 value="4"다.
- QUERY는 actual request/query type의 serde boundary를 검증한다. export companion=false를 유지한 true parsing은 실제 route 성공을 뜻하지 않는다.
- Runtime은 fresh owned state child에서 실제 token/loopback router/Store를 사용한다. 모든 요청은 valid `Authorization: token <actual-token>`이며 JSON content-type을 명시한다. cookie/CSRF/PR145 token-paste scope를 섞지 않는다.
- Populated seed는 schema5, typed setting3, 두 jobs와 unrelated canary metadata/events/queued run 및 admission sequence를 포함한다. engine/daemon/job 실행은 없다. row 전후 snapshot는 별도 read transaction이고 측정 window에 다른 writer가 없다. keeper Store를 유지해 phase 사이 last-writer-close/checkpoint를 분리한다.
- Absent DB 행은 private root/token/receiver/artifact 준비만 마쳤다. DB를 만들지 않는다. absent 확인 전 `GET /jobs`, Store::open, migration, helper readonly DB open을 호출하지 않는다.
- W0에는 actual API call receipt와 실제 listener의 setup-positive/drained readiness가 함께 필요하다. 실제 receiver가 없어서 send 실패 후 delivered0인 결과는 wake0 증거가 아니다.

## JSON: 57행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P001 | `JSON/create/missing_false` | JobCreateRequest; dry_run omitted | dry_run=false | J / P |
| P002 | `JSON/create/native_true` | JobCreateRequest; dry_run=true | dry_run=true | J / P |
| P003 | `JSON/create/native_false` | JobCreateRequest; dry_run=false | dry_run=false | J / P |
| P004 | `JSON/create/null_true` | JobCreateRequest; dry_run=null | dry_run=true | J / P |
| P005 | `JSON/create/empty_string_true` | JobCreateRequest; dry_run="" | dry_run=true | J / P |
| P006 | `JSON/create/true_string_true` | JobCreateRequest; dry_run="true" | dry_run=true | J / P |
| P007 | `JSON/create/one_string_true` | JobCreateRequest; dry_run="1" | dry_run=true | J / P |
| P008 | `JSON/create/false_string_false` | JobCreateRequest; dry_run="false" | dry_run=false | J / P |
| P009 | `JSON/create/zero_string_false` | JobCreateRequest; dry_run="0" | dry_run=false | J / P |
| P010 | `JSON/create/number_zero_reject` | JobCreateRequest; dry_run=0 | actual serde_json rejection | J / P |
| P011 | `JSON/create/number_one_reject` | JobCreateRequest; dry_run=1 | actual serde_json rejection | J / P |
| P012 | `JSON/create/negative_reject` | JobCreateRequest; dry_run=-1 | actual serde_json rejection | J / P |
| P013 | `JSON/create/fraction_reject` | JobCreateRequest; dry_run=0.5 | actual serde_json rejection | J / P |
| P014 | `JSON/create/array_reject` | JobCreateRequest; dry_run=[] | actual serde_json rejection | J / P |
| P015 | `JSON/create/object_reject` | JobCreateRequest; dry_run={} | actual serde_json rejection | J / P |
| P016 | `JSON/create/yes_reject` | JobCreateRequest; dry_run="yes" | actual serde_json rejection | J / P |
| P017 | `JSON/create/uppercase_TRUE_reject` | JobCreateRequest; dry_run="TRUE" | actual serde_json rejection | J / P |
| P018 | `JSON/create/padded_true_reject` | JobCreateRequest; dry_run=" true " | actual serde_json rejection | J / P |
| P019 | `JSON/create/two_string_reject` | JobCreateRequest; dry_run="2" | actual serde_json rejection | J / P |
| P020 | `JSON/update/missing_false` | JobUpdateRequest; dry_run omitted | dry_run=false | J / P |
| P021 | `JSON/update/native_true` | JobUpdateRequest; dry_run=true | dry_run=true | J / P |
| P022 | `JSON/update/native_false` | JobUpdateRequest; dry_run=false | dry_run=false | J / P |
| P023 | `JSON/update/null_true` | JobUpdateRequest; dry_run=null | dry_run=true | J / P |
| P024 | `JSON/update/empty_string_true` | JobUpdateRequest; dry_run="" | dry_run=true | J / P |
| P025 | `JSON/update/true_string_true` | JobUpdateRequest; dry_run="true" | dry_run=true | J / P |
| P026 | `JSON/update/one_string_true` | JobUpdateRequest; dry_run="1" | dry_run=true | J / P |
| P027 | `JSON/update/false_string_false` | JobUpdateRequest; dry_run="false" | dry_run=false | J / P |
| P028 | `JSON/update/zero_string_false` | JobUpdateRequest; dry_run="0" | dry_run=false | J / P |
| P029 | `JSON/update/number_zero_reject` | JobUpdateRequest; dry_run=0 | actual serde_json rejection | J / P |
| P030 | `JSON/update/number_one_reject` | JobUpdateRequest; dry_run=1 | actual serde_json rejection | J / P |
| P031 | `JSON/update/negative_reject` | JobUpdateRequest; dry_run=-1 | actual serde_json rejection | J / P |
| P032 | `JSON/update/fraction_reject` | JobUpdateRequest; dry_run=0.5 | actual serde_json rejection | J / P |
| P033 | `JSON/update/array_reject` | JobUpdateRequest; dry_run=[] | actual serde_json rejection | J / P |
| P034 | `JSON/update/object_reject` | JobUpdateRequest; dry_run={} | actual serde_json rejection | J / P |
| P035 | `JSON/update/yes_reject` | JobUpdateRequest; dry_run="yes" | actual serde_json rejection | J / P |
| P036 | `JSON/update/uppercase_TRUE_reject` | JobUpdateRequest; dry_run="TRUE" | actual serde_json rejection | J / P |
| P037 | `JSON/update/padded_true_reject` | JobUpdateRequest; dry_run=" true " | actual serde_json rejection | J / P |
| P038 | `JSON/update/two_string_reject` | JobUpdateRequest; dry_run="2" | actual serde_json rejection | J / P |
| P039 | `JSON/settings/missing_false` | SettingsPutRequest; dry_run omitted | dry_run=false | J / P |
| P040 | `JSON/settings/native_true` | SettingsPutRequest; dry_run=true | dry_run=true | J / P |
| P041 | `JSON/settings/native_false` | SettingsPutRequest; dry_run=false | dry_run=false | J / P |
| P042 | `JSON/settings/null_true` | SettingsPutRequest; dry_run=null | dry_run=true | J / P |
| P043 | `JSON/settings/empty_string_true` | SettingsPutRequest; dry_run="" | dry_run=true | J / P |
| P044 | `JSON/settings/true_string_true` | SettingsPutRequest; dry_run="true" | dry_run=true | J / P |
| P045 | `JSON/settings/one_string_true` | SettingsPutRequest; dry_run="1" | dry_run=true | J / P |
| P046 | `JSON/settings/false_string_false` | SettingsPutRequest; dry_run="false" | dry_run=false | J / P |
| P047 | `JSON/settings/zero_string_false` | SettingsPutRequest; dry_run="0" | dry_run=false | J / P |
| P048 | `JSON/settings/number_zero_reject` | SettingsPutRequest; dry_run=0 | actual serde_json rejection | J / P |
| P049 | `JSON/settings/number_one_reject` | SettingsPutRequest; dry_run=1 | actual serde_json rejection | J / P |
| P050 | `JSON/settings/negative_reject` | SettingsPutRequest; dry_run=-1 | actual serde_json rejection | J / P |
| P051 | `JSON/settings/fraction_reject` | SettingsPutRequest; dry_run=0.5 | actual serde_json rejection | J / P |
| P052 | `JSON/settings/array_reject` | SettingsPutRequest; dry_run=[] | actual serde_json rejection | J / P |
| P053 | `JSON/settings/object_reject` | SettingsPutRequest; dry_run={} | actual serde_json rejection | J / P |
| P054 | `JSON/settings/yes_reject` | SettingsPutRequest; dry_run="yes" | actual serde_json rejection | J / P |
| P055 | `JSON/settings/uppercase_TRUE_reject` | SettingsPutRequest; dry_run="TRUE" | actual serde_json rejection | J / P |
| P056 | `JSON/settings/padded_true_reject` | SettingsPutRequest; dry_run=" true " | actual serde_json rejection | J / P |
| P057 | `JSON/settings/two_string_reject` | SettingsPutRequest; dry_run="2" | actual serde_json rejection | J / P |

## QUERY: 99행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P058 | `QUERY/list.all/missing_false` | ListJobsQuery; empty query string | all=false | Q / P |
| P059 | `QUERY/list.all/bare_true` | ListJobsQuery; all | all=true | Q / P |
| P060 | `QUERY/list.all/empty_true` | ListJobsQuery; all= | all=true | Q / P |
| P061 | `QUERY/list.all/true_true` | ListJobsQuery; all=true | all=true | Q / P |
| P062 | `QUERY/list.all/one_true` | ListJobsQuery; all=1 | all=true | Q / P |
| P063 | `QUERY/list.all/false_false` | ListJobsQuery; all=false | all=false | Q / P |
| P064 | `QUERY/list.all/zero_false` | ListJobsQuery; all=0 | all=false | Q / P |
| P065 | `QUERY/list.all/uppercase_TRUE_reject` | ListJobsQuery; all=TRUE | actual serde_urlencoded rejection | Q / P |
| P066 | `QUERY/list.all/two_reject` | ListJobsQuery; all=2 | actual serde_urlencoded rejection | Q / P |
| P067 | `QUERY/list.all/null_text_reject` | ListJobsQuery; all=null | actual serde_urlencoded rejection | Q / P |
| P068 | `QUERY/list.all/padded_true_reject` | ListJobsQuery; all=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P069 | `QUERY/run.wait/missing_false` | RunJobQuery; empty query string | wait=false | Q / P |
| P070 | `QUERY/run.wait/bare_true` | RunJobQuery; wait | wait=true | Q / P |
| P071 | `QUERY/run.wait/empty_true` | RunJobQuery; wait= | wait=true | Q / P |
| P072 | `QUERY/run.wait/true_true` | RunJobQuery; wait=true | wait=true | Q / P |
| P073 | `QUERY/run.wait/one_true` | RunJobQuery; wait=1 | wait=true | Q / P |
| P074 | `QUERY/run.wait/false_false` | RunJobQuery; wait=false | wait=false | Q / P |
| P075 | `QUERY/run.wait/zero_false` | RunJobQuery; wait=0 | wait=false | Q / P |
| P076 | `QUERY/run.wait/uppercase_TRUE_reject` | RunJobQuery; wait=TRUE | actual serde_urlencoded rejection | Q / P |
| P077 | `QUERY/run.wait/two_reject` | RunJobQuery; wait=2 | actual serde_urlencoded rejection | Q / P |
| P078 | `QUERY/run.wait/null_text_reject` | RunJobQuery; wait=null | actual serde_urlencoded rejection | Q / P |
| P079 | `QUERY/run.wait/padded_true_reject` | RunJobQuery; wait=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P080 | `QUERY/run.dry-run/missing_false` | RunJobQuery; empty query string | dry-run=false | Q / P |
| P081 | `QUERY/run.dry-run/bare_true` | RunJobQuery; dry-run | dry-run=true | Q / P |
| P082 | `QUERY/run.dry-run/empty_true` | RunJobQuery; dry-run= | dry-run=true | Q / P |
| P083 | `QUERY/run.dry-run/true_true` | RunJobQuery; dry-run=true | dry-run=true | Q / P |
| P084 | `QUERY/run.dry-run/one_true` | RunJobQuery; dry-run=1 | dry-run=true | Q / P |
| P085 | `QUERY/run.dry-run/false_false` | RunJobQuery; dry-run=false | dry-run=false | Q / P |
| P086 | `QUERY/run.dry-run/zero_false` | RunJobQuery; dry-run=0 | dry-run=false | Q / P |
| P087 | `QUERY/run.dry-run/uppercase_TRUE_reject` | RunJobQuery; dry-run=TRUE | actual serde_urlencoded rejection | Q / P |
| P088 | `QUERY/run.dry-run/two_reject` | RunJobQuery; dry-run=2 | actual serde_urlencoded rejection | Q / P |
| P089 | `QUERY/run.dry-run/null_text_reject` | RunJobQuery; dry-run=null | actual serde_urlencoded rejection | Q / P |
| P090 | `QUERY/run.dry-run/padded_true_reject` | RunJobQuery; dry-run=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P091 | `QUERY/cancel.acknowledge-unconfirmed/missing_false` | CancelQuery; empty query string | acknowledge-unconfirmed=false | Q / P |
| P092 | `QUERY/cancel.acknowledge-unconfirmed/bare_true` | CancelQuery; acknowledge-unconfirmed | acknowledge-unconfirmed=true | Q / P |
| P093 | `QUERY/cancel.acknowledge-unconfirmed/empty_true` | CancelQuery; acknowledge-unconfirmed= | acknowledge-unconfirmed=true | Q / P |
| P094 | `QUERY/cancel.acknowledge-unconfirmed/true_true` | CancelQuery; acknowledge-unconfirmed=true | acknowledge-unconfirmed=true | Q / P |
| P095 | `QUERY/cancel.acknowledge-unconfirmed/one_true` | CancelQuery; acknowledge-unconfirmed=1 | acknowledge-unconfirmed=true | Q / P |
| P096 | `QUERY/cancel.acknowledge-unconfirmed/false_false` | CancelQuery; acknowledge-unconfirmed=false | acknowledge-unconfirmed=false | Q / P |
| P097 | `QUERY/cancel.acknowledge-unconfirmed/zero_false` | CancelQuery; acknowledge-unconfirmed=0 | acknowledge-unconfirmed=false | Q / P |
| P098 | `QUERY/cancel.acknowledge-unconfirmed/uppercase_TRUE_reject` | CancelQuery; acknowledge-unconfirmed=TRUE | actual serde_urlencoded rejection | Q / P |
| P099 | `QUERY/cancel.acknowledge-unconfirmed/two_reject` | CancelQuery; acknowledge-unconfirmed=2 | actual serde_urlencoded rejection | Q / P |
| P100 | `QUERY/cancel.acknowledge-unconfirmed/null_text_reject` | CancelQuery; acknowledge-unconfirmed=null | actual serde_urlencoded rejection | Q / P |
| P101 | `QUERY/cancel.acknowledge-unconfirmed/padded_true_reject` | CancelQuery; acknowledge-unconfirmed=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P102 | `QUERY/export.include-values/missing_false` | ExportQuery; empty query string | include-values=false | Q / P |
| P103 | `QUERY/export.include-values/bare_true` | ExportQuery; include-values | include-values=true | Q / P |
| P104 | `QUERY/export.include-values/empty_true` | ExportQuery; include-values= | include-values=true | Q / P |
| P105 | `QUERY/export.include-values/true_true` | ExportQuery; include-values=true | include-values=true | Q / P |
| P106 | `QUERY/export.include-values/one_true` | ExportQuery; include-values=1 | include-values=true | Q / P |
| P107 | `QUERY/export.include-values/false_false` | ExportQuery; include-values=false | include-values=false | Q / P |
| P108 | `QUERY/export.include-values/zero_false` | ExportQuery; include-values=0 | include-values=false | Q / P |
| P109 | `QUERY/export.include-values/uppercase_TRUE_reject` | ExportQuery; include-values=TRUE | actual serde_urlencoded rejection | Q / P |
| P110 | `QUERY/export.include-values/two_reject` | ExportQuery; include-values=2 | actual serde_urlencoded rejection | Q / P |
| P111 | `QUERY/export.include-values/null_text_reject` | ExportQuery; include-values=null | actual serde_urlencoded rejection | Q / P |
| P112 | `QUERY/export.include-values/padded_true_reject` | ExportQuery; include-values=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P113 | `QUERY/export.acknowledge-plaintext/missing_false` | ExportQuery; empty query string | acknowledge-plaintext=false | Q / P |
| P114 | `QUERY/export.acknowledge-plaintext/bare_true` | ExportQuery; acknowledge-plaintext | acknowledge-plaintext=true | Q / P |
| P115 | `QUERY/export.acknowledge-plaintext/empty_true` | ExportQuery; acknowledge-plaintext= | acknowledge-plaintext=true | Q / P |
| P116 | `QUERY/export.acknowledge-plaintext/true_true` | ExportQuery; acknowledge-plaintext=true | acknowledge-plaintext=true | Q / P |
| P117 | `QUERY/export.acknowledge-plaintext/one_true` | ExportQuery; acknowledge-plaintext=1 | acknowledge-plaintext=true | Q / P |
| P118 | `QUERY/export.acknowledge-plaintext/false_false` | ExportQuery; acknowledge-plaintext=false | acknowledge-plaintext=false | Q / P |
| P119 | `QUERY/export.acknowledge-plaintext/zero_false` | ExportQuery; acknowledge-plaintext=0 | acknowledge-plaintext=false | Q / P |
| P120 | `QUERY/export.acknowledge-plaintext/uppercase_TRUE_reject` | ExportQuery; acknowledge-plaintext=TRUE | actual serde_urlencoded rejection | Q / P |
| P121 | `QUERY/export.acknowledge-plaintext/two_reject` | ExportQuery; acknowledge-plaintext=2 | actual serde_urlencoded rejection | Q / P |
| P122 | `QUERY/export.acknowledge-plaintext/null_text_reject` | ExportQuery; acknowledge-plaintext=null | actual serde_urlencoded rejection | Q / P |
| P123 | `QUERY/export.acknowledge-plaintext/padded_true_reject` | ExportQuery; acknowledge-plaintext=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P124 | `QUERY/import.accept-plaintext-values/missing_false` | ImportQuery; empty query string | accept-plaintext-values=false | Q / P |
| P125 | `QUERY/import.accept-plaintext-values/bare_true` | ImportQuery; accept-plaintext-values | accept-plaintext-values=true | Q / P |
| P126 | `QUERY/import.accept-plaintext-values/empty_true` | ImportQuery; accept-plaintext-values= | accept-plaintext-values=true | Q / P |
| P127 | `QUERY/import.accept-plaintext-values/true_true` | ImportQuery; accept-plaintext-values=true | accept-plaintext-values=true | Q / P |
| P128 | `QUERY/import.accept-plaintext-values/one_true` | ImportQuery; accept-plaintext-values=1 | accept-plaintext-values=true | Q / P |
| P129 | `QUERY/import.accept-plaintext-values/false_false` | ImportQuery; accept-plaintext-values=false | accept-plaintext-values=false | Q / P |
| P130 | `QUERY/import.accept-plaintext-values/zero_false` | ImportQuery; accept-plaintext-values=0 | accept-plaintext-values=false | Q / P |
| P131 | `QUERY/import.accept-plaintext-values/uppercase_TRUE_reject` | ImportQuery; accept-plaintext-values=TRUE | actual serde_urlencoded rejection | Q / P |
| P132 | `QUERY/import.accept-plaintext-values/two_reject` | ImportQuery; accept-plaintext-values=2 | actual serde_urlencoded rejection | Q / P |
| P133 | `QUERY/import.accept-plaintext-values/null_text_reject` | ImportQuery; accept-plaintext-values=null | actual serde_urlencoded rejection | Q / P |
| P134 | `QUERY/import.accept-plaintext-values/padded_true_reject` | ImportQuery; accept-plaintext-values=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P135 | `QUERY/import.dry-run/missing_false` | ImportQuery; empty query string | dry-run=false | Q / P |
| P136 | `QUERY/import.dry-run/bare_true` | ImportQuery; dry-run | dry-run=true | Q / P |
| P137 | `QUERY/import.dry-run/empty_true` | ImportQuery; dry-run= | dry-run=true | Q / P |
| P138 | `QUERY/import.dry-run/true_true` | ImportQuery; dry-run=true | dry-run=true | Q / P |
| P139 | `QUERY/import.dry-run/one_true` | ImportQuery; dry-run=1 | dry-run=true | Q / P |
| P140 | `QUERY/import.dry-run/false_false` | ImportQuery; dry-run=false | dry-run=false | Q / P |
| P141 | `QUERY/import.dry-run/zero_false` | ImportQuery; dry-run=0 | dry-run=false | Q / P |
| P142 | `QUERY/import.dry-run/uppercase_TRUE_reject` | ImportQuery; dry-run=TRUE | actual serde_urlencoded rejection | Q / P |
| P143 | `QUERY/import.dry-run/two_reject` | ImportQuery; dry-run=2 | actual serde_urlencoded rejection | Q / P |
| P144 | `QUERY/import.dry-run/null_text_reject` | ImportQuery; dry-run=null | actual serde_urlencoded rejection | Q / P |
| P145 | `QUERY/import.dry-run/padded_true_reject` | ImportQuery; dry-run=%20true%20 | actual serde_urlencoded rejection | Q / P |
| P146 | `QUERY/prune.dry-run/missing_false` | PruneQuery; empty query string | dry-run=false | Q / P |
| P147 | `QUERY/prune.dry-run/bare_true` | PruneQuery; dry-run | dry-run=true | Q / P |
| P148 | `QUERY/prune.dry-run/empty_true` | PruneQuery; dry-run= | dry-run=true | Q / P |
| P149 | `QUERY/prune.dry-run/true_true` | PruneQuery; dry-run=true | dry-run=true | Q / P |
| P150 | `QUERY/prune.dry-run/one_true` | PruneQuery; dry-run=1 | dry-run=true | Q / P |
| P151 | `QUERY/prune.dry-run/false_false` | PruneQuery; dry-run=false | dry-run=false | Q / P |
| P152 | `QUERY/prune.dry-run/zero_false` | PruneQuery; dry-run=0 | dry-run=false | Q / P |
| P153 | `QUERY/prune.dry-run/uppercase_TRUE_reject` | PruneQuery; dry-run=TRUE | actual serde_urlencoded rejection | Q / P |
| P154 | `QUERY/prune.dry-run/two_reject` | PruneQuery; dry-run=2 | actual serde_urlencoded rejection | Q / P |
| P155 | `QUERY/prune.dry-run/null_text_reject` | PruneQuery; dry-run=null | actual serde_urlencoded rejection | Q / P |
| P156 | `QUERY/prune.dry-run/padded_true_reject` | PruneQuery; dry-run=%20true%20 | actual serde_urlencoded rejection | Q / P |

## PREVIEW: 14행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P157 | `PREVIEW/create_populated/native_true` | POST /api/v1/jobs; dry_run=true; new owned job definition; canary metadata/environment | create id=<non-durable>, dry_run=true, submitted metadata exact, independent redacted definition; no new revision/event/cursor/job | O,L0,F,W0,E / H |
| P158 | `PREVIEW/create_populated/legacy_one` | POST /api/v1/jobs; dry_run="1"; new owned job definition; canary metadata/environment | create id=<non-durable>, dry_run=true, submitted metadata exact, independent redacted definition; no new revision/event/cursor/job | O,L0,F,W0,E / H |
| P159 | `PREVIEW/update_populated/native_true` | PUT /api/v1/jobs/{seed-job-id}; dry_run=true; description="would change" only | id=seed-id, revision=before+1, changed_fields=[description], schedule_changed=false, cursor=before, independent redacted before/after; old durable description/revision/event intact | O,L0,F,W0,E / H |
| P160 | `PREVIEW/update_populated/legacy_one` | PUT /api/v1/jobs/{seed-job-id}; dry_run="1"; description="would change" only | id=seed-id, revision=before+1, changed_fields=[description], schedule_changed=false, cursor=before, independent redacted before/after; old durable description/revision/event intact | O,L0,F,W0,E / H |
| P161 | `PREVIEW/typed_setting_populated/native_true` | PUT /api/v1/settings/global_concurrency; dry_run=true; value="4" (seeded durable value3) | data={key:global_concurrency,value:"4",dry_run:true}; durable value3 and settings.updated_at_us intact | O,L0,F,W0,E / H |
| P162 | `PREVIEW/typed_setting_populated/legacy_one` | PUT /api/v1/settings/global_concurrency; dry_run="1"; value="4" (seeded durable value3) | data={key:global_concurrency,value:"4",dry_run:true}; durable value3 and settings.updated_at_us intact | O,L0,F,W0,E / H |
| P163 | `PREVIEW/environment_created_populated/native_true` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=true; value=owned nonce secret; PR144_NAME absent in seed | action=created, configured=true, value_redacted=true, dry_run=true; no plaintext canary and env map still lacks new entry | O,L0,F,W0,E / H |
| P164 | `PREVIEW/environment_created_populated/legacy_one` | PUT /api/v1/settings/environment.PR144_NAME; dry_run="1"; value=owned nonce secret; PR144_NAME absent in seed | action=created, configured=true, value_redacted=true, dry_run=true; no plaintext canary and env map still lacks new entry | O,L0,F,W0,E / H |
| P165 | `PREVIEW/environment_replaced_populated/native_true` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=true; new owned nonce secret; old seeded secret differs | action=replaced, configured=true, value_redacted=true, dry_run=true; old exact env value and settings.updated_at_us intact | O,L0,F,W0,E / H |
| P166 | `PREVIEW/environment_replaced_populated/legacy_one` | PUT /api/v1/settings/environment.PR144_NAME; dry_run="1"; new owned nonce secret; old seeded secret differs | action=replaced, configured=true, value_redacted=true, dry_run=true; old exact env value and settings.updated_at_us intact | O,L0,F,W0,E / H |
| P167 | `PREVIEW/environment_absent/native_true` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=true; value=owned nonce secret; DB has never been opened | action=created, configured=true, value_redacted=true, dry_run=true; DB/WAL/SHM/journal still absent before any live open | O,A0,F,W0,E / H |
| P168 | `PREVIEW/environment_absent/legacy_one` | PUT /api/v1/settings/environment.PR144_NAME; dry_run="1"; value=owned nonce secret; DB has never been opened | action=created, configured=true, value_redacted=true, dry_run=true; DB/WAL/SHM/journal still absent before any live open | O,A0,F,W0,E / H |
| P169 | `PREVIEW/create_absent/native_true` | POST /api/v1/jobs; dry_run=true; new independent valid owned job definition; DB has never been opened | id=<non-durable>, dry_run=true, independent redacted definition; DB/WAL/SHM/journal still absent before any live open | O,A0,F,W0,E / H |
| P170 | `PREVIEW/create_absent/legacy_one` | POST /api/v1/jobs; dry_run="1"; new independent valid owned job definition; DB has never been opened | id=<non-durable>, dry_run=true, independent redacted definition; DB/WAL/SHM/journal still absent before any live open | O,A0,F,W0,E / H |

## LIVE: 8행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P171 | `LIVE/create/native_false` | POST /api/v1/jobs; dry_run=false; new independent valid owned job definition | HTTP200 OK; canonical new UUID/job revision1 + created_by add job_revision + cursor + job_added event; mandatory exact submitted fields, one real wake; all unrelated logical rows unchanged | O,L+,F,W+,E / H |
| P172 | `LIVE/create/missing` | POST /api/v1/jobs; dry_run omitted; new independent valid owned job definition | HTTP200 OK; canonical new UUID/job revision1 + created_by add job_revision + cursor + job_added event; mandatory exact submitted fields, one real wake; all unrelated logical rows unchanged | O,L+,F,W+,E / H |
| P173 | `LIVE/update/native_false` | PUT /api/v1/jobs/{seed-job-id}; dry_run=false; description="would change" only | HTTP200 OK; same id revision before+1 + created_by update revision + matching cursor + job_updated event(revision); schedule/cursor/other fields preserved; one real wake | O,L+,F,W+,E / H |
| P174 | `LIVE/update/missing` | PUT /api/v1/jobs/{seed-job-id}; dry_run omitted; description="would change" only | HTTP200 OK; same id revision before+1 + created_by update revision + matching cursor + job_updated event(revision); schedule/cursor/other fields preserved; one real wake | O,L+,F,W+,E / H |
| P175 | `LIVE/typed_setting/native_false` | PUT /api/v1/settings/global_concurrency; dry_run=false; value="4"; seeded value3 and timestamp100 | HTTP200 redacted settings; persisted integer global_concurrency4 and settings.updated_at_us within request wall-time bounds; no history event inserted by set_setting; one real wake | O,L+,F,W+,E / H |
| P176 | `LIVE/typed_setting/missing` | PUT /api/v1/settings/global_concurrency; dry_run omitted; value="4"; seeded value3 and timestamp100 | HTTP200 redacted settings; persisted integer global_concurrency4 and settings.updated_at_us within request wall-time bounds; no history event inserted by set_setting; one real wake | O,L+,F,W+,E / H |
| P177 | `LIVE/environment/native_false` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=false; new owned nonce secret; existing old secret | HTTP200 redacted action=replaced,dry_run=false; durable exact env value changed once and settings.updated_at_us within request bounds; no history event inserted by set_environment; one real wake | O,L+,F,W+,E / H |
| P178 | `LIVE/environment/missing` | PUT /api/v1/settings/environment.PR144_NAME; dry_run omitted; new owned nonce secret; existing old secret | HTTP200 redacted action=replaced,dry_run=false; durable exact env value changed once and settings.updated_at_us within request bounds; no history event inserted by set_environment; one real wake | O,L+,F,W+,E / H |

## ENV-REFUSE: 6행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P179 | `ENV-REFUSE/preview/invalid_name` | PUT /api/v1/settings/environment.BAD-NAME; dry_run=true; value=owned nonce secret | HTTP400 invalid_request; exact message=invalid or reserved environment name BAD-NAME | O,L0,F,W0,R400 / H |
| P180 | `ENV-REFUSE/preview/reserved_LOCRON_name` | PUT /api/v1/settings/environment.LOCRON_QUALIFICATION; dry_run=true; value=owned nonce secret | HTTP400 invalid_request; exact message=invalid or reserved environment name LOCRON_QUALIFICATION | O,L0,F,W0,R400 / H |
| P181 | `ENV-REFUSE/preview/NUL_value` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=true; value="\u0000" valid JSON string | HTTP400 invalid_request; exact message=environment value for PR144_NAME contains NUL | O,L0,F,W0,R400 / H |
| P182 | `ENV-REFUSE/live/invalid_name` | PUT /api/v1/settings/environment.BAD-NAME; dry_run=false; value=owned nonce secret | HTTP400 invalid_request; exact message=invalid or reserved environment name BAD-NAME | O,L0,F,W0,R400 / H |
| P183 | `ENV-REFUSE/live/reserved_LOCRON_name` | PUT /api/v1/settings/environment.LOCRON_QUALIFICATION; dry_run=false; value=owned nonce secret | HTTP400 invalid_request; exact message=invalid or reserved environment name LOCRON_QUALIFICATION | O,L0,F,W0,R400 / H |
| P184 | `ENV-REFUSE/live/NUL_value` | PUT /api/v1/settings/environment.PR144_NAME; dry_run=false; value="\u0000" valid JSON string | HTTP400 invalid_request; exact message=environment value for PR144_NAME contains NUL | O,L0,F,W0,R400 / H |

## BODY-REFUSE: 9행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| P185 | `BODY-REFUSE/create/number` | POST /api/v1/jobs; dry_run=0; complete valid JobCreateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P186 | `BODY-REFUSE/create/array` | POST /api/v1/jobs; dry_run=[]; complete valid JobCreateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P187 | `BODY-REFUSE/create/object` | POST /api/v1/jobs; dry_run={}; complete valid JobCreateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P188 | `BODY-REFUSE/update/number` | PUT /api/v1/jobs/never-stored-reference; dry_run=0; complete valid JobUpdateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P189 | `BODY-REFUSE/update/array` | PUT /api/v1/jobs/never-stored-reference; dry_run=[]; complete valid JobUpdateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P190 | `BODY-REFUSE/update/object` | PUT /api/v1/jobs/never-stored-reference; dry_run={}; complete valid JobUpdateRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P191 | `BODY-REFUSE/settings/number` | PUT /api/v1/settings/global_concurrency; dry_run=0; complete valid SettingsPutRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P192 | `BODY-REFUSE/settings/array` | PUT /api/v1/settings/global_concurrency; dry_run=[]; complete valid SettingsPutRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |
| P193 | `BODY-REFUSE/settings/object` | PUT /api/v1/settings/global_concurrency; dry_run={}; complete valid SettingsPutRequest | HTTP422 actual JsonDataError before handler/Store selection; DB and sidecars remain absent; no API wake attempt | O,A0,F,W0,R422 / H |

## WIN-ENV: 2행

| 행 | 기존 key | 실제 target / wire | expected | oracle / clock |
|---|---|---|---|---|
| W001 | `WIN-ENV/case_insensitive_replaced_preview` | PUT /api/v1/settings/environment.PR144_NAME; seed mixed case Pr144_Name with old owned secret; request PR144_NAME, dry_run=true, new owned secret | HTTP200 action=replaced; exact requested key, configured=true,value_redacted=true,dry_run=true; canonical-equivalent old env key/value persists without insertion/casing change; real API wake attempt0 | N,O,L0,F,W0,E / H |
| W002 | `WIN-ENV/case_insensitive_reserved_name_refusal` | PUT /api/v1/settings/environment.lOcRoN_QUALIFICATION; dry_run=true; value=owned nonce secret | HTTP400 invalid_request; exact message invalid or reserved environment name lOcRoN_QUALIFICATION; no setter/durable delta/API wake attempt | N,O,L0,F,W0,R400 / H |

## Ledger acceptance

- Unix run: expected portable key set193/193 complete, native Windows rows are outside that platform set. Native Windows run:195/195 complete. All keys must be unique and exactly equal to this pinned key set. Missing, duplicate, foreign, skip, timeout, fixture-unavailable or unjoined worker를 PASS로 바꾸지 않는다.
- 행 시작 receipt와 actual completed receipt를 구분한다. actual assertions/side-effects/receiver evidence after worker completion에만 완료 key를 남긴다. parser reject는 reject assertion을 실제 완료한 PASS이며 생략이 아니다.
- 새로운 193/195 proof는 같은 candidate head/checkout/compiler/artifact에서 얻어야 한다. 기존 run의 green31 server-library/contract20·21 결과는 이 새 ledger의 PASS로 전입하지 않는다.
- expected key-set SHA-256: `e2318e39838f2a1ce6ad1e372bc4a8a6a6ccab74f8d9a5488489ba3bcc89de78`.
