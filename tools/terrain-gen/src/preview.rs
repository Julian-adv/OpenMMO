//! `preview` command: generate the global map up to the currently-implemented
//! phase and write PNGs for visual inspection.

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use onlinerpg_shared::worldgen::{
    continent, elevation, erosion, rivers, GlobalMap, WorldGenConfig,
};
use std::path::Path;
use std::time::Instant;

pub fn run(config: &WorldGenConfig, out_root: &Path) -> Result<()> {
    let seed_dir = out_root.join(format!("{:016x}", config.seed));
    std::fs::create_dir_all(&seed_dir)
        .with_context(|| format!("failed to create {}", seed_dir.display()))?;

    eprintln!(
        "Generating {}×{} global map (seed={:#x}, sea_ratio={:.2})…",
        config.global_res, config.global_res, config.seed, config.sea_ratio
    );

    // --- Phase 1: continent / sea mask --------------------------------------
    let t0 = Instant::now();
    let mut map = continent::generate_continent_mask(config);
    eprintln!(
        "Phase 1 (continent mask): {:.2}s  measured sea = {:.3}",
        t0.elapsed().as_secs_f32(),
        map.measured_sea_ratio()
    );

    // --- Phase 2: elevation -------------------------------------------------
    let t_ph2 = Instant::now();
    elevation::generate_elevation(&mut map);
    let max_elev = map
        .elevation_m
        .iter()
        .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    eprintln!(
        "Phase 2 (elevation):      {:.2}s  max = {:.0}m",
        t_ph2.elapsed().as_secs_f32(),
        max_elev
    );

    // --- Phase 3: hydraulic erosion -----------------------------------------
    if config.erosion_droplet_count > 0 {
        let t_ph3 = Instant::now();
        erosion::erode_hydraulic(&mut map);
        let max_post = map
            .elevation_m
            .iter()
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        eprintln!(
            "Phase 3 (erosion):        {:.2}s  {} droplets, max = {:.0}m",
            t_ph3.elapsed().as_secs_f32(),
            config.erosion_droplet_count,
            max_post
        );
    }

    // --- Phase 4: flow accumulation + river extraction ----------------------
    let t_ph4 = Instant::now();
    let mut river_map = rivers::compute_flow(&map);
    // Peak-based extraction: rivers start at elevation local maxima above
    // 40% of max elevation (so they originate in real mountains). Each
    // peak traces downstream; tributaries branch off and merge visibly.
    let min_peak = config.max_elevation_m * 0.4;
    let min_length = 20usize;
    rivers::extract_rivers(&map, &mut river_map, min_peak, min_length);
    let max_flow = river_map.flow.iter().cloned().fold(0.0f32, f32::max);
    eprintln!(
        "Phase 4 (rivers):         {:.2}s  {} rivers (peaks ≥ {:.0}m), max flow = {:.0}",
        t_ph4.elapsed().as_secs_f32(),
        river_map.rivers.len(),
        min_peak,
        max_flow
    );

    let t1 = Instant::now();
    // Coast distance field: used by the land/sea previews so that sand
    // appears only at the actual coastline (not wherever the independent
    // potential noise happens to be low).
    let coast_dist = coast_distance(&map.land_mask, config.global_res as usize);
    write_potential_png(&map, &seed_dir.join("01_potential.png"))?;
    write_land_sea_png(&map, &coast_dist, &seed_dir.join("01_land_sea.png"))?;
    write_land_sea_shifted_png(&map, &coast_dist, &seed_dir.join("01_land_sea_shifted.png"))?;
    write_elevation_grayscale_png(&map, &seed_dir.join("02_elevation.png"))?;
    write_elevation_hypso_png(&map, &seed_dir.join("02_elevation_hypso.png"))?;
    write_rivers_png(&map, &river_map, &seed_dir.join("03_rivers.png"))?;
    eprintln!("  wrote PNGs: {:.2}s", t1.elapsed().as_secs_f32());

    // --- Meta ---------------------------------------------------------------
    let meta = serde_json::json!({
        "config": {
            "seed": config.seed,
            "world_size_m": config.world_size_m,
            "global_res": config.global_res,
            "sea_ratio": config.sea_ratio,
            "mountain_ratio": config.mountain_ratio,
            "continent_frequency": config.continent_frequency,
            "continent_octaves": config.continent_octaves,
            "continent_gain": config.continent_gain,
            "min_island_cells": config.min_island_cells,
        },
        "measured": {
            "sea_ratio": map.measured_sea_ratio(),
            "sea_level_potential": map.sea_level_potential,
        },
    });
    std::fs::write(
        seed_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    eprintln!("Wrote preview to {}", seed_dir.display());
    Ok(())
}

/// Hypso map with extracted river polylines drawn on top in bright blue.
/// Line width scales with flow accumulation at the polyline's mouth so big
/// rivers read thicker than small streams.
fn write_rivers_png(
    map: &GlobalMap,
    river_map: &rivers::RiverMap,
    path: &Path,
) -> anyhow::Result<()> {
    let n = map.config.global_res as usize;
    let max_h = map.config.max_elevation_m.max(1.0);
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    // Base: muted hypso so rivers pop.
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let px = if map.land_mask[i] == 0 {
                Rgb([35, 80, 140])
            } else {
                let c = hypso_color(map.elevation_m[i] / max_h);
                // Desaturate slightly so the blue rivers stand out.
                Rgb([
                    ((c.0[0] as u16 * 3 + 128) / 4) as u8,
                    ((c.0[1] as u16 * 3 + 128) / 4) as u8,
                    ((c.0[2] as u16 * 3 + 128) / 4) as u8,
                ])
            };
            img.put_pixel(x as u32, y as u32, px);
        }
    }

    // Overlay: render each polyline with thickness proportional to the log
    // of its flow at the mouth. River color is a bright blue distinct from
    // sea.
    for poly in &river_map.rivers {
        if poly.points.is_empty() {
            continue;
        }
        // Use flow at the last point (mouth) to set thickness.
        let mouth = *poly.points.last().unwrap();
        let mouth_idx = (mouth.1 as usize) * n + (mouth.0 as usize);
        let f = river_map.flow[mouth_idx];
        let thickness = (f.ln().max(1.0) * 0.6) as i32;
        let thickness = thickness.clamp(1, 4);
        for &(x, y) in &poly.points {
            stamp_disk(
                &mut img,
                n,
                x as i32,
                y as i32,
                thickness,
                Rgb([80, 160, 240]),
            );
        }
    }

    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Paint a filled disk of `radius` cells at (cx, cy), wrapping X, clamping Y.
