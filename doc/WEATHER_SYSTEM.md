# Weather System

Shipped with PR #173 (2026-09-12). Supersedes the moving-cloud draft of
2026-08-08.

## Goal

Regional weather. Each part of the world has a climate; rain forms over a
region, drifts slowly with the seasonal wind while growing or shrinking,
and clears. Only the ground under a rain cell gets
dimmed light, rain and sound. Tone: subtle, cozy rain that fits the warm
quarter-view look — not a gray realism filter. Because every cell is a pure
function of time, a map forecast can be added later without touching the
model; the world map is left as it is for now.

Winter cells fall as snow (see Snow). Non-goals: weather-dependent fishing,
sky dome (quarter view — the sky is never on screen).

## Why slow drifting cells

The first draft moved a noise field with the wind; on the ground it read as a
rain border sweeping through the village. The shipped model then kept cells
fixed at their spawn spot, which looked like rain switching on and off in
place.

Cells now keep their spawn schedule but drift at 5–14 m per game minute,
0.7–1.9 m/s in real time, within ±25° of the seasonal heading below, and end their life at 0.6–1.6× their birth
radius. The fade band (30 % of the radius) takes about 5 real minutes to cross
a fixed point for the smallest, fastest coastal shower and up to 30 for large
slow cells, so the edge never sweeps visibly, and a player walking at
3 m/s can still outrun a cell. What makes weather feel alive is that it is
visible, predictable and avoidable (FFXIV keys weather to zones from a
deterministic timestamp hash, which makes its forecasts possible); the drift
stays a pure function of time, so that holds.

## Model

### Climate zones (baked)

One climate byte per land plot, 1,024 per region — the exact shape of the land
grade grid (`terrain/src/land.rs`, `/api/terrain/land-grades/{rx}/{rz}`,
`landGradeStore.ts`). Baked from worldgen elevation, no hand authoring:

| Zone | Rule (seed 42 measurements) |
|---|---|
| Wet coast | ≤ 1 km from the sea and below 900 m |
| Alpine | ≥ 1,500 m (worldgen paints permanent snow from 1,800 m) |
| Temperate | everything else on land |

Sea plots carry zone 0 and never host a cell. A static westerly rain-shadow
zone was replaced by the seasonal lee (see Orographic rain); Alpine is byte 3.

### Sectors (baked)

The bake gives each zone one sector per `SECTOR_KM2` of area (wet coast 4,
temperate 7, alpine 12 km²), spread through the zone by
farthest-point sampling; a sector's spots are the 16 plots around its centre.
Inland spots sit as far from the zone border as the zone allows; wet-coast
spots hug the shoreline (within 128 m of the sea) so coastal showers spill
seaward instead of soaking the zones behind them. Written world-wide to `data/terrain/weather-sectors.json`
(`shared/src/worldgen/weather_sectors.rs`), 117 sectors on seed 42, so clients
never analyse the climate grid themselves. Each sector also carries its
ground elevation and the highest upwind ridge in 16 headings
(`climate::upwind_ridges`), since clients have no elevation data. File
version 2.

### Cells (derived, never stored)

A sector hosts at most one cell at a time. For sector `s` with period `P` and
lifetime range `[L0, L0 + Lv]`, cycle `k` yields

```
h1, h2, h3 = hash(seed, s, k)
life     = min(L0 + h2 * Lv, 0.9 * P)
birth    = k * P + h3 * (P - life)         // the cell fits inside its cycle
heading  = seasonal heading(birth) ± 25°
active   = h1 < chance * bias * (1 - 0.89 * lee(sector, heading))
envelope = ramp-up 25 % of life → full → ramp-down 30 % of life
centre   = spawnSpot + v * age            // v along heading, zone speed range
radius   = r0 * g^progress * (0.6 + 0.4 * envelope)   // g in [0.6, 1.6], log-uniform
falloff(d) = 1 - smoothstep(0.7, 1.0, d)   // flat top, short edge, 0 at the radius
rain(x, z, t) = min(1, sum over cells of envelope(t) * falloff(dist / radius))
```

The falloff is flat to 70 % of the radius and fades to zero at the radius, so
the disc on the map is exactly where it rains and a cell never soaks the
next zone from beyond its own edge. A Gaussian was tried first: it still held
37 % at the radius and drizzled out to twice it, which made the map
over-promise and let coastal cells wet the zones behind them.

Everything is a pure function of `(seed, sectors, t)` — `shared/src/weather.rs`.
`t` is game minutes since the calendar epoch (`weather::game_minutes`, on top
of `moon::game_day_index`), already synced by `GameTimeSync`. Only cycle `k`
can be live at `t`, so the runtime checks one cycle per sector. Anyone who
knows the seed can evaluate any time — that is the forecast.

