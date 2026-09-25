#!/usr/bin/env bash
set -euo pipefail

client_dir=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$client_dir/node_modules/.cache"
exec 9>"$client_dir/node_modules/.cache/build-wasm.lock"
if ! flock -n 9; then
    echo "Waiting for the current WASM build to finish..."
    flock 9
fi

cd "$client_dir"
npm run generate:csv
npm run generate:footprints
npm run generate:monster-clips
npm run generate:animations
rm -rf src/lib/wasm
cd ../shared
wasm-pack build --target web --out-dir ../client/src/lib/wasm
