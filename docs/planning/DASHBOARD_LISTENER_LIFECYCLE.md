# Dashboard listener lifecycle corrections

Refs #30. Base: main `46445881e320f440c02d94d7e9e5838033933fea`.

The frozen dashboard SPEC requires loopback-only binding, truthful startup/exit, fixed service ports and finite cooperative shutdown. This is an implementation correction under those existing requirements, not a new network exposure or service policy.

## Source-established gaps

`bind` parses arbitrary IP addresses without enforcing its documented loopback-only contract. Its foreground range uses `preferred + 10` in u16 arithmetic, which cannot represent the upper end of the port range. `serve_until` does not observe listener termination until an external shutdown signal, and a signal-registration error returns before broadcasting the existing shutdown notification. During drain, an early listener error returns before the remaining listener tasks have been joined.

## Implementation and Verify handoff

1. Validate the entire address list before opening a listener; reject non-loopback IPs with InvalidInput. Bound foreground candidates with inclusive saturating arithmetic, retaining at most ten candidates followed by the existing ephemeral fallback. Verify direct-library non-loopback/mixed-address refusal before socket creation, IPv4/IPv6 success, fixed-port conflict, and foreground ports 65526 through 65535 without overflow or wrap into low ports.
2. Observe the first listener result alongside Ctrl-C and composition-owned shutdown. An unexpected clean exit, I/O error or join failure initiates the same cooperative broadcast and surfaces as an error. A console-signal registration error also goes through drain. Verify an injected failed listener cannot leave a service waiting forever for Ctrl-C; all surviving listeners receive shutdown and the originating error remains visible.
3. Drain every remaining listener while retaining the first error instead of returning at the first failed join. Preserve one existing ten-second transport deadline, abort-and-join behavior on expiry, existing timeout text, and the Windows headless-console fallback. Verify error-plus-live-listener, normal cooperative shutdown, active SSE, and stalled header/body cases. Run Rust formatting, server library/contract tests and the existing native dashboard lifecycle gates before merge.

The requested deliverable is an implementation Draft. Regression-test additions, native execution and complete CI qualification are explicit reviewer handoff items rather than claimed completed work. No dependency, public AppState shape, HTTP route, scheduler, Store or task-registration changes are planned.

This does NOT establish cleanup of Axum-detached connections or started spawn_blocking Store workers. The separate shutdown-ownership acceptance notes in #30 remain open. No installation, release, merge or issue closure is part of this change. No independent development/review sub-session is available in this chat; independent review remains a Draft gate.
