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

    /// Of the land, target fraction that becomes mountain terrain (0..1).
    /// Remainder is plains/hills.
    pub mountain_ratio: f32,

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
    /// Maximum elevation in meters (used as a cap for land cells and as the
    /// target peak for the north/south mountain wall).
    pub max_elevation_m: f32,

    /// Target height of the smooth base-elevation gradient that rises with
    /// distance from coast. Reached around 40% of the coast-to-interior
    /// distance, then flattens.
    pub base_elevation_m: f32,

    /// Amplitude (± meters) of detail noise inside mountain regions — the
    /// dominant driver of peak/valley height variation there.
    pub mountain_amplitude_m: f32,

    /// Amplitude (± meters) of detail noise in plain regions. Much smaller
    /// than mountain amplitude so plains look flat-to-gently-rolling.
    pub plain_amplitude_m: f32,

    /// Wavelength (global cells) of the mountain/plain selector noise.
    /// Larger = broader mountain ranges and plains.
    pub mountain_selector_wavelength_cells: f32,

    /// Wavelength (global cells) of the high-frequency detail noise that
    /// creates local peaks and valleys on top of the base gradient.
    pub detail_wavelength_cells: f32,

    /// Number of cells of the north/south border where land is boosted
    /// toward `max_elevation_m` to form an impassable mountain wall (since
    /// Y doesn't wrap). 0 = disabled.
    pub y_border_wall_cells: u32,

    /// Peak height of the Y-border mountain wall, in meters. Typically
    /// close to `max_elevation_m` so the wall reliably blocks traversal.
    pub y_border_wall_height_m: f32,

    // --- Phase 3: hydraulic erosion ---------------------------------------
    /// Number of water droplets simulated across the whole map. Scales with
    /// map area; ~200k-500k is reasonable at 4096² res. 0 = erosion off.
    pub erosion_droplet_count: u32,

    /// Max steps a single droplet takes before being discarded.
    pub erosion_max_steps: u32,

    /// Momentum factor (0..1). 0 = droplet follows the gradient exactly;
    /// higher = it overshoots and carves flatter valleys.
    pub erosion_inertia: f32,

    /// Sediment capacity scaling: capacity = slope · speed · water · factor.
    /// Larger = droplets can carry more sediment → more carving.
    pub erosion_capacity_factor: f32,

    /// Minimum effective slope for sediment-capacity calculation, so drops
    /// on near-flat ground can still pick up sediment.
    pub erosion_min_slope: f32,

    /// Erosion rate (0..1): fraction of capacity-deficit that actually erodes
    /// in one step.
    pub erosion_rate: f32,

    /// Deposition rate (0..1): fraction of excess sediment dropped when over
    /// capacity.
    pub erosion_deposition_rate: f32,

    /// Water evaporation per step (0..1).
    pub erosion_evaporation_rate: f32,

    /// Erosion brush radius in cells. Erosion and deposition distribute over
    /// a disk of this radius so gullies are smooth, not single-cell deep.
    pub erosion_radius_cells: u32,

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

    /// Settlements within this distance (meters) of the south map edge are
    /// rejected. The southern strip is dominated by the Y-border wall and
    /// reads as polar terrain; villages clustered there look out of place.
    /// 0 = disabled. North edge is unaffected (the wall ramp on the north
    /// side currently blocks placement on its own).
    pub settlement_south_edge_exclusion_m: f32,

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
    /// river. 0 disables the pass. Typical: ~4000 m so no plain is more
    /// than a 4 km walk from a stream.
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
            mountain_ratio: 0.20,
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
            base_elevation_m: 500.0,
            mountain_amplitude_m: 1800.0,
            plain_amplitude_m: 40.0,
            mountain_selector_wavelength_cells: 900.0,
            detail_wavelength_cells: 80.0,
            y_border_wall_cells: 16,
            y_border_wall_height_m: 2200.0,
            erosion_droplet_count: 300_000,
            erosion_max_steps: 50,
            erosion_inertia: 0.05,
            erosion_capacity_factor: 4.0,
            erosion_min_slope: 0.01,
            erosion_rate: 0.3,
            erosion_deposition_rate: 0.3,
            erosion_evaporation_rate: 0.02,
            erosion_radius_cells: 3,
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
            settlement_south_edge_exclusion_m: 1700.0,
            settlement_max_gap_m: 4000.0,
            river_gap_max_m: 4000.0,
            road_extra_neighbors: 5,
            // No hand-tuned hotspots/carves by default — Phase 5's coverage-
            // gap pass guarantees cities everywhere, so engineering mountains
            // and rivers to attract cities is no longer needed. The struct
            // types remain available for one-off art-direction tuning.
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
    /// per-actual-cell frequency consumed by `fbm_wrap_x` etc.
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
