#!/bin/sh
# Classify the exact workspace version on crates.io as none, all, or partial.

set -eu

version=${1:?usage: crates-version-inventory.sh VERSION}
api_base=${CRATES_IO_API_BASE:-https://crates.io/api/v1}
packages="locron-core locron-store locron-engine locron-server locron"
present=""
absent=""

for package in $packages; do
    url="$api_base/crates/$package/$version"
    status=$(curl -sS -o /dev/null -w '%{http_code}' \
        -A "locron-release-inventory/$version (+https://github.com/WhiteKiwi/locron)" \
        "$url") || {
        echo "error: crates.io inventory request failed for $package $version" >&2
        exit 2
    }
    case "$status" in
        200) present="$present $package" ;;
        404) absent="$absent $package" ;;
        *)
            echo "error: crates.io inventory returned HTTP $status for $package $version" >&2
            exit 2
            ;;
    esac
done

if [ -z "$present" ]; then
    echo "none"
elif [ -z "$absent" ]; then
    echo "all"
else
    echo "error: partial crates.io publication for $version" >&2
    echo "present:$present" >&2
    echo "absent:$absent" >&2
    echo "publish only the missing packages from this exact commit with explicit -p selections, then rerun the release" >&2
    exit 3
fi
