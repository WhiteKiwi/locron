# PR145 selected public-entry exception controls

These six controls supplement every original198 control; none replaces or weakens one. The selected union is204=backend190+client14, with13 backend drivers. All are UNMEASURED.

| Key | Actual Cookie field | Actual route | Expected actual observations |
| --- | --- | --- | --- |
| PUBLIC-MALFORMED-ENTRY | one readable `locron_session=not-hex` | GET `/` | 200; AuthKind::Unauthenticated; actual request body polls0; Next1; Set-Cookie0; full embedded index bytes |
| PUBLIC-MALFORMED-SCRIPT | one readable `locron_session=not-hex` | GET actual index-referenced `/assets/*.js` | 200; Unauthenticated; body polls0; Next1; Set-Cookie0; full embedded referenced JS bytes |
| PUBLIC-DUPLICATE-ENTRY | one readable `locron_session={T}; locron_session={T}` with actual synthetic valid T | GET `/` | 200; Unauthenticated; body polls0; Next1; Set-Cookie0; full embedded index bytes |
| PUBLIC-DUPLICATE-SCRIPT | one readable `locron_session={T}; locron_session={T}` | GET actual index-referenced `/assets/*.js` | 200; Unauthenticated; body polls0; Next1; Set-Cookie0; full embedded referenced JS bytes |
| PUBLIC-UNREADABLE-ENTRY | actual HeaderValue bytes hex `6c6f63726f6e5f73657373696f6e3dff` | GET `/` | 200; Unauthenticated; body polls0; Next1; Set-Cookie0; full embedded index bytes |
| PUBLIC-UNREADABLE-SCRIPT | actual HeaderValue bytes hex `6c6f63726f6e5f73657373696f6e3dff` | GET actual index-referenced `/assets/*.js` | 200; Unauthenticated; body polls0; Next1; Set-Cookie0; full embedded referenced JS bytes |

All six omit Authorization and X-CSRF-Token and use exactly one allowed Host with the synthetic AppState token T. Derive the actual existing lowercase-js asset from Assets::get(index.html) before measurement, verify Assets::get(reference), and assert the complete served bytes/content type/cache/referrer/security headers. No guessed stale app hash, fallback asset, copied content, bearer-positive replacement or fake public handler is accepted. Observe AuthKind/Next/body with the actual five-middleware probe in the real layer order and independently use the real production Router for exact asset responses. These two calls are separate documented observations of the same row; production handler count is not inferred from file absence. Hold the group fixture/identity/guards and check effects0/cleanup before completing these keys under the same60s group clock.

The exception preserves original public GET outside `/api/`; malformed, duplicate and unreadable session states stay strict401/body0 for protected API and POST paste. Invalid supplied Authorization remains401 even for public entry. No general mutation/session exemption is added.