fn stamp_disk(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Rgb<u8>,
) {
    let r2 = radius * radius;
    for dy in -radius..=radius {
        let py = cy + dy;
        if py < 0 || py >= n as i32 {
            continue;
        }
        for dx in -radius..=radius {
            if dx * dx + dy * dy > r2 {
                continue;
            }
            let px = (cx + dx).rem_euclid(n as i32) as u32;
            img.put_pixel(px, py as u32, color);
        }
    }
}

/// Grayscale heightmap: black = sea level / 0m, white = `max_elevation_m`.
fn write_elevation_grayscale_png(map: &GlobalMap, path: &Path) -> Result<()> {
    let n = map.config.global_res;
    let max = map.config.max_elevation_m.max(1.0);
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let v = map.elevation_m[(y * n + x) as usize];
            let t = (v / max).clamp(0.0, 1.0);
            let g = (t * 255.0) as u8;
            img.put_pixel(x, y, Rgb([g, g, g]));
        }
    }
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Hypsometric tint: deep blue → light blue (sea) → sand → green → brown →
/// white (mountain peaks). Makes the elevation distribution easy to read.
fn write_elevation_hypso_png(map: &GlobalMap, path: &Path) -> Result<()> {
    let n = map.config.global_res as usize;
    let max = map.config.max_elevation_m.max(1.0);
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let px = if map.land_mask[i] == 0 {
                Rgb([40, 85, 155])
            } else {
                hypso_color(map.elevation_m[i] / max)
            };
            img.put_pixel(x as u32, y as u32, px);
        }
    }
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn hypso_color(t: f32) -> Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
    // Stops: (height_fraction, r, g, b). Sand band is intentionally narrow
    // (0 → 2% of max elevation = 0-50m at 2500m cap) so it reads as a
    // coastline, not a wide beach.
    let stops: [(f32, u8, u8, u8); 7] = [
        (0.00, 210, 200, 150), // sand at exact coast
        (0.02, 140, 175, 100), // quickly into lowland green
        (0.25, 95, 140, 75),   // upland green (plains plateau)
        (0.40, 150, 125, 75),  // foothill brown — mountain onset
        (0.65, 140, 110, 85),  // mountain brown
        (0.85, 200, 190, 180), // rocky slopes
        (1.00, 250, 250, 250), // snowy peaks
    ];
    for i in 0..stops.len() - 1 {
        let (t0, r0, g0, b0) = stops[i];
        let (t1, r1, g1, b1) = stops[i + 1];
        if t <= t1 {
            let s = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return Rgb([lerp_u8(r0, r1, s), lerp_u8(g0, g1, s), lerp_u8(b0, b1, s)]);
        }
    }
    Rgb([255, 255, 255])
}

