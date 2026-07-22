#!/usr/bin/env bash
# Build a tarball someone can unpack on their own machine and run.
#
# The agent-client needs its data/ directory next to the binary (prompts,
# animation timings) but none of the 3 GB terrain tree — that comes over HTTP.
# Output: dist/agent-client-<commit>-<target>.tar.gz
#
# Builds natively, so the artifact inherits the build host's glibc floor
# (currently 2.39 = Ubuntu 24.04+). That is the documented audience for the
# download; anyone older builds from source, which needs nothing but a Rust
# toolchain. Set TARGET to cross-compile (x86_64-unknown-linux-musl gives a
# static binary, but ring needs a musl-targeting cc for that).
set -euo pipefail

REPO=${REPO:-$(cd "$(dirname "$0")/.." && pwd)}
OUT_DIR=${OUT_DIR:-$REPO/dist}
# Host only: the WebSocket lives at /ws (the reverse proxy upgrades that path
# and serves the game page at /), and the tile API at /api/terrain.
HOST=${HOST:-openmmo.to.nexus}

# Google's device flow requires the installed-app secret in the token
# exchange. It is not confidential (RFC 8252 section 8.5) and every shipped
# copy needs it, but it stays out of the repo: committing it trips secret
# scanners, so it is injected here from the packaging environment instead.
CLIENT_SECRET=${GOOGLE_CLI_CLIENT_SECRET:-}
if [[ -z $CLIENT_SECRET ]]; then
    echo "error: set GOOGLE_CLI_CLIENT_SECRET (Google Cloud → the CLI OAuth client)." >&2
    echo "       Without it the packaged client cannot complete Google sign-in." >&2
    exit 1
fi

TARGET=${TARGET:-}

cd "$REPO"
commit=$(git rev-parse --short HEAD)
if [[ -n $TARGET ]]; then
    cargo build --release --target "$TARGET" -p agent-client
    binary="target/$TARGET/release/agent-client"
    suffix=${TARGET%%-unknown-linux-*}-${TARGET##*-}
else
    cargo build --release -p agent-client
    binary="target/release/agent-client"
    # Name the glibc floor, not the build host: it is what decides whether
    # the download runs on someone else's machine.
    glibc=$(objdump -T "$binary" | grep -oE "GLIBC_[0-9.]+" | sort -uV | tail -1)
    suffix="$(uname -m)-${glibc,,}"
fi
name="agent-client-$commit-$suffix"
stage="$OUT_DIR/$name"

rm -rf "$stage"
mkdir -p "$stage/data"
cp "$binary" "$stage/"
# No data/templates: those are operator NPC roles (merchant, guard). A user
# agent has no template_prompt and falls back to data/system_prompt.txt.
cp agent-client/data/system_prompt.txt agent-client/data/animation_durations.json "$stage/data/"

# Registry NPC personas are operator-side; a user agent plays its own character.
cat > "$stage/data/config.toml" <<EOF
# agent-client configuration. Run the binary from this directory.
server = "wss://$HOST/ws"
terrain = "https://$HOST"

[auth]
mode = "google"
client_secret = "$CLIENT_SECRET"

[[npcs]]
character_name = "Change Me"
character_class = "ranger"
llm = "codex"

[codex]
model = "gpt-5.4-mini"
EOF

cp "$REPO/doc/AGENT_CLIENT_QUICKSTART.md" "$stage/README.md"

tar -czf "$OUT_DIR/$name.tar.gz" -C "$OUT_DIR" "$name"
rm -rf "$stage"
echo "==> $OUT_DIR/$name.tar.gz"
