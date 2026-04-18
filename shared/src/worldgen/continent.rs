//! Phase 1: continent / sea mask.
//!
//! Build a fBm-based continental potential field at the global-map
//! resolution, using X-periodic noise so the world wraps east-west (a player
//! who exits the right edge reappears on the left, and the continental shape
//! continues seamlessly). The Y axis does *not* wrap — land that touches the
//! north/south border is expected to become mountain walls in Phase 2.
//!
//! A quantile threshold on the potential field picks the configured sea
//! ratio exactly. Tiny islands below `min_island_cells` are filtered out.

use super::config::WorldGenConfig;
use super::global_map::GlobalMap;
use super::growth;
use super::noise::{fbm_wrap_x, PerlinNoise3D};

const CONTINENT_LACUNARITY: f32 = 2.0;

/// Run Phase 1: produce continent potential + land mask.
pub fn generate_continent_mask(config: &WorldGenConfig) -> GlobalMap {
    let res = config.global_res as usize;
    let total = res * res;

    let noise = PerlinNoise3D::new(config.seed ^ 0xC0_0_C0_0_C0_0_C0_0_u64);
    let channel_noise = PerlinNoise3D::new(config.seed ^ 0x5EA_C_5EA_C_5EA_C_u64);
    let world_width = config.global_res as f32;
    let base_freq = config.continent_frequency;
    let channel_freq = 1.0 / config.sea_channel_wavelength.max(1.0);
    let channel_strength = config.sea_channel_strength.max(0.0);

    let mut potential = vec![0.0f32; total];
    for y in 0..res {
        let yf = y as f32;
        for x in 0..res {
            let xf = x as f32;
            let base = fbm_wrap_x(
                &noise,
                xf,
                yf,
                world_width,
                base_freq,
                config.continent_octaves.max(1),
                CONTINENT_LACUNARITY,
                config.continent_gain,
            );
            // Ridge-shaped sea channels: wherever a secondary low-freq noise
            // crosses zero, carve a strait. `1 - |n|` peaks at n=0; raising
            // to a power narrows the ridge so channels feel more stroke-like
            // than blob-like.
            let ridge_bias = if channel_strength > 0.0 {
                let n = fbm_wrap_x(
                    &channel_noise,
                    xf,
                    yf,
                    world_width,
                    channel_freq,
                    3,
                    CONTINENT_LACUNARITY,
                    0.5,
                );
                let ridge = (1.0 - n.abs()).max(0.0);
                ridge.powi(4) * channel_strength
            } else {
                0.0
            };
            potential[y * res + x] = base - ridge_bias;
        }
    }

    // Land mask is built by seeded region growth (Eden growth with union-
    // find merging), NOT by noise thresholding. The fBm `potential` above is
    // kept for Phase 2 elevation shading / future use — here it only serves
    // visualization. See `growth.rs` for the growth algorithm.
    let mut land_mask = growth::growth_mask(config);

    if config.min_island_cells > 0 {
        remove_small_islands(&mut land_mask, res, config.min_island_cells as usize);
    }

    // Optional post-processing (off by default): narrow-strait opening and
    // isthmus cuts. With the growth approach these are rarely needed since
    // the top-N filter already enforces a clean component count.
    if config.min_strait_width_cells > 0 {
        let radius = (config.min_strait_width_cells as usize) / 2;
        if radius > 0 {
            binary_open(&mut land_mask, res, radius);
            if config.min_island_cells > 0 {
                remove_small_islands(&mut land_mask, res, config.min_island_cells as usize);
            }
        }
    }
    if config.max_isthmus_width_cells > 0 {
        cut_isthmuses(
            &mut land_mask,
            res,
            (config.max_isthmus_width_cells as usize) / 2,
        );
        if config.min_island_cells > 0 {
            remove_small_islands(&mut land_mask, res, config.min_island_cells as usize);
        }
    }

    // sea_level_potential: pick the quantile on `potential` so that the same
    // fraction of cells is "below sea level" as are sea in the mask. Lets the
    // shading PNG stay consistent with what growth produced.
    let mask_sea_fraction =
        1.0 - (land_mask.iter().filter(|&&b| b == 1).count() as f32 / total as f32);
    let threshold = quantile(&potential, mask_sea_fraction.clamp(0.0, 1.0));

    GlobalMap {
        config: config.clone(),
        continent_potential: potential,
        land_mask,
        sea_level_potential: threshold,
        elevation_m: vec![0.0; total],
    }
}

