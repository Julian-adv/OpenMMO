//! Seeded continent generation via domain-warped Voronoi.
//!
//! Algorithm:
//!   1. Scatter K seed points across the map (Poisson-disk-style rejection
//!      for minimum spacing; X wraps).
//!   2. For each cell, perturb its coordinates with a 2-component noise
//!      warp (fBm_x, fBm_y), then find the nearest seed to the warped point
//!      (X-wrapped distance). This gives each seed an organic-shaped
//!      "territory" instead of a straight-edged Voronoi cell.
//!   3. Record each cell's distance to its assigned seed.
//!   4. Threshold at the (1 - sea_ratio) quantile: cells closer than the
//!      threshold become land. Distant cells (between seed territories)
//!      become sea, producing natural gaps between continents.
//!   5. Connected-component analysis on the resulting mask; keep only the
//!      top-N largest landmasses to enforce the target continent count.
//!
//! The warp's noise fields give continents irregular, organic shapes that
//! don't look like perfect disks. Seeds close together merge into one
//! continent at step 5; seeds far apart remain separated.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use super::config::WorldGenConfig;
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_x, PerlinNoise3D};

/// Warp noise parameters — tuned so the warped-Voronoi boundaries look
/// organic without being chaotic.
const WARP_WAVELENGTH: f32 = 500.0;
const WARP_STRENGTH: f32 = 350.0;
const WARP_OCTAVES: u32 = 4;

