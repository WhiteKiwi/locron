#!/bin/sh

set -eu

root=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
fixture=$(mktemp -d "${TMPDIR:-/tmp}/locron-crates-inventory.XXXXXX")
trap 'rm -rf "$fixture"' EXIT INT TERM HUP

cat > "$fixture/curl" <<'EOF'
#!/bin/sh
url=''
for argument do url=$argument; done
package=${url%/*}; package=${package##*/}
case "${INVENTORY_MODE:?}:$package" in
    none:*) code=404 ;;
    all:*) code=200 ;;
    partial:locron-core|partial:locron-store) code=200 ;;
    partial:*) code=404 ;;
    *) code=500 ;;
esac
printf '%s' "$code"
EOF
chmod +x "$fixture/curl"

for mode in none all; do
    actual=$(PATH="$fixture:$PATH" INVENTORY_MODE=$mode CRATES_IO_API_BASE=https://fixture.invalid \
        sh "$root/scripts/crates-version-inventory.sh" 0.8.0)
    [ "$actual" = "$mode" ] || { echo "expected $mode, got $actual" >&2; exit 1; }
done

if PATH="$fixture:$PATH" INVENTORY_MODE=partial CRATES_IO_API_BASE=https://fixture.invalid \
    sh "$root/scripts/crates-version-inventory.sh" 0.8.0 >"$fixture/out" 2>"$fixture/error"; then
    echo "partial inventory unexpectedly succeeded" >&2
    exit 1
fi
grep -q 'partial crates.io publication' "$fixture/error"
grep -q 'present: locron-core locron-store' "$fixture/error"
grep -q 'absent: locron-engine locron-server locron' "$fixture/error"
