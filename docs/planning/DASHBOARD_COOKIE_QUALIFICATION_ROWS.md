# PR145 proposed per-row qualification ledger

Read-only proposal. **198 distinct proposed keys = backend184 + client14. All UNMEASURED.**
The lost earlier private198 ledger cannot be recovered byte-for-byte. Its surviving group counts are retained; these explicit keys are new proposed selection inputs for Root's forthcoming Docs/Issue handoff.

## Wire and oracle conventions

T and C are the synthetic 64-hex values in the JSON proposal. S and D are distinct 64-hex values; uppercase T is a stale exact secret, while uppercase C with the same uppercase echo is valid. Arrays preserve repeated HeaderMap fields. A `{hex: ...ff}` entry is one actual opaque HeaderValue and must not become absence. There is no installed/user token.

Each backend row keeps valid Host `localhost:10824`, genuinely absent Origin, and the exact indicated Authorization/echo/content-type fields. Unspecified headers are absent. Each request has a real observed body stream. Session/status/issuer and public outcomes use the actual production Router; measurable Next/metadata observations use a separately composed private Router with the same actual five production middleware functions. They are distinct observations, not a copied parser or a DOM proof.

`ZERO` means zero actual Stream::poll_next calls. `POSITIVE_BEFORE_NEXT` means the CSRF form reader really polled the stream; `ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT` separates no CSRF sniffing from successful Next consumption. In production Session JSON extractor rejection cases the middleware probe is admitted, while the actual Json/exact-secret route determines status/issuer. Body/metadata preservation and checked owned-root cleanup are required before a key completes.

For each refusal assert the exact selected generic JSON profile or unchanged Axum Json status/text profile, no Set-Cookie, no-referrer, no secret/canary response bytes, no Next for security refusals, and no state/token/DB changes. Accepted issuer responses inspect the actual cookie delta/attributes; authorized Set-Cookie naturally contains T.

Finite group counts: session_cookie=50, csrf_cookie=25, csrf_header=14, media_type=14, form_field=21, body=7, method=7, positive=8, paste_auth=11, recovery_body=12, boundary=8, guard=7, client=14.

Exact error profiles: `generic_auth` = 401/unauthenticated/`a valid access token or session cookie is required`; `csrf_cookie_refusal` = 403/refused/`cookie-authenticated mutations require a CSRF token`; `csrf_echo_refusal` = 403/refused/`X-CSRF-Token does not match the csrf_token cookie`; `secret_rejected` = 401/unauthenticated/`access token rejected`. All have the existing exact versioned envelope. Json extractor responses keep their existing text/status. Every backend result has state effects0 and no-referrer.

Default body for backend rows is the exact JSON `{"token":"{T}"}` with one application/json field, absent Authorization and absent CSRF echo, unless the row says otherwise. No cookie field is supplied unless listed. `cookies` in outcomes counts actual response Set-Cookie fields. `Next` refers only to the independent actual-middleware probe, never an invented API-handler counter. Raw fields are listed in preserved append order.

## session_cookie — 50 keys

