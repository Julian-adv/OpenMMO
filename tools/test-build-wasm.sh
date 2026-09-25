#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export TEST_WASM_WORK="$work"
mkdir -p "$work/repo/client/scripts" "$work/repo/shared" "$work/bin"
cp "$project_root/client/scripts/build-wasm.sh" "$work/repo/client/scripts/"

cat > "$work/bin/npm" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == 'run generate:csv' ]]; then
    mkdir "$TEST_WASM_WORK/active"
    echo start >> "$TEST_WASM_WORK/events"
    sleep 0.2
fi
MOCK

cat > "$work/bin/wasm-pack" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == 'build --target web --out-dir ../client/src/lib/wasm' ]]
[[ ! -e ../client/src/lib/wasm/package.json ]]
mkdir -p ../client/src/lib/wasm
printf '{"files":["game.wasm"]}\n' > ../client/src/lib/wasm/package.json
sleep 0.2
test -f ../client/src/lib/wasm/package.json
echo end >> "$TEST_WASM_WORK/events"
rmdir "$TEST_WASM_WORK/active"
exit "${TEST_WASM_EXIT:-0}"
MOCK
chmod +x "$work/bin/npm" "$work/bin/wasm-pack"
export PATH="$work/bin:$PATH"

bash "$work/repo/client/scripts/build-wasm.sh" > "$work/first.log" 2>&1 &
first=$!
bash "$work/repo/client/scripts/build-wasm.sh" > "$work/second.log" 2>&1 &
second=$!
wait "$first"
wait "$second"
printf 'start\nend\nstart\nend\n' > "$work/expected"
cmp "$work/expected" "$work/events"
wait_log=$(cat "$work/first.log" "$work/second.log")
[[ "$wait_log" == *'Waiting for the current WASM build'* ]]

if TEST_WASM_EXIT=7 bash "$work/repo/client/scripts/build-wasm.sh"; then
    echo "Expected the build failure to propagate" >&2
    exit 1
else
    [[ $? == 7 ]]
fi
bash "$work/repo/client/scripts/build-wasm.sh"
echo "WASM build serialization and failure recovery passed."
