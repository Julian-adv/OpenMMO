# Weather System

Status: design, implementation in progress on `feature/weather-system`; the
proposal goes upstream once it is built and tested. Supersedes the
moving-cloud draft of 2026-08-08.

## Goal

Regional weather. Each part of the world has a climate; rain forms over a
region, falls for a while, and clears. Only the ground under a rain cell gets
dimmed light, rain and sound. Players can open the world map and see where it
rains now and where it will rain next. Tone: subtle, cozy rain that fits the
warm quarter-view look — not a gray realism filter.

Non-goals (v1): snow, lightning visuals, wetness debuffs, weather-dependent
fishing, seasons, sky dome (quarter view — the sky is never on screen).

## Why stationary cells, not travelling clouds

The first draft moved a noise field with the wind. Two problems:

- In a quarter view the only place a moving cloud is legible is the map; on
  the ground it reads as a rain border sweeping through the village.
- No shipped MMO does it. FFXIV keys weather to zones from a deterministic
  timestamp hash (which is what makes its fishing forecasts possible); Black
  Desert triggers rain per region from temperature/humidity thresholds; Sea of
  Thieves has one roaming storm. What makes weather feel alive is that it is
  visible, predictable and avoidable — not physics.

Stationary regional cells give that, and they extend to climates later (a wet
coast, a rain shadow, a snowy range) the way FFXIV's per-zone tables do.

## Model

### Climate zones (baked)

One climate byte per land plot, 1,024 per region — the exact shape of the land
grade grid (`terrain/src/land.rs`, `/api/terrain/land-grades/{rx}/{rz}`,
`landGradeStore.ts`). Baked from worldgen elevation, no hand authoring:

| Zone | Rule (seed 42 measurements) |
|---|---|
| Wet coast | ≤ 1 km from the sea and below 900 m |
| Rain shadow | a ≥ 1,200 m ridge spanning ≥ 1 km north–south within 14 km upwind (west), no sea in between, and below 700 m |
| Alpine | ≥ 1,500 m (worldgen paints permanent snow from 1,800 m) |
| Temperate | everything else on land |

Sea plots carry zone 0 and never host a cell.

### Sectors (baked)

The bake gives each zone one sector per `SECTOR_KM2` of area (wet coast 4,
temperate 7, rain shadow 36, alpine 12 km²), spread through the zone by
farthest-point sampling; a sector's spots are the 16 plots around its centre.
Inland spots sit as far from the zone border as the zone allows; wet-coast
spots hug the shoreline (within 128 m of the sea) so coastal showers spill
seaward instead of soaking the zones behind them. Written world-wide to `data/terrain/weather-sectors.json`
(`shared/src/worldgen/weather_sectors.rs`), a few hundred sectors, so clients
never analyse the climate grid themselves.

### Cells (derived, never stored)

A sector hosts at most one cell at a time. For sector `s` with period `P` and
lifetime range `[L0, L0 + Lv]`, cycle `k` yields

```
h1, h2, h3 = hash(seed, s, k)
active   = h1 < chance
life     = min(L0 + h3 * Lv, 0.9 * P)
birth    = k * P + h2 * (P - life)         // the cell fits inside its cycle
envelope = ramp-up 25 % of life → full → ramp-down 30 % of life
falloff(d) = 1 - smoothstep(0.7, 1.0, d)   // flat top, short edge, 0 at the radius
rain(x, z, t) = min(1, sum over cells of envelope(t) * falloff(dist / radius))
```

The falloff is flat to 70 % of the radius and fades to zero at the radius, so
the disc on the map is exactly where it rains and a cell never soaks the
next zone from beyond its own edge. A Gaussian was tried first: it still held
37 % at the radius and drizzled out to twice it, which made the map
over-promise and let coastal cells wet the rain shadow.

Everything is a pure function of `(seed, sectors, t)` — `shared/src/weather.rs`.
`t` is game minutes since the calendar epoch (`weather::game_minutes`, on top
of `moon::game_day_index`), already synced by `GameTimeSync`. Only cycle `k`
can be live at `t`, so the runtime cost is one hash per sector. Anyone who
knows the seed can evaluate any time — that is the forecast.

### Schedule (game minutes; a game day is 3 real hours)

| Zone | km² per sector | Period | Lifetime | Chance | Radius | Real-time feel |
|---|---|---|---|---|---|---|
| Wet coast | 4 | 510 (8.5 h) | 165–300 | 0.9 | 1.6–2.6 km | 21–37 min of rain every ~1 h |
| Temperate | 7 | 2,000 (~1.4 d) | 120–240 | 0.8 | 3.5–5.4 km | 15–30 min every ~4 h |
| Rain shadow | 36 | 6,800 (~5 d) | 120–180 | 0.6 | 3.0–4.6 km | 15–22 min every ~14 h |
| Alpine | 12 | 1,260 (21 h) | 180–360 | 0.9 | 3.3–5.2 km | 22–45 min every ~2.6 h |

A rain event must be felt inside a play session; 15–40 real minutes matches
FFXIV's 23-minute weather slot. Dryness is expressed by the gap between events,
not by shorter events. Measured share of time a plot is wet (30 game days,
seed 42, Valdran: 22 coastal, 15 temperate, 1 shadow, 1 alpine sector):
wet coast 30 %, temperate 20 %, alpine 22 %, rain shadow 16 %. Cells reach
3–5 km, so the rain shadow is "the least rainy place", never bone dry — most
of its rain is spill from the zones around it.

Two lessons from tuning on the real bake: sector count must follow zone
area (a grid-bucket cut gave the 1 km coastal band ten times too many
sectors), and 3–5 km coastal cells centred in that band soaked every zone
behind it — hence the small shoreline showers. The ignored test
`weather_zone_shares_from_bake` in `terrain/src/tests.rs` re-measures this
from baked climate files in under a second.

