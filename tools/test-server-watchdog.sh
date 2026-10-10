#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "$0")/.." && pwd)
binary="$project_root/target/debug/onlinerpg-server"
test -x "$binary" || { echo 'Run cargo build -p onlinerpg-server first' >&2; exit 1; }
work=$(mktemp -d)
unit="openmmo-watchdog-test-$$.service"
port=${TEST_WATCHDOG_PORT:-19007}
cleanup() {
    result=$?
    if [[ $result != 0 ]]; then
        journalctl --user -u "$unit" -n 40 --no-pager >&2 || true
    fi
    systemctl --user stop "$unit" >/dev/null 2>&1 || true
    systemctl --user reset-failed "$unit" >/dev/null 2>&1 || true
    rm -rf "$work"
}
trap cleanup EXIT

properties=()
while IFS= read -r setting; do
    [[ -z "$setting" || "$setting" == \[* || "$setting" == \#* ]] && continue
    properties+=(--property="$setting")
done < "$project_root/tools/systemd/openmmo-server.service.d/watchdog.conf"

systemd-run --user --quiet --no-block --unit="$unit" \
    "${properties[@]}" --property=Restart=always --property=RestartSec=5s \
    --working-directory="$project_root" \
    "$binary" --port 0 --terrain-port "$port" --state-dir "$work/state" \
    --npc-data-dir "$work/npcs" --tales-ledger "$work/ledger.txt"

wait_healthy() {
    local deadline=$((SECONDS + 120))
    while (( SECONDS < deadline )); do
        if [[ $(systemctl --user show "$unit" -p ActiveState --value) == active ]] &&
            curl --fail --silent --max-time 2 "http://127.0.0.1:$port/api/health" > "$work/health.json"; then
            return 0
        fi
        sleep 1
    done
    echo 'Test service did not become healthy' >&2
    return 1
}

wait_healthy
first_pid=$(systemctl --user show "$unit" -p MainPID --value)
echo 'PASS empty server completes startup and reports healthy'
cat "$work/health.json"
echo

systemctl --user kill --signal=STOP --kill-whom=main "$unit"
deadline=$((SECONDS + 45))
while [[ $(systemctl --user show "$unit" -p NRestarts --value) == 0 ]]; do
    (( SECONDS < deadline )) || { echo 'Watchdog did not restart the stopped process' >&2; exit 1; }
    sleep 1
done
wait_healthy
test "$(systemctl --user show "$unit" -p MainPID --value)" != "$first_pid"
journalctl --user -u "$unit" --no-pager | grep -F 'Watchdog timeout'
echo 'PASS watchdog kills a stopped runtime and restarts a healthy server'

systemctl --user stop "$unit"
! systemctl --user is-active --quiet "$unit"
journalctl --user -u "$unit" --no-pager | grep -F 'Graceful shutdown complete'
echo 'PASS normal shutdown drains successfully'
