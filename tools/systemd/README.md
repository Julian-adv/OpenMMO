# systemd units

The units running on prod. Paths assume the deploy layout `deploy-prod.sh` expects:
repo at `/home/ubuntu/work/OnlineRPG`, running as `ubuntu`.

## Install

```bash
sudo cp tools/systemd/*.service /etc/systemd/system/
sudo install -D -m 644 tools/systemd/openmmo-server.service.d/watchdog.conf \
  /etc/systemd/system/openmmo-server.service.d/watchdog.conf
sudo systemctl daemon-reload
sudo systemctl enable --now openmmo-server openmmo-agent-client
```

## Secrets

Neither unit carries credentials. Both read an optional `EnvironmentFile` that is
absent from this repo by design:

- `/etc/openmmo/server.env`
- `/etc/openmmo/agent-client.env`

The agent client additionally needs a logged-in codex CLI (`codex login`, writes
`~/.codex/auth.json`) and `agent-client/data/config.toml`, which is gitignored
because it holds deployment-only values. Copy `data/config.toml.example` and fill
it in.

## Game progress watchdog

Every periodic loop the server spawns through `run_ticks` registers a health
check at startup; nothing has to be wired by hand. `GET /api/health` on the
REST port returns 200 once each loop has completed a tick at least once and
none is stale; otherwise 503, with per-check `name`, `last_completed_unix_ms`,
`age_ms`, `timeout_ms` and `stale` in the JSON body. A loop is stale after
`max(3 × period, 30 s)` without a completed tick, so `player_movement`,
`monster_ai`, `party_positions`, `party_vitals`, `fishing`, `hunger` and
`time_sync` have 30 s, the 30 s loops
(`dungeon_refill`, `ground_item_despawn`, `weather`) 90 s, `monster_cleanup`
180 s, `terrain_cache_sweep` 15 min and `buyback_expiry` 3 h. Loops whose pass
can legitimately be slow carry a fixed deadline instead: `batch_save` (its own
32 s loop now, 120 s), `combat_audit` (file IO, 120 s) and `concurrent_metrics`
(DB write, 300 s). A tick that panics still counts as progress; the watchdog
targets hangs, not bugs. Not covered: `health_monitor` itself, the event-driven
player attack loop, the hourly metrics loop, and the hardware and traffic
samplers. Player attacks, hourly metrics and the traffic sampler share game or
persistence locks with watched loops, so a lock-order hang there still
surfaces; the hardware sampler touches no game state. An empty server stays
healthy. The endpoint reads
atomics only, so it keeps answering while game locks are stuck.

`openmmo-server.service.d/watchdog.conf` switches the unit to `Type=notify` with a
30-second watchdog and a 180-second startup deadline. The server sends `READY=1`
on the first healthy sample and a keepalive every five seconds while healthy. A
stalled loop or a frozen runtime stops keepalives; systemd then SIGKILLs the
process (changes since the last save may be lost) and restarts it after
`RestartSec`. Because of `Type=notify`, `systemctl restart` blocks until the
server is ready: every loop ticks once at startup, so READY follows the first
healthy sample, within about five seconds. Normal shutdown sends `STOPPING=1`
before the drain, which keeps the 90-second stop timeout.

`tools/deploy-prod.sh` installs the drop-in and reloads systemd before
restarting. To roll back to a binary without notify support, remove the drop-in
and run `systemctl daemon-reload` first.

Verification after deployment:

```bash
curl --fail --max-time 5 http://127.0.0.1:10007/api/health
systemctl show openmmo-server -p Type -p WatchdogUSec -p WatchdogTimestamp -p NRestarts -p Result
journalctl -u openmmo-server --since '10 minutes ago'
```

External monitors should alert on non-200 responses or timeouts from
`/api/health`. Never send `SIGSTOP` to the live service to test the watchdog;
`cargo build -p onlinerpg-server && bash tools/test-server-watchdog.sh` runs that
fault injection against a transient user service on REST port 19007
(`TEST_WATCHDOG_PORT` overrides it).
