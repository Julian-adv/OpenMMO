#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

collect_changed_files() {
  {
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | sort -u
}

run_npm_checks() {
  local dir="$1"
  local abs_dir="$REPO_ROOT/$dir"

  if [[ ! -f "$abs_dir/package.json" ]]; then
    echo "[skip] $dir has no package.json"
    return
  fi

  echo "[check] $dir: npm run format"
  (
    cd "$abs_dir"
    npm run format
    echo "[check] $dir: npm run lint"
    npm run lint
    echo "[check] $dir: npm run check"
    npm run check
  )
}

run_server_checks() {
  local abs_dir="$REPO_ROOT/server"

  echo "[check] server: cargo fmt"
  (
    cd "$abs_dir"
    cargo fmt
    echo "[check] server: cargo check"
    cargo check
  )
}

changed_files="$(collect_changed_files)"
if [[ -z "${changed_files}" ]]; then
  echo "[info] No changes detected"
  exit 0
fi

client_changed=0
server_changed=0
declare -A changed_tools=()

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  case "$file" in
    client/*)
      client_changed=1
      ;;
    server/*)
      server_changed=1
      ;;
    tools/*/*)
      tool_dir="$(cut -d/ -f1-2 <<< "$file")"
      changed_tools["$tool_dir"]=1
      ;;
  esac
done <<< "$changed_files"

if [[ "$client_changed" -eq 1 ]]; then
  run_npm_checks "client"
fi

if [[ "${#changed_tools[@]}" -gt 0 ]]; then
  while IFS= read -r tool_dir; do
    [[ -z "$tool_dir" ]] && continue
    run_npm_checks "$tool_dir"
  done < <(printf '%s\n' "${!changed_tools[@]}" | sort)
fi

if [[ "$server_changed" -eq 1 ]]; then
  run_server_checks
fi

echo "[ok] All required checks passed"
