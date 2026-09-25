---
description: "Deploy OpenMMO to prod, transfer complete public village changes while preserving player data, and verify services and the changed world. Use when the user asks to deploy, ship, or push to prod. A full deployment restarts the game and disconnects live players."
---

You are deploying OnlineRPG to production. The deploy script (`tools/deploy-prod.sh`)
runs **on the prod host**: it `git pull --ff-only`s master, builds both Rust
binaries and the client bundle, rsyncs the bundle to the webroot, then restarts
both systemd units. Restarting disconnects everyone currently playing.

The prod host is the `prod` SSH alias. See README.md "Production Deployment" for
the reference.

## 1. Preflight — before touching prod

Read `~/work/notes/DEPLOY_NOTES.md` first. Follow pending transfers, production
data preservation rules, and completed hotfix records; do not replay completed
transfers or replace a production correction with an older development file.

Gitignored operator data does not ride the deploy. Prepare and validate scoped
patches before launching. Coordinate their application with runtime caches and
the planned restart; the script does not discover or transfer village data.

- **Push first.** The script pulls `master` on prod, so anything not on
  `origin/master` will not deploy. Run `git status` and `git log origin/master..HEAD`;
  if HEAD is ahead of origin or the tree is dirty, stop and get it committed and
  pushed (this repo commits straight to master — no feature branch). Confirm the
  commit you intend to ship is the one at `origin/master`.
- **Player-facing change? Write an announcement first.** Announcements show on the
  login screen. They live in `data/announcements/` but are **gitignored**
  (operator content — `.gitignore` excludes `*.md` except `_README.md`), so they
  do **not** ride the deploy. Format is in `data/announcements/_README.md`
  (`YYYY-MM-DD-title.md`, `title`/`title_en`/`category` frontmatter, `[en]` marker
  for the English body). Match the existing files' habits: include the
  server-restart notice ("업데이트 적용을 위해 서버가 잠시 재시작됩니다…"), write from
  the player's point of view (what they saw, not the mechanism), and if the fix is
  unverified in live play, say so. Then copy it up:
  ```bash
  scp data/announcements/<file>.md prod:~/work/OnlineRPG/data/announcements/
  ```
  A pure internal change (logging, refactor with no player-visible effect) needs
  no announcement — say so and skip it.
- **Banned names.** `data/banned_names.txt` is gitignored too (real names);
  the local copy on this host is the master. Diff against prod first — a
  prod-only line is an operator edit to merge locally, not overwrite — then
  push:
  ```bash
  ssh prod 'cat ~/work/OnlineRPG/data/banned_names.txt' | diff - data/banned_names.txt
  scp data/banned_names.txt prod:~/work/OnlineRPG/data/banned_names.txt
  ```
  Identical files → say so and skip the scp.
