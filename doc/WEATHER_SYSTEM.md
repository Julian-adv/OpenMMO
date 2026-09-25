# Weather System

Shipped with PR #173 (2026-09-12). Supersedes the moving-cloud draft of
2026-08-08.

## Goal

Regional weather. Each part of the world has a climate; rain forms over a
region, falls for a while, and clears. Only the ground under a rain cell gets
dimmed light, rain and sound. Tone: subtle, cozy rain that fits the warm
quarter-view look — not a gray realism filter. Because every cell is a pure
function of time, a map forecast can be added later without touching the
model; the world map is left as it is for now.

Non-goals: snow, weather-dependent
fishing, sky dome (quarter view — the sky is never on screen).

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
life     = min(L0 + h2 * Lv, 0.9 * P)
birth    = k * P + h3 * (P - life)         // the cell fits inside its cycle
active   = h1 < chance * bias * seasonalMultiplier(spawnSpot, birth)
envelope = ramp-up 25 % of life → full → ramp-down 30 % of life
falloff(d) = 1 - smoothstep(0.7, 1.0, d)   // flat top, short edge, 0 at the radius
rain(x, z, t) = min(1, sum over cells of envelope(t) * falloff(dist / radius))
```

The falloff is flat to 70 % of the radius and fades to zero at the radius, so
the disc on the map is exactly where it rains and a cell never soaks the
next zone from beyond its own edge. A Gaussian was tried first: it still held
37 % at the radius and drizzled out to twice it, which made the map
over-promise and let coastal cells wet the rain shadow.

Everything is a pure function of `(seed, sectors, t)` and the compiled
seasonal region settings — `shared/src/weather.rs`.
`t` is game minutes since the calendar epoch (`weather::game_minutes`, on top
of `moon::game_day_index`), already synced by `GameTimeSync`. Only cycle `k`
can be live at `t`, so the runtime checks one cycle per sector. Anyone who
knows the seed can evaluate any time — that is the forecast.

### Schedule (game minutes; a game day is 3 real hours)

These are the base schedules, before regional seasonal multipliers.

| Zone | km² per sector | Period | Lifetime | Chance | Radius | Real-time feel |
|---|---|---|---|---|---|---|
| Wet coast | 4 | 560 (9.3 h) | 165–240 | 0.9 | 1.6–2.6 km | 21–30 min of rain every ~1.2 h |
| Temperate | 7 | 2,000 (~1.4 d) | 120–240 | 0.8 | 3.5–5.4 km | 15–30 min every ~4 h |
| Rain shadow | 36 | 6,800 (~5 d) | 120–180 | 0.6 | 3.0–4.6 km | 15–22 min every ~14 h |
| Alpine | 12 | 1,100 (18 h) | 180–240 | 0.9 | 3.3–5.2 km | 22–30 min every ~2.3 h |

A rain event must be felt inside a play session: 15–30 real minutes. Games
with compressed clocks converge on 10–25 real minutes per event regardless
of day length (FFXIV 23 min slots, Mabinogi 20 min blocks, Minecraft 10–20
min), Black Desert's 40–60 min draws "30 is enough" complaints, and Red
Dead Online's 1–2 min reads as broken, so the floor stays at 15 and the cap
is 30. Dryness is expressed by the gap between events,
not by shorter events. Measured share of time a plot is wet (30 game days,
seed 42, Valdran: 22 coastal, 15 temperate, 1 shadow, 1 alpine sector):
wet coast 25 %, temperate 18 %, alpine 19 %, rain shadow 14 %. Cells reach
3–5 km, so the rain shadow is "the least rainy place", never bone dry — most
of its rain is spill from the zones around it.

Two lessons from tuning on the real bake: sector count must follow zone
area (a grid-bucket cut gave the 1 km coastal band ten times too many
sectors), and 3–5 km coastal cells centred in that band soaked every zone
behind it — hence the small shoreline showers. The ignored test
`weather_zone_shares_from_bake` in `terrain/src/tests.rs` re-measures this
from baked climate files in under a second.

### Seasonal rain in western Valdran

`data-src/weather.json` defines a winter-wet, summer-dry region around
Aldermark at `(-1475.2, 4741.6)`. The multiplier applies fully to cell birth
positions within 6 km and fades to the base schedule between 6 and 8 km.
Cells outside the region retain their original schedules. This includes
all nearby coastal and inland cells that can reach the village.

The calendar follows the existing solstices and equinoxes:

| Season | Dates | Chance multiplier in the region core | Village wet-time target |
|---|---|---|---|
| Winter | 12/30–3/29 | 1.0 | about 37% |
| Spring | 3/30–6/29 | smoothly decreases from 1.0 to 0.11 | about 21% on average |
| Summer | 6/30–9/29 | 0.11 | about 5% |
| Autumn | 9/30–12/29 | smoothly increases from 0.11 to 1.0 | about 21% on average |

The targets describe time spent under rain at the village, not the chance
of a single sector producing a cell. Overlapping cells make those different
quantities. Spring and autumn use smoothstep interpolation; winter and summer
hold their values. The multiplier is evaluated at the cell's scheduled birth,
so an accepted event keeps its original duration, strength and fade even
across season boundaries. `WEATHER_BIAS` still multiplies the final chance.

The configuration is compiled into both native Rust and client WASM. No
terrain re-bake or sector-file migration is needed. After rebuilding WASM,
run `node tools/measure-weather.mjs 20` from the repository root to sample
20 game years at the configured player spawn, with `bias=1`, rain intensity
above 0.01, and a five-game-minute step.

With the current seed-42 bake (104 sectors), that sample gives winter 36.8%,
spring 21.6%, summer 4.8%, autumn 20.7%, and an annual mean of 21.0%.
The same sample before seasonality averaged 36.7%, so yearly wet time drops
by about 43%. These are long-run shares; individual years and nearby
positions vary.

Derived values: `rainIntensity = rain(x, z, t)`; `cloudFactor =
smoothstep(0.35, 0.80, rain)`. There is no separate background cloud layer:
in a quarter view the sky is never on screen, so a drifting shadow pattern
would only bring back the sweeping border this model exists to avoid. A cell's
ramp-up is what darkens the ground before rain.

### Where the function lives

`shared/` crate, exported through `wasm_api` next to the message codec — the
same home as `celestial.rs` and `moon.rs`. Server calls it natively for rain
exposure, client through wasm once per frame at the player. One
implementation, no drift.

## Server

- `WeatherState { seed, bias, sectors, sectors_json, sectors_tag, rain_override }` in `GameState`
  next to `game_clock` (`server/src/game_state/weather.rs`). The seed is
  read from `weather-sectors.json` — it is the seed the sectors were placed
  with, so the server keeps no other record of the world seed; without the
  file, weather stays off and the server logs why. The file bytes stay in
  memory with a content hash (`sectors_tag`), so a re-bake reaches clients
  only through a restart and every client evaluates the list the seed was
  broadcast with. `bias` multiplies every zone's
  `chance` (1.0 = baked schedule, 0.5 = half the cells, 0 = off); it comes
  from `--weather-bias` / `WEATHER_BIAS` at boot, so the amount of rain is a
  deployment setting rather than a code change.
- `ServerMessage::WeatherSync { seed, bias, sectors_tag, rain_override }` on connection
  accept, on admin override changes, and from `run_ticks("weather", 30 s)` (same scaffolding as the
  time-sync tick). Seeds cross the wire as JS numbers; `place_sectors` masks
  them to 53 bits.
- Climate grid served at `/api/terrain/climate/{rx}/{rz}`, revalidated like
  tree files. The sector list at `/api/terrain/weather-sectors?v=<tag>` is
  served from memory as immutable; the tag in the URL is the cache key.
- `PROTOCOL_VERSION` 69 → 70; agent-client 0.50.0 lists `WeatherSync` as
  noise so it never wakes the LLM. Version 78 adds `rain_override`.
- One weather broadcast per 30 s. Rain exposure evaluates the active cells
  once per second and shares them across player samples, using cached sectors
  and room footprints without terrain IO.

### Rain exposure

Continuous outdoor rain applies the existing `wet` debuff after ten game
minutes at full intensity (75 real seconds). Exposure scales with intensity;
half-strength rain takes twice as long. The server's one-second hunger sweep
includes stationary players and ignores graphics settings.

Dry weather (intensity at or below 0.02), ground-floor rooms, upper floors,
dungeons, death, and loading reset partial exposure. Room footprints are
cached when houses load or change and removed on demolition, so gaps between
rooms stay exposed. Official NPCs remain exempt from debuffs.

Continued rain refreshes existing wetness to 450 seconds when less than 300
seconds remain, including wetness acquired in water. Shelter stops exposure;
the existing debuff then dries naturally or faster by a lit campfire.
See [DEBUFF.md](DEBUFF.md) for movement and armor-weight effects.

### Admin debugging

| Command | Effect |
| --- | --- |
| `/weather rain [intensity]` | Force rain server-wide; intensity is 0–1, default 1. |
| `/weather clear` | Force zero rain server-wide. |
| `/weather auto` | Resume the current regional and seasonal weather. |

These chat commands use the existing server admin gate and appear in admin
`/help` and autocomplete. The override broadcasts immediately, persists through
reconnects, and lasts until `/weather auto` or a server restart. It requires
loaded weather data. Invalid arguments leave the current weather unchanged.

The override changes sampled rain intensity and its cloud factor, so particles,
ambience, shadows, and petals follow the same weather path. Existing indoor and
dungeon suppression still applies. It leaves the seed, bias, sector list, and
game clock intact; automatic weather continues to advance underneath it.

## Client

`weatherStore.ts` (mirrors `timeStore.ts`) fed from `messageHandlers.ts`,
fetching the sector list into wasm when the sync carries a tag it has not
loaded; a reconnect with the same tag fetches nothing. The per-plot climate
route has no client reader yet; a climate map layer can add one modelled on
`landGradeStore.ts`. The rain function is called through wasm
(`weather_set_sectors`, `weather_day_start_minutes`, `weather_rain_at`,
`weather_cloud_factor`, and `weather_cells_at` for the debug radar) — the
client never re-implements
it, and even the game-minute conversion stays in Rust so `t` cannot drift a
day from the server's. Per-frame local sample drives:

1. **Lighting** — `cloudFactor` multiplied where `eclipseFactor` already is
   (`scene-lighting.ts`): directional `× (1 − 0.5·cf)`, ambient and
   environment `× (1 − 0.25·cf)`. Full overcast reads "cloudy afternoon",
   never "night". Sun shadow intensity fades from 1 to 0.03 as rain rises
   from 0 to 0.2, including every CSM cascade.
2. **Rain particles** — `GameSceneRainLayer.svelte` from the prototype
   (instanced streaks, ground splash rings, pool ≤ 1,100, measured 66 fps /
   0.017 ms sim on the dev machine), spawn rate scaled by the local sample;
   a cell is kilometres wide, so one sample covers the whole view.
   Streaks and ground splashes respond to scene lighting, including torch/fire
   color and distance attenuation. Their diffuse scattering ignores billboard
   orientation, keeping nearby rain visible with light behind the drops.
   High/medium use up to 1,100 streaks and 350 ground splashes. Low and mobile
   share a 300-streak pool with about 27% of the full spawn rate. They allocate
   no splash particles, mesh, or texture. Changing presets recreates the rain
   pools and disposes their previous GPU resources. Rain stays off
   indoors/dungeons; petals stop spawning under rain.
   Rain also accumulates on the terrain as described below.
3. **Audio** — a sparse droplet loop for light rain, crossfading into the
   original heavy-rain recording above intensity 0.45 (smoothstep, fully
   replaced at 1), plus distant thunder one-shots. Sources and licenses are
   recorded in `doc/assets/sfx.md`. Both loops follow the SFX volume/mute
   settings and share the existing 0.5 gain multiplier, with thunder volume
   unchanged. Ducked indoors; the BGM playlist goes quiet
   through the same quiet-zone path as bard performances, with hysteresis
   (on above 0.35, off below 0.2) so it does not flap at a cell edge.
4. **Lightning** — above rain intensity 0.35, the existing directional light
   flashes white at the game's maximum sunlight intensity (10), holds for
   50 ms, then fades back to the current sun/moon lighting over 450 ms.
   Thunder follows 2–5 seconds after the flash. The first flash comes after
   16–60 seconds, then repeats every 50–140 seconds. Each strike picks an
   independent sky direction (any azimuth, elevation 30–75°), with the same
   sampled direction throughout the strike. As it fades, direction and color
   blend back to sunlight/moonlight by their intensity contributions.
   The flash is weaker indoors.
   It lights scene surfaces on every graphics preset, even with SFX muted.
   Dry weather, dungeons, and leaving the game cancel pending strikes.
   Settings → Lightning Flashes disables current and future flashes without
   changing rain or thunder audio. It defaults to on and is saved per browser.

The world map is deliberately untouched. A forecast layer (cells as soft
discs in the atlas pass, a slider that evaluates the same function at a later
time) was prototyped and works, but it is held back until there is a
gameplay reason for players to read the weather ahead.

### Weather radar (debug overlay)

RADAR in the debug panel opens the tool used while tuning the schedule: every
live cell over the baked region minimaps, coloured by stage, with the radius
and the time left; the rain at the player and when the next one reaches them;
and a scrub that evaluates the same function up to half a game day ahead,
with a fast-forward. It reads the server clock, never the sun-debug display
hour, and samples on its own 250 ms timer rather than the render loop, so the
whole-continent sweep and the next-rain scan stay out of the frame. Cells
whose disc reaches the drawn window are listed even when their centre sits
across the world seam, the way `rain_at` measures distance. Escape or the
title-bar button closes it.

### Rain puddles

Wet terrain darkens across the whole surface, including between puddles.
Dampness reaches its full strength at 45% accumulated wetness, reducing the
lit ground's linear color by 38%. Irregular puddles spread over nearly flat
ground and retain a transparent bed. Inside each puddle, stone normals blend
toward the flat terrain normal, baked color contrast is softened, and crevice
occlusion is reduced. The shared puddle mask also controls a narrow dark rim;
its transition follows pixel width so the edge stays sharp without aliasing.
Rain ripples add narrow highlights on crests facing the camera. Their brightness
follows daylight, and they fade as each wave expands. Ripples settle when rain
stops; the wet ground and puddles remain until they dry. Puddles use no projected
cloud or light-streak texture.
Slopes and terrain below sea level do not form puddles. Dungeon terrain stays
dry, and the map editor's brush/grid remains unobscured.

`rainPuddles.ts` controls the timing in real seconds: full rain fills the ground
in 90 seconds; lighter rain takes longer. Rain at or below 0.02 lets it dry.
Once rain stops, puddles shrink from their edges, leaving damp ground that
returns to its original appearance within 240 seconds. Rain restarting refills
the remaining water.

High graphics includes wet ground, puddles, and animated rain ripples.
Medium keeps wet ground and puddle accumulation/drying, skipping ripple
animation and lighting calculations. Low and mobile disable all three and
pause puddle weather sampling. Preset changes apply through shader uniforms
without recompiling terrain materials. Returning from low restores wetness
from current weather history; high/medium switches keep accumulated water.

Rain is sampled about once per second at shared 64 m tile corners. Wetness
advances every frame and its displayed value eases over 0.35 seconds to avoid
stepping edges. Weather queries, including the initial 330-second history
replay, share a limit of eight calls per frame. Only visible corners receive
updates; unvisited corners restore their history on return. The cache retains
up to 256 corners, discarding samples older than eight minutes on return.
Overrides accumulate from their first observation because the server does not
provide their start time.

Puddles reuse the terrain draw and the existing baked value-noise texture.
Color, normal, occlusion and ripple highlights reuse the same mask and noise samples.
Ground below the puddle visibility threshold skips those noise samples and
only applies dampness darkening.
They add no scene captures, render targets or draw calls on any graphics preset.
The former planar capture of trees/buildings/characters was removed to avoid
repeating scene geometry, skinning and shadow work. The normal sea/river
reflection stays at sea level.

The ripple facing direction and daylight strength are computed once per frame.
Ripple animation uses the continuous render clock. Calm puddles skip ripple math.
`__togglePuddles()` hides/shows just the puddle shader for comparison; wetness
keeps advancing. `__profile()` reports puddle CPU time separately under
`puddles`, alongside rain particles, rendering and other scene work.

## NPC shelter

Wick's night stall and Signe's daytime square performance have
`shelter_from_rain: true` in their schedules. When rain intensity at that
outdoor destination exceeds 0.02, the agent uses its `at: "rain"` entry:
Wick rests in chair 42 and Signe in chair 39 on the inn's ground floor.
They pack up their stall or tip hat and stop playing before moving. Their
rain routine invites quiet conversation and listening to the rain.

The agent consumes `WeatherSync`, caches the tagged weather-sector endpoint,
and evaluates the shared weather model against `GameTimeSync`. Admin rain
and clear overrides apply as well. Rain is sampled at the outdoor work spot,
so entering the inn does not make the NPC immediately go back outside.
When rain clears, the current time's routine resumes. Sleep, meals, the
merchants' meeting, and Signe's indoor evening performance keep their usual
times. Rain entries never activate from the clock alone.

## Test plan

- Unit: determinism (fixed seed → golden cells), one-cell-per-sector invariant
  (`life ≤ 0.9·P`, cells never overlap in a sector), zone wet-time shares within
  ±5 pp of the table over 30 game days, X-wrap of sectors at the seam, envelope
  monotone up/down.
- Bake: every settlement in `data/map_labels.json` lands on a land zone;
  sea plots are zone 0.
- Protocol: `WeatherSync` round-trip through the codec.
- Client: store updates, lighting multiplier applied, rain ambience
  lifecycle.
- Live E2E: stand in a cell as it forms; dim, particles and sound rise
  together and settle back to idle.
