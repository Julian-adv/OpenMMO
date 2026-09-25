//! Offline tree placements and per-cell grass densities.

use super::tile_bake::{HEIGHT_BIAS, HEIGHT_STEP, TILE_DIM, VERTS_PER_SIDE};
use crate::tree_format::{
    TREE_EXCLUSION_RADIUS, TREE_V1_BYTES_PER_INSTANCE, TREE_V1_HEADER_BYTES, TREE_V1_MAGIC,
    TREE_V1_SCALE,
};

#[inline]
fn decode_height(v: u16) -> f32 {
    v as f32 * HEIGHT_STEP - HEIGHT_BIAS
}

/// Decode a 65×65 uint16 heightmap into f32 meters. Accepts the little-endian
/// byte buffer that `tile_bake::encode_heightmap` writes.
fn decode_heightmap(bytes: &[u8]) -> Vec<f32> {
    debug_assert_eq!(bytes.len(), VERTS_PER_SIDE * VERTS_PER_SIDE * 2);
    let mut out = Vec::with_capacity(VERTS_PER_SIDE * VERTS_PER_SIDE);
    for chunk in bytes.as_chunks::<2>().0 {
        let v = u16::from_le_bytes([chunk[0], chunk[1]]);
        out.push(decode_height(v));
    }
    out
}

/// Bilinear height sample at fractional tile-local coordinates. Matches the
/// client's `sampleHeight`: clamps the input to `[0, TILE_DIM-1]`, then reads
/// the 2×2 neighborhood in the 65×65 vertex grid.
fn sample_height(heights: &[f32], local_x: f32, local_z: f32) -> f32 {
    let td = TILE_DIM as f32;
    let cx = local_x.clamp(0.0, td - 1.0);
    let cz = local_z.clamp(0.0, td - 1.0);
    let ix = cx as usize;
    let iz = cz as usize;
    let fx = cx - ix as f32;
    let fz = cz - iz as f32;
    let ix1 = (ix + 1).min(TILE_DIM);
    let iz1 = (iz + 1).min(TILE_DIM);

    let h00 = heights[iz * VERTS_PER_SIDE + ix];
    let h10 = heights[iz * VERTS_PER_SIDE + ix1];
    let h01 = heights[iz1 * VERTS_PER_SIDE + ix];
    let h11 = heights[iz1 * VERTS_PER_SIDE + ix1];
    let h0 = h00 + (h10 - h00) * fx;
    let h1 = h01 + (h11 - h01) * fx;
    h0 + (h1 - h0) * fz
}

/// 2-channel central-difference slope at cell `(cx, cz)`. Matches the client's
/// `computeSlope` in `tree-data.ts` (edges fall back to the center sample, so
/// the slope is damped at the tile border — accepting a slight asymmetry
/// against neighbor tiles at this scale).
fn compute_slope(heights: &[f32], cx: usize, cz: usize) -> f32 {
    let hc = heights[cz * VERTS_PER_SIDE + cx];
    let hl = if cx > 0 {
        heights[cz * VERTS_PER_SIDE + cx - 1]
    } else {
        hc
    };
    let hr = if cx < TILE_DIM {
        heights[cz * VERTS_PER_SIDE + cx + 1]
    } else {
        hc
    };
    let hu = if cz > 0 {
        heights[(cz - 1) * VERTS_PER_SIDE + cx]
    } else {
        hc
    };
    let hd = if cz < TILE_DIM {
        heights[(cz + 1) * VERTS_PER_SIDE + cx]
    } else {
        hc
    };
    let dx = hr - hl;
    let dz = hd - hu;
    (dx * dx + dz * dz).sqrt() / 2.0
}

// ------------------------------------------------------------------
// Mulberry32 RNG, a 1-to-1 port of `createRng` in `simplex-noise.ts`.
// ------------------------------------------------------------------

/// The JS implementation returns an f64 in [0, 1), and downstream comparisons
/// (`rand() < 0.08` etc.) are all performed in f64. We mirror that here so
/// tree/grass placements are deterministic against the same tile seeds.
struct Rng {
    s: u32,
}

impl Rng {
    fn new(seed_i32: i32) -> Self {
        Self { s: seed_i32 as u32 }
    }

