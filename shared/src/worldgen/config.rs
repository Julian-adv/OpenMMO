use serde::{Deserialize, Serialize};

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
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            seed: 0xC0FFEE,
            world_size_m: 32768,
            global_res: 4096,
            sea_ratio: 0.50,
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
            small_island_count: 5,
            small_island_radius_cells: 50,
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
        }
    }
}

impl WorldGenConfig {
    /// Meters per global cell.
    pub fn meters_per_cell(&self) -> f32 {
        self.world_size_m as f32 / self.global_res as f32
    }

    /// Total number of global-map cells.
    pub fn cell_count(&self) -> usize {
        (self.global_res as usize) * (self.global_res as usize)
    }
}
