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

# The CLI's `--format human` mode currently emits machine JSON for most
# commands, so the demo filters it down to the readable facts. If jq is
# missing, fall back to the raw output rather than failing the recording.
JQ_FILTER="${JQ_FILTER:-}"
if [ -z "${JQ_FILTER}" ]; then
    if command -v jq >/dev/null 2>&1; then
        JQ_FILTER="jq -r"
    else
        JQ_FILTER="cat"
    fi
fi

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

enter "locron --format json add fetch-repo --every 15m -- git -C ~/projects/app fetch | $JQ_FILTER '\"added \\(.data.name)\"'"
enter "locron --format json add nightly-backup --cron '0 3 * * *' --timezone Asia/Seoul --shell './scripts/backup.sh' | $JQ_FILTER '\"added \\(.data.name)\"'"
enter "locron --format json add health-check --every 5m --http GET https://example.com/health | $JQ_FILTER '\"added \\(.data.name)\"'"
enter "locron --format json add demo-job --every 1h -- /bin/echo demo done | $JQ_FILTER '\"added \\(.data.name)\"'"
enter "locron list | $JQ_FILTER '.[] | \"\\(.name)  enabled=\\(.enabled)\"'"
enter "locron preview nightly-backup --count 3 | $JQ_FILTER '.occurrences[]'"
enter "locron --format json run demo-job | $JQ_FILTER '\"\\(.data.state)\"'"
sleep 2
enter "locron history demo-job | $JQ_FILTER '.[] | \"\\(.state)\"'"
enter "locron why nightly-backup | $JQ_FILTER '.explanation'"
enter "locron doctor | $JQ_FILTER '.checks[]'"

sleep 2
