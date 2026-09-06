# Weather System (design, pre-implementation)

Status: design, implementation in progress on `feature/weather-system`; the
proposal goes upstream once it is built and tested. Supersedes the
moving-cloud draft of 2026-08-08.

## Goal

Regional weather. Each part of the world has a climate; rain forms over a
region, falls for a while, and clears. Only the ground under a rain cell gets
dimmed light, rain and sound. Players can open the world map and see where it
rains now and where it will rain next. Tone: subtle, cozy rain that fits the
warm quarter-view look — not a gray realism filter.

Non-goals (v1): snow, lightning/thunder, wetness debuffs, weather-dependent
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

### Cells (derived, never stored)

Each zone is split into a few *sectors* (spawn spots spread through the zone,
inset from its border, chosen from the seed). A sector hosts at most one cell
at a time. For sector `s` with period `P` and lifetime range `[L0, L0 + Lv]`,
cycle `k` yields

```
h1, h2, h3 = hash(seed, s, k)
active   = h1 < chance
life     = min(L0 + h3 * Lv, 0.9 * P)
birth    = k * P + h2 * (P - life)         // the cell fits inside its cycle
envelope = ramp-up 25 % of life → full → ramp-down 30 % of life
rain(x, z, t) = max over nearby cells of envelope(t) * falloff(dist / radius)
```

Everything is a pure function of `(seed, zone grid, t)`. `t` is game time,
already synced by `GameTimeSync`; `game_day_index` in `shared/src/moon.rs`
gives a stable epoch. Anyone who knows the seed can evaluate any time — that is
the forecast.

### Schedule (game minutes; a game day is 3 real hours)

| Zone | Sectors | Period | Lifetime | Chance | Radius | Real-time feel |
|---|---|---|---|---|---|---|
| Wet coast | 7 | 510 (8.5 h) | 165–300 | 0.9 | 3.0–4.8 km | 21–37 min of rain every ~1 h |
| Temperate | 5 | 2,800 (~2 d) | 120–240 | 0.8 | 3.4–5.4 km | 15–30 min every ~6 h |
| Rain shadow | 1 | 6,800 (~5 d) | 120–180 | 0.6 | 2.6–4.0 km | 15–22 min every ~14 h |
| Alpine | 1 | 1,260 (21 h) | 180–360 | 0.9 | 2.8–4.4 km | 22–45 min every ~2.6 h |

A rain event must be felt inside a play session; 15–40 real minutes matches
FFXIV's 23-minute weather slot. Dryness is expressed by the gap between events,
not by shorter events. Measured share of time a spot is wet (30 game days,
seed 42): wet coast 33 %, temperate 23 %, alpine 22 %, rain shadow 17 %. The
continent is only 22 km wide and cells are 3–5 km, so the rain shadow is "the
least rainy place", never bone dry.

Sector count scales with zone area; the first cut with equal sectors made the
narrow rain shadow wetter than the coast because neighbours spilled in.

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

- `WeatherState { seed: u64, bias: f32 }` in `GameState` next to `game_clock`.
  Seed rolled once and persisted; `bias` is a global multiplier on `chance`
  for events/admin (`/weather` later).
- `ServerMessage::WeatherSync { seed, bias }` on connection accept and from a
  slow `run_ticks("weather", 30 s)` (same scaffolding as the time-sync tick).
- Climate grid: baked by `terrain-gen` into `data/terrain/climate/`, served
  like land grades. No runtime state.
- `PROTOCOL_VERSION` 57 → 58.
- Load at 5,000 CCU: one tiny broadcast per 30 s, zero per-player work.

## Client

`weatherStore.ts` (mirrors `timeStore.ts`) fed from `messageHandlers.ts`;
`climateStore.ts` mirrors `landGradeStore.ts`. Per-frame local sample drives:

1. **Lighting** — `cloudFactor` multiplied where `eclipseFactor` already is
   (`scene-lighting.ts`): directional `× (1 − 0.5·cf)`, ambient `× (1 −
   0.25·cf)`. Full overcast reads "cloudy afternoon", never "night".
2. **Rain particles** — `GameSceneRainLayer.svelte` from the prototype
   (instanced streaks, ground splash rings, pool ≤ 1,100, measured 66 fps /
   0.017 ms sim on the dev machine). `enableRainParticles` preset flag. Off
   indoors/dungeons; petals stop spawning under rain.
3. **Audio** — rain loop + distant thunder one-shots from the prototype's
   ambience manager (CC0 assets recorded in `doc/assets/sfx.md`); ducked
   indoors; BGM quiet zone while raining.
4. **Map overlay** — painted in `WorldMapDialog`'s atlas pass next to
   `drawLandPlotCells`: translucent cells by intensity, cell state labels
   (forming / rain / clearing), a toggle, and a forecast scrub (+N game hours)
   that re-evaluates the same function.

## Phases

| Phase | Scope | Tier |
|---|---|---|
| A | climate bake + shared cell function + WeatherSync + stores + lighting | T2 |
| B | rain layer + preset flag + ambience (port from `experiment/rain-prototype`) | T1–T2 |
| C | map overlay + forecast scrub | T1 |
| D | snow on the alpine zone once a snow region is decided; lightning; wetness (campfire/hunger); fishing tables; wet/dry seasons by scaling `P` with the 360-day year in `celestial.rs`; `/weather` override | separate proposals |

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
3. v1 scope confirmation (dim + rain + sound; no snow/thunder yet).
4. Map exposure: world-map toggle only, or minimap too.