- **Public village changes — inventory every deploy.** Check the deployment
  notes, requested map changes, and scoped development/production data against
  the last deployment record. Git status and mtimes cannot establish that the
  village is unchanged. Follow [the deployment skill's village procedure](../../.codex/skills/deploy-prod/SKILL.md#public-village-transfers)
  whenever public buildings, furniture, terrain, paths, or vegetation changed.
  - Inventory each affected facility or area with its building/object IDs,
    world bounds, dependent terrain layers, planned operation, and expected
    result. Every relevant layer needs an action or a verified no-change reason.
  - Preserve production estates, DB rows, private houses, furniture, fences,
    and edits outside the chosen public area. Objects/furniture can live in
    `data/terrain/objects/` or estate DB tables; housing JSON alone is incomplete.
  - `tools/sync-terrain.sh` is a whole-tree copier, not a village migration.
    Do not use its `--apply` or `--delete` for selective village transfers.
    A small rsync count does not establish safe scope; a shared tile may contain
    both public work and private player edits. Compare relevant content and
    merge only selected cells/IDs into the current production data.
  - Validate the staged result's floor clearance, grass/tree exclusions, and
    paving removal masks even when development and production hashes match.
    Copying a building file does not run the construction side effects.
  - Prepare backups and verify the production baseline again before applying.
    Apply file-based changes in the planned maintenance window, or use supported
    edit APIs and explicitly refresh dependent caches and terrain versions.
  Record why no village transfer is needed when the inventory finds none.

## 2. Launch the deploy, detached

A foreground run dies with the SSH connection and loses the whole build, so
detach it:
```bash
ssh prod 'setsid nohup bash ~/work/OnlineRPG/tools/deploy-prod.sh > ~/deploy-latest.log 2>&1 < /dev/null &'
```
The script builds before publishing binaries/web assets and restarting services.
Separately applied world-data edits are already live changes; account for them
in the deployment's backup, cache-refresh, and recovery plan.

## 3. Watch it to completion — with a monitor that ends itself

The log ends at `==> deployed <commit>`, and that marker is the script's **last**
line. So a plain `tail -f` never terminates: nothing is written after the marker,
so even a `grep`/`awk` that exits on it leaves `tail` blocked on the file with no
SIGPIPE to kill it, and the monitor lingers until timeout. Don't use `tail -f`.

Poll the log on prod instead and `exit` when the marker lands, so the monitor
closes itself. Pass this to the Monitor tool (single ssh, streams progress +
failures, self-terminating):
```bash
ssh prod 'last=0; while :; do
  n=$(wc -l < ~/deploy-latest.log 2>/dev/null || echo "$last")
  if [ "$n" -gt "$last" ]; then
    sed -n "$((last+1)),${n}p" ~/deploy-latest.log \
      | grep -E "==>|warning|error|Error|error\[|failed|FAILED|panic|Killed|No space|fatal"
    last=$n
  fi
  tail -5 ~/deploy-latest.log | grep -q "==> deployed" && exit 0
  sleep 3
done'
```
The `==>` in the filter catches every progress marker (git pull → builds → publish
→ restarts → deployed); the error signatures catch a build that aborts under
`set -euo pipefail` without ever reaching the marker. A typical run is a few
minutes (two release builds + wasm + Vite bundle). Keep the Monitor's own timeout
as a backstop in case the deploy hangs and neither the marker nor an error appears.

## 4. Verify it came up

```bash
ssh prod 'systemctl is-active openmmo-server openmmo-agent-client'
ssh prod 'journalctl -u openmmo-server --since "2 min ago" -p err --no-pager -o cat | tail'
ssh prod 'journalctl -u openmmo-server -n 15 --no-pager -o cat'   # startup + passability cache line
```
Confirm both units are `active`, the startup log shows no panics, and the
"Passability cache ready" / "Server started successfully" lines are present.
A dead `openmmo-agent-client` (expired LLM login, outage) does **not** fail the
deploy — the game is already live — but flag it.

For a village transfer, also complete the data and in-game checks in
[the deployment skill's village procedure](../../.codex/skills/deploy-prod/SKILL.md#public-village-transfers). Check the public files
referenced by current terrain manifests, actual floor/vegetation conditions,
and preservation outside the patch. Enter the game and inspect the changed
area. If in-game access is unavailable, finish available data checks and record
the visual check as pending; do not mark the village transfer fully verified.

## 5. Agent-client release — when the deploy needs one

If `PROTOCOL_VERSION` ([shared/src/lib.rs](../../shared/src/lib.rs)) changed since the
last `agent-client-v*` tag, every distributed agent-client is refused by the new
server ("update agent-client"), so a GitHub release must ship with the deploy.
Check with:
```bash
git log $(git describe --tags --match 'agent-client-v*' --abbrev=0)..HEAD --oneline -- shared/ agent-client/
```
A meaningful `agent-client/` change without a protocol bump also warrants one;
pure server/web-client work does not — say so and skip.

1. Tag the deployed commit `agent-client-vX.Y.0` and push the tag.
2. Linux tarball, on this host:
   ```bash
   GOOGLE_CLI_CLIENT_SECRET=$(cat ~/.config/openmmo/cli-secret) bash tools/package-agent-client.sh
   ```
3. Windows zip, on `pc4090` (repo `C:\Users\jake\work\OnlineRPG`): bring the repo
   to the tagged commit first — it often holds uncommitted local files; back them
   up (rename to `*.bak`), never delete. Build with **pwsh 7, not powershell.exe**
   (5.1 writes backslash zip paths that break extraction outside Windows):
   ```bash
   ssh pc4090 'pwsh -NoProfile -Command "cd C:\Users\jake\work\OnlineRPG; $env:GOOGLE_CLI_CLIENT_SECRET=(Get-Content C:\Users\jake\.config\openmmo\cli-secret -Raw).Trim(); .\tools\package-agent-client.ps1"'
   ```
   scp the zip back and verify the archive has zero backslash entry paths.
4. `gh release create agent-client-vX.Y.0 <tarball> <zip>` — match the previous
   release's Korean notes: why the protocol bumped, old versions refused at
   connect, what changed for agents, asset list.
5. Add an in-game announcement telling agent operators to re-download (step 1's
   announcement flow; a server restart is needed to show it).

Details: doc/REMOTE_AGENT_CLIENT.md "패키징 메모".

## 6. Report

Tell the user the deployed commit (`==> deployed <hash>`), that both units are up,
whether the announcement shipped, and the agent-client release URL if one was
needed. If the deploy was to fix a bug with a log
signal (e.g. the `Blocked move` warns), compare its rate before vs after the
restart with `journalctl` rather than claiming success from a clean build alone —
a clean build only proves it compiled, not that the fix worked in play.
Include the village areas transferred and any pending visual checks. Update
`~/work/notes/DEPLOY_NOTES.md` with completed transfers, corrections, backup
locations, and unresolved work so subsequent deployments preserve the result.

$ARGUMENTS