    fn next_f64(&mut self) -> f64 {
        self.s = self.s.wrapping_add(0x6d2b_79f5);
        let mut t = (self.s ^ (self.s >> 15)).wrapping_mul(1 | self.s);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

/// `((tx * 48271) ^ (tz * 16807)) | 0` — matches `tileSeed` in `tree-data.ts`.
/// The factors fit easily in i32 for any realistic tile range, so `wrapping_mul`
/// matches the `| 0` truncation in JS.
fn tile_seed_trees(tx: i32, tz: i32) -> i32 {
    tx.wrapping_mul(48271) ^ tz.wrapping_mul(16807)
}

/// `((tx * 73856093) ^ (tz * 19349663)) | 0` — matches `tileSeed` in
/// `grass-data.ts`. These factors overflow i32 for |tile| ≳ 30, but JS's
/// `a * b | 0` truncates to the low 32 bits, which is exactly what
/// `wrapping_mul` gives us.
fn tile_seed_grass(tx: i32, tz: i32) -> i32 {
    tx.wrapping_mul(73_856_093) ^ tz.wrapping_mul(19_349_663)
}

// ------------------------------------------------------------------
// Splat vegMeta bands (must match `grass-material.ts`).
// ------------------------------------------------------------------

const SHORT_GRASS_R_MIN: u8 = 230;
const SHORT_GRASS_R_MAX: u8 = 239;
const TALL_GRASS_R_MIN: u8 = 240;
const TALL_GRASS_R_MAX: u8 = 249;

/// Inverse of `short_grass_veg` / `tall_grass_veg` in the splat baker:
/// returns the 0..=9 density encoded in `r_val`. Caller is responsible for
/// having already verified `r_val` falls inside one of the two grass bands.
#[inline]
fn veg_density(r_val: u8) -> u8 {
    if r_val >= TALL_GRASS_R_MIN {
        r_val - TALL_GRASS_R_MIN
    } else {
        r_val - SHORT_GRASS_R_MIN
    }
}

const CHANNELS: usize = 4;
const VEGMETA_OFFSET: usize = 3;

// Tile-world offset: tile n covers x ∈ [n*TILE_DIM - TILE_DIM/2, n*TILE_DIM + TILE_DIM/2).
// Matches `TERRAIN_TILE_SIZE = 64` in the client.
fn tile_min_world(t: i32) -> f32 {
    t as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5
}

// ==================================================================
// Trees (V1 format).
// ==================================================================

/// Per-cell probability that a grass cell (vegMeta 230–249) tries to spawn a
/// tree. Lowered from the client-runtime value of 0.08 because the bake now
/// covers the whole world — the runtime value was tuned when only visited
/// tiles had data and surrounding tiles silently rendered empty.
const TREE_PROBABILITY: f64 = 0.025;

/// Minimum grass density (within either short or tall band) required for a
/// cell to be eligible for tree spawning. The splatmap fades grass density
/// to zero around rivers (and trails off near other features); gating trees
/// here keeps the sparse fringe cells grass-only so trees don't push right
/// up against the bank.
const TREE_MIN_DENSITY: u8 = 4;

/// Exclusion rectangle in world-space meters: `[min_x, min_z, max_x, max_z]`.
/// Currently only used when baking over an already-laid housing footprint —
/// the offline pipeline doesn't generate houses, so the slice is typically
/// empty. Left in place so the baker can be folded into a replat flow later.
pub type ExclusionRect = [f32; 4];

/// Placement pass for a single tile. Reads vegMeta from the splatmap and the
/// decoded heightmap, returns the V1-encoded tree binary (empty header +
/// zero instances if the tile produced no trees).
pub fn bake_trees(
    tx: i32,
    tz: i32,
    splatmap: &[u8],
    heightmap_bytes: &[u8],
    exclusion_rects: &[ExclusionRect],
) -> Vec<u8> {
    let heights = decode_heightmap(heightmap_bytes);
    let tile_min_x = tile_min_world(tx);
    let tile_min_z = tile_min_world(tz);
    let mut rng = Rng::new(tile_seed_trees(tx, tz));

    // Two parallel arrays of (local_x, local_z, rotation, scale), one per
    // tree type. Local coords are in tile-space [0, TILE_DIM) so encoding
    // into the u16 position slot is a single multiply.
    let mut tree1: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut tree2: Vec<(f32, f32, f32, f32)> = Vec::new();

    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            let r_val = splatmap[(cz * TILE_DIM + cx) * CHANNELS + VEGMETA_OFFSET];
            if !(SHORT_GRASS_R_MIN..=TALL_GRASS_R_MAX).contains(&r_val) {
                continue;
            }
            if veg_density(r_val) < TREE_MIN_DENSITY {
                continue;
            }
            if rng.next_f64() >= TREE_PROBABILITY {
                continue;
            }
            let slope = compute_slope(&heights, cx, cz);
            if slope > 1.5 {
                continue;
            }

            let local_x = cx as f32 + (rng.next_f64() as f32) * 0.8 + 0.1;
            let local_z = cz as f32 + (rng.next_f64() as f32) * 0.8 + 0.1;
            let world_y = sample_height(&heights, local_x, local_z);
            if world_y < 0.5 {
                continue;
            }

            let rotation = (rng.next_f64() as f32) * std::f32::consts::TAU;
            let is_tree1 = rng.next_f64() < 0.5;
            let slot = if is_tree1 { 0 } else { 1 };
            let (scale_min, scale_range) = TREE_V1_SCALE[slot];
            let scale = scale_min + (rng.next_f64() as f32) * scale_range;

            if !exclusion_rects.is_empty() {
                let world_x = tile_min_x + local_x;
                let world_z = tile_min_z + local_z;
                let r = TREE_EXCLUSION_RADIUS[slot] * scale;
                let mut blocked = false;
                for &[r_min_x, r_min_z, r_max_x, r_max_z] in exclusion_rects {
                    if world_x > r_min_x - r
                        && world_x < r_max_x + r
                        && world_z > r_min_z - r
                        && world_z < r_max_z + r
                    {
                        blocked = true;
                        break;
                    }
                }
                if blocked {
                    continue;
                }
            }

            let bucket = if is_tree1 { &mut tree1 } else { &mut tree2 };
            bucket.push((local_x, local_z, rotation, scale));
        }
    }

