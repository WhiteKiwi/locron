# Security Policy

## Supported versions

`locron` is pre-1.0. Security fixes are released against the latest published minor version only;
there are no long-term support branches.

| Version | Supported |
| --- | --- |
| 0.2.x | ✅ |
| < 0.2 | ❌ — upgrade to the latest release |

Releases follow the versioning and remediation rules in [`docs/RELEASE.md`](docs/RELEASE.md).

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues, pull requests, or
discussions.**

Report privately through GitHub Security Advisories:

👉 **[Report a vulnerability](https://github.com/WhiteKiwi/locron/security/advisories/new)**

(Repository → **Security** tab → **Report a vulnerability**.)

This creates a private advisory visible only to you and the maintainer.

### What to include

The more of this you can provide, the faster a fix lands:

- The version — output of `locron --version`.
- Operating system and architecture (for example macOS 15 arm64, Ubuntu 24.04 x86_64).
- Install method (Homebrew, `.deb`/`.rpm`, tarball, built from source).
- Reproduction steps, ideally a minimal job definition or command sequence.
- What an attacker gains: which boundary is crossed, and what they must already have to exploit it.
- Any proof-of-concept output, with secrets redacted.

### What to expect

- **Acknowledgement within 7 days** that the report was received.
- **An initial assessment within 14 days** — whether it is accepted, and a rough severity.
- Fixes are shipped as a patch release, and the advisory is published on GitHub once users have a
  release to move to.
- You will be credited in the advisory unless you ask not to be.

This project is maintained by one person in their own time. If you have not heard back within the
windows above, a polite nudge on the advisory thread is welcome.

## Scope

`locron` runs as an unprivileged user process, supervises child processes, and owns durable state in
a per-user state directory. Reports that cross one of these boundaries are in scope:

- **Privilege or user boundary.** Another local user reading, modifying, or deleting the state
  directory, job definitions, execution history, or captured output; any path that lets an
  unprivileged process influence jobs owned by a different user.
- **Daemon ownership.** Taking over daemon lifetime or lock ownership for a state directory the
  attacker does not own, or forcing another user's daemon to execute an attacker-chosen target.
- **Unintended command execution.** Getting `locron` to execute a command, argument vector, or shell
  string other than the one configured — including through job import, configuration parsing, or
  schedule/target syntax — beyond the documented shell semantics in [`docs/CLI.md`](docs/CLI.md).
- **Secret disclosure.** HTTP target headers, environment values, or other configured secrets
  leaking into logs, execution history, diagnostics, or exports where the documented behavior says
  they should not appear.
- **State integrity exploited for execution.** SQLite state corruption or a migration flaw that can
  be steered into arbitrary command execution or into silently running jobs the user disabled.
- **Supply chain.** Compromise of the release pipeline, published artifacts, or checksum integrity.

## Out of scope

These are working as designed, or are not vulnerabilities in `locron`:

- **`locron` runs the commands you configure.** A job set up to run a destructive command doing
  exactly that is the product working correctly, not a vulnerability. The trust boundary is the user
  account: anyone who can already write to your state directory or run commands as you can already
  do anything `locron` can.
- Resource exhaustion caused by the user's own configuration — extreme schedules, unbounded
  concurrency, or targets that generate enormous output. Report these as ordinary bugs if a
  documented bound is not being honored.
- Vulnerabilities in dependencies that are already public and already fixed upstream. Open a normal
  issue, or let the scheduled dependency audit pick them up.
- Findings that require an attacker to already have root, or to already control the user account.
- Missing hardening that has no demonstrated impact (for example "binary is not PIE"), absent a
  concrete exploitation path.

If you are unsure whether something is in scope, report it privately anyway. A short conversation is
cheaper for both of us than a public disclosure that turned out to be avoidable.
