use serde::{Deserialize, Serialize};

use super::config::WorldGenConfig;

/// The low-resolution global map. Built up phase by phase; each field is
/// populated by a specific generation stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMap {
    pub config: WorldGenConfig,

    /// Continent potential field from Phase 1: raw fBm output in roughly
    /// [-1, 1] after edge falloff. Higher = more "land-like". Used both to
    /// build the land mask and as a seed for Phase 2 elevation layering.
    /// Length = `config.cell_count()`.
    pub continent_potential: Vec<f32>,

    /// Boolean land mask from Phase 1, packed as 0 (sea) / 1 (land).
    /// Length = `config.cell_count()`.
    pub land_mask: Vec<u8>,

    /// Quantile threshold on `continent_potential` used to separate sea from
    /// land — below this value is sea. Recorded for downstream phases that
    /// need to treat the continental shelf as a soft boundary.
    pub sea_level_potential: f32,

    /// Per-cell elevation in meters. Populated by Phase 2 (`elevation.rs`).
    /// Sea cells are 0.0; land cells range from ~0 at the coast up to the
    /// configured `max_elevation_m`. Length = `config.cell_count()`.
    pub elevation_m: Vec<f32>,
}

impl GlobalMap {
    /// Index of cell (x, y) in `continent_potential` / `land_mask`.
    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.config.global_res && y < self.config.global_res);
        (y as usize) * (self.config.global_res as usize) + (x as usize)
    }

    /// Fraction of the world that is sea (0..1). Useful to verify generation
    /// hit the target ratio.
    pub fn measured_sea_ratio(&self) -> f32 {
        let sea_cells = self.land_mask.iter().filter(|&&b| b == 0).count();
        sea_cells as f32 / self.land_mask.len() as f32
    }
}