    encode_tree_v1(&tree1, &tree2)
}

fn encode_tree_v1(tree1: &[(f32, f32, f32, f32)], tree2: &[(f32, f32, f32, f32)]) -> Vec<u8> {
    let total = tree1.len() + tree2.len();
    let mut out = Vec::with_capacity(TREE_V1_HEADER_BYTES + total * TREE_V1_BYTES_PER_INSTANCE);
    out.extend_from_slice(&TREE_V1_MAGIC.to_le_bytes());
    out.extend_from_slice(&(tree1.len() as u32).to_le_bytes());
    out.extend_from_slice(&(tree2.len() as u32).to_le_bytes());

    let pos_scale = 65535.0 / TILE_DIM as f32;
    let rot_scale = 255.0 / std::f32::consts::TAU;

    for (slot, list) in [tree1, tree2].iter().enumerate() {
        let (scale_min, scale_range) = TREE_V1_SCALE[slot];
        let scale_scale = 255.0 / scale_range;
        for &(lx, lz, rot, scale) in list.iter() {
            let px = (lx * pos_scale).round().clamp(0.0, 65535.0) as u16;
            let pz = (lz * pos_scale).round().clamp(0.0, 65535.0) as u16;
            let r = ((rot * rot_scale).round() as i32) & 0xff;
            let s = ((scale - scale_min) * scale_scale)
                .round()
                .clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&px.to_le_bytes());
            out.extend_from_slice(&pz.to_le_bytes());
            out.push(r as u8);
            out.push(s);
        }
    }

    out
}

const SHORT_BLADES_PER_CELL: u8 = 64;
const TALL_BLADES_PER_CELL: u8 = 36;
const BOUNDARY_BLEND_RATIO: f32 = 0.3;

fn is_boundary_cell(splatmap: &[u8], cx: usize, cz: usize, other_min: u8, other_max: u8) -> bool {
    [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|&(dx, dz)| {
        let x = cx as i32 + dx;
        let z = cz as i32 + dz;
        if x < 0 || z < 0 || x >= TILE_DIM as i32 || z >= TILE_DIM as i32 {
            return false;
        }
        let value = splatmap[(z as usize * TILE_DIM + x as usize) * CHANNELS + VEGMETA_OFFSET];
        (other_min..=other_max).contains(&value)
    })
}

