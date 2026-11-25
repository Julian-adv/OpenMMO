#!/usr/bin/env bash
# Packs the Windows openmmo-mcp.exe into a Claude Desktop extension.
# Usage: tools/package-openmmo-mcpb.sh <openmmo-mcp.exe> [output.mcpb]
# The version comes from openmmo-mcp/Cargo.toml.
set -euo pipefail
exe=${1:?path to openmmo-mcp.exe}
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(grep -m1 '^version' "$root/openmmo-mcp/Cargo.toml" | cut -d'"' -f2)
out=${2:-$root/dist/openmmo-$version.mcpb}
if ! grep -aq 'GOCSPX-' "$exe"; then
    echo "error: $exe has no Google client secret; build it with GOOGLE_CLI_CLIENT_SECRET set." >&2
    exit 1
fi
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/server" "$(dirname "$out")"
sed "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" "$root/openmmo-mcp/desktop/manifest.json" >"$stage/manifest.json"
cp "$root/client/public/icons/app-icon-512.png" "$stage/icon.png"
cp "$exe" "$stage/server/openmmo-mcp.exe"
npx -y @anthropic-ai/mcpb@2.1.2 pack "$stage" "$out"
