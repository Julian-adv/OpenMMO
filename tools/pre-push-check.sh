#!/usr/bin/env bash
# Local stand-in for CI on master pushes. Runs only the sections touched by
# unpushed commits; pass --all to run everything.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

run_rust=0 run_client=0 run_dashboard=0 run_tools=0

if [[ "${1:-}" == "--all" ]]; then
  run_rust=1 run_client=1 run_dashboard=1 run_tools=1
else
  base="$(git rev-parse --verify --quiet '@{upstream}' || git rev-parse --verify --quiet origin/master || true)"
  if [[ -z "$base" ]]; then
    run_rust=1 run_client=1 run_dashboard=1 run_tools=1
  else
    while IFS= read -r f; do
      case "$f" in
        dashboard/*) run_dashboard=1 ;;
        client/scripts/build-wasm.sh) run_client=1 run_tools=1 ;;
        client/*) run_client=1 ;;
        shared/* | data-src/*) run_rust=1 run_client=1 ;;
        tools/lockscan.py) run_rust=1 ;;
        tools/convert.mjs) run_client=1 ;;
        tools/deploy-prod.sh | tools/test-deploy-prod.sh | tools/test-build-wasm.sh) run_tools=1 ;;
        *.rs | Cargo.toml | Cargo.lock | */Cargo.toml) run_rust=1 ;;
      esac
    done < <(git diff --name-only "$base"...HEAD)
  fi
fi

step() {
  echo "==> $*"
  "$@"
}

if ((run_rust)); then
  step python3 -I tools/lockscan.py --check server/src
  step cargo fmt --all --check
  step cargo clippy --workspace --all-targets --locked -- -D warnings
  step cargo test --workspace --locked
fi

if ((run_tools)); then
  (cd client && step bash ../tools/test-deploy-prod.sh && step bash ../tools/test-build-wasm.sh)
fi

if ((run_client)); then
  (
    cd client
    step npm run build:wasm
    step npm test
    step npm run check
    step npm run lint
    step npm run format:check
  )
fi

if ((run_dashboard)); then
  (
    cd dashboard
    step npm run check
    step npm run lint
    step npm test
    step npm run build
  )
fi

echo "==> pre-push checks passed"
