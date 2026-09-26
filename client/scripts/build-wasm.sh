#!/usr/bin/env bash
set -euo pipefail

client_dir=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$client_dir/node_modules/.cache"
lock="$client_dir/node_modules/.cache/build-wasm.lock"
if command -v flock >/dev/null; then
    exec 9>"$lock"
    if ! flock -n 9; then
        echo "Waiting for the current WASM build to finish..."
        flock 9
    fi
else
    # Git Bash on Windows has no flock; mkdir is atomic. A dead owner's pid frees a stale lock.
    lock_dir="$lock.d"
    waited=
    until mkdir "$lock_dir" 2>/dev/null; do
        owner=$(cat "$lock_dir/pid" 2>/dev/null || true)
        if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then
            rm -rf "$lock_dir"
            continue
        fi
        [ -n "$waited" ] || echo "Waiting for the current WASM build to finish..."
        waited=1
        sleep 1
    done
    echo $$ > "$lock_dir/pid"
    trap 'rm -rf "$lock_dir"' EXIT
fi

cd "$client_dir"
npm run generate:csv
npm run generate:footprints
npm run generate:monster-clips
npm run generate:animations
rm -rf src/lib/wasm
cd ../shared
wasm-pack build --target web --out-dir ../client/src/lib/wasm
