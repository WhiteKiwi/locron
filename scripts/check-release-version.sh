#!/bin/sh
# Verify one release version across Cargo metadata, the CLI, and changelog.

set -eu

version=${1:?usage: check-release-version.sh VERSION}
metadata_file=${LOCRON_RELEASE_METADATA_FILE:-}
changelog=${LOCRON_RELEASE_CHANGELOG_FILE:-CHANGELOG.md}

if [ -n "$metadata_file" ]; then
    metadata=$(cat "$metadata_file")
else
    metadata=$(cargo metadata --locked --no-deps --format-version 1)
fi

versions=$(printf '%s' "$metadata" | jq -r '.packages[].version' | sort -u)
[ "$versions" = "$version" ] || {
    echo "error: workspace package versions do not all equal $version: $versions" >&2
    exit 1
}
names=$(printf '%s' "$metadata" | jq -r '.packages[].name' | sort)
expected=$(printf '%s\n' locron locron-core locron-engine locron-server locron-store)
[ "$names" = "$expected" ] || {
    echo "error: unexpected publish package inventory" >&2
    exit 1
}

internal_dependencies=$(printf '%s' "$metadata" | jq -r '
    [.packages[] as $package
     | $package.dependencies[]
     | select(.kind == null or .kind == "normal")
     | select(.name == "locron-core" or .name == "locron-store"
              or .name == "locron-engine" or .name == "locron-server")
     | "\($package.name)->\(.name)=\(.req)"]
    | sort[]')
expected_dependencies=$(printf '%s\n' \
    "locron->locron-core==$version" \
    "locron->locron-engine==$version" \
    "locron->locron-server==$version" \
    "locron->locron-store==$version" \
    "locron-engine->locron-core==$version" \
    "locron-server->locron-core==$version" \
    "locron-server->locron-store==$version" \
    "locron-store->locron-core==$version" | sort)
[ "$internal_dependencies" = "$expected_dependencies" ] || {
    echo "error: normal internal dependency requirements must match the exact release =$version" >&2
    echo "expected:" >&2
    printf '%s\n' "$expected_dependencies" >&2
    echo "actual:" >&2
    printf '%s\n' "$internal_dependencies" >&2
    exit 1
}

if [ -n "${LOCRON_RELEASE_BINARY_OUTPUT+x}" ]; then
    binary_output=$LOCRON_RELEASE_BINARY_OUTPUT
else
    binary_output=$(cargo run --locked -q -p locron -- -V)
fi
[ "$binary_output" = "locron $version" ] || {
    echo "error: binary reports '$binary_output', expected 'locron $version'" >&2
    exit 1
}
grep -qF "## [$version]" "$changelog" || {
    echo "error: CHANGELOG.md has no section for $version" >&2
    exit 1
}
