//! Procedural world generation.
//!
//! Two-tier pipeline: a low-resolution global map captures continent shape,
//! biomes, rivers, settlements, and roads for the entire world (e.g. 4096×4096
//! covering a 32768m × 32768m world, so 8m per global cell). High-resolution
//! per-tile detail is generated on demand by sampling the global map and
//! adding fine-scale noise.
//!
//! Phases are built up incrementally: Phase 1 covers the continent/sea mask.

pub mod config;
pub mod continent;
pub mod elevation;
pub mod erosion;
pub mod global_map;
pub(crate) mod grid;
pub mod growth;
pub mod noise;
pub mod rivers;

pub use config::WorldGenConfig;
pub use global_map::GlobalMap;
