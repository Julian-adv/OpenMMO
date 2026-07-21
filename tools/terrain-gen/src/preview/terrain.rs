//! Pixel-shading PNGs of the underlying terrain fields: elevation
//! (grayscale + hypsometric tint), the raw continent-potential noise, and
//! the stylised land/sea map shaded by distance to the coast. The
//! coast-distance BFS that the land/sea images consume lives here too —
//! it's only used by these renderers and the orchestrator that calls them.

use anyhow::Result;
use image::{ImageBuffer, Rgb};
use onlinerpg_shared::worldgen::GlobalMap;
use std::path::Path;

use std::convert::identity;

use super::canvas::{finish_png, lerp_u8, paint_hypso_bg};

/// Grayscale heightmap: black = sea level / 0m, white = `max_elevation_m`.
pub(super) fn write_elevation_grayscale_png(map: &GlobalMap, path: &Path) -> Result<()> {
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
    finish_png(&mut img, map, path)?;
    Ok(())
}

/// Hypsometric tint: deep blue → light blue (sea) → sand → green → brown →
/// white (mountain peaks). Makes the elevation distribution easy to read.
/// Reuses the shared `paint_hypso_bg` so the per-pixel land color comes
/// from the cache built once per `write_pngs` call; passes `identity` for
/// `dim_land` since this writer wants the raw hypso tint, no dimming.
pub(super) fn write_elevation_hypso_png(
    map: &GlobalMap,
    hypso_cache: &[Rgb<u8>],
    path: &Path,
) -> Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, hypso_cache, Rgb([40, 85, 155]), identity);
    finish_png(&mut img, map, path)?;
    Ok(())
}

/// Grayscale map of the raw continent potential field. Min → black, max → white.
pub(super) fn write_potential_png(map: &GlobalMap, path: &Path) -> Result<()> {
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
    finish_png(&mut img, map, path)?;
    Ok(())
}

/// Half-world-shifted version of the land/sea map on both axes. If the
/// torus wrap is working, the resulting image has both seams *inside*
/// (where the original edges used to be), so any discontinuity at either
/// wrap boundary becomes visible as a line through the middle — a cross
/// of artifacts marks a broken wrap. A clean output = seamless torus.
pub(super) fn write_land_sea_shifted_png(
    map: &GlobalMap,
    coast_dist: &[u16],
    path: &Path,
) -> Result<()> {
    let n = map.config.global_res as usize;
    let half = n / 2;

    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    for y in 0..n {
        let src_y = (y + half) % n;
        for x in 0..n {
            let src_x = (x + half) % n;
            let i = src_y * n + src_x;
            let px = shade_cell(map, coast_dist, i);
            img.put_pixel(x as u32, y as u32, px);
        }
    }
    finish_png(&mut img, map, path)?;
    Ok(())
}

/// Stylized land/sea map shaded by distance to coast — sand at the shoreline
/// only, green through brown with distance inland, deep blue at open sea.
pub(super) fn write_land_sea_png(map: &GlobalMap, coast_dist: &[u16], path: &Path) -> Result<()> {
    let n = map.config.global_res;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) as usize;
            img.put_pixel(x, y, shade_cell(map, coast_dist, i));
        }
    }
    finish_png(&mut img, map, path)?;
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
/// nearest land; land→coast = distance to nearest sea). Both axes wrap.
/// The returned Vec contains the same value for sea and land cells — for
/// sea cells it's distance-to-nearest-land, for land it's distance-to-sea.
/// Capped at u16 max for memory compactness.
pub(super) fn coast_distance(land_mask: &[u8], res: usize) -> Vec<u16> {
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
        let up = if y == 0 { res - 1 } else { y - 1 };
        let down = if y + 1 == res { 0 } else { y + 1 };
        let mut touches_opposite = false;
        for &n in &[
            y * res + left,
            y * res + right,
            up * res + x,
            down * res + x,
        ] {
            if land_mask[n] != here {
                touches_opposite = true;
                break;
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
        let up = if y == 0 { res - 1 } else { y - 1 };
        let down = if y + 1 == res { 0 } else { y + 1 };
        for &n in &[
            y * res + left,
            y * res + right,
            up * res + x,
            down * res + x,
        ] {
            if land_mask[n] == here && dist[n] > d.saturating_add(1) {
                dist[n] = d.saturating_add(1);
                queue.push_back(n);
            }
        }
    }
    dist
}

fn min_max(values: &[f32]) -> (f32, f32) {
    values
        .iter()
        .copied()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(mn, mx), v| {
            (mn.min(v), mx.max(v))
        })
}