/// 4-connected flood fill (X-periodic) that drops land components smaller
/// than `min_cells`. X wrap matters here: a continent that straddles the
/// x=0/x=res-1 boundary is a single component in a toroidal-in-X world, and
/// must be treated as such so it doesn't get mistakenly split and culled.
fn remove_small_islands(mask: &mut [u8], res: usize, min_cells: usize) {
    let total = res * res;
    let mut visited = vec![false; total];
    let mut stack: Vec<usize> = Vec::with_capacity(1024);
    let mut component: Vec<usize> = Vec::with_capacity(1024);

    for start in 0..total {
        if mask[start] == 0 || visited[start] {
            continue;
        }
        component.clear();
        stack.clear();
        stack.push(start);
        visited[start] = true;

        while let Some(i) = stack.pop() {
            component.push(i);
            let x = i % res;
            let y = i / res;
            // 4-connected neighbors, with X wrapped.
            let left = if x == 0 { res - 1 } else { x - 1 };
            let right = if x + 1 == res { 0 } else { x + 1 };
            let neighbors = [
                y * res + left,
                y * res + right,
                if y > 0 { Some((y - 1) * res + x) } else { None }.unwrap_or(usize::MAX),
                if y + 1 < res {
                    Some((y + 1) * res + x)
                } else {
                    None
                }
                .unwrap_or(usize::MAX),
            ];
            for &n in &neighbors {
                if n != usize::MAX && mask[n] == 1 && !visited[n] {
                    visited[n] = true;
                    stack.push(n);
                }
            }
        }

        if component.len() < min_cells {
            for &i in &component {
                mask[i] = 0;
            }
        }
    }
}

/// Morphological opening on the binary land mask: erode by `radius`, then
/// dilate by `radius`. Removes land features narrower than `2 * radius`
/// while preserving thicker land's shape and coastline.
///
/// X-axis wraps (consistent with the world's east-west periodicity).
/// Y-axis does not wrap; out-of-bounds cells are treated as sea during
/// erosion (so land touching the N/S border gets eroded along that edge,
/// which is fine — Phase 2 turns those into mountain walls anyway).
///
/// Separable (row pass then column pass) for O(radius · res²) per erosion
/// or dilation instead of O(radius² · res²).
fn binary_open(mask: &mut [u8], res: usize, radius: usize) {
    let eroded = erode_box(mask, res, radius);
    let dilated = dilate_box(&eroded, res, radius);
    mask.copy_from_slice(&dilated);
}

fn erode_box(mask: &[u8], res: usize, radius: usize) -> Vec<u8> {
    // Row pass (X wraps).
    let mut h = vec![0u8; mask.len()];
    for y in 0..res {
        for x in 0..res {
            let mut ok = true;
            for dx in -(radius as isize)..=(radius as isize) {
                let xx = (x as isize + dx).rem_euclid(res as isize) as usize;
                if mask[y * res + xx] == 0 {
                    ok = false;
                    break;
                }
            }
            h[y * res + x] = if ok { 1 } else { 0 };
        }
    }
    // Column pass on h (Y does not wrap). Out-of-bounds Y is treated as
    // LAND so that land adjacent to the north/south border isn't falsely
    // eroded — those cells are expected to become Phase 2 mountain walls.
    let mut v = vec![0u8; mask.len()];
    for y in 0..res {
        for x in 0..res {
            let mut ok = true;
            for dy in -(radius as isize)..=(radius as isize) {
                let yy = y as isize + dy;
                if yy < 0 || yy >= res as isize {
                    continue;
                }
                if h[(yy as usize) * res + x] == 0 {
                    ok = false;
                    break;
                }
            }
            v[y * res + x] = if ok { 1 } else { 0 };
        }
    }
    v
}

fn dilate_box(mask: &[u8], res: usize, radius: usize) -> Vec<u8> {
    // Row pass (X wraps).
    let mut h = vec![0u8; mask.len()];
    for y in 0..res {
        for x in 0..res {
            let mut any = false;
            for dx in -(radius as isize)..=(radius as isize) {
                let xx = (x as isize + dx).rem_euclid(res as isize) as usize;
                if mask[y * res + xx] == 1 {
                    any = true;
                    break;
                }
            }
            h[y * res + x] = if any { 1 } else { 0 };
        }
    }
    // Column pass on h (Y does not wrap; out-of-bounds contributes nothing).
    let mut v = vec![0u8; mask.len()];
    for y in 0..res {
        for x in 0..res {
            let mut any = false;
            for dy in -(radius as isize)..=(radius as isize) {
                let yy = y as isize + dy;
                if yy < 0 || yy >= res as isize {
                    continue;
                }
                if h[(yy as usize) * res + x] == 1 {
                    any = true;
                    break;
                }
            }
            v[y * res + x] = if any { 1 } else { 0 };
        }
    }
    v
}

