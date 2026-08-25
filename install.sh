#!/bin/sh
# locron installer — one-line install and update for macOS and Linux.
#
#   curl -fsSL https://locron.whitekiwi.link/install.sh | sh
#
# (a short URL that 302-redirects here; the canonical one-liner is
#  https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh)
#
# Resolves the latest published release through static `releases/latest/download`
# redirects (no GitHub API), verifies the selected archive against the release's
# SHA256SUMS.txt, and atomically replaces the target binary. Re-running the same
# command replaces the binary with the latest release; this is the update path
# for script-installed locron.
#
# Configuration:
#   LOCRON_VERSION        Pin a release tag such as v0.1.1 instead of latest.
#   LOCRON_INSTALL_DIR    Full path of the installed binary; defaults to
#                         $HOME/.local/bin/locron. Must be a file path, not a
#                         directory.
#   LOCRON_UPDATE_ASSET_BASE
#                         Base URL for release assets; tests point this at a
#                         local fixture. Defaults to the GitHub release host.
#   LOCRON_NO_SERVICE     Set to 1 to skip registering the installed binary as a
#                         login service (launchd/systemd user). Registration is
#                         best-effort: a failure warns and keeps the install
#                         successful, and `locron service install` can retry it.

set -eu

LOCRON_VERSION="${LOCRON_VERSION:-}"
LOCRON_INSTALL_DIR="${LOCRON_INSTALL_DIR:-${HOME:-}/.local/bin/locron}"
LOCRON_UPDATE_ASSET_BASE="${LOCRON_UPDATE_ASSET_BASE:-https://github.com/WhiteKiwi/locron}"

if [ -d "$LOCRON_INSTALL_DIR" ]; then
    echo "error: LOCRON_INSTALL_DIR must be a file path, not a directory: $LOCRON_INSTALL_DIR" >&2
    exit 1
fi
install_path="$LOCRON_INSTALL_DIR"
install_dir=$(dirname "$install_path")

fail() {
    echo "error: $*" >&2
    exit 1
}

# --- platform detection ------------------------------------------------------

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Darwin) os_rust="apple-darwin" ;;
    Linux) os_rust="unknown-linux-gnu" ;;
    *)
        fail "unsupported operating system '$os'; locron publishes builds for macOS and Linux"
        ;;
esac

case "$arch" in
    x86_64) arch_rust="x86_64" ;;
    arm64 | aarch64) arch_rust="aarch64" ;;
    *)
        fail "unsupported CPU architecture '$arch'; locron publishes aarch64 and x86_64 builds"
        ;;
esac

target="${arch_rust}-${os_rust}"

if [ "$os" = "Linux" ] && command -v ldd >/dev/null 2>&1 \
    && ldd --version 2>&1 | head -n 1 | grep -qi musl; then
    fail "musl-based Linux is not supported by the published locron builds (glibc only); use a glibc-based distribution or a package manager"
fi

# --- download and verification helpers ----------------------------------------

download() {
    if command -v curl >/dev/null 2>&1; then
        if ! curl -fsSL "$1" -o "$2"; then
            fail "failed to download $1; check the network connection and re-run the installer"
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget -q -O "$2" "$1"; then
            fail "failed to download $1; check the network connection and re-run the installer"
        fi
    else
        fail "this installer needs curl or wget to download locron; install one of them and re-run"
    fi
}

verify_checksums() {
    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$tmpdir" && sha256sum -c target.sha256 >/dev/null 2>&1)
    elif command -v shasum >/dev/null 2>&1; then
        (cd "$tmpdir" && shasum -a 256 -c target.sha256 >/dev/null 2>&1)
    else
        fail "this installer needs sha256sum or shasum to verify the download"
    fi
}

# --- resolve the release and target archive -----------------------------------

if [ -n "$LOCRON_VERSION" ]; then
    case "$LOCRON_VERSION" in
        v*) ;;
        *) fail "LOCRON_VERSION must look like a release tag such as v0.1.1 (got '$LOCRON_VERSION')" ;;
    esac
    base_url="$LOCRON_UPDATE_ASSET_BASE/releases/download/$LOCRON_VERSION"
    tarball="locron-${LOCRON_VERSION}-${target}.tar.gz"
else
    base_url="$LOCRON_UPDATE_ASSET_BASE/releases/latest/download"
    tarball=""
fi

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/locron-install.XXXXXX")
tmp_bin=""
tmp_receipt=""
trap 'rm -rf "$tmpdir" "$tmp_bin" "$tmp_receipt"' EXIT INT TERM HUP

download "$base_url/SHA256SUMS.txt" "$tmpdir/SHA256SUMS.txt"

if [ -n "$tarball" ]; then
    checksum_line=$(grep -F "$tarball" "$tmpdir/SHA256SUMS.txt" || true)
    if [ -z "$checksum_line" ]; then
        fail "checksum entry for $tarball is missing from SHA256SUMS.txt; the release may be incomplete"
    fi
