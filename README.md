<p align="center">
  <img src="assets/banner.jpg" alt="locron banner" width="800">
</p>

<h1 align="center">locron</h1>

<p align="center">
  <strong>Local-first cron jobs, made simple.</strong>
</p>

<p align="center">
  <a href="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml"><img src="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/WhiteKiwi/locron/releases"><img src="https://img.shields.io/github/v/release/WhiteKiwi/locron?color=blue" alt="Latest release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License"></a>
</p>

---

`locron` is a local-first job scheduler for **macOS** and **Linux**. It keeps schedules, durable run identities, execution history, and bounded captured output in a private per-user state directory — instead of scattering jobs across operating-system crontabs, launchd plists, and systemd units.

When a job does not run, you get an answer instead of silence: `locron why` explains the decision that was made, and `locron doctor` reports the health of the daemon and the state directory.

---

## Features

- ⚡ **Local-first and independent** — everything runs on your machine, with no external service.
- 🕒 **Flexible scheduling** — 5-field cron expressions, fixed intervals at second resolution, and one-time executions, with IANA timezones and DST duplicate safety.
- 🛡️ **Predictable policies** — explicit per-job overlap (`skip`, `replace`, `allow`) and missed-run (`skip`, `latest`, `all`) behavior.
- 🔍 **Explainable** — `locron why` tells you why a job ran, was skipped, or failed.
- 🌳 **Real process supervision** — process groups, configurable timeouts, SIGTERM/SIGKILL grace periods, and bounded output capture.
- 💾 **Durable and crash-safe** — bundled SQLite with WAL transactions, atomic migrations, and restartable recovery.
- 🌐 **HTTP targets** — call webhooks and health endpoints natively, with retry handling and header management.

---

## Installation

### Homebrew (macOS and Linux)

```sh
brew tap whitekiwi/tap && brew trust whitekiwi/tap && brew install locron
```

Newer Homebrew requires `brew trust` for third-party taps.

### Install script (macOS and Linux)

Installs the latest release into `~/.local/bin/locron` after verifying it against the release's `SHA256SUMS.txt`:

```sh
curl -fsSL https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh | sh
```

Set `LOCRON_VERSION` to pin a version, or `LOCRON_INSTALL_DIR` to install elsewhere. The script never edits your shell configuration — it prints the `PATH` line to add if one is needed.

### Debian, Ubuntu, Fedora, RHEL

Download the package for your architecture from [Releases](https://github.com/WhiteKiwi/locron/releases):

```sh
sudo dpkg -i locron_<version>_amd64.deb    # or _arm64.deb
sudo rpm -i locron-<version>.x86_64.rpm    # or .aarch64.rpm
```

### Pre-built binaries

Tarballs for macOS and Linux on x86_64 and aarch64 are attached to every [release](https://github.com/WhiteKiwi/locron/releases):

```sh
tar -xzf locron-v<version>-<target>.tar.gz && sudo mv locron /usr/local/bin/
```

### From source

Requires Rust 1.94 or newer:

```sh
git clone https://github.com/WhiteKiwi/locron.git && cd locron
cargo build --release -p locron-cli
sudo cp target/release/locron /usr/local/bin/
```

### Updating

| Installed with | Update with |
| --- | --- |
| Homebrew | `brew upgrade locron` |
| Install script or tarball | `locron self-update` |
| `.deb` / `.rpm` | Install the new package |

`locron self-update` verifies the checksum and replaces the binary atomically. A running `locron daemon run` keeps the old code until you restart it.

---

## Quick start

**1. Start the scheduler daemon.**

```sh
locron daemon run
```

Keep it running with your system's process manager — `launchd` on macOS, `systemd --user` on Linux.

**2. Add some jobs.**

```sh
# Run a command every 15 minutes
locron add fetch-repo --every 15m -- git -C ~/projects/app fetch

# Run a shell script nightly at 03:00 Seoul time
locron add nightly-backup --cron "0 3 * * *" --timezone Asia/Seoul --shell "./scripts/backup.sh"

# Poll an HTTP endpoint every 5 minutes
locron add health-check --every 5m --http GET https://example.com/health

# Run once, at a specific instant
locron add deploy-task --at 2026-09-01T09:00:00+09:00 -- /usr/local/bin/deploy
```

**3. Inspect and manage.**

```sh
locron list                              # every registered job
locron preview nightly-backup --count 5  # the next 5 execution times
locron run nightly-backup                # trigger one now
locron logs nightly-backup --follow      # stream the latest run
locron history nightly-backup            # past runs and outcomes
locron why nightly-backup                # why is it in this state?
locron doctor                            # daemon and state directory health
```

**Before you commit to a job**, validate it without writing anything:

```sh
locron add test-job --cron "0 12 * * *" --dry-run -- /usr/bin/true
```

Every command takes `--format json` for versioned, machine-readable `locron.cli/v1` output. State lives in `~/.local/share/locron` (or `$XDG_DATA_HOME/locron`), overridable with `--state-dir` or `LOCRON_STATE_DIR`.

---

## MCP integration

`locron mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io) over stdio, so Claude Desktop, Cursor, and other MCP clients can schedule and diagnose jobs through the same application boundary as the CLI — the same validation, redaction, and durable transactions.

It exposes **13 tools**, **5 resources**, and **2 prompts**. Every mutating tool accepts `"dry_run": true`, and domain failures come back as tool errors with `isError: true` rather than protocol errors, so the assistant can read the reason and act on it.

Register it with any MCP client using the same entry — `claude_desktop_config.json` for Claude Desktop, `.cursor/mcp.json` or Cursor Settings → MCP for Cursor:

```json
{
  "mcpServers": {
    "locron": {
      "command": "locron",
      "args": ["mcp"]
    }
  }
}
```

If `locron` is not on the client's `PATH`, use an absolute path. See [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) for the full tool, resource, and prompt reference.

---

## Documentation

- **[Operator Guide](docs/OPERATOR.md)** — daily operations, policy configuration, and troubleshooting.
- **[CLI Reference](docs/CLI.md)** — every command, option, and output contract.
- **[Architecture](docs/ARCHITECTURE.md)** — system design, invariants, and the durable state model.
- **[MCP Specification](docs/MCP_SPEC.md)** — tools, resources, and prompts in full.
- **[Release Policy](docs/RELEASE.md)** — versioning, packaging, and release automation.
- **[Changelog](CHANGELOG.md)** — notable changes in each release.

---

## Contributing

Contributions are welcome. `locron` is developed **documentation-first** — the planning documents change before the code does — so please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

- [Report a bug or propose a feature](https://github.com/WhiteKiwi/locron/issues/new/choose)
- [Report a security vulnerability privately](https://github.com/WhiteKiwi/locron/security/advisories/new) — never in a public issue ([`SECURITY.md`](SECURITY.md))

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md) code of conduct.

---

## License

Dual-licensed under either of:

- MIT License ([`LICENSE-MIT`](LICENSE-MIT))
- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
