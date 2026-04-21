//! Warped-Voronoi grass patch field used by Phase 7 splat classification.
//!
//! Two layers are combined per query: a primary layer of large patches that
//! defines the main grass shapes, and a secondary layer of small clumps that
//! scatters filler vegetation into the gaps. Both layers share the same
//! domain warp so their boundaries deform consistently; per-query the
//! stronger patch wins (ties broken to primary).
//!
//! Seeds are derived on demand from `(master_seed, layer_salt, gx, gz)` via
//! `SmallRng` — no seed table. The X axis wraps (`rem_euclid` on the grid
//! index, wrap-aware Euclidean distance); Z does not.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use super::noise::{fbm_wrap_x, smoothstep, PerlinNoise3D};

#[derive(Clone, Copy)]
struct LayerCfg {
    /// Target grid spacing (m). World width is rounded to an integer count
    /// of cells so the X-wrap seam lands on a cell edge.
    target_grid_m: f32,
    radius_mean_m: f32,
    /// `actual_radius = mean * (1 + jitter * R)`, R uniform in [-1, 1].
    radius_jitter: f32,
    /// Max seed displacement from the grid cell center, as a fraction of the
    /// grid half-size. <1.0 keeps seeds inside their own cell with margin.
    jitter_frac: f32,
    /// Probability a grid cell emits a seed. Lower = more bare gaps.
    occupancy: f32,
    /// Smoothstep fade width (m) at the patch edge.
    fade_m: f32,
    /// Probability a seed is the tall-grass variant.
    tall_prob: f32,
    /// XOR'd into the hash so different layers produce independent seed
    /// streams from the same master seed.
    seed_salt: u64,
}

const PRIMARY: LayerCfg = LayerCfg {
    target_grid_m: 50.0,
    radius_mean_m: 22.0,
    radius_jitter: 0.35,
    jitter_frac: 0.85,
    occupancy: 0.72,
    fade_m: 6.0,
    tall_prob: 0.28,
    seed_salt: 0x6B61_7373_0001_0001,
};

const SECONDARY: LayerCfg = LayerCfg {
    target_grid_m: 15.0,
    radius_mean_m: 5.5,
    radius_jitter: 0.35,
    jitter_frac: 0.85,
    occupancy: 0.45,
    fade_m: 2.5,
    tall_prob: 0.0,
    seed_salt: 0x6B61_7373_0001_0003,
};

// Domain-warp parameters. Much weaker than continent warp (500 m / 350 cells)
// because patches are already small — a strong warp dissolves the shape.
const WARP_WAVELENGTH_M: f32 = 70.0;
const WARP_STRENGTH_M: f32 = 10.0;
const WARP_OCTAVES: u32 = 3;
const WARP_FREQ: f32 = 1.0 / WARP_WAVELENGTH_M;

#[derive(Debug, Clone, Copy)]
pub struct PatchSample {
    /// 0 outside any patch, 1 deep inside the claiming patch.
    pub strength: f32,
    /// Variant of the claiming patch. Only meaningful when `strength > 0`.
    pub is_tall: bool,
}

impl PatchSample {
    pub const EMPTY: PatchSample = PatchSample {
        strength: 0.0,
        is_tall: false,
    };
}

struct LayerGrid {
    cfg: LayerCfg,
    grid_count: i32,
}

impl LayerGrid {
    fn new(cfg: LayerCfg, world_size_m: f32) -> Self {
        let grid_count = (world_size_m / cfg.target_grid_m).round().max(1.0) as i32;
        Self { cfg, grid_count }
    }
}

pub struct GrassPatchField {
    warp_x: PerlinNoise3D,
    warp_y: PerlinNoise3D,
    world_size_m: f32,
    world_half: f32,
    master_seed: u64,
    primary: LayerGrid,
    secondary: LayerGrid,
}

impl GrassPatchField {
    pub fn new(master_seed: u64, world_size_m: f32) -> Self {
        Self {
            warp_x: PerlinNoise3D::new(master_seed ^ 0x6B61_7373_0001_0001),
            warp_y: PerlinNoise3D::new(master_seed ^ 0x6B61_7373_0001_0002),
            world_size_m,
            world_half: world_size_m * 0.5,
            master_seed,
            primary: LayerGrid::new(PRIMARY, world_size_m),
            secondary: LayerGrid::new(SECONDARY, world_size_m),
        }
    }