/// Store counts per 1m cell; clients generate the individual blades.
pub fn bake_grass(tx: i32, tz: i32, splatmap: &[u8], heightmap_bytes: &[u8]) -> Vec<u8> {
    use crate::grass_format::{empty_grass, GRASS_CELL_TYPES, GRASS_HEADER_BYTES};
    let heights = decode_heightmap(heightmap_bytes);
    let mut output = empty_grass();
    let mut rng = Rng::new(tile_seed_grass(tx, tz) ^ 0xf10e);
    for z in 0..TILE_DIM {
        for x in 0..TILE_DIM {
            let value = splatmap[(z * TILE_DIM + x) * CHANNELS + VEGMETA_OFFSET];
            let (kind, maximum, other_min, other_max) = match value {
                SHORT_GRASS_R_MIN..=SHORT_GRASS_R_MAX => {
                    (0, SHORT_BLADES_PER_CELL, TALL_GRASS_R_MIN, TALL_GRASS_R_MAX)
                }
                TALL_GRASS_R_MIN..=TALL_GRASS_R_MAX => (
                    1,
                    TALL_BLADES_PER_CELL,
                    SHORT_GRASS_R_MIN,
                    SHORT_GRASS_R_MAX,
                ),
                _ => continue,
            };
            if sample_height(&heights, x as f32 + 0.5, z as f32 + 0.5) < 0.05 {
                continue;
            }
            let density = veg_density(value) as f32 / 9.0;
            let count = (density * maximum as f32).round() as u8;
            let converted = if is_boundary_cell(splatmap, x, z, other_min, other_max) {
                (count as f32 * BOUNDARY_BLEND_RATIO).round() as u8
            } else {
                0
            };
            let offset = GRASS_HEADER_BYTES + (z * TILE_DIM + x) * GRASS_CELL_TYPES;
            output[offset + kind] = count - converted;
            output[offset + 1 - kind] = converted;
            if kind == 0 && rng.next_f64() < 0.4 * 0.125f64.powf(density as f64) {
                output[offset + 2] = 1;
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_heights(meters: f32) -> Vec<u8> {
        let encoded = ((meters + HEIGHT_BIAS) / HEIGHT_STEP).round() as u16;
        let mut out = Vec::with_capacity(VERTS_PER_SIDE * VERTS_PER_SIDE * 2);
        for _ in 0..VERTS_PER_SIDE * VERTS_PER_SIDE {
            out.extend_from_slice(&encoded.to_le_bytes());
        }
        out
    }

    fn splat_with_veg(veg: u8) -> Vec<u8> {
        let mut out = vec![0u8; TILE_DIM * TILE_DIM * 4];
        for cz in 0..TILE_DIM {
            for cx in 0..TILE_DIM {
                out[(cz * TILE_DIM + cx) * 4 + VEGMETA_OFFSET] = veg;
            }
        }
        out
    }

    #[test]
    fn tree_header_is_v1() {
        let h = flat_heights(5.0);
        let s = splat_with_veg(235);
        let bin = bake_trees(0, 0, &s, &h, &[]);
        assert_eq!(&bin[0..4], &TREE_V1_MAGIC.to_le_bytes());
        // 12-byte header; rest must be a multiple of 6 (quantized tree struct).
        assert!(bin.len() >= TREE_V1_HEADER_BYTES);
        let body = bin.len() - TREE_V1_HEADER_BYTES;
        assert_eq!(body % TREE_V1_BYTES_PER_INSTANCE, 0);
    }

    #[test]
    fn grass_stores_cell_counts() {
        let h = flat_heights(5.0);
        let s = splat_with_veg(239); // dense short grass
        let bin = bake_grass(0, 0, &s, &h);
        use crate::grass_format::{GRASS_FILE_BYTES, GRASS_V4_MAGIC};
        assert_eq!(&bin[0..4], &GRASS_V4_MAGIC.to_le_bytes());
        assert_eq!(bin.len(), GRASS_FILE_BYTES);
        assert!(bin[4..]
            .as_chunks::<3>()
            .0
            .iter()
            .all(|cell| cell[0] == 64 && cell[1] == 0 && cell[2] <= 1));
        let tall = bake_grass(0, 0, &splat_with_veg(249), &h);
        assert!(tall[4..]
            .as_chunks::<3>()
            .0
            .iter()
            .all(|cell| *cell == [0, 36, 0]));
    }

    #[test]
    fn trees_skip_underwater_cells() {
        // Height below the 0.5 m floor → every cell rejected after the
        // sample, so tree count is zero even though vegMeta says grass.
        let h = flat_heights(0.2);
        let s = splat_with_veg(235);
        let bin = bake_trees(0, 0, &s, &h, &[]);
        let c1 = u32::from_le_bytes([bin[4], bin[5], bin[6], bin[7]]);
        let c2 = u32::from_le_bytes([bin[8], bin[9], bin[10], bin[11]]);
        assert_eq!(c1 + c2, 0);
    }

    #[test]
    fn grass_skips_underwater_cells() {
        let h = flat_heights(0.02);
        let s = splat_with_veg(239);
        let bin = bake_grass(0, 0, &s, &h);
        assert!(bin[4..].iter().all(|&count| count == 0));
    }

    #[test]
    fn trees_skip_low_density_cells() {
        // Sparse grass cells (density < TREE_MIN_DENSITY) must not spawn
        // trees. Without the threshold, river-edge fade cells (density 1-3)
        // would still pick up trees at the global probability.
        let h = flat_heights(5.0);
        let s = splat_with_veg(231); // short grass density 1
        let bin = bake_trees(0, 0, &s, &h, &[]);
        let c1 = u32::from_le_bytes([bin[4], bin[5], bin[6], bin[7]]);
        let c2 = u32::from_le_bytes([bin[8], bin[9], bin[10], bin[11]]);
        assert_eq!(c1 + c2, 0, "trees must skip density-1 short grass cells");

        let s_tall = splat_with_veg(241); // tall grass density 1
        let bin_t = bake_trees(0, 0, &s_tall, &h, &[]);
        let t1 = u32::from_le_bytes([bin_t[4], bin_t[5], bin_t[6], bin_t[7]]);
        let t2 = u32::from_le_bytes([bin_t[8], bin_t[9], bin_t[10], bin_t[11]]);
        assert_eq!(t1 + t2, 0, "trees must skip density-1 tall grass cells");
    }

    #[test]
    fn tree_min_density_boundary_is_exact() {
        // density 3 (just below) → no trees; density 4 (== TREE_MIN_DENSITY)
        // → trees can spawn. With 4096 cells and 0.025 probability the
        // expected count at density 4 is ~100, so a non-zero result is a
        // robust signal.
        let h = flat_heights(5.0);

        let just_below = splat_with_veg(233); // short density 3
        let bin_lo = bake_trees(0, 0, &just_below, &h, &[]);
        let c_lo = u32::from_le_bytes([bin_lo[4], bin_lo[5], bin_lo[6], bin_lo[7]])
            + u32::from_le_bytes([bin_lo[8], bin_lo[9], bin_lo[10], bin_lo[11]]);
        assert_eq!(c_lo, 0, "density 3 (TREE_MIN_DENSITY - 1) must skip trees");

        let at_threshold = splat_with_veg(234); // short density 4
        let bin_hi = bake_trees(0, 0, &at_threshold, &h, &[]);
        let c_hi = u32::from_le_bytes([bin_hi[4], bin_hi[5], bin_hi[6], bin_hi[7]])
            + u32::from_le_bytes([bin_hi[8], bin_hi[9], bin_hi[10], bin_hi[11]]);
        assert!(
            c_hi > 0,
            "density 4 (== TREE_MIN_DENSITY) must allow trees, got {c_hi}"
        );
    }

    #[test]
    fn veg_density_round_trips_encoders() {
        // veg_density is the inverse of short_grass_veg / tall_grass_veg in
        // the splat baker. Drift between the two would cause the tree
        // density gate to silently misclassify cells.
        for d in 0..=9u8 {
            assert_eq!(veg_density(SHORT_GRASS_R_MIN + d), d);
            assert_eq!(veg_density(TALL_GRASS_R_MIN + d), d);
        }
    }

    #[test]
    fn deterministic_for_same_tile() {
        let h = flat_heights(5.0);
        let s = splat_with_veg(235);
        let a = bake_trees(3, -2, &s, &h, &[]);
        let b = bake_trees(3, -2, &s, &h, &[]);
        assert_eq!(a, b);
        let g1 = bake_grass(3, -2, &s, &h);
        let g2 = bake_grass(3, -2, &s, &h);
        assert_eq!(g1, g2);
    }
}