Derived values: `rainIntensity = rain(x, z, t)`; `cloudFactor =
smoothstep(0.35, 0.80, rain)`. There is no separate background cloud layer:
in a quarter view the sky is never on screen, so a drifting shadow pattern
would only bring back the sweeping border this model exists to avoid. A cell's
ramp-up is what darkens the ground before rain.

### Where the function lives

`shared/` crate, exported through `wasm_api` next to the message codec — the
same home as `celestial.rs` and `moon.rs`. Server calls it natively (future
gameplay hooks), client through wasm: once per frame at the player and a coarse
grid when the map overlay is open. One implementation, no drift.

## Server

- `WeatherState { seed: u64, bias: f32 }` in `GameState` next to `game_clock`
  (`server/src/game_state/weather.rs`). The seed is read from
  `weather-sectors.json` — it is the seed the sectors were placed with, so
  the server keeps no other record of the world seed; without the file,
  weather stays off and the server logs why. `bias` is a global multiplier on
  `chance` for events/admin (`/weather` later).
- `ServerMessage::WeatherSync { seed, bias }` on connection accept and from
  `run_ticks("weather", 30 s)` (same scaffolding as the time-sync tick).
  Seeds cross the wire as JS numbers, so they must stay below 2^53.
- Climate grid served at `/api/terrain/climate/{rx}/{rz}` and the sector list
  at `/api/terrain/weather-sectors`, both revalidated like tree files.
- `PROTOCOL_VERSION` 69 → 70; agent-client 0.50.0 lists `WeatherSync` as
  noise so it never wakes the LLM.
- Load at 5,000 CCU: one tiny broadcast per 30 s, zero per-player work.

## Client

`weatherStore.ts` (mirrors `timeStore.ts`) fed from `messageHandlers.ts`,
fetching the sector list into wasm on the first sync. The per-plot climate
route has no client reader yet; a climate map layer can add one modelled on
`landGradeStore.ts`. The rain function is called through wasm
(`weather_set_sectors`, `weather_game_minutes`, `weather_cells_at`,
`weather_rain_at`, `weather_cloud_factor`) — the client never re-implements
it, and even the game-minute conversion stays in Rust so `t` cannot drift a
day from the server's. Per-frame local sample drives:

1. **Lighting** — `cloudFactor` multiplied where `eclipseFactor` already is
   (`scene-lighting.ts`): directional `× (1 − 0.5·cf)`, ambient and
   environment `× (1 − 0.25·cf)`. Full overcast reads "cloudy afternoon",
   never "night".
2. **Rain particles** — `GameSceneRainLayer.svelte` from the prototype
   (instanced streaks, ground splash rings, pool ≤ 1,100, measured 66 fps /
   0.017 ms sim on the dev machine), spawn rate scaled by the local sample;
   a cell is kilometres wide, so one sample covers the whole view.
   `enableRainParticles` preset flag (off on low and mobile). Off
   indoors/dungeons; petals stop spawning under rain.
3. **Audio** — rain loop + distant thunder one-shots from the prototype's
   ambience manager (CC0 assets recorded in `doc/assets/sfx.md`), following
   the SFX volume/mute settings; ducked indoors; the BGM playlist goes quiet
   through the same quiet-zone path as bard performances, with hysteresis
   (on above 0.35, off below 0.2) so it does not flap at a cell edge.
4. **Map overlay** — `drawRainCells` (`utils/weatherOverlay.ts`) painted in
   `WorldMapDialog`'s atlas pass after the house footprints: soft discs
   whose opacity follows the cell envelope, a toggle button, and a forecast
   slider (0 to +12 game hours in 30-minute steps) that re-evaluates the same
   function. The clock is read untracked so the time sync alone never redraws
   the atlas; the 30 s `WeatherSync` write is what refreshes an open map.

## Phases

| Phase | Scope | Tier |
|---|---|---|
| A | climate bake + shared cell function + WeatherSync + stores + lighting | T2 |
| B | rain layer + preset flag + ambience (port from `experiment/rain-prototype`) | T1–T2 |
| C | map overlay + forecast scrub | T1 |
| D | storms: a rare oversized cell (10–15 km, 1–2 real hours) from its own sector so it spans zones, with slanted rain, lightning, and a wind boost — still a pure function, so the map forecasts landfall; snow on the alpine zone once a snow region is decided; wetness (campfire/hunger); fishing tables; wet/dry seasons by scaling `P` with the 360-day year in `celestial.rs`; `/weather` override | separate proposals |
| E | river levels and flooding from accumulated rain — belongs to the water system, after the upstream water-field and erosion experiments settle; weather never touches water itself | later, with the maintainer |

## Test plan

- Unit: determinism (fixed seed → golden cells), one-cell-per-sector invariant
  (`life ≤ 0.9·P`, cells never overlap in a sector), zone wet-time shares within
  ±5 pp of the table over 30 game days, X-wrap of sectors at the seam, envelope
  monotone up/down.
- Bake: every settlement in `data/map_labels.json` lands on a land zone;
  sea plots are zone 0.
- Protocol: `WeatherSync` round-trip through the codec.
- Client: store updates, lighting multiplier applied, overlay draw smoke test
  with a fake function.
- Live E2E: stand in a forecast cell as it forms; dim, particles and sound
  rise together; map matches the ground.

## Questions for the maintainer (with the proposal)

1. Existing plans for weather? Intentionally omitted?
2. Zones baked from elevation vs hand-placed anchors like `land_grades.rs`.
3. v1 scope confirmation (dim + rain + sound with distant thunder; no snow or lightning visuals yet).
4. Map exposure: world-map toggle only, or minimap too.