    /// Sample the patch field at world position `(wx, wz)`.
    pub fn sample(&self, wx: f32, wz: f32) -> PatchSample {
        let nx = wx + self.world_half;
        let nz = wz + self.world_half;
        // Shared warp coords; only the noise field differs between axes. The
        // fBm calls are the dominant per-cell cost.
        let warp = |field: &PerlinNoise3D| {
            fbm_wrap_x(
                field,
                nx,
                nz,
                self.world_size_m,
                WARP_FREQ,
                WARP_OCTAVES,
                2.0,
                0.5,
            ) * WARP_STRENGTH_M
        };
        let qx = wx + warp(&self.warp_x);
        let qz = wz + warp(&self.warp_y);

        let primary = self.sample_layer(&self.primary, qx, qz);
        let secondary = self.sample_layer(&self.secondary, qx, qz);

        // Primary wins ties so small clumps deep inside a primary patch don't
        // spuriously flip the is_tall flag.
        if primary.strength >= secondary.strength {
            primary
        } else {
            secondary
        }
    }

    fn sample_layer(&self, layer: &LayerGrid, qx: f32, qz: f32) -> PatchSample {
        let grid_m = self.world_size_m / layer.grid_count as f32;
        let gx0 = ((qx + self.world_half) / grid_m).floor() as i32;
        let gz0 = ((qz + self.world_half) / grid_m).floor() as i32;

        let mut best_strength = 0.0f32;
        let mut best_is_tall = false;

        for ogz in -1..=1 {
            for ogx in -1..=1 {
                let sgx = (gx0 + ogx).rem_euclid(layer.grid_count);
                let sgz = gz0 + ogz;
                if sgz < 0 || sgz >= layer.grid_count {
                    continue;
                }
                let Some(seed) = self.seed_at(layer, grid_m, sgx, sgz) else {
                    continue;
                };

                let dx_raw = (qx - seed.wx).abs();
                let dx_w = dx_raw.min(self.world_size_m - dx_raw);
                let ddz = qz - seed.wz;
                let d_sq = dx_w * dx_w + ddz * ddz;

                if d_sq >= seed.radius * seed.radius {
                    continue;
                }
                let d = d_sq.sqrt();
                // Inverted smoothstep edges: 1 at the inner boundary
                // (r - fade), 0 at the outer (r). Reads as "distance-from-
                // center fade" without needing an explicit 1 - ... flip.
                let strength = smoothstep(seed.radius, seed.radius - layer.cfg.fade_m, d);
                if strength > best_strength {
                    best_strength = strength;
                    best_is_tall = seed.is_tall;
                }
            }
        }

        PatchSample {
            strength: best_strength,
            is_tall: best_is_tall,
        }
    }