/// Run the warped-Voronoi continent generator and return a binary land mask.
pub fn growth_mask(config: &WorldGenConfig) -> Vec<u8> {
    let res = config.global_res as usize;
    let total = res * res;
    let world_width = res as f32;

    let mut rng = SmallRng::seed_from_u64(config.seed ^ 0x60_0_60_0_60_0_60_0_u64);
    let seeds = place_seeds(
        &mut rng,
        res,
        config.continent_seed_count.max(1) as usize,
        config.continent_seed_min_distance_cells as usize,
    );
    if seeds.is_empty() {
        return vec![0; total];
    }

    // Cluster seeds into N groups — each group becomes one final continent.
    // Inter-group boundaries are forced to sea so continents can't merge.
    let n_groups = (config.target_continent_count.max(1) as usize).min(seeds.len());
    let seed_group = cluster_seeds(&seeds, n_groups, world_width, &mut rng);

    let noise_x = PerlinNoise3D::new(config.seed ^ 0x7A_A_7A_A_7A_A_7A_A_u64);
    let noise_y = PerlinNoise3D::new(config.seed ^ 0x7B_B_7B_B_7B_B_7B_B_u64);
    let warp_freq = 1.0 / WARP_WAVELENGTH;

    // For each cell: warp coords, compute best distance to each group.
    // `dist_sq[i]` = distance² to nearest seed (any group).
    // `gap_margin[i]` = sqrt(2nd-best-group) − sqrt(best-group), used to
    //                  enforce the inter-group sea gap. Only allocated when
    //                  a gap is configured — it's the largest buffer here.
    let gap = config.continent_gap_cells as f32;
    let needs_gap = n_groups > 1 && gap > 0.0;
    let mut dist_sq = vec![0.0f32; total];
    let mut gap_margin: Vec<f32> = if needs_gap {
        vec![0.0; total]
    } else {
        Vec::new()
    };
    let mut best_group_d = vec![f32::INFINITY; n_groups];

    for y in 0..res {
        for x in 0..res {
            let wx_off = fbm_wrap_x(
                &noise_x,
                x as f32,
                y as f32,
                world_width,
                warp_freq,
                WARP_OCTAVES,
                2.0,
                0.5,
            ) * WARP_STRENGTH;
            let wy_off = fbm_wrap_x(
                &noise_y,
                x as f32,
                y as f32,
                world_width,
                warp_freq,
                WARP_OCTAVES,
                2.0,
                0.5,
            ) * WARP_STRENGTH;
            let wx = x as f32 + wx_off;
            let wy = y as f32 + wy_off;

            for g in 0..n_groups {
                best_group_d[g] = f32::INFINITY;
            }
            for (id, &(sx, sy)) in seeds.iter().enumerate() {
                let dx_raw = (wx - sx as f32).abs();
                let dx = dx_raw.min(world_width - dx_raw);
                let dy = wy - sy as f32;
                let d = dx * dx + dy * dy;
                let g = seed_group[id] as usize;
                if d < best_group_d[g] {
                    best_group_d[g] = d;
                }
            }
            let mut best = f32::INFINITY;
            let mut second = f32::INFINITY;
            for &d in &best_group_d {
                if d < best {
                    second = best;
                    best = d;
                } else if d < second {
                    second = d;
                }
            }
            let i = y * res + x;
            dist_sq[i] = best;
            if needs_gap {
                gap_margin[i] = if second.is_finite() {
                    second.sqrt() - best.sqrt()
                } else {
                    f32::INFINITY
                };
            }
        }
    }

    // Threshold: target the configured land fraction using distance to any
    // seed. Gap cells (near inter-group boundaries) will later be forced to
    // sea; we budget for that by using sqrt distance for the quantile.
    let target_land_frac = (1.0 - config.sea_ratio.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let idx = ((target_land_frac * total as f32) as usize).min(total.saturating_sub(1));
    let mut sorted = dist_sq.clone();
    let (_, threshold_ref, _) = sorted.select_nth_unstable_by(idx, f32::total_cmp);
    let threshold = *threshold_ref;

    let mut mask = vec![0u8; total];
    for i in 0..total {
        if dist_sq[i] > threshold {
            continue;
        }
        if needs_gap && gap_margin[i] < gap {
            continue;
        }
        mask[i] = 1;
    }

    // Safety net: the mask may still have >1 component within a single
    // group (rare, from noise warp creating isolated peninsulas). Keep the
    // top-N overall so stray small components don't clutter the result.
    keep_top_components(
        &mut mask,
        res,
        config.target_continent_count.max(1) as usize,
    );

    // Scatter small islands in open sea (after top-N so they're not culled).
    scatter_small_islands(
        &mut mask,
        res,
        config.small_island_count as usize,
        config.small_island_radius_cells as f32,
        config.small_island_min_clearance_cells as usize,
        &noise_x,
        world_width,
        &mut rng,
    );

    mask
}

/// Place `count` small noisy-circle islands in sea cells that are far enough
/// from any existing land. Each island's radius is randomized around
/// `mean_radius`; its edge is perturbed by the supplied noise field so it
/// doesn't look like a perfect disk.
#[allow(clippy::too_many_arguments)]
fn scatter_small_islands(
    mask: &mut [u8],
    res: usize,
    count: usize,
    mean_radius: f32,
    min_clearance: usize,
    noise: &PerlinNoise3D,
    world_width: f32,
    rng: &mut SmallRng,
) {
    if count == 0 || mean_radius <= 0.0 {
        return;
    }

    // Distance field: for every sea cell, BFS distance (in cells) to the
    // nearest land cell. Used to reject candidate centers that are too
    // close to continents. O(N) total.
    let dist_to_land = bfs_distance_from(mask, res, 1);

    // Each placed island's "claim radius" (center + radius + clearance) must
    // not overlap any other placed island's claim. Track placements.
    let mut placed: Vec<(usize, usize, f32)> = Vec::with_capacity(count);
    let mut placed_count = 0usize;
    let mut attempts = 0usize;
    let max_attempts = count * 500;

    while placed_count < count && attempts < max_attempts {
        attempts += 1;
        let cx = rng.gen_range(0..res);
        let cy = rng.gen_range(0..res);
        let center_idx = cy * res + cx;
        if mask[center_idx] != 0 {
            continue;
        }

        // Randomized radius around the mean (0.5× to 1.5×).
        let radius = mean_radius * rng.gen_range(0.5..1.5);
        let total_clearance = radius + min_clearance as f32;

        // Reject if a continent is too close.
        if (dist_to_land[center_idx] as f32) < total_clearance {
            continue;
        }
        // Reject if an already-placed island is too close.
        let too_close_to_other = placed.iter().any(|&(px, py, pr)| {
            let dx_raw = (px as f32 - cx as f32).abs();
            let dx = dx_raw.min(world_width - dx_raw);
            let dy = py as f32 - cy as f32;
            let d = (dx * dx + dy * dy).sqrt();
            d < pr + radius + min_clearance as f32
        });
        if too_close_to_other {
            continue;
        }

        // Carve the island. Edge is perturbed by noise so it doesn't look
        // like a stamped circle.
        let noise_scale = 1.0 / (radius * 1.2).max(1.0);
        let noise_strength = radius * 0.35;
        let bb = (radius + noise_strength + 2.0).ceil() as i32;
        for dy in -bb..=bb {
            let ny = cy as i32 + dy;
            if ny < 0 || ny >= res as i32 {
                continue;
            }
            for dx in -bb..=bb {
                let nx = (cx as i32 + dx).rem_euclid(res as i32) as usize;
                let dist_from_center = ((dx * dx + dy * dy) as f32).sqrt();
                let n = noise.sample(
                    (cx as f32 + dx as f32) * noise_scale,
                    (cy as f32 + dy as f32) * noise_scale,
                    0.0,
                );
                let effective_radius = radius + n * noise_strength;
                if dist_from_center < effective_radius {
                    mask[ny as usize * res + nx] = 1;
                }
            }
        }
        placed.push((cx, cy, radius));
        placed_count += 1;
    }
}

/// Simple k-means clustering on 2D seed positions with X-wrap awareness
/// (circular mean in X, regular mean in Y). Returns group id [0, n_groups)
/// for each seed.
fn cluster_seeds(
    seeds: &[(usize, usize)],
    n_groups: usize,
    world_width: f32,
    rng: &mut SmallRng,
) -> Vec<u8> {
    if n_groups <= 1 || seeds.len() <= n_groups {
        // Trivial: each seed is its own group (capped at n_groups).
        return (0..seeds.len())
            .map(|i| i.min(n_groups - 1) as u8)
            .collect();
    }

    // Initialize centroids by picking n distinct seeds at random.
    let mut centroid_indices: Vec<usize> = Vec::with_capacity(n_groups);
    while centroid_indices.len() < n_groups {
        let i = rng.gen_range(0..seeds.len());
        if !centroid_indices.contains(&i) {
            centroid_indices.push(i);
        }
    }
    let mut centroids: Vec<(f32, f32)> = centroid_indices
        .iter()
        .map(|&i| (seeds[i].0 as f32, seeds[i].1 as f32))
        .collect();

    let mut assignment = vec![0u8; seeds.len()];
    for _ in 0..30 {
        // Assign each seed to the closest centroid (X-wrap distance).
        let mut changed = false;
        for (i, &(sx, sy)) in seeds.iter().enumerate() {
            let mut best_d = f32::INFINITY;
            let mut best_c: u8 = 0;
            for (c, &(cx, cy)) in centroids.iter().enumerate() {
                let dx_raw = (sx as f32 - cx).abs();
                let dx = dx_raw.min(world_width - dx_raw);
                let dy = sy as f32 - cy;
                let d = dx * dx + dy * dy;
                if d < best_d {
                    best_d = d;
                    best_c = c as u8;
                }
            }
            if assignment[i] != best_c {
                assignment[i] = best_c;
                changed = true;
            }
        }
        // Update centroids.
        for c in 0..n_groups {
            let members: Vec<&(usize, usize)> = seeds
                .iter()
                .enumerate()
                .filter(|(i, _)| assignment[*i] as usize == c)
                .map(|(_, s)| s)
                .collect();
            if members.is_empty() {
                continue;
            }
            // Circular mean in X (so wrap-adjacent seeds cluster together).
            let (mut sx_sin, mut sx_cos) = (0.0f32, 0.0f32);
            let mut sy_sum = 0.0f32;
            for &&(x, y) in &members {
                let a = 2.0 * std::f32::consts::PI * (x as f32) / world_width;
                sx_sin += a.sin();
                sx_cos += a.cos();
                sy_sum += y as f32;
            }
            let angle = sx_sin.atan2(sx_cos);
            let cx = ((angle / (2.0 * std::f32::consts::PI)) * world_width + world_width)
                .rem_euclid(world_width);
            let cy = sy_sum / members.len() as f32;
            centroids[c] = (cx, cy);
        }
        if !changed {
            break;
        }
    }
    assignment
}

/// Flood-fill connected components (4-connected, X-wrap), keep the `n`
/// largest by cell count, convert the rest to sea.
fn keep_top_components(mask: &mut [u8], res: usize, n: usize) {
    let total = res * res;
    let mut label = vec![0u32; total]; // 0 = unvisited or sea
    let mut sizes: Vec<usize> = vec![0]; // sentinel for label 0
    let mut next_label: u32 = 1;
    let mut stack: Vec<usize> = Vec::with_capacity(1024);

    for start in 0..total {
        if mask[start] == 0 || label[start] != 0 {
            continue;
        }
        stack.clear();
        stack.push(start);
        label[start] = next_label;
        let mut count = 0usize;
        while let Some(i) = stack.pop() {
            count += 1;
            let x = i % res;
            let y = i / res;
            let left = if x == 0 { res - 1 } else { x - 1 };
            let right = if x + 1 == res { 0 } else { x + 1 };
            let cands = [
                Some(y * res + left),
                Some(y * res + right),
                if y > 0 { Some((y - 1) * res + x) } else { None },
                if y + 1 < res {
                    Some((y + 1) * res + x)
                } else {
                    None
                },
            ];
            for c in cands.iter().flatten() {
                if mask[*c] == 1 && label[*c] == 0 {
                    label[*c] = next_label;
                    stack.push(*c);
                }
            }
        }
        sizes.push(count);
        next_label += 1;
    }

    // Decide which labels to keep (labels 1..next_label, sizes[1..]).
    let mut ranked: Vec<(u32, usize)> = (1..next_label).map(|l| (l, sizes[l as usize])).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    let keep: std::collections::HashSet<u32> = ranked.iter().take(n).map(|&(l, _)| l).collect();
    for i in 0..total {
        if mask[i] == 1 && !keep.contains(&label[i]) {
            mask[i] = 0;
        }
    }
}

/// Poisson-disk-ish rejection sampling: place up to `count` seeds each
/// farther than `min_dist` cells (Euclidean with X wrap) from any previously
/// placed seed. Gives up after a bounded number of attempts.
fn place_seeds(
    rng: &mut SmallRng,
    res: usize,
    count: usize,
    min_dist: usize,
) -> Vec<(usize, usize)> {
    let mut seeds: Vec<(usize, usize)> = Vec::with_capacity(count);
    let min_dist_sq = (min_dist as i64) * (min_dist as i64);
    let res_i = res as i64;
    let attempts_per_seed = 200;

    for _ in 0..count {
        let mut placed = false;
        for _ in 0..attempts_per_seed {
            let sx = rng.gen_range(0..res);
            let sy = rng.gen_range(0..res);
            let ok = seeds.iter().all(|&(px, py)| {
                let dx = (sx as i64 - px as i64).abs();
                let dx = dx.min(res_i - dx); // X-wrap
                let dy = sy as i64 - py as i64;
                dx * dx + dy * dy >= min_dist_sq
            });
            if ok {
                seeds.push((sx, sy));
                placed = true;
                break;
            }
        }
        if !placed {
            break;
        }
    }
    seeds
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
            continent_octaves: 4,
            continent_gain: 0.5,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            sea_channel_strength: 0.0,
            sea_channel_wavelength: 1000.0,
            max_isthmus_width_cells: 0,
            continent_seed_count: 5,
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
    fn seeds_respect_min_distance() {
        let mut rng = SmallRng::seed_from_u64(1);
        let seeds = place_seeds(&mut rng, 100, 8, 25);
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                let (ax, ay) = seeds[i];
                let (bx, by) = seeds[j];
                let dx = (ax as i64 - bx as i64)
                    .abs()
                    .min(100 - (ax as i64 - bx as i64).abs());
                let dy = (ay as i64 - by as i64).abs();
                let d2 = dx * dx + dy * dy;
                assert!(
                    d2 >= 25 * 25,
                    "seeds {:?} and {:?} too close: d²={d2}",
                    seeds[i],
                    seeds[j]
                );
            }
        }
    }

    #[test]
    fn growth_produces_at_most_target_components() {
        let cfg = test_config(128, 0.4);
        let mask = growth_mask(&cfg);
        // Count connected components in the mask (4-connected, X-wrap) by
        // flood fill.
        let n = cfg.global_res as usize;
        let mut visited = vec![false; mask.len()];
        let mut components = 0usize;
        for start in 0..mask.len() {
            if mask[start] == 0 || visited[start] {
                continue;
            }
            components += 1;
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                if visited[i] {
                    continue;
                }
                visited[i] = true;
                let x = i % n;
                let y = i / n;
                let left = if x == 0 { n - 1 } else { x - 1 };
                let right = if x + 1 == n { 0 } else { x + 1 };
                for &nb in &[
                    Some(y * n + left),
                    Some(y * n + right),
                    if y > 0 { Some((y - 1) * n + x) } else { None },
                    if y + 1 < n {
                        Some((y + 1) * n + x)
                    } else {
                        None
                    },
                ] {
                    if let Some(nb) = nb {
                        if mask[nb] == 1 && !visited[nb] {
                            stack.push(nb);
                        }
                    }
                }
            }
        }
        assert!(
            components <= cfg.target_continent_count as usize,
            "got {components} components, expected ≤ {}",
            cfg.target_continent_count
        );
        assert!(components >= 1, "expected at least one landmass");
    }

    #[test]
    fn measured_land_ratio_roughly_matches_target() {
        let cfg = test_config(128, 0.4);
        let mask = growth_mask(&cfg);
        let land = mask.iter().filter(|&&b| b == 1).count();
        let total = mask.len();
        let measured_land_ratio = land as f32 / total as f32;
        let target_land_ratio = 1.0 - cfg.sea_ratio;
        // Growth may stop short if frontiers empty; but we also drop non-kept
        // components after growth. Target is a soft bound — accept within
        // 15% of total area (top-N trimming can remove meaningful land).
        let diff = (measured_land_ratio - target_land_ratio).abs();
        assert!(
            diff < 0.15,
            "target {target_land_ratio}, measured {measured_land_ratio}"
        );
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(128, 0.4);
        let a = growth_mask(&cfg);
        let b = growth_mask(&cfg);
        assert_eq!(a, b);
    }
}