### Schedule (game minutes; a game day is 3 real hours)

These are the base schedules, before the lee.

| Zone | km² per sector | Period | Lifetime | Chance | Birth radius | Drift (m/game min) | Real-time feel |
|---|---|---|---|---|---|---|---|
| Wet coast | 4 | 560 (9.3 h) | 165–240 | 0.45 | 1.6–2.6 km | 5–9 | 21–30 min of rain every ~2.6 h |
| Temperate | 7 | 2,000 (~1.4 d) | 120–240 | 0.4 | 3.5–5.4 km | 8–14 | 15–30 min every ~10 h |
| Alpine | 12 | 1,100 (18 h) | 180–240 | 0.45 | 3.3–5.2 km | 6–12 | 22–30 min every ~5 h |

Chances were halved on 2026-09-26: at 0.8–0.9 the wettest places were under
rain nearly 40 % of a season, too much to play in.

Wet-coast showers drift slowest so they stay near the shore they formed on.

The heading follows the rain-season calendar world-wide and is fixed at birth,
so each cell moves in a straight line. Winter and summer are not opposite:
the summer wind comes from the east-southeast so it crosses the central
Valdran massif before reaching Aldermark.

| Season | Drift heading |
|---|---|
| Winter | east-northeast (22.5° north of east) |
| Spring | turns through north |
| Summer | west-northwest (22.5° north of west) |
| Autumn | turns through south |

A rain event must be felt inside a play session: 15–30 real minutes. Games
with compressed clocks converge on 10–25 real minutes per event regardless
of day length (FFXIV 23 min slots, Mabinogi 20 min blocks, Minecraft 10–20
min), Black Desert's 40–60 min draws "30 is enough" complaints, and Red
Dead Online's 1–2 min reads as broken, so the floor stays at 15 and the cap
is 30. Dryness is expressed by the gap between events,
not by shorter events. Measured share of time a plot is wet (30 game days,
seed 42, current bake), winter / summer wind: wet coast 12.5 / 11.9 %,
temperate 10.0 / 10.4 %, alpine 11.4 / 11.4 %. Zone averages barely move with
the season; the lee moves rain from one side of a massif to the other.

Two lessons from tuning on the real bake: sector count must follow zone
area (a grid-bucket cut gave the 1 km coastal band ten times too many
sectors), and 3–5 km coastal cells centred in that band soaked every zone
behind it — hence the small shoreline showers. The ignored test
`weather_zone_shares_from_bake` in `terrain/src/tests.rs` re-measures this
from the baked climate files and sector list in seconds, for winter and
summer wind.

### Orographic rain

A cell born downwind of a ridge has little moisture left. At birth, the
sector's ridge profile is read in the upwind direction (the cell's heading
+ 180°, interpolated between the 16 baked headings):

- The profile holds the highest land within 14 km upwind, the lowest of three
  parallel rays 1 km apart, so a lone spur casts no shadow. 1 km of open sea
  on the centre ray ends the scan; inlets do not.
- `lee = smoothstep(1000, 1400, ridge) * (1 - smoothstep(700, 1200, elevation))`.
  Coastal ranges (about 1,000 m on seed 42) stay under the threshold; sectors
  on high ground catch their own rain.
- A full lee leaves 11 % of the chance. The windward side keeps the base
  schedule; nothing is boosted.

The check uses the birth heading, so an accepted event keeps its duration,
strength and fade across season changes. `WEATHER_BIAS` still multiplies the
final chance. Thresholds live in `weather.rs`, so retuning needs no re-bake.

With the seasonal wind, western Valdran is wet in winter and drier in summer,
and the lowlands east of the central massif flip. `node tools/measure-weather.mjs
20 [x z]` samples 20 game years (bias 1, rain above 0.01, 5-minute step) at the
spawn or a given point. Seed 42, winter / spring / summer / autumn:

| Point | Wet time |
|---|---|
| Aldermark (-1475, 4741), NW tip of the massif | 20.3 / 11.3 / 2.9 / 14.5 % |
| East lowland (10000, 9000) | 6.9 / 16.2 / 20.1 / 15.8 % |
| Southwest plain (-2500, 11000) | 12.9 / 8.9 / 4.3 / 10.5 % |
| Southeast (6000, 16000) | 4.4 / 14.1 / 22.9 / 12.8 % |

Annual means are 9–15 %. Spring turns the wind through north, putting the
massif south of Aldermark upwind; autumn turns through south and brings sea
air.

Derived values: `rainIntensity = rain(x, z, t)`; `cloudFactor =
smoothstep(0.35, 0.80, rain)`. There is no separate background cloud layer:
in a quarter view the sky is never on screen, and a fast shadow pattern would
bring back the sweeping border. A cell's ramp-up is what darkens the ground
before rain.

