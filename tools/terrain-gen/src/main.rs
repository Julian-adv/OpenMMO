//! Offline procedural world generator.
//!
//! Iterative workflow:
//!   1. `terrain-gen preview --seed N`  — dump PNGs for visual inspection
//!   2. tweak seed / config, repeat until satisfied
//!
//! See `doc/TERRAIN_GENERATION.md` for the full design.

mod preview;

use anyhow::Result;
use clap::{Parser, Subcommand};
use onlinerpg_shared::worldgen::WorldGenConfig;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "terrain-gen", version, about = "Procedural terrain generator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate the low-res global map and dump PNGs for visual inspection.
    Preview {
        /// Master seed.
        #[arg(long, default_value_t = 0xC0FFEE)]
        seed: u64,

        /// Global map resolution (cells per side). 4096 is the design default.
        /// Lower values (e.g. 512, 1024) iterate faster during tuning.
        #[arg(long, default_value_t = 4096)]
        res: u32,

        /// Target sea fraction (0..1). 0.50 gives 2-3 distinct continents;
        /// 0.30 gives a single large landmass with internal bays.
        #[arg(long, default_value_t = 0.50)]
        sea: f32,

        /// Continent wavelength in global cells. Controls continent size:
        /// larger = fewer/bigger continents. Good values: 512 (many small),
        /// 700-900 (2-3 continents), 2048+ (single supercontinent).
        #[arg(long, default_value_t = 700)]
        wavelength: u32,

        /// fBm octaves for continent shape. More = more jagged coastlines,
        /// fewer = smoother. Range 1-6.
        #[arg(long, default_value_t = 4)]
        octaves: u32,

        /// fBm gain (persistence) for continent shape. 0.3 = very smooth,
        /// 0.65 = very rough.
        #[arg(long, default_value_t = 0.5)]
        gain: f32,

        /// Minimum land-component size, in global cells. Components smaller
        /// than this are dropped (removing tiny islands). 0 = disabled.
        #[arg(long, default_value_t = 400)]
        min_islands: u32,

        /// Minimum land-bridge width in global cells. Narrower isthmuses get
        /// cut (morphological opening), breaking single continents into
        /// archipelagos. 0 = disabled. At 8m/cell, 10 ≈ 80m strait.
        #[arg(long, default_value_t = 10)]
        min_strait: u32,

        /// Sea-channel ridge noise strength (experimental, off by default:
        /// produces spiky artifacts). 0 = off.
        #[arg(long, default_value_t = 0.0)]
        channel_strength: f32,

        /// Sea-channel wavelength (global cells). Larger = fewer/longer
        /// straits; smaller = more intricate network. Typical 500-1500.
        #[arg(long, default_value_t = 1000.0)]
        channel_wavelength: f32,

        /// Isthmus-cut width in global cells. Off by default since the
        /// growth-based mask rarely needs isthmus cuts.
        #[arg(long, default_value_t = 0)]
        max_isthmus: u32,

        /// Number of initial continent seed points (more = more mergers).
        #[arg(long, default_value_t = 20)]
        seeds: u32,

        /// Minimum spacing between continent seeds in global cells.
        #[arg(long, default_value_t = 450)]
        seed_distance: u32,

        /// Target number of final continents (top-N largest kept; rest → sea).
        #[arg(long, default_value_t = 3)]
        continents: u32,

        /// Minimum sea gap in global cells between different continents.
        /// Seeds are clustered into `continents` groups; cells near a
        /// group-boundary are forced to sea so continents never merge.
        /// At 8m/cell, 120 ≈ 960m channel.
        #[arg(long, default_value_t = 120)]
        gap: u32,

        /// Number of small scattered islands to add in open sea after the
        /// main continents are placed.
        #[arg(long, default_value_t = 5)]
        islands: u32,

        /// Hydraulic-erosion droplet count (Phase 3). 0 = disabled.
        /// Typical 200k-500k at 4096² res for visible gullies.
        #[arg(long, default_value_t = 300_000)]
        droplets: u32,

        /// Mean radius of each small island in global cells. Actual radius
        /// is randomized 0.5× to 1.5× of this.
        #[arg(long, default_value_t = 50)]
        island_radius: u32,

        /// Minimum clearance in global cells between a small island and any
        /// continent (or another island). 150 ≈ 1200m at 8m/cell.
        #[arg(long, default_value_t = 150)]
        island_clearance: u32,

        /// Output directory. A sub-folder named after the seed is created inside.
        #[arg(long, default_value = "preview_out")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Preview {
            seed,
            res,
            sea,
            wavelength,
            octaves,
            gain,
            min_islands,
            min_strait,
            channel_strength,
            channel_wavelength,
            max_isthmus,
            seeds,
            seed_distance,
            continents,
            gap,
            islands,
            island_radius,
            island_clearance,
            droplets,
            out,
        } => {
            let cfg = WorldGenConfig {
                seed,
                global_res: res,
                sea_ratio: sea,
                continent_frequency: 1.0 / (wavelength.max(1) as f32),
                continent_octaves: octaves.max(1),
                continent_gain: gain,
                min_island_cells: min_islands,
                min_strait_width_cells: min_strait,
                sea_channel_strength: channel_strength,
                sea_channel_wavelength: channel_wavelength.max(1.0),
                max_isthmus_width_cells: max_isthmus,
                continent_seed_count: seeds.max(1),
                continent_seed_min_distance_cells: seed_distance,
                target_continent_count: continents.max(1),
                continent_gap_cells: gap,
                small_island_count: islands,
                small_island_radius_cells: island_radius,
                small_island_min_clearance_cells: island_clearance,
                erosion_droplet_count: droplets,
                ..WorldGenConfig::default()
            };
            preview::run(&cfg, &out)
        }
    }
}
