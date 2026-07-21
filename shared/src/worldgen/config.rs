use serde::{Deserialize, Serialize};

/// A region-targeted mountain insertion. World-meter coordinates with origin
/// at map center. The boost cascades: peaks ≥ 40% of `max_elevation_m` seed
/// rivers, which then attract settlements via the dist_to_river fitness term.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationHotspot {
    pub center_x_m: f32,
    pub center_y_m: f32,
    pub radius_m: f32,
    pub peak_m: f32,
    /// Cap on final elevation inside the disk. `None` = uncapped (only the
    /// global `max_elevation_m` applies). Use to hold a hotspot under the
    /// splatmap snow line (1800 m) regardless of how stacking noise lands.
    #[serde(default)]
    pub cap_elev_m: Option<f32>,
}

/// A linear elevation-carve along a polyline. Each cell within `width_m / 2`
/// of the segment is clamped (via `min`) to a linearly-interpolated target
/// elevation — no raises, only lowers — yielding a monotonic downhill chain
/// that flow accumulation will follow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverCarvePath {
    pub start_x_m: f32,
    pub start_y_m: f32,
    pub end_x_m: f32,
    pub end_y_m: f32,
    pub width_m: f32,
    pub start_elev_m: f32,
    pub end_elev_m: f32,
}

