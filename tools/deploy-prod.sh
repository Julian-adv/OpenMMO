#!/usr/bin/env bash
# Build and publish master on the production host.
set -euo pipefail

# Keep git pull from rewriting the running script.
if [[ ${DEPLOY_FROM_SNAPSHOT:-} != 1 ]]; then
    snap=$(mktemp)
    cat "$0" > "$snap"
    DEPLOY_FROM_SNAPSHOT=1 exec bash "$snap" "$@"
fi
trap 'rm -f "$0"' EXIT

REPO=${REPO:-$HOME/work/OnlineRPG}
WEBROOT=${WEBROOT:-/var/www/openmmo}
DASHBOARD_WEBROOT=${DASHBOARD_WEBROOT:-/var/www/openmmo-dashboard}
export DASHBOARD_BASE=${DASHBOARD_BASE:-/dashboard/}
SERVICE=${SERVICE:-openmmo-server}
AGENT_SERVICE=${AGENT_SERVICE:-openmmo-agent-client}

cd "$REPO"

game_root=$(realpath -m -- "$WEBROOT")
dashboard_root=$(realpath -m -- "$DASHBOARD_WEBROOT")
if [[ "$game_root/" == "$dashboard_root/"* || "$dashboard_root/" == "$game_root/"* ]]; then
    echo "error: WEBROOT and DASHBOARD_WEBROOT must not overlap" >&2
    exit 1
fi

echo "==> git pull"
git pull --ff-only

echo "==> assets"
bash tools/fetch-assets.sh client/public

# Operator data is transferred during deployment preflight.
if [[ ! -f data/banned_names.txt ]]; then
    echo "warning: data/banned_names.txt missing — restart loads an empty list" >&2
fi

echo "==> server (release)"
cargo build --release -p onlinerpg-server

echo "==> agent client (release)"
cargo build --release -p agent-client

echo "==> client deps"
(cd client && npm ci)

dashboard_config=$(
    cd client
    node --input-type=module <<'JS'
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { loadEnv } from 'vite';
import { loadDashboardEnv } from '../dashboard/env.mjs';

const dashboardEnv = loadDashboardEnv(loadEnv, 'production', '../dashboard');
const clientId = dashboardEnv.VITE_GOOGLE_CLIENT_ID;
if (!clientId || /[\r\n]/.test(clientId)) {
    throw new Error('Set VITE_GOOGLE_CLIENT_ID in dashboard or client production environment');
}
const environment = Object.fromEntries(Object.entries(dashboardEnv)
    .filter(([key]) => key.startsWith('VITE_')).sort(([a], [b]) => a.localeCompare(b)));
const fingerprint = createHash('sha256').update(JSON.stringify({
    source: execFileSync('git', ['rev-parse', 'HEAD:dashboard'], { encoding: 'utf8' }).trim(),
    environment,
    base: process.env.DASHBOARD_BASE,
    node: process.version,
})).digest('hex');
console.log(fingerprint);
console.log(clientId);
JS
)
mapfile -t dashboard_inputs <<< "$dashboard_config"
dashboard_fingerprint=${dashboard_inputs[0]}
dashboard_client_id=${dashboard_inputs[1]}
dashboard_marker="$DASHBOARD_WEBROOT/.deploy-fingerprint"
dashboard_changed=1
if [[ -f "$DASHBOARD_WEBROOT/index.html" && -f "$dashboard_marker" ]] &&
    [[ $(cat "$dashboard_marker") == "$dashboard_fingerprint" ]]; then
    dashboard_changed=0
fi

# The client build also rebuilds WASM.
echo "==> client bundle"
(cd client && npm run build)

if [[ "$dashboard_changed" == 1 ]]; then
    echo "==> dashboard deps and bundle"
    (cd dashboard && npm ci && VITE_GOOGLE_CLIENT_ID="$dashboard_client_id" npm run build)
    test -f dashboard/dist/index.html
else
    echo "==> dashboard unchanged; keeping published bundle"
fi

# Publish only after every required build succeeds.
if [[ "$dashboard_changed" == 1 ]]; then
    echo "==> publish dashboard to $DASHBOARD_WEBROOT"
    sudo install -d -o www-data -g www-data -m 755 "$DASHBOARD_WEBROOT"
    sudo rm -f -- "$dashboard_marker"
    sudo rsync -a --delete dashboard/dist/ "$DASHBOARD_WEBROOT/"
    sudo chown -R www-data:www-data "$DASHBOARD_WEBROOT"
    printf '%s\n' "$dashboard_fingerprint" | sudo tee "$dashboard_marker" >/dev/null
fi

echo "==> publish to $WEBROOT"
sudo rsync -a --delete client/dist/ "$WEBROOT/"
sudo chown -R www-data:www-data "$WEBROOT"

echo "==> restart $SERVICE"
sudo systemctl restart "$SERVICE"
sleep 3
sudo systemctl is-active "$SERVICE"

# Restart the NPC service when installed.
if systemctl cat "$AGENT_SERVICE.service" >/dev/null 2>&1; then
    echo "==> restart $AGENT_SERVICE"
    sudo systemctl restart "$AGENT_SERVICE"
    sleep 3
    # NPC failures must not fail an otherwise live game deployment.
    sudo systemctl is-active "$AGENT_SERVICE" ||
        echo "warning: $AGENT_SERVICE is not running — check 'journalctl -u $AGENT_SERVICE'" >&2
fi

echo "==> deployed $(git log --oneline -1)"
