# locron Backlog

This document preserves deferred ideas that are not active commitments or implementation TODOs.
Move an item back to `docs/TODO.md` only after deciding to pursue it and completing the repository's
planning workflow: update `docs/SPEC.md` when observable product behavior or scope changes, record
research in `docs/FINDINGS.md` when needed, then review `docs/IMPLEMENTATION.md` and add a verified
implementation checklist to `docs/TODO.md` before changing code.

## README demo screencast

- Generate `assets/screencast.svg` from the verified `assets/screencast.sh` recording and embed it
  beneath the README badges. The script records `add → list → preview → run → history →
  why → doctor` against an isolated throwaway state directory. Rendering currently requires
  `svg-term` (`npm install -g svg-term-cli`) or an equivalent GIF workflow such as `vhs`.
- Before publishing, confirm that the full sequence plays in a browser and on GitHub, the README
  image renders at the repository front page, and no recording-host paths, machine names, or local
  state appear in the result. The intended embed is
  `<p align="center"><img src="assets/screencast.svg" alt="locron demo" width="800"></p>`.

## Local usage statistics

- Consider a future `locron stats` command that aggregates the user's durable local run history.
  This is separate from the maintainer-facing `scripts/usage.sh`, which measures public distribution
  channels.
- Reactivation requires a reviewed product specification covering the exact metrics, aggregation
  windows, output contract, retention interactions, performance bounds, and redaction/privacy
  behavior before implementation planning begins.

## Distribution usage history

- Consider persisting dated output from `scripts/usage.sh --json` only when historical trends are
  worth the repository noise and write permissions. The existing weekly read-only workflow checks
  the live snapshot but deliberately does not commit a history file.
- Reactivation must choose a retention location and cadence, account for third-party API failures
  and count resets, and avoid making live analytics a push or release gate.
