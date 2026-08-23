#!/usr/bin/env bash
#
# Records the README demo. Modeled on fd's doc/screencast.sh.
#
# Generate the SVG (requires Node):
#
#   npm install -g svg-term-cli
#   svg-term --command="bash assets/screencast.sh" \
#            --out assets/screencast.svg \
#            --padding=10 --window
#
# Or record to an asciinema cast first, then convert:
#
#   asciinema rec --command "bash assets/screencast.sh" /tmp/locron.cast
#   svg-term --in /tmp/locron.cast --out assets/screencast.svg --padding=10 --window
#
# Then embed it in README.md under the badges:
#
#   <p align="center"><img src="assets/screencast.svg" alt="locron demo" width="800"></p>
#
# The demo runs against a throwaway state directory and its own daemon, so it
# never touches your real jobs.

set -euo pipefail

LOCRON_BIN="${LOCRON_BIN:-locron}"
STATE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/locron-demo.XXXXXX")"
DAEMON_PID=""

# The CLI's human output is readable by default (issue #4), so the demo runs
# the commands as-is. `--format json` remains available for machine output.

cleanup() {
    [ -n "${DAEMON_PID}" ] && kill "${DAEMON_PID}" 2>/dev/null || true
    rm -rf "${STATE_DIR}"
}
trap cleanup EXIT

locron() {
    "${LOCRON_BIN}" --state-dir "${STATE_DIR}" "$@"
}

PROMPT="▶"

# Print a command as if it were being typed, then run it.
enter() {
    printf '%s ' "${PROMPT}"
    local i ch
    for ((i = 0; i < ${#1}; i++)); do
        ch="${1:i:1}"
        printf '%s' "${ch}"
        sleep 0.03
    done
    printf '\n'
    sleep 0.4
    eval "$1" || true
    printf '\n'
    sleep 1
}

clear

# Start the scheduler in the background so the demo has something to talk to.
"${LOCRON_BIN}" --state-dir "${STATE_DIR}" daemon run >/dev/null 2>&1 &
DAEMON_PID=$!
sleep 1

enter "locron add fetch-repo --every 15m -- git -C ~/projects/app fetch"
enter "locron add nightly-backup --cron '0 3 * * *' --timezone Asia/Seoul --shell './scripts/backup.sh'"
enter "locron add health-check --every 5m --http GET https://example.com/health"
enter "locron add demo-job --every 1h -- /bin/echo demo done"
enter "locron list"
enter "locron preview nightly-backup --count 3"
enter "locron run demo-job"
sleep 2
enter "locron history demo-job"
enter "locron why nightly-backup"
enter "locron doctor"

sleep 2