/// Cut isthmuses — land cells that have sea on opposing sides within
/// `radius` cells, measured along any of 4 axes (cardinal: E-W, N-S; and
/// diagonal: NE-SW, NW-SE). Using 8 directions rather than only cardinal
/// ones avoids axis-aligned rectangular cut artifacts.
///
/// O(res²) total (one running-distance pass per direction, 8 passes).
/// X wraps for cardinal horizontal directions; diagonal scans don't wrap
/// (slight edge asymmetry, usually invisible at scale).
fn cut_isthmuses(mask: &mut [u8], res: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let r = radius as u32;
    let total = res * res;

    // --- Cardinal horizontal (E-W), with X wrap ---
    let mut left_d = vec![u32::MAX; total];
    let mut right_d = vec![u32::MAX; total];
    for y in 0..res {
        let mut d = u32::MAX;
        for x in 0..res {
            step_dist(mask[y * res + x], &mut d);
            left_d[y * res + x] = d;
        }
        let mut d = left_d[y * res + (res - 1)];
        if d != u32::MAX {
            for x in 0..res {
                step_dist(mask[y * res + x], &mut d);
                if d < left_d[y * res + x] {
                    left_d[y * res + x] = d;
                }
            }
        }
        let mut d = u32::MAX;
        for x in (0..res).rev() {
            step_dist(mask[y * res + x], &mut d);
            right_d[y * res + x] = d;
        }
        let mut d = right_d[y * res];
        if d != u32::MAX {
            for x in (0..res).rev() {
                step_dist(mask[y * res + x], &mut d);
                if d < right_d[y * res + x] {
                    right_d[y * res + x] = d;
                }
            }
        }
    }

    // --- Cardinal vertical (N-S), no Y wrap ---
    let mut top_d = vec![u32::MAX; total];
    let mut bot_d = vec![u32::MAX; total];
    for x in 0..res {
        let mut d = u32::MAX;
        for y in 0..res {
            step_dist(mask[y * res + x], &mut d);
            top_d[y * res + x] = d;
        }
        let mut d = u32::MAX;
        for y in (0..res).rev() {
            step_dist(mask[y * res + x], &mut d);
            bot_d[y * res + x] = d;
        }
    }

    // --- Diagonal NE-SW: y + x = const (no wrap; tolerate edge cells) ---
    let mut ne_d = vec![u32::MAX; total];
    let mut sw_d = vec![u32::MAX; total];
    for k in 0..(2 * res - 1) {
        let x_lo = k.saturating_sub(res - 1);
        let x_hi = k.min(res - 1);
        // Walk NE (x increasing, y decreasing along the diagonal).
        let mut d = u32::MAX;
        for x in x_lo..=x_hi {
            let y = k - x;
            let i = y * res + x;
            step_dist(mask[i], &mut d);
            sw_d[i] = d;
        }
        let mut d = u32::MAX;
        for x in (x_lo..=x_hi).rev() {
            let y = k - x;
            let i = y * res + x;
            step_dist(mask[i], &mut d);
            ne_d[i] = d;
        }
    }

    // --- Diagonal NW-SE: y - x = const (shifted by +res so index is u) ---
    let mut nw_d = vec![u32::MAX; total];
    let mut se_d = vec![u32::MAX; total];
    let kspan = res as isize - 1;
    for k in -kspan..=kspan {
        // y - x = k; y = k + x. Valid x range: y in [0, res), so x in
        // [max(0, -k), min(res-1, res-1-k)].
        let x_lo = (-k).max(0) as usize;
        let x_hi = ((res as isize - 1 - k).min(res as isize - 1)).max(0) as usize;
        if x_lo > x_hi {
            continue;
        }
        let mut d = u32::MAX;
        for x in x_lo..=x_hi {
            let y = (k + x as isize) as usize;
            let i = y * res + x;
            step_dist(mask[i], &mut d);
            nw_d[i] = d;
        }
        let mut d = u32::MAX;
        for x in (x_lo..=x_hi).rev() {
            let y = (k + x as isize) as usize;
            let i = y * res + x;
            step_dist(mask[i], &mut d);
            se_d[i] = d;
        }
    }

    // Cut cells where *any* of the 4 axis pairs finds opposing sea within r.
    let mut cut = vec![false; total];
    for i in 0..total {
        if mask[i] == 0 {
            continue;
        }
        let h = left_d[i] <= r && right_d[i] <= r;
        let v = top_d[i] <= r && bot_d[i] <= r;
        let d1 = ne_d[i] <= r && sw_d[i] <= r;
        let d2 = nw_d[i] <= r && se_d[i] <= r;
        if h || v || d1 || d2 {
            cut[i] = true;
        }
    }
    for i in 0..total {
        if cut[i] {
            mask[i] = 0;
        }
    }
}

