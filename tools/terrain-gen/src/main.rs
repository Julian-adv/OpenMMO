//! Offline procedural world generator.
//!
//! Iterative workflow:
//!   1. `terrain-gen preview --seed N`  — dump PNGs for visual inspection
//!   2. tweak seed / config, repeat until satisfied
//!   3. `terrain-gen bake --seed N --out <dir>` — write per-tile height &
//!      splat files matching the runtime `TerrainIO` format.
//!
//! See `doc/TERRAIN_GENERATION.md` for the full design.

mod bake;
mod preview;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use onlinerpg_shared::worldgen::WorldGenConfig;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "terrain-gen", version, about = "Procedural terrain generator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// CLI parameters that map onto `WorldGenConfig`. Shared by `preview` and
/// `bake` so tuning established at preview time reproduces exactly during a
/// bake of the same seed.
#[derive(Args, Clone)]
struct GenArgs {
    /// Master seed.
    #[arg(long, default_value_t = 7)]
    seed: u64,

    /// Global map resolution (cells per side). 4096 is the design default.
    /// Lower values (e.g. 512, 1024) iterate faster during tuning.
    #[arg(long, default_value_t = 4096)]
    res: u32,

    /// Target sea fraction (0..1).
    #[arg(long, default_value_t = 0.50)]
    sea: f32,

    /// Continent wavelength in global cells.
    #[arg(long, default_value_t = 700)]
    wavelength: u32,

    /// fBm octaves for continent shape.
    #[arg(long, default_value_t = 4)]
    octaves: u32,

    /// fBm gain (persistence) for continent shape.
    #[arg(long, default_value_t = 0.5)]
    gain: f32,

    /// Minimum land-component size, in global cells.
    #[arg(long, default_value_t = 400)]
    min_islands: u32,

    /// Minimum land-bridge width in global cells.
    #[arg(long, default_value_t = 10)]
    min_strait: u32,

    /// Sea-channel ridge noise strength.
    #[arg(long, default_value_t = 0.0)]
    channel_strength: f32,

    /// Sea-channel wavelength in global cells.
    #[arg(long, default_value_t = 1000.0)]
    channel_wavelength: f32,

    /// Isthmus-cut width in global cells.
    #[arg(long, default_value_t = 0)]
    max_isthmus: u32,

    /// Number of initial continent seed points.
    #[arg(long, default_value_t = 20)]
    seeds: u32,

    /// Minimum spacing between continent seeds in global cells.
    #[arg(long, default_value_t = 450)]
    seed_distance: u32,

    /// Target number of final continents.
    #[arg(long, default_value_t = 3)]
    continents: u32,

    /// Minimum sea gap between continents in global cells.
    #[arg(long, default_value_t = 120)]
    gap: u32,

    /// Number of small scattered islands in open sea.
    #[arg(long, default_value_t = 15)]
    islands: u32,

    /// Mean radius of each small island in global cells.
    #[arg(long, default_value_t = 90)]
    island_radius: u32,

    /// Minimum clearance in global cells between small islands and other land.
    #[arg(long, default_value_t = 150)]
    island_clearance: u32,

    /// Hydraulic-erosion droplet count. 0 disables.
    #[arg(long, default_value_t = 300_000)]
    droplets: u32,

    /// Target city count for Phase 5a.
    #[arg(long, default_value_t = 60)]
    settlements: u32,

    /// Minimum spacing between settlements in global cells.
    #[arg(long, default_value_t = 70)]
    settlement_spacing: u32,
}

impl GenArgs {
    fn into_config(self) -> WorldGenConfig {
        WorldGenConfig {
            seed: self.seed,
            global_res: self.res,
            sea_ratio: self.sea,
            continent_frequency: 1.0 / (self.wavelength.max(1) as f32),
            continent_octaves: self.octaves.max(1),
            continent_gain: self.gain,
            min_island_cells: self.min_islands,
            min_strait_width_cells: self.min_strait,
            sea_channel_strength: self.channel_strength,
            sea_channel_wavelength: self.channel_wavelength.max(1.0),
            max_isthmus_width_cells: self.max_isthmus,
            continent_seed_count: self.seeds.max(1),
            continent_seed_min_distance_cells: self.seed_distance,
            target_continent_count: self.continents.max(1),
            continent_gap_cells: self.gap,
            small_island_count: self.islands,
            small_island_radius_cells: self.island_radius,
            small_island_min_clearance_cells: self.island_clearance,
            erosion_droplet_count: self.droplets,
            settlement_target_count: self.settlements,
            settlement_min_spacing_cells: self.settlement_spacing.max(1),
            ..WorldGenConfig::default()
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate the low-res global map and dump PNGs for visual inspection.
    Preview {
        #[command(flatten)]
        gen: GenArgs,

        /// Output directory. A sub-folder named after the seed is created inside.
        #[arg(long, default_value = "preview_out")]
        out: PathBuf,
    },

    /// Bake per-tile heightmap + splatmap files into a `data/terrain/`-shaped
    /// layout. Baking the full world is expensive (~6 GB), so the default
    /// region range is an 8×8 square around the origin. Expand explicitly
    /// when ready to write the whole world.
    Bake {
        #[command(flatten)]
        gen: GenArgs,

        /// Output directory (written directly: `out/height/…`, `out/splat/…`,
        /// `out/meta/…`, `out/worldgen.json`). No seed subfolder so the
        /// layout matches what the runtime `TerrainIO` expects.
        #[arg(long)]
        out: PathBuf,

        /// Minimum region X (inclusive). Region = 16 tiles = 1024 m at 64 m/tile.
        #[arg(long, default_value_t = -4, allow_hyphen_values = true)]
        region_x_min: i32,

        /// Maximum region X (inclusive).
        #[arg(long, default_value_t = 3, allow_hyphen_values = true)]
        region_x_max: i32,

        /// Minimum region Z (inclusive).
        #[arg(long, default_value_t = -4, allow_hyphen_values = true)]
        region_z_min: i32,

        /// Maximum region Z (inclusive).
        #[arg(long, default_value_t = 3, allow_hyphen_values = true)]
        region_z_max: i32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Preview { gen, out } => {
            let cfg = gen.into_config();
            preview::run(&cfg, &out)
        }
        Cmd::Bake {
            gen,
            out,
            region_x_min,
            region_x_max,
            region_z_min,
            region_z_max,
        } => {
            if region_x_max < region_x_min || region_z_max < region_z_min {
                anyhow::bail!(
                    "invalid region range: x[{},{}] z[{},{}]",
                    region_x_min,
                    region_x_max,
                    region_z_min,
                    region_z_max,
                );
            }
            let cfg = gen.into_config();
            bake::run(
                &cfg,
                &out,
                (region_x_min, region_z_min),
                (region_x_max, region_z_max),
            )
        }
    }
}