/// Configuration for a full procedural world generation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldGenConfig {
    /// Master seed. Sub-systems derive their own seeds from this.
    pub seed: u64,

    /// World extent in meters (square world). Typical: 32768.
    pub world_size_m: u32,

    /// Global map resolution (cells per side). Typical: 4096.
    /// One global cell covers `world_size_m / global_res` meters.
    pub global_res: u32,

    /// Reference resolution for interpreting all `*_cells` and
    /// `*_wavelength*` fields. Each such field is measured in *reference
    /// cells*; at runtime it is scaled to actual cells via
    /// `res_scale = global_res / reference_res`. This lets the same config
    /// produce the same macro world shape at any `global_res` — tuning at
    /// low res then baking at high res is lossless. Typical: 4096 (matches
    /// the default `global_res`). Tests often set this equal to `global_res`
    /// so the literal field values apply without scaling.
    pub reference_res: u32,

    /// Target fraction of the world covered by sea (0..1).
    pub sea_ratio: f32,

    /// Continent noise frequency, in cycles per global cell.
    /// Lower = fewer, larger continents. Typical: 1.0 / 2048.0 at 4096 res
    /// (so full map spans ~2 cycles, giving 2-3 continents).
    pub continent_frequency: f32,

    /// Number of fBm octaves for continental shape. More octaves = more
    /// jagged, fractal coastlines. Fewer = smoother, simpler shapes.
    pub continent_octaves: u32,

    /// Amplitude decay per octave (aka persistence). Higher = more of the
    /// high-frequency detail shows through, making coastlines more jagged.
    /// Lower = smoother shapes. Range roughly 0.3 (smooth) to 0.65 (rough).
    pub continent_gain: f32,

    /// Minimum land-component size in global cells. Components smaller than
    /// this are reclassified as sea after the main mask is computed, removing
    /// tiny islands. Set to 0 to disable. Typical: 50-500 depending on taste.
    pub min_island_cells: u32,

    /// Minimum width (in global cells) of a land isthmus. Narrower land
    /// bridges are cut via morphological opening (erode + dilate by radius =
    /// width / 2). Breaks natural continents apart where they're narrowly
    /// connected, producing archipelagos that require boats to traverse.
    /// Set to 0 to disable. Typical: 6-20 cells (at 8m/cell = 48-160m).
    pub min_strait_width_cells: u32,

    /// Strength of the secondary "sea channel" ridge noise subtracted from
    /// the continental potential. Zero-crossings of a low-frequency secondary
    /// noise form ridge lines that get carved as seas, producing natural
    /// straits that split otherwise-connected landmasses. 0.0 = disabled;
    /// 0.4-0.8 gives visible straits; 1.0+ turns continents into archipelagos.
    pub sea_channel_strength: f32,

    /// Wavelength (in global cells) of the sea-channel noise. Larger = fewer,
    /// longer straits. Smaller = more intricate channel network. Typical:
    /// 500-1500 cells.
    pub sea_channel_wavelength: f32,

    /// Cut isthmuses by connecting nearby seas. A land cell is reclassified
    /// as sea if there exists sea within this many cells on both sides
    /// (cardinal: left+right OR top+bottom), meaning it's on a neck between
    /// two separate sea regions. Larger values cut wider isthmuses. 0 = off.
    /// At 8m/cell, 80 ≈ 640m wide isthmus gets cut.
    pub max_isthmus_width_cells: u32,

    /// Number of continent "seed" points to scatter before region growth.
    /// More seeds → more potential landmasses, more mergers during growth.
    /// Typical 6-12.
    pub continent_seed_count: u32,

    /// Minimum spacing between continent seeds in global cells (Poisson-disk
    /// rejection distance). Too small = seeds cluster; too large = placement
    /// may fail for high seed counts.
    pub continent_seed_min_distance_cells: u32,

    /// After Eden growth, keep only the `N` largest merged landmasses; the
    /// rest are converted to sea. This enforces the final continent count.
    /// Typical 2-4.
    pub target_continent_count: u32,

    /// Minimum sea gap (in global cells) enforced between different
    /// continents. Seeds are clustered into `target_continent_count` groups
    /// via k-means; cells near the boundary between two *different* groups
    /// are forced to sea so continents can never merge. 0 = no forced gap
    /// (continents may merge if their territories touch).
    pub continent_gap_cells: u32,

    /// Number of additional small islands to scatter in open sea after the
    /// main continents are placed. Islands are noisy circles placed far
    /// enough from existing land that they look like independent specks
    /// on the map. 0 = none.
    pub small_island_count: u32,

    /// Mean radius of a small island in global cells. Each island's actual
    /// radius is randomized around this (roughly 0.5× to 1.5×).
    pub small_island_radius_cells: u32,

    /// Minimum clearance (in global cells) between a small island and any
    /// existing land or already-placed island. Prevents islands from
    /// visually attaching to continents.
    pub small_island_min_clearance_cells: u32,

    // --- Phase 2: elevation ------------------------------------------------
    /// Maximum elevation in meters. Caps land cells; also acts as the
    /// physical scale for the dandrino erosion sim (terrain is internally
    /// normalized to [0, 1] = [0, max_elevation_m]).
    pub max_elevation_m: f32,

    /// Mean elevation in meters of the pre-erosion FBM heightmap on land.
    /// Erosion will redistribute mass downhill from this baseline.
    pub base_elevation_m: f32,

    /// Initial relief amplitude on land before erosion, as a fraction of
    /// `max_elevation_m`. The pre-erosion heightmap is `base + amp · fbm`
    /// where fbm ∈ [-1, 1]. Higher amplitude = more dramatic mountains
    /// after erosion; lower = gentler hills.
    pub initial_relief_amp: f32,

    /// Wavelength (global cells) of the pre-erosion FBM. Sets the spacing
    /// of macro mountain ranges.
    pub initial_relief_wavelength_cells: f32,

    /// Number of fBm octaves for the pre-erosion heightmap.
    pub initial_relief_octaves: u32,

    /// fBm gain (persistence) for the pre-erosion heightmap. ~0.5 gives
    /// classic red-noise mountain ranges.
    pub initial_relief_gain: f32,

    // --- Phase 3: hydraulic erosion (dandrino simulation) ----------------
    // Faithful port of https://github.com/dandrino/terrain-erosion-3-ways
    // simulation.py. Terrain is internally normalized to [0, 1] (= [0,
    // max_elevation_m]) before the sim runs; constants below are in those
    // unit-less terms, identical to dandrino's defaults.
    /// Resolution at which the erosion sim runs (cells per side). The
    /// pipeline's `global_res` is downsampled to this, the sim runs, and
    /// the result is upsampled back. 0 = use `global_res` directly. Typical:
    /// 1024 (~16× faster than 4096 with no visible loss of macro shape).
    pub erosion_sim_res: u32,

    /// Number of sim iterations. 0 = auto = `ceil(1.4 · sim_res)`, matching
    /// dandrino's default scaling so changes on one side of the grid have
    /// time to reach the other.
    pub erosion_iterations: u32,

    /// Sim-space size of one cell. dandrino uses `200 / 512 ≈ 0.39`. Larger
    /// cell = gentler slope (slope = Δh / cell_width); smaller = steeper.
    /// Couples to `erosion_repose_slope` and the capacity formula.
    pub erosion_cell_width: f32,

    /// Per-iteration mean rainfall, multiplied by `cell_area` inside the
    /// sim. dandrino: 0.0008.
    pub erosion_rain_rate: f32,

    /// Fraction of water removed each iteration. dandrino: 0.0005.
    pub erosion_evaporation_rate: f32,

    /// Floor for `height_delta / cell_width` in the capacity formula so
    /// near-flat cells still carry some sediment. dandrino: 0.05.
    pub erosion_min_height_delta: f32,

    /// Cells whose slope (`|gradient| / cell_width`) exceeds this get
    /// blurred toward neighbors — a loose angle-of-repose enforcement.
    /// dandrino: 0.03.
    pub erosion_repose_slope: f32,

    /// Multiplier from `height_delta / cell_width` to per-cell velocity
    /// (used in the next iteration's capacity calc). dandrino: 30.0.
    pub erosion_gravity: f32,

    /// Sediment-capacity scaling constant. Larger = water carries more
    /// sediment → deeper carving. dandrino: 50.0.
    pub erosion_sediment_capacity: f32,

    /// Fraction of capacity-deficit dissolved into sediment per iteration
    /// (i.e. erosion rate, when below capacity). dandrino: 0.25.
    pub erosion_dissolving_rate: f32,

    /// Fraction of excess sediment deposited per iteration when above
    /// capacity. dandrino: 0.001.
    pub erosion_deposition_rate: f32,

    // --- Phase 3b: post-erosion shaping ----------------------------------
    // The dandrino sim is faithful to its reference and produces a clean
    // mountain/valley silhouette, but two things are gameplay-driven and
    // need explicit fixup after the sim runs:
    //   1. The pre-erosion FBM puts every land cell at `base ± relief`, so
    //      coast cells start at whatever happens to fall on the land/sea
    //      boundary — typically 5–20 m. Erosion does not pull them all the
    //      way to zero, so coastlines read as small cliffs by default.
    //   2. The peak-preserving renormalization at the end of the sim keeps
    //      mountains tall but also keeps lowlands stacked near the
    //      pre-erosion `base_h`, leaving very little area below ~350 m.
    /// Peak-preserving power applied to post-erosion land elevation:
    /// `h' = peak · (h/peak)^p`. `p > 1` compresses lowlands toward 0,
    /// expanding the visual area of plains while leaving mountain peaks at
    /// their original height. 1.0 = no remap.
    pub lowland_expansion_power: f32,

    /// Pivot (m) for the piecewise deep-plain compression that runs after
    /// the global `lowland_expansion_power` curve. Cells already below
    /// this elevation are squashed further toward 0 via
    /// `h' = pivot · (h/pivot)^plain_band_power`; cells at or above the
    /// pivot are left unchanged. Targets the very-low band specifically
    /// (where players walk) without flattening mid-mountains. 0 disables.
    pub plain_band_pivot_m: f32,

    /// Power for the piecewise deep-plain compression. `q > 1` pulls the
    /// sub-pivot range down toward 0, expanding the visual area of cells
    /// at the lowest elevations (the playable plains). 1.0 = no remap.
    pub plain_band_power: f32,

    /// Maximum elevation (m) at the immediate shoreline after Phase 3b's
    /// coast ramp. Land cells within `coast_beach_distance_cells` of the
    /// sea are pulled down toward this height via a smoothstep ramp.
    /// 0 disables the ramp; tiny values (e.g. 0.1 m) give natural
    /// beach-to-water transitions instead of an 8 m drop at every cell.
    pub coast_beach_max_height_m: f32,

    /// Hard floor on land elevation (m). Land cells below this floor are
    /// raised to it as the last step of Phase 3b. Must clear the river
    /// water surface so banks occlude the river mesh; raise it if low
    /// coastal land lets the river bleed across whole tiles. 0 disables.
    pub min_land_height_m: f32,

    /// Peak-to-peak amplitude (m) of the noise modulation added on top of
    /// `min_land_height_m`, so the floor reads as gentle bumps instead
    /// of a flat plateau. 0 disables noise.
    pub min_land_height_noise_amp_m: f32,

    /// Wavelength (reference cells) of the floor-modulation noise.
    pub min_land_height_noise_wavelength_cells: f32,

    /// Width of the coast ramp in reference cells. The ramp blends
    /// linearly from `coast_beach_max_height_m` at d=0 to the natural
    /// post-erosion elevation at d=N. Wider = gentler, more sand;
    /// narrower = steeper transition right at the water's edge.
    pub coast_beach_distance_cells: u32,

    /// Fraction (0..1) of the coastline that should remain as cliffs at
    /// the natural post-erosion elevation, exempted from the beach ramp.
    /// Implemented via a slow noise pattern: cells where the noise sits
    /// in the top `coast_cliff_fraction` of its range keep their height.
    /// 0 = ramp the whole coast down (no cliffs); 1 = no ramp anywhere.
    pub coast_cliff_fraction: f32,

    /// Wavelength of the cliff/beach pattern in reference cells. Sets the
    /// typical along-coast length of a single cliff section or beach
    /// section before the pattern flips. Larger = chunkier pattern.
    pub coast_cliff_wavelength_cells: f32,

    /// Peak height (m) of automatic hills stamped on small land
    /// components after erosion. Erosion grinds small islands —
    /// every cell drains directly to sea — almost flat, so they
    /// read as featureless beige discs in the hypso preview. This
    /// pass detects each small component, places a hotspot at its
    /// interior centroid with radius proportional to its
    /// distance-to-coast, and bumps the elevation so the island
    /// reads as an island. 0 disables the pass.
    pub small_island_hill_peak_m: f32,

    /// Component-size cutoff (in reference cells, area) for the
    /// small-island hill pass. Land components with `≤ N` cells are
    /// treated as small islands and receive a hill; larger
    /// components already have erosion-carved relief and are
    /// skipped. 0 disables.
    pub small_island_hill_max_cells: u32,

    /// Hotspot radius as a fraction of the island's max
    /// distance-to-coast (the island's effective "inner radius").
    /// Below 1.0 keeps the hill short of the shoreline so the
    /// beach ramp can still pull the immediate coast down to
    /// `coast_beach_max_height_m`.
    pub small_island_hill_radius_frac: f32,

    /// Peak-elevation threshold (m) for the small-island river pass.
    /// Cells must clear this to qualify as a stream source on a
    /// small island. Must be well below `small_island_hill_peak_m`
    /// so the artificially stamped crowns reliably produce sources;
    /// 0 disables the pass.
    pub small_island_river_min_peak_m: f32,

    /// Minimum polyline length (in cells) for an island river to be
    /// kept. The mainland threshold (≈20) discards every plausible
    /// island stream — small islands are only tens of cells across,
    /// so this knob is independent. 0 disables the pass.
    pub small_island_river_min_length: u32,

    /// Minimum spacing (in cells, X-wrap aware) between two peaks
    /// kept as island-river sources. Tight spacing yields multiple
    /// streams per island; wider spacing keeps it to one stem per
    /// crown.
    pub small_island_river_peak_spacing_cells: u32,

    // --- Phase 5: settlements ---------------------------------------------
    /// Target number of settlements to place across the world. Greedy
    /// min-spacing selection may end up with fewer if candidates run out.
    pub settlement_target_count: u32,

    /// Minimum spacing (in global cells, X-wrapped) between any two
    /// settlements. Controls density; at 8m/cell, 80 ≈ 640m.
    pub settlement_min_spacing_cells: u32,

    /// Settlements are rejected on land above this elevation (meters).
    /// Keeps cities out of alpine peaks.
    pub settlement_max_elevation_m: f32,

    /// Settlements are rejected on land steeper than this normalized
    /// gradient (rise over run, meters per meter). 0.3 ≈ 17°, 0.5 ≈ 27°.
    pub settlement_max_slope: f32,

    /// Flow-accumulation threshold above which a cell is considered on a
    /// river, granting a score bonus that biases placement toward riverbanks.
    pub settlement_river_flow_threshold: f32,

    /// Additional settlements (villages) seeded along the road network in
    /// Phase 6, on top of the initial `settlement_target_count` cities. The
    /// roads themselves come from Prim MST + A* over the initial cities.
    pub settlement_along_road_count: u32,

    /// Minimum distance from coast (in cells) for Phase A city placement.
    /// Without this, per-river selection always lands at the river mouth
    /// because coast+river+lowland+flat all peak together at the mouth —
    /// pushing the inland buffer past the coastal band forces cities into
    /// the middle reaches of each river. 0 = disabled.
    pub settlement_inland_buffer_cells: u32,

    /// Multiplier applied to `settlement_min_spacing_cells` when at least
    /// one of the two candidates is within `settlement_inland_buffer_cells`
    /// of the coast. Pushes coastal settlements further apart so they don't
    /// line up in a regular fence along the shore.
    pub settlement_coastal_spacing_mult: f32,

    /// Number of rivers (top by mouth flow) whose Phase-A pick is placed at
    /// the coastal mouth instead of the inland middle reach. Default is set
    /// well above the typical river count so Phase A defaults to mouth-first
    /// for *every* river (with a fallback to the inland middle reach when no
    /// habitable cell exists near the mouth) — matches the real-world
    /// pattern where almost all major settlements grew at river mouths.
    pub settlement_mouth_count: u32,

    /// Spacing multiplier applied to Phase-A river picks. Without an inflated
    /// spacing, sibling rivers in one wide valley plain land their middle-
    /// reach picks within ~100 cells of each other (each picks the highest-
    /// score cell in its own drainage, but those cells line up along the
    /// same foothill contour). Multiplying spacing here forces Phase-A picks
    /// onto distinct valleys; Phase B / Phase C use the unmultiplied spacing
    /// so islands and along-road villages aren't starved.
    pub settlement_phase_a_spacing_mult: f32,

    /// Maximum allowable distance (meters) from any habitable cell to its
    /// nearest settlement. After Phases A-C run, a coverage-fill pass adds
    /// settlements at the most isolated habitable cells until every cell
    /// is within this distance of one. Bypasses min-spacing so the gap
    /// guarantee holds even in regions the fitness placer skipped.
    /// 0 disables the pass. Typical: ~4000 m so no inhabitable area is
    /// more than a 4 km hike from a town.
    pub settlement_max_gap_m: f32,

    /// Maximum allowable distance (meters) from a lowland land cell to the
    /// nearest river. After Phase 4's first river extraction, a gap-fill
    /// pass drops a low mountain (`ElevationHotspot`) at the most isolated
    /// lowland cell, then re-runs flow accumulation + river extraction so
    /// the seeded peak births a fresh river. Iterates until every lowland
    /// cell is within this distance of either a real or hotspot-anchored
    /// river. 0 disables the pass. Default: 3500 m — at 4000 m, regions
    /// like (r+12, -7) on seed 42 had every cell barely-but-not-quite a
    /// gap (max ~3920 m to a river), so visually riverless plains escaped
    /// the gap-fill; tightening to 3500 m catches them.
    pub river_gap_max_m: f32,

    // --- Phase 6: roads ---------------------------------------------------
    /// K in the K-nearest-neighbor graph added on top of the MST when
    /// computing the road network. 0 = MST only; higher = denser graph
    /// with more cross-links → more hubs and inland routes. Typical: 2-3.
    pub road_extra_neighbors: u32,

    /// Region-targeted mountain hotspots applied after baseline elevation,
    /// before erosion. Use these to guarantee that a specific part of the
    /// world hosts mountains/rivers/cities even when the seed's noise
    /// otherwise leaves it as featureless plain. Empty = pure procedural.
    #[serde(default)]
    pub elevation_hotspots: Vec<ElevationHotspot>,

    /// Polyline channels carved into the elevation after hotspots. Used to
    /// engineer river courses across plains where the noise's drainage
    /// divides would otherwise strand flow. Cells along each path are
    /// clamped (via `min`) to a linearly-interpolated target elevation,
    /// producing a guaranteed downhill valley.
    #[serde(default)]
    pub river_carve_paths: Vec<RiverCarvePath>,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            seed: 7,
            world_size_m: 32768,
            global_res: 4096,
            reference_res: 4096,
            sea_ratio: 0.30,
            continent_frequency: 1.0 / 700.0,
            continent_octaves: 4,
            continent_gain: 0.5,
            min_island_cells: 400,
            min_strait_width_cells: 10,
            sea_channel_strength: 0.0,
            sea_channel_wavelength: 1000.0,
            max_isthmus_width_cells: 0,
            continent_seed_count: 20,
            continent_seed_min_distance_cells: 450,
            target_continent_count: 3,
            continent_gap_cells: 120,
            small_island_count: 15,
            small_island_radius_cells: 90,
            small_island_min_clearance_cells: 150,
            max_elevation_m: 2500.0,
            base_elevation_m: 1000.0,
            initial_relief_amp: 0.4,
            initial_relief_wavelength_cells: 700.0,
            initial_relief_octaves: 6,
            initial_relief_gain: 0.5,
            // Erosion: dandrino simulation.py defaults. sim_res 1024 keeps
            // a single preview under ~1 min on a typical workstation while
            // matching dandrino's visual style.
            erosion_sim_res: 1024,
            erosion_iterations: 0,
            erosion_cell_width: 200.0 / 512.0,
            erosion_rain_rate: 0.0008,
            erosion_evaporation_rate: 0.0005,
            erosion_min_height_delta: 0.05,
            erosion_repose_slope: 0.03,
            erosion_gravity: 30.0,
            erosion_sediment_capacity: 50.0,
            erosion_dissolving_rate: 0.25,
            erosion_deposition_rate: 0.001,
            // Phase 3b post-erosion shaping. 1.4 roughly doubles the area
            // below `0.27·max_elev` (~675 m at 2500 m cap), which puts
            // most of the lowland green band well below 350 m. Beach ramp
            // takes the natural ~5–20 m coast height down to 0.1 m over
            // 12 cells (~96 m at 8 m/cell), with 20% of the coastline
            // exempted as cliffs via the slow cliff-noise pattern.
            lowland_expansion_power: 1.4,
            // Deep-plain compression: pivot 1000 m, power 10.0 collapses
            // sub-pivot elevations hard enough that "below 30 m" — the
            // band that actually reads as flat plain — expands ~1.6×.
            // 700–1000 m foothills are heavily flattened so the playable
            // ground reads as plain rather than sloped hill; mountains
            // above 1000 m are left untouched.
            plain_band_pivot_m: 1200.0,
            plain_band_power: 15.0,
            coast_beach_max_height_m: 0.1,
            coast_beach_distance_cells: 12,
            // 1 m floor with ±25 cm bumpiness keeps coastal land safely
            // above the river water surface (0.5 m) so the river mesh's
            // depth-fade terminates at the natural bank instead of
            // flooding across the whole tile.
            min_land_height_m: 1.0,
            min_land_height_noise_amp_m: 0.5,
            min_land_height_noise_wavelength_cells: 6.0,
            coast_cliff_fraction: 0.2,
            coast_cliff_wavelength_cells: 80.0,
            // Small-island hill pass: stamp a ~200 m bump on every
            // land component up to ~80k cells (≈ a 160-cell-radius
            // disc at 8 m/cell). The cutoff has to clear medium islands
            // that sit in a dead zone: too big for this pass but too
            // small/flat to grow a natural ≥750 m peak, so they get no
            // river and the river-gap pass also skips them (every cell
            // is unreachable from a mainland river, and the whole island
            // is within the 2 km coast buffer). On seed 42 the only such
            // island was 46k cells; 80k catches it (and similar mediums)
            // while staying well under the next natural-river island
            // (~193k cells), which keeps a dome off terrain that already
            // carves its own relief. 0.85× radius keeps the hill crown
            // short of the beach so the coast ramp still wins at the
            // shoreline.
            small_island_hill_peak_m: 200.0,
            small_island_hill_max_cells: 80_000,
            small_island_hill_radius_frac: 0.85,
            // Island rivers: 80 m peak threshold sits comfortably
            // below the 200 m hill crown so seeded peaks always
            // qualify. 5-cell minimum length (~40 m at 8 m/cell)
            // catches the shortest plausible streams without
            // littering single-cell trickles. 10-cell spacing
            // keeps each island to one trunk where possible.
            small_island_river_min_peak_m: 80.0,
            small_island_river_min_length: 5,
            small_island_river_peak_spacing_cells: 10,
            settlement_target_count: 60,
            settlement_min_spacing_cells: 70,
            settlement_max_elevation_m: 1200.0,
            settlement_max_slope: 0.35,
            settlement_river_flow_threshold: 100.0,
            settlement_along_road_count: 40,
            settlement_inland_buffer_cells: 80,
            settlement_coastal_spacing_mult: 1.6,
            settlement_mouth_count: 500,
            settlement_phase_a_spacing_mult: 2.0,
            settlement_max_gap_m: 4000.0,
            river_gap_max_m: 3500.0,
            road_extra_neighbors: 5,
            elevation_hotspots: vec![],
            river_carve_paths: vec![],
        }
    }
}