/// Grayscale map of the raw continent potential field. Min → black, max → white.
fn write_potential_png(map: &GlobalMap, path: &Path) -> Result<()> {
    let n = map.config.global_res;
    let (mn, mx) = min_max(&map.continent_potential);
    let range = (mx - mn).max(1e-6);

    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let v = map.continent_potential[(y * n + x) as usize];
            let t = ((v - mn) / range).clamp(0.0, 1.0);
            let g = (t * 255.0) as u8;
            img.put_pixel(x, y, Rgb([g, g, g]));
        }
    }
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Horizontally-shifted version of the land/sea map: the right half of the
/// map is moved to the left. If the X-wrap is working, the resulting image
/// has its seam *inside* (where the original left/right edges used to be),
/// so any discontinuity at the original wrap boundary becomes visible as a
/// line down the middle. A clean output = seamless wrap.
fn write_land_sea_shifted_png(map: &GlobalMap, coast_dist: &[u16], path: &Path) -> Result<()> {
    let n = map.config.global_res as usize;
    let half = n / 2;

    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    for y in 0..n {
        for x in 0..n {
            let src_x = (x + half) % n;
            let i = y * n + src_x;
            let px = shade_cell(map, coast_dist, i);
            img.put_pixel(x as u32, y as u32, px);
        }
    }
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Stylized land/sea map shaded by distance to coast — sand at the shoreline
/// only, green through brown with distance inland, deep blue at open sea.
fn write_land_sea_png(map: &GlobalMap, coast_dist: &[u16], path: &Path) -> Result<()> {
    let n = map.config.global_res;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) as usize;
            img.put_pixel(x, y, shade_cell(map, coast_dist, i));
        }
    }
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Pick a color for cell `i` given the land mask and a coast-distance field.
/// Sand appears only in a narrow band at the coast; inland hues are driven
/// by combined coast-distance + noise for some variation.
fn shade_cell(map: &GlobalMap, coast_dist: &[u16], i: usize) -> Rgb<u8> {
    if map.land_mask[i] == 0 {
        // Sea: shade by distance from coast (continental shelf → deep ocean).
        let d = coast_dist[i] as f32;
        let depth = (d / 120.0).clamp(0.0, 1.0);
        sea_color(depth)
    } else {
        // Land: shade by distance from coast — sand at the shoreline, broad
        // green interior, tan in the deepest inland regions.
        let d = coast_dist[i] as f32;
        let elev = (d / 500.0).clamp(0.0, 1.0);
        land_color(elev)
    }
}

