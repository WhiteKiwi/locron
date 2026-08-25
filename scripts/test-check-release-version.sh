#!/bin/sh

set -eu

root=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
fixture=$(mktemp -d "${TMPDIR:-/tmp}/locron-release-version.XXXXXX")
trap 'rm -rf "$fixture"' EXIT INT TERM HUP

cat > "$fixture/metadata.json" <<'EOF'
{"packages":[
  {"name":"locron-core","version":"0.8.0","dependencies":[]},
  {"name":"locron-store","version":"0.8.0","dependencies":[
    {"name":"locron-core","kind":null,"req":"=0.8.0"}]},
  {"name":"locron-engine","version":"0.8.0","dependencies":[
    {"name":"locron-core","kind":null,"req":"=0.8.0"}]},
  {"name":"locron-server","version":"0.8.0","dependencies":[
    {"name":"locron-core","kind":null,"req":"=0.8.0"},
    {"name":"locron-store","kind":null,"req":"=0.8.0"}]},
  {"name":"locron","version":"0.8.0","dependencies":[
    {"name":"locron-core","kind":null,"req":"=0.8.0"},
    {"name":"locron-engine","kind":null,"req":"=0.8.0"},
    {"name":"locron-server","kind":null,"req":"=0.8.0"},
    {"name":"locron-store","kind":null,"req":"=0.8.0"}]}
]}
EOF
printf '## [0.8.0]\n' > "$fixture/CHANGELOG.md"

LOCRON_RELEASE_METADATA_FILE="$fixture/metadata.json" \
LOCRON_RELEASE_CHANGELOG_FILE="$fixture/CHANGELOG.md" \
LOCRON_RELEASE_BINARY_OUTPUT='locron 0.8.0' \
    sh "$root/scripts/check-release-version.sh" 0.8.0

if LOCRON_RELEASE_METADATA_FILE="$fixture/metadata.json" \
    LOCRON_RELEASE_CHANGELOG_FILE="$fixture/CHANGELOG.md" \
    LOCRON_RELEASE_BINARY_OUTPUT='locron 0.8.0' \
    sh "$root/scripts/check-release-version.sh" 0.8.1 >"$fixture/out" 2>"$fixture/error"; then
    echo "version mismatch unexpectedly succeeded" >&2
    exit 1
fi
grep -q 'do not all equal 0.8.1' "$fixture/error"

for requirement in '=0.7.0' '^0.8.0'; do
    sed "s/\"req\":\"=0.8.0\"/\"req\":\"$requirement\"/g" \
        "$fixture/metadata.json" > "$fixture/bad-dependency.json"
    if LOCRON_RELEASE_METADATA_FILE="$fixture/bad-dependency.json" \
        LOCRON_RELEASE_CHANGELOG_FILE="$fixture/CHANGELOG.md" \
        LOCRON_RELEASE_BINARY_OUTPUT='locron 0.8.0' \
        sh "$root/scripts/check-release-version.sh" 0.8.0 \
        >"$fixture/dependency-out" 2>"$fixture/dependency-error"; then
        echo "non-exact internal dependency '$requirement' unexpectedly succeeded" >&2
        exit 1
    fi
    grep -q 'normal internal dependency requirements must match the exact release =0.8.0' \
        "$fixture/dependency-error"
done