/// Running-distance step: sea cell resets to 0; land cell adds 1 (saturating).
#[inline]
fn step_dist(cell: u8, d: &mut u32) {
    if cell == 0 {
        *d = 0;
    } else if *d != u32::MAX {
        *d = d.saturating_add(1);
    }
}

/// Return the value at the given quantile (0..1) in `values`.
/// Uses O(n) select_nth_unstable; fine for a one-shot call at global-map
/// resolution. NaN triggers panic via `total_cmp` ordering assumptions.
fn quantile(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut buf: Vec<f32> = values.to_vec();
    let idx = ((q * buf.len() as f32) as usize).min(buf.len() - 1);
    *buf.select_nth_unstable_by(idx, f32::total_cmp).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(res: u32, sea_ratio: f32) -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF,
            world_size_m: 4096,
            global_res: res,
            sea_ratio,
            mountain_ratio: 0.2,
            continent_frequency: 1.0 / 64.0,
            continent_octaves: 5,
            continent_gain: 0.55,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            sea_channel_strength: 0.0,
            sea_channel_wavelength: 1000.0,
            max_isthmus_width_cells: 0,
            continent_seed_count: 6,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 3,
            continent_gap_cells: 0,
            small_island_count: 0,
            small_island_radius_cells: 0,
            small_island_min_clearance_cells: 0,
            max_elevation_m: 2500.0,
            base_elevation_m: 600.0,
            mountain_amplitude_m: 1500.0,
            plain_amplitude_m: 150.0,
            mountain_selector_wavelength_cells: 64.0,
            detail_wavelength_cells: 16.0,
            y_border_wall_cells: 0,
            y_border_wall_height_m: 0.0,
            erosion_droplet_count: 0,
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

    #[test]
    fn isthmus_cut_preserves_thick_land() {
        // A 30x30 solid land block. No cell has sea within radius 5 on any
        // side. Nothing should be cut.
        let res = 30usize;
        let mut mask = vec![1u8; res * res];
        cut_isthmuses(&mut mask, res, 5);
        let land_count: u32 = mask.iter().map(|&b| b as u32).sum();
        assert_eq!(land_count, (res * res) as u32);
    }

    #[test]
    fn isthmus_cut_handles_narrow_vertical_neck() {
        // Land everywhere except sea bands on the left and right middle. The
        // central column of land between them is a vertical neck. With
        // radius covering the sea distance, the neck middle cells should be
        // cut.
        let res = 20usize;
        let mut mask = vec![1u8; res * res];
        // Sea band on left at x=0..3, y=8..12.
        for y in 8..12 {
            for x in 0..3 {
                mask[y * res + x] = 0;
            }
        }
        // Sea band on right at x=17..20, y=8..12.
        for y in 8..12 {
            for x in 17..20 {
                mask[y * res + x] = 0;
            }
        }
        cut_isthmuses(&mut mask, res, 10);
        // Middle cell at (10, 10) has sea 8 cells left and 8 cells right —
        // within radius 10 on both sides. Should be cut.
        assert_eq!(mask[10 * res + 10], 0);
        // Cell at y=0, x=10 has no sea directly above/below within 10 cells,
        // and is far from left/right seas only horizontally (dist 10 on each
        // side). 10 <= 10 so this is cut too — acceptable.
    }

    #[test]
    fn opening_cuts_thin_bridge_between_thick_lands() {
        // Two 6x6 land blocks connected by a 1-cell-wide bridge. Opening
        // with radius 1 should cut the bridge while leaving both blocks.
        let res = 24usize;
        let mut mask = vec![0u8; res * res];
        // Left block 2..8, 2..8.
        for y in 2..8 {
            for x in 2..8 {
                mask[y * res + x] = 1;
            }
        }
        // Right block 16..22, 2..8.
        for y in 2..8 {
            for x in 16..22 {
                mask[y * res + x] = 1;
            }
        }
        // 1-cell bridge along y=5 from x=8..16.
        for x in 8..16 {
            mask[5 * res + x] = 1;
        }
        binary_open(&mut mask, res, 1);
        // Bridge cut.
        for x in 8..16 {
            assert_eq!(mask[5 * res + x], 0, "bridge cell at x={x} should be cut");
        }
        // Block interiors survive.
        assert_eq!(mask[4 * res + 4], 1, "left block interior should survive");
        assert_eq!(mask[4 * res + 19], 1, "right block interior should survive");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(128, 0.3);
        let a = generate_continent_mask(&cfg);
        let b = generate_continent_mask(&cfg);
        assert_eq!(a.continent_potential, b.continent_potential);
        assert_eq!(a.land_mask, b.land_mask);
        assert_eq!(a.sea_level_potential, b.sea_level_potential);
    }

    #[test]
    fn different_seed_produces_different_mask() {
        let mut cfg = test_config(128, 0.3);
        let a = generate_continent_mask(&cfg);
        cfg.seed = 0xF00D;
        let b = generate_continent_mask(&cfg);
        assert_ne!(a.land_mask, b.land_mask);
    }

    #[test]
    fn measured_sea_ratio_within_tolerance() {
        // Growth + top-N trimming makes the exact sea ratio a soft target.
        // Tolerance of 0.15 absolute accounts for component trimming.
        for target in [0.3, 0.4, 0.5] {
            let cfg = test_config(128, target);
            let m = generate_continent_mask(&cfg);
            let measured = m.measured_sea_ratio();
            assert!(
                (measured - target).abs() < 0.15,
                "target sea {target}, measured sea {measured}"
            );
        }
    }

    #[test]
    fn potential_is_x_periodic() {
        // The fundamental wrap guarantee: sampling at x=0 and x=res (i.e.
        // exactly one world-width further) must give the same potential, so
        // the world seamlessly connects east-to-west. We test this via the
        // noise function directly since the stored grid only covers
        // [0, res-1]; x=res would be written as x=0.
        use super::super::noise::{fbm_wrap_x, PerlinNoise3D};
        let cfg = test_config(64, 0.4);
        let noise = PerlinNoise3D::new(cfg.seed ^ 0xC0_0_C0_0_C0_0_C0_0_u64);
        let world_width = cfg.global_res as f32;
        for y in 0..cfg.global_res {
            let a = fbm_wrap_x(
                &noise,
                0.0,
                y as f32,
                world_width,
                cfg.continent_frequency,
                cfg.continent_octaves,
                CONTINENT_LACUNARITY,
                cfg.continent_gain,
            );
            let b = fbm_wrap_x(
                &noise,
                world_width,
                y as f32,
                world_width,
                cfg.continent_frequency,
                cfg.continent_octaves,
                CONTINENT_LACUNARITY,
                cfg.continent_gain,
            );
            assert!((a - b).abs() < 1e-5, "wrap mismatch at y={y}: {a} vs {b}");
        }
    }

    #[test]
    fn flood_fill_wraps_x() {
        // A single land stripe that touches both x=0 and x=res-1 should be
        // one component under X-wrap flood fill. With min_cells larger than
        // half the stripe, non-wrapping fill would split it in two small
        // parts and delete both; wrapping fill keeps the whole stripe.
        let res = 10usize;
        let mut mask = vec![0u8; res * res];
        for x in 0..res {
            mask[5 * res + x] = 1;
        }
        remove_small_islands(&mut mask, res, 6);
        let surviving: u32 = mask[5 * res..5 * res + res].iter().map(|&b| b as u32).sum();
        assert_eq!(surviving, res as u32, "X-wrap stripe should survive intact");
    }

    #[test]
    fn small_island_removal_drops_tiny_components() {
        let res = 10usize;
        let mut mask = vec![0u8; res * res];
        // Large 5x5 component.
        for y in 0..5 {
            for x in 0..5 {
                mask[y * res + x] = 1;
            }
        }
        // Tiny 2-cell island, far from borders so X-wrap can't accidentally
        // merge it with the large component.
        mask[res * 7 + 7] = 1;
        mask[res * 7 + 8] = 1;
        remove_small_islands(&mut mask, res, 5);
        let mut big_sum: u32 = 0;
        for y in 0..5 {
            for x in 0..5 {
                big_sum += mask[y * res + x] as u32;
            }
        }
        assert_eq!(big_sum, 25, "large component should survive");
        assert_eq!(mask[res * 7 + 7], 0, "tiny island should be removed");
        assert_eq!(mask[res * 7 + 8], 0, "tiny island should be removed");
    }

    #[test]
    fn mask_values_are_binary() {
        let cfg = test_config(64, 0.4);
        let m = generate_continent_mask(&cfg);
        for &b in &m.land_mask {
            assert!(b == 0 || b == 1, "non-binary mask value {b}");
        }
    }

    #[test]
    fn config_cell_count_matches_mask_length() {
        let cfg = test_config(96, 0.25);
        let m = generate_continent_mask(&cfg);
        assert_eq!(m.land_mask.len(), cfg.cell_count());
        assert_eq!(m.continent_potential.len(), cfg.cell_count());
    }
}
