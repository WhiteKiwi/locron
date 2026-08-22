<p align="center">
  <img src="assets/banner.jpg" alt="locron banner" width="800">
</p>

<h1 align="center">locron</h1>

<p align="center">
  <strong>Local-first cron jobs, made simple.</strong>
</p>

<p align="center">
  <a href="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml"><img src="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/WhiteKiwi/locron/releases"><img src="https://img.shields.io/github/v/release/WhiteKiwi/locron?color=blue" alt="Latest Release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License"></a>
  <a href="#installation"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg" alt="Platform"></a>
  <a href="#build-from-source"><img src="https://img.shields.io/badge/rust-1.94%2B-orange.svg" alt="Rust Version"></a>
</p>

---

`locron` is a local-first job scheduler for **macOS** and **Linux**. It keeps schedules, durable run identities, execution history, and bounded captured output in a private per-user state directory instead of scattering jobs across operating-system crontabs, launchd plists, or systemd units.

---

## ✨ Features

- ⚡ **Local-First & Independent**: Everything runs locally on your machine with zero external service dependencies.
- 🕒 **Flexible Scheduling**: Standard 5-field cron expressions, fixed intervals (second-level resolution), and one-time executions.
- 🛡️ **Predictable Policies**: Explicit per-job overlap policies (`skip`, `replace`, `allow`) and missed-run policies (`skip`, `latest`, `all`).
- 🔍 **First-Class Observability**: Understand *why* any job ran, was skipped, or failed with `locron why` and `locron doctor`.
- 🌳 **Process Group Supervision**: Supervise child processes with configurable timeouts, SIGTERM/SIGKILL grace periods, and bounded output capture.
- 💾 **Durable & Crash-Safe**: Powered by bundled SQLite WAL transactions with atomic migrations and restartable recovery.
- 🌐 **HTTP Target Support**: Trigger webhooks and HTTP endpoints natively with retry handling and header management.

---

## 📦 Installation

### Homebrew (macOS & Linux)

The easiest way to install `locron` on macOS and Linux is via [Homebrew Tap](https://github.com/WhiteKiwi/homebrew-tap):

```sh
brew tap whitekiwi/tap
brew install locron
```

Or in a single command:

```sh
brew install whitekiwi/tap/locron
```

### Linux Packages (Debian / Ubuntu / Fedora / RHEL)

Pre-built `.deb` and `.rpm` packages are available on the [GitHub Releases](https://github.com/WhiteKiwi/locron/releases) page:

```sh
# Debian / Ubuntu
sudo dpkg -i locron_<version>_amd64.deb   # for x86_64
sudo dpkg -i locron_<version>_arm64.deb   # for ARM64

# Fedora / RHEL / Rocky Linux
sudo rpm -i locron-<version>.x86_64.rpm  # for x86_64
sudo rpm -i locron-<version>.aarch64.rpm # for ARM64
```

### Pre-built Binary Tarballs

Download the appropriate archive for your platform from [GitHub Releases](https://github.com/WhiteKiwi/locron/releases), unpack it, and move the binary to your `$PATH`:

```sh
tar -xzf locron-v<version>-<target>.tar.gz
sudo mv locron /usr/local/bin/
```

### Build from Source

Requires **Rust 1.94+**:

```sh
git clone https://github.com/WhiteKiwi/locron.git
cd locron
cargo build --release -p locron-cli
sudo cp target/release/locron /usr/local/bin/
```

---

## 🚀 Quick Start

### 1. Start the Scheduler Daemon

Start the long-lived background scheduler process:

```sh
locron daemon run
```

> **Tip**: You can use your system's process manager (such as `launchd` on macOS or `systemd --user` on Linux) to keep `locron daemon run` running automatically in the background.

### 2. Register Scheduled Jobs

```sh
# Direct argv execution every 15 minutes
locron add fetch-repo --every 15m -- git -C ~/projects/app fetch

# Explicit shell execution at 03:00 every night (IANA timezone supported)
locron add nightly-backup --cron "0 3 * * *" --timezone Asia/Seoul --shell "./scripts/backup.sh"

# Native HTTP health check every 5 minutes
locron add health-check --every 5m --http GET https://example.com/health

# One-time scheduled execution with explicit offset
locron add deploy-task --at 2026-09-01T09:00:00+09:00 -- /usr/local/bin/deploy
```

### 3. Inspect and Manage

```sh
# List all registered jobs
locron list

# Preview future execution times before enabling
locron preview nightly-backup --count 5

# Trigger a manual execution immediately
locron run nightly-backup

# Stream logs of the latest run
locron logs nightly-backup --follow

# Inspect execution history
locron history nightly-backup

# Ask WHY a job is in its current state
locron why nightly-backup

# Diagnose scheduler daemon and state directory health
locron doctor
```

---

## 🔒 Safety and Inspection

- **Dry-run Validation**: Validate syntax and schedule policies without mutating state:
  ```sh
  locron add test-job --cron "0 12 * * *" --dry-run -- /usr/bin/true
  ```
- **Machine Output**: Use `--json` for structured, versioned `locron.cli/v1` machine-readable output.
- **State Directory**: Default state directory is located at `~/.local/share/locron` (or `$XDG_DATA_HOME/locron`). Override with `--state-dir PATH` or `LOCRON_STATE_DIR`.

---

## 📚 Documentation

For in-depth guides and architectural references:

- 📖 **[Operator Guide](docs/OPERATOR.md)**: Daily operations, policy configuration, and troubleshooting.
- 💻 **[CLI Reference](docs/CLI.md)**: Comprehensive command-line reference and options.
- 🏗️ **[Architecture](docs/ARCHITECTURE.md)**: System architecture, invariants, and durable state model.
- 📦 **[Release & CI/CD Policy](docs/RELEASE.md)**: Semantic versioning, packaging, and release automation.
- 📋 **[Milestone 1 Specification](docs/SPEC.md)**: Frozen product goals and completion criteria.

---

## 📄 License

Dual-licensed under either of:

- **MIT License** ([`LICENSE-MIT`](LICENSE-MIT))
- **Apache License, Version 2.0** ([`LICENSE-APACHE`](LICENSE-APACHE))

at your option.
