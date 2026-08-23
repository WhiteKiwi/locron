#!/usr/bin/env bash
# Enforce the dependency-direction rules of docs/ARCHITECTURE.md for the
# workspace crate graph:
#   - locron-server depends only on locron-core and locron-store among
#     workspace crates (never on locron-engine, locron-cli, or a cycle);
#   - nothing except locron-cli depends on locron-server.
# The rules are structural: they guard the server-never-owns-the-daemon and
# the CLI-composition-root boundaries against regression.
set -euo pipefail

cd "$(dirname "$0")/.."

workspace_members=(locron-core locron-store locron-engine locron-server locron-cli)

server_tree=$(cargo tree -p locron-server --prefix none 2>/dev/null | awk '{print $1}' | sort -u)
for member in "${workspace_members[@]}"; do
  case "$member" in
    locron-core | locron-store | locron-server) ;;
    *)
      if grep -qx "$member" <<<"$server_tree"; then
        echo "forbidden: locron-server depends on workspace crate $member" >&2
        exit 1
      fi
      ;;
  esac
done

dependents=$(cargo tree -i -p locron-server --prefix none 2>/dev/null | awk '{print $1}' | sort -u)
for member in "${workspace_members[@]}"; do
  case "$member" in
    locron-cli | locron-server) ;;
    *)
      if grep -qx "$member" <<<"$dependents"; then
        echo "forbidden: workspace crate $member depends on locron-server" >&2
        exit 1
      fi
      ;;
  esac
done

echo "dependency direction ok: locron-server depends only on locron-core and locron-store; only locron-cli depends on it"