### Snow

Each cell draws once at birth whether it falls as snow, so a cell keeps its
kind for its whole life:

```
snow chance = winter_depth(birth) * max(latitude share(spot z), high(elevation))
```

- `winter_depth` is 1 from day 15 to day 75 of winter and eases to 0 half a
  month into autumn and spring.
- The latitude share is 0.95 up to z 5,500 (Aldermark is at z 4,742) and
  eases to 0.5 by z 8,500, so the southern lowlands still get winter rain
  half the time. Sectors above 600–1,200 m snow whatever their latitude.
- `rain_at` stays the total precipitation, so shelter, cloud factor and NPC
  shelter treat snow like rain. `precip_at` splits it into rain and snow
  (one pass; wasm `weather_precip_at`).

Lying snow (`snow_cover_at`) is also a pure function of time: it replays the
cells that crossed the point in the last 2,000 / 0.6 + 300 game minutes in
5-minute steps. Full snowfall covers the ground in 90 game minutes (11 real
minutes). A full cover melts in 180 game minutes on the mildest winter spot
and 1,000 on the coldest, 0.6× as fast at night and 1.5× at noon; full rain
alone clears it in 150. It costs about 25 µs in wasm. Seed 42, midwinter
(winter days 15–75, 10 years):

| Point | Snowfall | Any snow on the ground | Cover over half |
|---|---|---|---|
| Aldermark (-1475, 4741) | 18.7 % | 75 % | 55 % |
| Garasden (1929, 2746) | 9.1 % | 46 % | 27 % |
| Riftmark (-2704, 10328) | 7.7 % (7.1 % rain) | 24 % | 12 % |
| Southeast (6000, 16000) | 3.5 % | 10 % | 5 % |

Snow sets off the `cold` debuff instead of `wet` ([DEBUFF.md](DEBUFF.md)).
`/weather snow [intensity]` forces snowfall; the client builds and melts the
forced cover locally at the midwinter pace, since the override has no
history. Protocol 103: cells now fall as snow and `WeatherSync` carries
`snow_override`.

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
  noise so it never wakes the LLM. Version 78 adds `rain_override`. Version
  102 makes cells drift; the wire is unchanged, but an older build would place
  rain elsewhere than the server.
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

5. **Snow** — `GameSceneSnowLayer.svelte` drifts round flakes (about 6 s to
   fall) with the wind, gusts and per-flake sway; the pool is 2.2× the
   rain pool of the preset. Rain streaks, splashes, rain audio, lightning and
   the BGM quiet follow only the rain part; the overcast sky and dim light
   follow both.
6. **Lying snow** — one `cover` uniform (`snow-cover-nodes.ts`), sampled at
   the player every 2 s and eased over 3 s, drives every snowy surface; a
   cell's edge is hundreds of metres wide, so one value covers the view.
   Flat ground, Perlin hollows and crevices (low bed AO, e.g. paving grout)
   whiten first, slopes need a deeper cover and
   cliffs stay bare; specks just outside the patches read as a dusting. The
   terrain blends to the alpine snow texture (palette slot 3) with its normal,
   roughness and AO, zero metalness, and no puddles under it. Grass blades
   inside a patch sink to 15 % of their height and the rest frost at the tips.
   Trees and roof slopes (`GeoEntry.outdoor`, a separate cached material per
   roof texture) whiten on faces pointing up. Walkers on snowy ground leave
   cool-tinted prints that last 120 s, masked to the white patches. Every
   preset shows lying snow; bare ground skips the noise and snow-texture
   samples.
   The editor grid and brush ring draw on top of snow; snow hides only
   while a brush is active.
   `__snowCover(v)` pins the cover for look-dev; no argument releases it.

The world map is deliberately untouched. A forecast layer (cells as soft
discs in the atlas pass, a slider that evaluates the same function at a later
time) was prototyped and works, but it is held back until there is a
gameplay reason for players to read the weather ahead.

### Weather radar (debug overlay)

RADAR in the debug panel opens the tool used while tuning the schedule: every
live cell over the baked region minimaps, coloured by stage, with the radius,
the time left and a line to where its centre will be when it dies; the rain at the player and when the next one reaches them;
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
  monotone up/down, seasonal drift heading within the zone speed range.
- Bake: every settlement in `data/map_labels.json` lands on a land zone;
  sea plots are zone 0.
- Protocol: `WeatherSync` round-trip through the codec.
- Client: store updates, lighting multiplier applied, rain ambience
  lifecycle.
- Live E2E: stand in a cell as it forms; dim, particles and sound rise
  together and settle back to idle.
