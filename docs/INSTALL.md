# locron Installation Guide

Every supported installation channel, plus how to update and uninstall each one. For running and
supervising the daemon once it is installed, see the [Operator Guide](OPERATOR.md); for the exact
command contracts mentioned here, see the [CLI Reference](CLI.md).

`locron` supports **macOS** and **Linux** on **x86_64** and **aarch64**. Windows is not a supported
target.

## Homebrew (macOS and Linux)

```sh
brew install whitekiwi/tap/locron
```

Homebrew 6 requires trust for third-party taps. Installing by fully-qualified name auto-taps the
repository and records trust for this formula alone — no separate `brew tap` or `brew trust`
step, and nothing else in the tap is trusted.

The Homebrew formula ships a `service` block, so `brew services` supervises the daemon.
Installation never starts it automatically:

```sh
brew services start locron
```

`brew upgrade` leaves a running service on the old version — run `brew services restart locron`
after an upgrade. On Homebrew (and all package-managed installs), `locron service install|uninstall`
and `locron self-update` refuse and point at the package manager. The full registration semantics
are in the [Operator Guide](OPERATOR.md#run-the-daemon-as-a-service).

## Install script (macOS and Linux)

Installs the latest release into `~/.local/bin/locron` after verifying it against the release's
`SHA256SUMS.txt`, then registers the daemon as a per-user service (a LaunchAgent on macOS, a
systemd user unit on Linux) so schedules run immediately:

```sh
curl -fsSL https://locron.whitekiwi.link/install.sh | sh
```

The short URL 302-redirects to the canonical release asset
`https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh`, so the script served is
always the one attached to the latest release — the GitHub URL is the canonical one-liner.

Set `LOCRON_VERSION` to pin a version, `LOCRON_INSTALL_DIR` to install elsewhere, or
`LOCRON_NO_SERVICE=1` to skip service registration (registration is best-effort — a failure warns
and keeps the install successful; retry with `locron service install`). The script never edits your
shell configuration — it prints the `PATH` line to add if one is needed.

The installer writes an owner-only receipt beside the binary. That receipt positively identifies
the standalone channel to `locron self-update`. An older script installation can adopt it by
rerunning the installer once.

## Cargo (Rust users)

Requires Rust 1.94 or newer and builds from source:

```sh
cargo install --locked locron
```

Cargo installs only the `locron` executable. It does not register a daemon or dashboard service;
those remain available when wanted:

```sh
locron service install
locron dashboard enable
```

Update with `cargo install --locked locron` and remove with `cargo uninstall locron`.
`locron self-update` refuses Cargo installations before network access because Cargo owns the
binary lifecycle.

## Debian, Ubuntu, Fedora, RHEL

Download the package for your architecture from
[Releases](https://github.com/WhiteKiwi/locron/releases):

```sh
sudo dpkg -i locron_<version>_amd64.deb    # or _arm64.deb
sudo rpm -i locron-<version>.x86_64.rpm    # or .aarch64.rpm
```

Package installs never register the daemon automatically; the package prints guidance on how to
register and start it from a login session.

## Pre-built binaries

Tarballs for macOS and Linux on x86_64 and aarch64 are attached to every
[release](https://github.com/WhiteKiwi/locron/releases). Download the archive for your platform,
verify it against the release's `SHA256SUMS.txt`, and unpack:

```sh
sha256sum -c SHA256SUMS.txt          # Linux — or: shasum -a 256 -c SHA256SUMS.txt (macOS)
tar -xzf locron-v<version>-<target>.tar.gz
sudo mv locron-v<version>-<target>/locron /usr/local/bin/
```

Manually copied tarballs are not owned by the standalone installer, so `locron self-update`
refuses them. Download and replace the archive again to update.

## From source

Requires Rust 1.94 or newer:

```sh
git clone https://github.com/WhiteKiwi/locron.git && cd locron
cargo build --release -p locron
sudo cp target/release/locron /usr/local/bin/
```

The pinned toolchain in `rust-toolchain.toml` selects the right Rust version automatically. See
[CONTRIBUTING.md](../CONTRIBUTING.md#getting-started) for the full development setup.

## Updating

| Installed with | Update with |
| --- | --- |
| Homebrew | `brew upgrade locron` |
| Install script | `locron self-update` |
| Cargo | `cargo install --locked locron` |
| Tarball / source | Rebuild or replace the binary through the same channel |
| `.deb` / `.rpm` | Install the new package |

`locron self-update` verifies the checksum and replaces the binary atomically. A running
`locron daemon run` keeps the old code until you restart it. Per-channel update semantics — when a
service registration is refreshed, and why package-managed installs refuse self-update — are in the
[Operator Guide](OPERATOR.md#updating-locron).

## Uninstalling

- **Homebrew:** `brew services stop locron` (if running), then `brew uninstall locron`. Remove the
  tap with `brew untap whitekiwi/tap` if nothing else uses it.
- **Install script:** `locron service uninstall` stops the daemon and removes the registration,
  then delete the binary and its sibling `.locron-install-receipt-v1`.
- **Cargo:** `locron service uninstall` if registered, then `cargo uninstall locron`.
- **Debian / RPM:** `sudo dpkg -r locron` or `sudo rpm -e locron`.
- **Tarball / source:** `locron service uninstall` (if you registered it), then remove the binary
  from `$PATH`.

None of these remove the state directory: schedules and history live in `~/.local/share/locron`
(or `$XDG_DATA_HOME/locron`), so delete it explicitly if you want a complete removal.

## Verify the installation

```sh
locron --version   # prints the installed version
locron doctor      # daemon and state directory health
```

Then follow the [Quick start](../README.md#quick-start) to add your first job.