    fn seed_at(&self, layer: &LayerGrid, grid_m: f32, gx: i32, gz: i32) -> Option<Seed> {
        // SmallRng reseed per neighbor cell (up to 18 per query across both
        // layers). Cheaper than it looks vs. the warp fBm that dominates.
        let cell_seed = (self.master_seed ^ layer.cfg.seed_salt)
            .wrapping_add((gx as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_add((gz as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
        let mut rng = SmallRng::seed_from_u64(cell_seed);

        let occupancy: f32 = rng.gen();
        if occupancy >= layer.cfg.occupancy {
            return None;
        }

        let jx: f32 = rng.gen::<f32>() * 2.0 - 1.0;
        let jz: f32 = rng.gen::<f32>() * 2.0 - 1.0;
        let r_jitter: f32 = rng.gen::<f32>() * 2.0 - 1.0;
        let tall_roll: f32 = rng.gen();

        let half_grid = grid_m * 0.5;
        let cx = gx as f32 * grid_m - self.world_half + half_grid;
        let cz = gz as f32 * grid_m - self.world_half + half_grid;

        Some(Seed {
            wx: cx + jx * half_grid * layer.cfg.jitter_frac,
            wz: cz + jz * half_grid * layer.cfg.jitter_frac,
            radius: layer.cfg.radius_mean_m * (1.0 + r_jitter * layer.cfg.radius_jitter),
            is_tall: tall_roll < layer.cfg.tall_prob,
        })
    }
}

struct Seed {
    wx: f32,
    wz: f32,
    radius: f32,
    is_tall: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_deterministic() {
        let a = GrassPatchField::new(42, 1024.0);
        let b = GrassPatchField::new(42, 1024.0);
        for &(x, z) in &[(0.0, 0.0), (123.4, -56.7), (500.0, 500.0), (-500.0, 400.0)] {
            let sa = a.sample(x, z);
            let sb = b.sample(x, z);
            assert_eq!(sa.strength, sb.strength);
            assert_eq!(sa.is_tall, sb.is_tall);
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let a = GrassPatchField::new(1, 1024.0);
        let b = GrassPatchField::new(2, 1024.0);
        let mut diffs = 0;
        for i in 0..64 {
            for j in 0..64 {
                let x = (i as f32 - 32.0) * 16.0;
                let z = (j as f32 - 32.0) * 16.0;
                if a.sample(x, z).strength != b.sample(x, z).strength {
                    diffs += 1;
                }
            }
        }
        assert!(
            diffs > 100,
            "expected most samples to differ between seeds, got {diffs} / 4096"
        );
    }

    #[test]
    fn coverage_is_partial_not_uniform() {
        // The whole point of switching away from fBm+threshold: the new
        // field must produce *some* grass-free ground.
        let field = GrassPatchField::new(42, 2048.0);
        let mut covered = 0;
        let mut uncovered = 0;
        for i in 0..128 {
            for j in 0..128 {
                let x = (i as f32 - 64.0) * 8.0;
                let z = (j as f32 - 64.0) * 8.0;
                if field.sample(x, z).strength > 0.0 {
                    covered += 1;
                } else {
                    uncovered += 1;
                }
            }
        }
        let total = 128 * 128;
        assert!(
            covered > total / 10,
            "expected at least 10 % patch coverage, got {covered}/{total}"
        );
        assert!(
            uncovered > total / 10,
            "expected at least 10 % bare ground, got {uncovered}/{total}"
        );
    }

    #[test]
    fn x_wrap_continuity() {
        // The world is cylindrical in X — the seam must not show a
        // discontinuity.
        let world = 1024.0;
        let field = GrassPatchField::new(42, world);
        for j in 0..16 {
            let z = (j as f32 - 8.0) * 50.0;
            let left = field.sample(-world * 0.5, z);
            let right = field.sample(world * 0.5, z);
            assert!(
                (left.strength - right.strength).abs() < 1e-4,
                "x-seam discontinuity at z={z}: {} vs {}",
                left.strength,
                right.strength
            );
            assert_eq!(left.is_tall, right.is_tall);
        }
    }

    #[test]
    fn patch_interior_reaches_full_strength() {
        let field = GrassPatchField::new(42, 2048.0);
        let mut max_strength = 0.0f32;
        for i in 0..128 {
            for j in 0..128 {
                let x = (i as f32 - 64.0) * 8.0;
                let z = (j as f32 - 64.0) * 8.0;
                let s = field.sample(x, z).strength;
                if s > max_strength {
                    max_strength = s;
                }
            }
        }
        assert!(max_strength > 0.99, "max strength {max_strength} < 0.99");
    }

    #[test]
    fn strength_stays_in_unit_interval() {
        // Invariant: the plain-branch density calculation
        // `(patch.strength * eligibility * 9.0).round().clamp(0.0, 9.0)`
        // assumes strength ∈ [0, 1]. A regression that lets strength go
        // negative or > 1 would either knock out grass on valid cells or
        // overshoot the density range.
        let field = GrassPatchField::new(42, 2048.0);
        for i in 0..256 {
            for j in 0..256 {
                let x = (i as f32 - 128.0) * 4.0;
                let z = (j as f32 - 128.0) * 4.0;
                let s = field.sample(x, z).strength;
                assert!(
                    (0.0..=1.0).contains(&s),
                    "strength out of range at ({x}, {z}): {s}"
                );
            }
        }
    }

    #[test]
    fn secondary_layer_fills_gaps() {
        // Regression guard: the secondary (small-clump) layer must produce
        // some coverage outside primary patches. If this count is zero, the
        // second LayerGrid is dead weight.
        let full = GrassPatchField::new(42, 2048.0);
        let mut primary_only = GrassPatchField::new(42, 2048.0);
        primary_only.secondary.cfg.occupancy = 0.0;

        let mut gap_hits = 0;
        for i in 0..256 {
            for j in 0..256 {
                let x = (i as f32 - 128.0) * 4.0;
                let z = (j as f32 - 128.0) * 4.0;
                let full_s = full.sample(x, z).strength;
                let primary_s = primary_only.sample(x, z).strength;
                if full_s > 0.0 && primary_s == 0.0 {
                    gap_hits += 1;
                }
            }
        }
        assert!(
            gap_hits > 200,
            "secondary layer fired in {gap_hits} gap cells (expected > 200)"
        );
    }
}