else
    checksum_line=$(grep -E "locron-.*-${target}\.tar\.gz$" "$tmpdir/SHA256SUMS.txt" || true)
    if [ -z "$checksum_line" ]; then
        fail "no archive for $target found in SHA256SUMS.txt; the release may be incomplete"
    fi
    if [ "$(printf '%s\n' "$checksum_line" | grep -c .)" -ne 1 ]; then
        fail "expected exactly one archive for $target in SHA256SUMS.txt"
    fi
    tarball=$(printf '%s' "$checksum_line" | awk '{print $2}')
fi

expected=$(printf '%s' "$checksum_line" | awk '{print $1}')
case "$expected" in
    *[!0-9a-fA-F]*)
        fail "malformed checksum for $tarball in SHA256SUMS.txt: '$expected' is not hexadecimal"
        ;;
esac
if [ "${#expected}" -ne 64 ]; then
    fail "malformed checksum for $tarball in SHA256SUMS.txt: expected 64 hex characters, got '${expected}'"
fi

# --- download, verify, and extract --------------------------------------------

download "$base_url/$tarball" "$tmpdir/$tarball"

grep -F "$tarball" "$tmpdir/SHA256SUMS.txt" > "$tmpdir/target.sha256"
if ! verify_checksums; then
    fail "checksum verification failed for $tarball (expected $expected); the download may be corrupted, re-run the installer"
fi

if ! tar -xzf "$tmpdir/$tarball" -C "$tmpdir"; then
    fail "failed to extract $tarball; the download may be corrupted, re-run the installer"
fi
binary="$tmpdir/${tarball%.tar.gz}/locron"
if [ ! -f "$binary" ]; then
    fail "the archive $tarball does not contain a locron binary"
fi

# --- install atomically ---------------------------------------------------------

if ! mkdir -p "$install_dir"; then
    fail "cannot create install directory $install_dir; choose another location with LOCRON_INSTALL_DIR"
fi
if [ ! -w "$install_dir" ]; then
    fail "install directory $install_dir is not writable; choose another location with LOCRON_INSTALL_DIR"
fi

tmp_bin="$install_dir/.locron.$$.tmp"
if ! cp "$binary" "$tmp_bin"; then
    fail "cannot write to $install_dir; choose another location with LOCRON_INSTALL_DIR"
fi
chmod 755 "$tmp_bin"
if ! mv -f "$tmp_bin" "$install_path"; then
    rm -f "$tmp_bin"
    fail "cannot replace $install_path; check that the directory is writable or choose another location with LOCRON_INSTALL_DIR"
fi
if ! "$install_path" -V >/dev/null 2>&1; then
    fail "installed $install_path but it cannot be executed; the filesystem may be mounted noexec — choose another location with LOCRON_INSTALL_DIR"
fi

# Positive ownership for built-in self-update. Write the exact, versioned
# payload with owner-only permissions and publish it with a same-directory
# rename only after the verified binary replacement can execute.
receipt_path="$install_dir/.locron-install-receipt-v1"
tmp_receipt="$install_dir/.locron-install-receipt-v1.$$.tmp"
if ! (umask 077 && printf 'locron.install/v1\nstandalone\n' > "$tmp_receipt"); then
    fail "cannot write the standalone installation receipt in $install_dir"
fi
if ! chmod 600 "$tmp_receipt" || ! mv -f "$tmp_receipt" "$receipt_path"; then
    rm -f "$tmp_receipt"
    fail "cannot install the standalone installation receipt in $install_dir"
fi
tmp_receipt=""

# --- report ---------------------------------------------------------------------

suffix="-$target.tar.gz"
version=$tarball
version=${version#locron-}
version=${version%"$suffix"}

echo "Installed locron $version to $install_path"
echo "Run 'locron -V' to confirm the installation." >&2

# --- register as a login service (best-effort) --------------------------------
# The binary replacement is the essential install; the registration attempt is
# best-effort by design and `LOCRON_NO_SERVICE=1` declines it. A guidance exit
# (zero with a no-session note on Linux) passes through; any other failure
# warns and keeps the installation successful.

if [ -z "${LOCRON_NO_SERVICE:-}" ]; then
    if "$install_path" service install; then
        :
    else
        status=$?
        echo "warning: could not register locron as a login service (exit $status); run 'locron service install' to retry" >&2
    fi
fi

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        shell_name=$(basename "${SHELL:-/bin/sh}")
        case "$shell_name" in
            fish)
                echo "Add $install_dir to your PATH with: fish_add_path $install_dir" >&2
                ;;
            *)
                echo "Add $install_dir to your PATH, for example: export PATH=\"$install_dir:\$PATH\"" >&2
                ;;
        esac
        ;;
esac