/// depth: 0 = shoreline, 1 = deepest. Light blue → navy.
fn sea_color(depth: f32) -> Rgb<u8> {
    let t = depth.clamp(0.0, 1.0);
    let r = lerp_u8(110, 20, t);
    let g = lerp_u8(180, 40, t);
    let b = lerp_u8(220, 90, t);
    Rgb([r, g, b])
}

/// height: 0 = shoreline, 1 = highest land. Green → tan gradient; sand band
/// intentionally narrow so it reads as a coastline line rather than a beach.
fn land_color(height: f32) -> Rgb<u8> {
    let t = height.clamp(0.0, 1.0);
    if t < 0.02 {
        // Narrow sand line at the coast.
        Rgb([210, 195, 150])
    } else if t < 0.5 {
        // Lowland green — covers the bulk of a continent's width.
        let s = (t - 0.02) / (0.5 - 0.02);
        Rgb([
            lerp_u8(120, 150, s),
            lerp_u8(165, 145, s),
            lerp_u8(95, 85, s),
        ])
    } else {
        // Upland toward tan/brown for deep-interior cells.
        let s = (t - 0.5) / (1.0 - 0.5);
        Rgb([
            lerp_u8(150, 200, s),
            lerp_u8(145, 180, s),
            lerp_u8(85, 160, s),
        ])
    }
}

/// Multi-source BFS distance field: distance (in cells) from each cell to
/// the nearest cell of the *opposite* type (sea→land coast = distance to
/// nearest land; land→coast = distance to nearest sea). X wraps; Y doesn't.
/// The returned Vec contains the same value for sea and land cells — for
/// sea cells it's distance-to-nearest-land, for land it's distance-to-sea.
/// Capped at u16 max for memory compactness.
fn coast_distance(land_mask: &[u8], res: usize) -> Vec<u16> {
    use std::collections::VecDeque;
    let total = res * res;
    let mut dist = vec![u16::MAX; total];
    let mut queue: VecDeque<usize> = VecDeque::new();
    // Initialize: every boundary cell (land adjacent to sea or vice versa)
    // sits at distance 0 of its own side's coast-distance.
    for i in 0..total {
        let x = i % res;
        let y = i / res;
        let here = land_mask[i];
        let left = if x == 0 { res - 1 } else { x - 1 };
        let right = if x + 1 == res { 0 } else { x + 1 };
        let mut touches_opposite = false;
        for &n in &[
            Some(y * res + left),
            Some(y * res + right),
            if y > 0 { Some((y - 1) * res + x) } else { None },
            if y + 1 < res {
                Some((y + 1) * res + x)
            } else {
                None
            },
        ] {
            if let Some(n) = n {
                if land_mask[n] != here {
                    touches_opposite = true;
                    break;
                }
            }
        }
        if touches_opposite {
            dist[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let d = dist[i];
        let x = i % res;
        let y = i / res;
        let here = land_mask[i];
        let left = if x == 0 { res - 1 } else { x - 1 };
        let right = if x + 1 == res { 0 } else { x + 1 };
        for &n in &[
            Some(y * res + left),
            Some(y * res + right),
            if y > 0 { Some((y - 1) * res + x) } else { None },
            if y + 1 < res {
                Some((y + 1) * res + x)
            } else {
                None
            },
        ] {
            if let Some(n) = n {
                if land_mask[n] == here && dist[n] > d.saturating_add(1) {
                    dist[n] = d.saturating_add(1);
                    queue.push_back(n);
                }
            }
        }
    }
    dist
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

fn min_max(values: &[f32]) -> (f32, f32) {
    let mut mn = f32::INFINITY;
    let mut mx = f32::NEG_INFINITY;
    for &v in values {
        if v < mn {
            mn = v;
        }
        if v > mx {
            mx = v;
        }
    }
    (mn, mx)
}
