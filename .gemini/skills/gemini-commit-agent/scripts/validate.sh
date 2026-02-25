#!/bin/bash
set -e

# Function to run checks in a directory
run_npm_checks() {
  local dir=$1
  echo "🔍 Running quality checks in $dir..."
  cd "$dir"
  npm run format || echo "⚠️ format failed, continuing..."
  npm run lint
  npm run check
  cd - > /dev/null
}

run_cargo_checks() {
  local dir=$1
  echo "🔍 Running quality checks in $dir..."
  cd "$dir"
  cargo fmt --all -- --check || (echo "Formatting issues found. Running cargo fmt..." && cargo fmt)
  cargo check
  cd - > /dev/null
}

# Detect changes
CHANGED_FILES=$(git diff --name-only HEAD)

if echo "$CHANGED_FILES" | grep -q "^client/"; then
  run_npm_checks "client"
fi

if echo "$CHANGED_FILES" | grep -q "^server/"; then
  run_cargo_checks "server"
fi

if echo "$CHANGED_FILES" | grep -q "^tools/"; then
  # Find which specific tool changed
  for tool_dir in tools/*/; do
    if echo "$CHANGED_FILES" | grep -q "^$tool_dir"; then
      run_npm_checks "$tool_dir"
    fi
  done
fi

echo "✅ All quality checks passed!"