impl WorldGenConfig {
    /// Meters per global cell.
    pub fn meters_per_cell(&self) -> f32 {
        self.world_size_m as f32 / self.global_res as f32
    }

    /// World meters → fractional global-cell coordinate. World origin sits
    /// at cell (`global_res / 2`, `global_res / 2`).
    pub fn world_m_to_cell(&self, x_m: f32, y_m: f32) -> (f32, f32) {
        let mpc = self.meters_per_cell();
        let origin = self.global_res as f32 * 0.5;
        (x_m / mpc + origin, y_m / mpc + origin)
    }

    /// Total number of global-map cells.
    pub fn cell_count(&self) -> usize {
        (self.global_res as usize) * (self.global_res as usize)
    }

    /// `global_res / reference_res`. Equals 1.0 when running at the
    /// reference resolution.
    pub fn res_scale(&self) -> f32 {
        self.global_res as f32 / self.reference_res.max(1) as f32
    }

    /// Convert a frequency declared in cycles-per-reference-cell to the
    /// per-actual-cell frequency consumed by `fbm_wrap_xy` etc.
    pub fn scaled_freq(&self, ref_freq: f32) -> f32 {
        ref_freq / self.res_scale()
    }

    /// Convert a linear length from reference cells to actual cells.
    pub fn scaled_cells(&self, ref_cells: f32) -> f32 {
        ref_cells * self.res_scale()
    }

    /// Same as `scaled_cells` but returns a rounded, non-negative `usize`
    /// for use as an index or loop bound. Always ≥ 0 even if the input
    /// rounds below zero from an unsigned domain.
    pub fn scaled_cells_usize(&self, ref_cells: u32) -> usize {
        (ref_cells as f32 * self.res_scale()).round().max(0.0) as usize
    }

    /// Convert a 2D-area cell count from reference cells to actual cells.
    /// Rounds up to 1 so tiny reference counts don't degenerate to zero.
    pub fn scaled_area_cells(&self, ref_area: u32) -> u32 {
        let s = self.res_scale();
        ((ref_area as f32) * s * s).round().max(1.0) as u32
    }
}