**Observation:** production_router.
**Required interpretation:** A valid bearer is deliberately absent; bearer independence has separate positive rows.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **SC01-status** / missing | GET /api/v1/session; Cookie absent; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC01-paste** / missing | POST /api/v1/session; Cookie absent; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Unauthenticated |
| **SC02-status** / unrelated_only | GET /api/v1/session; Cookie=["other=OTHER"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC02-paste** / unrelated_only | POST /api/v1/session; Cookie=["other=OTHER"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Unauthenticated |
| **SC03-status** / current_only | GET /api/v1/session; Cookie=["locron_session={T}"]; X-CSRF-Token absent | 200 `session_success`; polls=`ZERO`; Next=1; cookies=1; AuthKind=Session |
| **SC03-paste** / current_only | POST /api/v1/session; Cookie=["locron_session={T}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **SC04-status** / current_combined_with_csrf | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`ZERO`; Next=1; cookies=0; AuthKind=Session |
| **SC04-paste** / current_combined_with_csrf | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **SC05-status** / current_split_with_csrf | GET /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`ZERO`; Next=1; cookies=0; AuthKind=Session |
| **SC05-paste** / current_split_with_csrf | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **SC06-status** / current_last_after_unrelated_fields | GET /api/v1/session; Cookie=["other=OTHER","csrf_token={C}","last=LAST; locron_session={T}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`ZERO`; Next=1; cookies=0; AuthKind=Session |
| **SC06-paste** / current_last_after_unrelated_fields | POST /api/v1/session; Cookie=["other=OTHER","csrf_token={C}","last=LAST; locron_session={T}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **SC07-status** / stale_wellformed_hex | GET /api/v1/session; Cookie=["locron_session={S}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC07-paste** / stale_wellformed_hex | POST /api/v1/session; Cookie=["locron_session={S}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Unauthenticated |
| **SC08-status** / uppercase_secret_stale | GET /api/v1/session; Cookie=["locron_session={UPPER_T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC08-paste** / uppercase_secret_stale | POST /api/v1/session; Cookie=["locron_session={UPPER_T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Unauthenticated |
| **SC09-status** / empty_target | GET /api/v1/session; Cookie=["locron_session=; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC09-paste** / empty_target | POST /api/v1/session; Cookie=["locron_session=; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC10-status** / bare_target | GET /api/v1/session; Cookie=["locron_session; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC10-paste** / bare_target | POST /api/v1/session; Cookie=["locron_session; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC11-status** / short63 | GET /api/v1/session; Cookie=["locron_session={T63}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC11-paste** / short63 | POST /api/v1/session; Cookie=["locron_session={T63}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC12-status** / long65 | GET /api/v1/session; Cookie=["locron_session={T65}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC12-paste** / long65 | POST /api/v1/session; Cookie=["locron_session={T65}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC13-status** / nonhex64 | GET /api/v1/session; Cookie=["locron_session={NONHEX64}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC13-paste** / nonhex64 | POST /api/v1/session; Cookie=["locron_session={NONHEX64}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC14-status** / quoted64 | GET /api/v1/session; Cookie=["locron_session=\"{T}\"; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC14-paste** / quoted64 | POST /api/v1/session; Cookie=["locron_session=\"{T}\"; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC15-status** / space_after_equals | GET /api/v1/session; Cookie=["locron_session= {T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC15-paste** / space_after_equals | POST /api/v1/session; Cookie=["locron_session= {T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC16-status** / space_before_equals | GET /api/v1/session; Cookie=["locron_session ={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC16-paste** / space_before_equals | POST /api/v1/session; Cookie=["locron_session ={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC17-status** / percent_encoded_value | GET /api/v1/session; Cookie=["locron_session=%30{T_TAIL}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC17-paste** / percent_encoded_value | POST /api/v1/session; Cookie=["locron_session=%30{T_TAIL}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC18-status** / duplicate_equal_single | GET /api/v1/session; Cookie=["locron_session={T}; locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC18-paste** / duplicate_equal_single | POST /api/v1/session; Cookie=["locron_session={T}; locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC19-status** / duplicate_different_single | GET /api/v1/session; Cookie=["locron_session={T}; locron_session={S}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC19-paste** / duplicate_different_single | POST /api/v1/session; Cookie=["locron_session={T}; locron_session={S}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC20-status** / duplicate_equal_split | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}","locron_session={T}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC20-paste** / duplicate_equal_split | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}","locron_session={T}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC21-status** / duplicate_different_split | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}","locron_session={S}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC21-paste** / duplicate_different_split | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}","locron_session={S}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC22-status** / current_then_malformed_single | GET /api/v1/session; Cookie=["locron_session={T}; locron_session=%recoverable; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC22-paste** / current_then_malformed_single | POST /api/v1/session; Cookie=["locron_session={T}; locron_session=%recoverable; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC23-status** / malformed_then_current_single | GET /api/v1/session; Cookie=["locron_session=%recoverable; locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC23-paste** / malformed_then_current_single | POST /api/v1/session; Cookie=["locron_session=%recoverable; locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC24-status** / current_then_unreadable_later_field | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}",{"hex":"4f504151554543414e415259ff"}]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC24-paste** / current_then_unreadable_later_field | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}",{"hex":"4f504151554543414e415259ff"}]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC25-status** / unreadable_earlier_then_current | GET /api/v1/session; Cookie=[{"hex":"4f504151554543414e415259ff"},"locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **SC25-paste** / unreadable_earlier_then_current | POST /api/v1/session; Cookie=[{"hex":"4f504151554543414e415259ff"},"locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |

## csrf_cookie — 25 keys

**Observation:** production_router.
**Required interpretation:** All Cookie fields must be scanned. An unreadable field makes session admission fail before CSRF; readable duplicate CSRF remains the existing 403.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **CC01** / missing | POST /api/v1/session; Cookie=["locron_session={T}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC02** / current_lowercase_hex | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC03** / uppercase_hex | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={UPPER_C}"]; X-CSRF-Token=["{UPPER_C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC04** / combined_with_session | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC05** / split_with_session | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}","other=OTHER"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC06** / valid_last_after_unrelated | POST /api/v1/session; Cookie=["locron_session={T}","first=FIRST","last=LAST; csrf_token={C}"]; X-CSRF-Token=["{C}"] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC07** / empty | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token="]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC08** / bare | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC09** / short63 | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C63}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC10** / long65 | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C65}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC11** / nonhex64 | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={NONHEX64}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC12** / quoted | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=\"{C}\""]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC13** / leading_value_space | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token= {C}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC14** / key_space_before_equals | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token ={C}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC15** / percent_encoded_hex | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=%61{C_TAIL}"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC16** / percent_recoverable | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=%recoverable"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session |
| **CC17** / duplicate_equal_single | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}; csrf_token={C}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC18** / duplicate_different_single | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}; csrf_token={D}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC19** / duplicate_equal_split | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}","csrf_token={C}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC20** / duplicate_different_split | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}","csrf_token={D}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC21** / malformed_then_valid_single | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=%recoverable; csrf_token={C}"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC22** / valid_then_malformed_split | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token={C}","csrf_token=%recoverable"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC23** / malformed_then_bare_split | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=%recoverable","csrf_token"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **CC24** / malformed_then_unreadable_field | POST /api/v1/session; Cookie=["locron_session={T}","csrf_token=%recoverable",{"hex":"4f504151554543414e415259ff"}]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **CC25** / unreadable_then_malformed | POST /api/v1/session; Cookie=["locron_session={T}",{"hex":"4f504151554543414e415259ff"},"csrf_token=%recoverable"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |

## csrf_header — 14 keys

**Observation:** production_middleware_probe.
**Required interpretation:** Invalid supplied echoes cannot be rescued by this otherwise valid form. Successful form/header paths preserve every request field; matched uppercase passes, mismatched case refuses.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **HD01** / missing_form_fallback | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **HD02** / valid_lowercase | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **HD03** / valid_uppercase_matched_cookie | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={UPPER_C}"]; X-CSRF-Token=["{UPPER_C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={UPPER_C}&other=BODYCANARY" | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **HD04** / wellformed_uppercase_mismatch | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{UPPER_C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD05** / empty | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=[""]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD06** / short63 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C63}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD07** / long65 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C65}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD08** / nonhex64 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{NONHEX64}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD09** / quoted64 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["\"{C}\""]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD10** / leading_whitespace | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=[" {C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD11** / duplicate_equal | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}","{C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD12** / duplicate_different | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}","{D}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD13** / unreadable | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=[{"hex":"48454144455243414e415259ff"}]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **HD14** / comma_combined | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}, {C}"]; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |

## media_type — 14 keys

**Observation:** production_middleware_probe.
**Required interpretation:** Existing essence-only behavior is retained; parameters are not parsed or percent-decoded. Each positive also asserts exact restored body and metadata.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **MT01** / missing | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=[]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT02** / exact_form | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MT03** / uppercase_form | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["APPLICATION/X-WWW-FORM-URLENCODED"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MT04** / charset_parameter | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded; charset=utf-8"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MT05** / trimmed_essence_ows | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=[" application/x-www-form-urlencoded "]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MT06** / suffix_extra | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded+extra"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT07** / prefixed_garbage | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["bad-application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT08** / json | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT09** / multipart | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["multipart/form-data; boundary=BODYCANARY"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT10** / duplicate_equal | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded","application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT11** / duplicate_different | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded","application/json"]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT12** / unreadable | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=[{"hex":"4d4544494143414e415259ff"}]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT13** / empty | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=[""]; body="csrf_token={C}&other=BODYCANARY" | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MT14** / ordinary_parameter | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded; ignored=PARAMCANARY"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |

## form_field — 21 keys

**Observation:** production_middleware_probe.
**Required interpretation:** A successful read restores bytes even when the form value is invalid. Invalid forms suppress Next. Assert this through the actual helper plus the real middleware, not a model parser.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **FF01** / correct_single | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **FF02** / unrelated_before | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="other=BODYCANARY&csrf_token={C}" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **FF03** / unrelated_after | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&other=BODYCANARY" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **FF04** / unrelated_bare | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="other&csrf_token={C}" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **FF05** / other_empty | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="other=&csrf_token={C}" | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **FF06** / case_different_target | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="CSRF_TOKEN={C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF07** / duplicate_equal | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&csrf_token={C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF08** / duplicate_different | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&csrf_token={D}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF09** / bare_target | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF10** / empty_target | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token=" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF11** / short63 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C63}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF12** / long65 | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C65}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF13** / nonhex | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={NONHEX64}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF14** / quoted | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token=\"{C}\"" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF15** / percent_encoded_key | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="%63srf_token={C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF16** / percent_encoded_value | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token=%61{C_TAIL}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF17** / plus_value | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token=+{C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF18** / space_key | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token ={C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF19** / space_value | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token= {C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF20** / bare_then_valid | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token&csrf_token={C}" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |
| **FF21** / valid_then_bare | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&csrf_token" | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session |

## body — 7 keys

**Observation:** production_middleware_probe_and_direct_actual_form_helper.
**Required interpretation:** Increment actual Stream::poll_next calls, including EOF/error. A controlled Stream error is not a claimed OS failure. Re-create the stream for the direct-helper metadata/body conservation observation.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **BD01** / chunked_valid | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"chunks":["csrf_","token={C}","&other=BODYCANARY"]}; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session; EXACT_RESTORED |
| **BD02** / read_error_first | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"chunks":[{"controlled_stream_error":"first-read-BODYCANARY"}]}; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session; EMPTY_AFTER_READ_ERROR_METADATA_RETAINED |
| **BD03** / read_error_after_prefix | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"chunks":["csrf_token=",{"controlled_stream_error":"after-prefix-BODYCANARY"}]}; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session; EMPTY_AFTER_READ_ERROR_METADATA_RETAINED |
| **BD04** / exact_1mib | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"construction":"csrf_token={C}&padding= plus ASCII 'a' to exact 1_048_576 bytes","exact_bytes":1048576}; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session; EXACT_RESTORED |
| **BD05** / over_1mib | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"construction":"same valid prefix, exact 1_048_577 bytes","exact_bytes":1048577}; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session; EMPTY_AFTER_LIMIT_ERROR_METADATA_RETAINED |
| **BD06** / invalid_utf8 | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"chunks":["csrf_token={C}&other=",{"hex":"ff"}]}; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session; EXACT_RESTORED |
| **BD07** / empty_stream | POST /api/v1/pr145-probe?query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; stream={"chunks":[],"count_eof_poll":true}; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_echo_refusal`; polls=`POSITIVE_BEFORE_NEXT`; Next=0; cookies=0; AuthKind=Session; EXACT_RESTORED_EMPTY |

## method — 7 keys

**Observation:** production_middleware_probe.
**Required interpretation:** Safe-method behavior is unchanged; POST to an ordinary path and every other unsafe method do not receive recovery. HEAD is asserted through observed handler entry/metadata because its response body is empty.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **MD01** / GET_malformed_csrf_boundary | GET /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MD02** / HEAD_malformed_csrf_boundary | HEAD /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MD03** / OPTIONS_malformed_csrf_boundary | OPTIONS /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **MD04** / POST_malformed_csrf_boundary | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MD05** / PUT_malformed_csrf_boundary | PUT /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MD06** / PATCH_malformed_csrf_boundary | PATCH /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |
| **MD07** / DELETE_malformed_csrf_boundary | DELETE /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0; AuthKind=Session |

## positive — 8 keys

**Observation:** production_middleware_probe; production_router.
**Required interpretation:** Use an independently accepted bearer or valid-session case; no invalid-field fallback may impersonate these positives. Initial paste with Missing Session keeps its original unauthenticated exemption even with readable malformed CSRF/echo. The probe captures body/headers/extensions out-of-band and returns a non-reflecting fixed response.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **PS01** / bearer_lower_no_cookie | POST /api/v1/pr145-probe; Cookie absent; Authorization=["token {T}"]; X-CSRF-Token absent; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Bearer |
| **PS02** / bearer_title_malformed_cookie | POST /api/v1/pr145-probe; Cookie=["locron_session=%bad; csrf_token=%recoverable"]; Authorization=["Token {T}"]; X-CSRF-Token=[""]; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Bearer |
| **PS03** / bearer_duplicate_targets_and_echo | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; locron_session={S}; csrf_token={C}; csrf_token={D}"]; Authorization=["token {T}"]; X-CSRF-Token=["{C}","{D}"]; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Bearer |
| **PS04** / bearer_unreadable_cookie_and_echo | POST /api/v1/pr145-probe; Cookie=[{"hex":"4f504151554543414e415259ff"}]; Authorization=["token {T}"]; X-CSRF-Token=[{"hex":"48454144455243414e415259ff"}]; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Bearer |
| **PS05** / session_correct_header_metadata | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{C}"]; body="{\"probe\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **PS06** / session_correct_form_exact_restore | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&probe=BODYCANARY"; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **PS07** / session_uppercase_csrf_exact_match | POST /api/v1/pr145-probe; Cookie=["locron_session={T}; csrf_token={UPPER_C}"]; X-CSRF-Token=["{UPPER_C}"]; body="{\"probe\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 200 `probe_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=0; AuthKind=Session |
| **PS08** / initial_paste_genuine_missing_session | POST /api/v1/session; Cookie=["csrf_token=%recoverable"]; X-CSRF-Token=[""]; original typed marker, HEADERCANARY, exact request metadata | 200 `session_success`; polls=`ZERO_BEFORE_NEXT_POSITIVE_IN_NEXT`; Next=1; cookies=2; AuthKind=Unauthenticated |

## paste_auth — 11 keys

**Observation:** production_router.
**Required interpretation:** Supplied Authorization strictness precedes the paste exemption and body validation. Good body secrets cannot rescue refused Authorization/session fields. The well-formed stale Session is Unauthenticated, so its original paste exemption ignores readable CSRF/echo defects; that is not the new valid-Session recovery exception.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **PA01** / wrong_token_with_valid_session | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=["token {S}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA02** / wrong_scheme_with_valid_session | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=["Bearer {T}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA03** / empty_authorization | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=[""]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA04** / duplicate_equal_valid_authorization | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=["token {T}","token {T}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA05** / duplicate_valid_invalid_authorization | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=["token {T}","token {S}"]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA06** / unreadable_authorization | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; Authorization=[{"hex":"4155544843414e415259ff"}]; X-CSRF-Token=["{C}"] | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA07** / malformed_session_absent_authorization | POST /api/v1/session; Cookie=["locron_session=%bad; csrf_token={C}"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA08** / duplicate_session_absent_authorization | POST /api/v1/session; Cookie=["locron_session={T}; locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA09** / unreadable_cookie_absent_authorization | POST /api/v1/session; Cookie=[{"hex":"4f504151554543414e415259ff"}]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **PA10** / stale_wellformed_session_repaste | POST /api/v1/session; Cookie=["locron_session={S}; csrf_token={C}; csrf_token=%recoverable"]; X-CSRF-Token=[""] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Unauthenticated |
| **PA11** / valid_bearer_independent_paste | POST /api/v1/session; Cookie=["locron_session=%bad; locron_session={S}; csrf_token=%recoverable"]; Authorization=["token {T}"]; X-CSRF-Token=[""] | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Bearer |

## recovery_body — 12 keys

**Observation:** production_router.
**Required interpretation:** Admission passes only this selected narrow exception. Probe admission succeeds independently; real production API alone determines 200/401/400/413/415/422 and cookie issuance. Json extractor errors are existing text responses, not fabricated versioned envelopes.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **RB01** / exact_secret | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 200 `session_success`; polls=`POSITIVE`; Next=1; cookies=2; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB02** / wrong_wellformed_secret | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"{S}\"}" | 401 `secret_rejected`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB03** / uppercase_secret_wrong_case | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"{UPPER_T}\"}" | 401 `secret_rejected`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB04** / missing_token | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"other\":\"BODYCANARY\"}" | 422 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB05** / token_integer | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":17}" | 422 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB06** / token_array | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":[\"BODYCANARY\"]}" | 422 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB07** / token_object | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":{\"key\":\"BODYCANARY\"}}" | 422 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB08** / token_null | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":null}" | 422 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB09** / malformed_json | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"BODYCANARY\"" | 400 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB10** / correct_secret_form_media | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="token={T}&csrf_token={C}" | 415 `json_extractor_rejection`; polls=`ZERO`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB11** / correct_secret_missing_media | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; Content-Type=[] | 415 `json_extractor_rejection`; polls=`ZERO`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |
| **RB12** / json_over_default_2mib | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body={"construction":"{\"token\":\"{T}\",\"padding\":\"ASCII a padding\"}","exact_bytes":2097153} | 413 `json_extractor_rejection`; polls=`POSITIVE`; Next=1; cookies=0; AuthKind=Session; probe=200, actual Json/secret sets API outcome |

## boundary — 8 keys

**Observation:** production_router.
**Required interpretation:** GET/session is the only new safe-method invalid-CSRF 401. Ordinary safe/public GET and valid-CSRF paste 403/form415 remain unchanged. Derive the script from actual embedded index rather than hard-code a stale Vite hash.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **BN01** / status_genuine_missing_csrf | GET /api/v1/session; Cookie=["locron_session={T}"]; X-CSRF-Token absent | 200 `session_success`; polls=`ZERO`; Next=1; cookies=1 |
| **BN02** / status_readable_malformed_csrf | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **BN03** / status_duplicate_csrf | GET /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}; csrf_token={C}"]; X-CSRF-Token absent | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |
| **BN04** / public_entry_session_malformed_csrf_safe | GET /; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; Content-Type=[]; body="BODYCANARY" | 200 `embedded_asset_exact`; polls=`ZERO`; Next=1; cookies=0 |
| **BN05** / public_referenced_script_missing_session | GET {ENTRY_REFERENCED_SCRIPT}; Cookie absent; X-CSRF-Token absent; Content-Type=[]; body="BODYCANARY" | 200 `embedded_asset_exact`; polls=`ZERO`; Next=1; cookies=0 |
| **BN06** / valid_csrf_paste_missing_echo_json | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **BN07** / valid_csrf_paste_wrong_echo | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token=["{D}"] | 403 `csrf_echo_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **BN08** / valid_csrf_paste_good_form_json_handler_415 | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token={C}"]; X-CSRF-Token absent; Content-Type=["application/x-www-form-urlencoded"]; body="csrf_token={C}&token={T}" | 415 `json_extractor_rejection`; polls=`POSITIVE_BEFORE_NEXT`; Next=1; cookies=0 |

## guard — 7 keys

**Observation:** production_middleware_probe; production_router.
**Required interpretation:** Recovery requires genuine echo absence. Every supplied echo class remains refusal/body0. Exact URI path and POST are required; queries retain the original uri.path semantics. Body/token/header/query canaries never appear in refusal bytes.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **GR01** / recovery_present_valid_echo | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token=["{C}"]; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR02** / recovery_present_empty_malformed_echo | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token=[""]; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR03** / recovery_duplicate_equal_echo | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token=["{C}","{C}"]; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR04** / recovery_unreadable_echo | POST /api/v1/session; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token=[{"hex":"48454144455243414e415259ff"}]; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR05** / trailing_slash_is_not_recovery | POST /api/v1/session/; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR06** / encoded_path_is_not_recovery | POST /api/v1/%73ession; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 403 `csrf_cookie_refusal`; polls=`ZERO`; Next=0; cookies=0 |
| **GR07** / generic_status_canary_body_suppression | GET /api/v1/session?token={T}&path=PATHCANARY&query=QUERYCANARY; Cookie=["locron_session={T}; csrf_token=%recoverable"]; X-CSRF-Token absent; body="{\"token\":\"{T}\",\"canary\":\"BODYCANARY\"}"; original typed marker, HEADERCANARY, exact request metadata | 401 `generic_auth`; polls=`ZERO`; Next=0; cookies=0 |

## client — 14 keys

**Observation:** actual_api_and_dom_fetch_boundary.
**Required interpretation:** The stored document.cookie condition must be observed, not fabricated with a getter. DOM fetch stubs do not prove server authentication/issuer/body suppression; pair with backend rows. Keep the existing route-title test unchanged.

| Key / concrete case | Actual wire condition | Expected result |
| --- | --- | --- |
| **CL01** / literal_percent_recoverable | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token=%recoverable","body":{"token":"{T}"}} | {"fetch_calls":1,"csrf_echo":"ABSENT","body_json_exact":"{\"token\":\"{T}\"}","thrown_uri_error":false} |
| **CL02** / literal_short63 | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token={C63}"} | {"fetch_calls":1,"csrf_echo":"ABSENT"} |
| **CL03** / literal_nonhex64 | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token={NONHEX64}"} | {"fetch_calls":1,"csrf_echo":"ABSENT"} |
| **CL04** / literal_percent_encoded_hex | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token=%61{C_TAIL}"} | {"fetch_calls":1,"csrf_echo":"ABSENT"} |
| **CL05** / literal_empty_cookie | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token="} | {"fetch_calls":1,"csrf_echo":"ABSENT"} |
| **CL06** / literal_missing_cookie | {"method":"POST","path":"/api/v1/session","document_cookie":"other=OTHER"} | {"fetch_calls":1,"csrf_echo":"ABSENT"} |
| **CL07** / literal_plain_lowercase | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token={C}"} | {"fetch_calls":1,"csrf_echo":"{C}","valid_case_bytes_unchanged":true} |
| **CL08** / literal_plain_uppercase | {"method":"POST","path":"/api/v1/session","document_cookie":"csrf_token={UPPER_C}"} | {"fetch_calls":1,"csrf_echo":"{UPPER_C}","valid_case_bytes_unchanged":true} |
| **CL09** / nonliteral_query_percent_cookie_old_failure | {"method":"POST","path":"/api/v1/session?x=QUERYCANARY","document_cookie":"csrf_token=%recoverable"} | {"fetch_calls":0,"error":"existing URIError","exception_scope":"literal frontend path only"} |
| **CL10** / ordinary_put_percent_cookie_old_failure | {"method":"PUT","path":"/api/v1/pr145-probe","document_cookie":"csrf_token=%recoverable"} | {"fetch_calls":0,"error":"existing URIError"} |
| **CL11** / ordinary_post_valid_cookie_exact_bytes | {"method":"POST","path":"/api/v1/pr145-probe","document_cookie":"csrf_token={C}","body":{"canary":"BODYCANARY"}} | {"fetch_calls":1,"csrf_echo":"{C}","content_type":"application/json","body_json_exact":"{\"canary\":\"BODYCANARY\"}","valid_case_bytes_unchanged":true} |
| **CL12** / app_bootstrap_401_enters_entry | {"component":"actual App","initial_request":"GET /api/v1/session","response":"generic versioned 401","document_cookie":"csrf_token=%recoverable"} | {"entry_visible":true,"password_type":"password","authenticated_route_requests":0,"diagnostics_poll_requests":0,"dom_canary_reflection":0} |
| **CL13** / entry_real_submit_trim_recovery_handoff | {"component":"actual App + Entry + api","initial_hash":"#/jobs","bootstrap_response":"generic 401","document_cookie":"csrf_token=%recoverable","typed_password":"  {T}  ","paste_response":"200 authenticated envelope","post_auth_fetch_responses":{"/api/v1/jobs?all=1":[],"/api/v1/diagnostics":{"daemon_running":false}}} | {"paste_fetch_calls":1,"paste_path":"/api/v1/session","csrf_echo":"ABSENT","body_json_exact":"{\"token\":\"{T}\"}","password_removed_or_empty_after_success":true,"authenticated_shell_visible":true,"jobs_empty_state_visible":true,"mocked_api_methods":0,"browser_cookie_delivery_claim":false} |
| **CL14** / ordinary_403_does_not_expire_app | {"component":"actual App + Jobs + api","initial_hash":"#/jobs","document_cookie":"csrf_token={C}","bootstrap_response":"200 authenticated","job_seed":{"id":"pr145-owned-job","name":"owned-error-boundary","enabled":false,"tags":[],"definition_json":"{\"schedule\":{\"kind\":\"every\",\"interval\":1000000,\"anchor\":0}}"},"other_fetch_responses":{"/api/v1/diagnostics":{"daemon_running":false},"/api/v1/runs?job=owned-error-boundary&limit=1":{"runs":[]}},"actual_dom_action":"click the real Jobs Run now action; observe POST /api/v1/jobs/pr145-owned-job/run","ordinary_request_response":"versioned 403 refused"} | {"error":"existing Jobs Feedback shows the ApiError message","ordinary_post_fetch_calls":1,"csrf_echo":"{C}","session_expired_events":0,"entry_visible":false,"authenticated_shell_retained":true} |

## Completion, ownership and build boundary

Use a literal expected-key set for each group; reject zero selection, missing/duplicate/extra IDs and skips. The twelve backend group drivers repeat the same184 keys independently on required native runners; fourteen client cases use actual api.ts/App/DOM in hosted JSDOM. These are not198 new test functions and are not native browser/account acceptance.

Future fixture ownership: adopt only actual successful creation, record actual root/leaf identities, disable implicit recursive cleanup, snapshot all owned inventory before/after requests, stop/reap any owned task before teardown, remove only known leaves with matching no-follow identity, then remove owned empty directories. A foreign replacement or uncertain late completion is a failure/quarantine, never recursive deletion or repaired permissions. No Store/Engine/installed token/service import belongs in this slice.

Production limits stay 1,048,576-byte form buffering and Axum's 2,097,152-byte Json default. Largest finite request is 2,097,153 bytes. Proposed test-only clocks: group60s from before fixture creation, request at most5s of remaining budget, response1MiB, DOM wait2s/case10s, teardown at most5s of remaining group budget. These do not change production policy, reset native entry deadlines or authorize retry.

Root must select Docs/Issue and the generated-assets route before Source. Preferred future route is one explicitly permitted pinned local frontend generation in a separate developer worktree. Alternatively lease a hosted artifact-generation job; current ordinary frontend CI has no dist upload path and must keep its exact-diff gates. Neither route ran here. See JSON/report for commands, protected modes/blobs and limitations.
