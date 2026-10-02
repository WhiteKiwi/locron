#!/bin/sh
set -eu
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
fixture=$(mktemp -d "${TMPDIR:-/tmp}/locron-checksums.XXXXXX")
trap 'rm -rf "$fixture"' EXIT INT TERM HUP
for name in locron-v1.2.3-aarch64-apple-darwin.tar.gz locron_1.2.3_arm64.deb locron-1.2.3-1.aarch64.rpm locron-v1.2.3-x86_64-pc-windows-msvc.zip; do
    printf 'fixture archive\n' > "$fixture/$name"
done
# A rerun must not include its existing output.
printf 'previous checksums\n' > "$fixture/SHA256SUMS.txt"
sh "$script_dir/generate-release-checksums.sh" "$fixture"
awk 'NF != 2 || length($1) != 64 || $2 ~ /\// || $2 == "SHA256SUMS.txt" { exit 1 } END { if (NR != 4) exit 1 }' "$fixture/SHA256SUMS.txt"
# The exact bare archive name is the old checksum parser's lookup contract.
awk '$2 == "locron-v1.2.3-aarch64-apple-darwin.tar.gz" { found = 1 } END { exit !found }' "$fixture/SHA256SUMS.txt"
(cd "$fixture" && sha256sum -c SHA256SUMS.txt)
