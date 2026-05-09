//! Drawing primitives + the region-grid overlay every preview PNG ends with.
//!
//! Everything here works on a `RgbImage` already sized to `global_res × global_res`
//! and keeps the same conventions as the original monolithic preview module:
//! X wraps (`rem_euclid` on the cell index), Y clamps. The grid overlay is
//! pulled in by `finish_png`, which most renderers call as their last step
//! before saving.

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use onlinerpg_shared::worldgen::GlobalMap;
use std::path::Path;

use super::text::draw_text_with_bg;

/// Draw region-grid lines (1 km spacing) and world-origin axes on top of an
/// already-rendered PNG, then save. Shared wrapper so every preview image
/// gets the same overlay without duplicating save error-context boilerplate.
pub(super) fn finish_png(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    map: &GlobalMap,
    path: &Path,
) -> Result<()> {
    overlay_region_grid(img, map);
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Geometry of the 1 km region grid mapped onto a global-map image. Region 0
/// starts at world −32 m (tile 0 spans [−32, +32) m), so its top/left edge
/// sits half a tile inside the world origin — same convention as
/// `terrain::coords`.
struct RegionGrid {
    res: i32,
    region_px: f32,
    region0_edge: f32,
    origin_cell: f32,
}

fn region_grid(map: &GlobalMap) -> RegionGrid {
    const REGION_SIZE_M: f32 = 1024.0;
    let res = map.config.global_res as i32;
    let mpc = map.config.meters_per_cell();
    let region_px = REGION_SIZE_M / mpc;
    let origin_cell = res as f32 * 0.5;
    let region0_edge = origin_cell - (onlinerpg_terrain::defaults::TILE_DIM as f32 * 0.5) / mpc;
    RegionGrid {
        res,
        region_px,
        region0_edge,
        origin_cell,
    }
}

/// Overlay region-boundary grid (1024 m pitch) plus origin axes on an image
/// the same size as the global map. Every 4th region gets a heavier tint,
/// origin axes are red.
pub(super) fn overlay_region_grid(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, map: &GlobalMap) {
    /// Thick line every N regions (4 km default) so counting is easy on large
    /// maps without having to tick off every 1 km square.
    const MAJOR_STRIDE: i32 = 4;

    let RegionGrid {
        res,
        region_px,
        region0_edge,
        origin_cell,
    } = region_grid(map);
    if region_px < 2.0 {
        // Regions smaller than a couple of pixels would produce a solid
        // tint; just skip the overlay in that extreme low-res case.
        return;
    }
    // Thickness scales with resolution so lines survive typical preview
    // downscaling: at 1024 res = 1 px, at 4096 res = 4 px. Major lines and
    // origin axes double up for extra contrast.
    let base_thick = ((res as f32 / 1024.0).round() as i32).max(1);
    let minor_thick = base_thick;
    let major_thick = base_thick * 2;
    let origin_thick = base_thick * 2;

    let major = Rgb([20, 20, 20]);
    let minor = Rgb([60, 60, 60]);
    let origin = Rgb([230, 60, 60]);

    let max_n = (res as f32 / region_px) as i32 + 2;
    for n in -max_n..=max_n {
        let pos_f = region0_edge + n as f32 * region_px;
        if pos_f < 0.0 || pos_f >= res as f32 {
            continue;
        }
        let pos = pos_f.round() as i32;
        if pos < 0 || pos >= res {
            continue;
        }
        let (color, alpha, thick) = if n.rem_euclid(MAJOR_STRIDE) == 0 {
            (major, 0.55, major_thick)
        } else {
            (minor, 0.3, minor_thick)
        };
        draw_axis_line(img, pos, res, thick, color, alpha, Axis::Vertical);
        draw_axis_line(img, pos, res, thick, color, alpha, Axis::Horizontal);
    }

    let o = origin_cell.round() as i32;
    if (0..res).contains(&o) {
        draw_axis_line(img, o, res, origin_thick, origin, 0.8, Axis::Vertical);
        draw_axis_line(img, o, res, origin_thick, origin, 0.8, Axis::Horizontal);
    }
}

/// Draw region-coordinate labels along the four image edges, e.g. `+0`, `-3`,
/// at the center of each region. Uses the same 5×7 bitmap font as settlement
/// labels so map references in conversation match what's drawn on disks.
pub(super) fn overlay_region_labels(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, map: &GlobalMap) {
    let RegionGrid {
        res,
        region_px,
        region0_edge,
        ..
    } = region_grid(map);
    if region_px < 24.0 {
        return;
    }

    // Match the settlement-label scale derivation so edge labels and disk
    // labels render at the same size on a given preview.
    let outer = (res / 200).max(5);
    let scale = (outer / 7).max(1);
    let char_w = 5 * scale;
    let char_h = 7 * scale;
    let gap = scale;
    let pad = scale;

    let text_color = Rgb([245, 245, 240]);
    let bg_color = Rgb([15, 15, 22]);

    let edge_inset = pad * 2;
    let cy_top = edge_inset + char_h / 2;
    let cy_bot = res - edge_inset - char_h / 2 - 1;
    // Reserve a horizontal slot wide enough for a 3-char label like "+12" so
    // the left/right column sits at a consistent X regardless of the actual
    // visible region range.
    let edge_slot_w = 3 * char_w + 2 * gap;
    let cx_left = edge_inset + edge_slot_w / 2;
    let cx_right = res - edge_inset - edge_slot_w / 2 - 1;

    let n_min = ((-region0_edge) / region_px).floor() as i32 - 1;
    let n_max = ((res as f32 - region0_edge) / region_px).ceil() as i32 + 1;

    let n_usize = res as usize;
    for n in n_min..=n_max {
        let center_f = region0_edge + (n as f32 + 0.5) * region_px;
        if center_f < 0.0 || center_f >= res as f32 {
            continue;
        }
        let center = center_f.round() as i32;
        let label = format!("{:+}", n);
        let mut place = |cx, cy| {
            draw_text_with_bg(
                img, n_usize, cx, cy, &label, text_color, bg_color, scale, pad,
            );
        };

        if center > cx_left + edge_slot_w / 2 + pad && center < cx_right - edge_slot_w / 2 - pad {
            place(center, cy_top);
            place(center, cy_bot);
        }
        if center > cy_top + char_h / 2 + pad && center < cy_bot - char_h / 2 - pad {
            place(cx_left, center);
            place(cx_right, center);
        }
    }
}

#[derive(Copy, Clone)]
enum Axis {
    Vertical,
    Horizontal,
}

/// Draw a straight grid line, `thickness` pixels wide, centered on `pos`.
/// `Axis::Vertical` draws down the column `pos`, `Axis::Horizontal` across
/// the row `pos`.
fn draw_axis_line(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    pos: i32,
    res: i32,
    thickness: i32,
    color: Rgb<u8>,
    alpha: f32,
    axis: Axis,
) {
    let half = thickness / 2;
    for d in -half..(thickness - half) {
        let minor_coord = pos + d;
        if minor_coord < 0 || minor_coord >= res {
            continue;
        }
        for major in 0..res as u32 {
            let (x, y) = match axis {
                Axis::Vertical => (minor_coord as u32, major),
                Axis::Horizontal => (major, minor_coord as u32),
            };
            let px = *img.get_pixel(x, y);
            img.put_pixel(x, y, blend_rgb(px, color, alpha));
        }
    }
}

fn blend_rgb(a: Rgb<u8>, b: Rgb<u8>, t: f32) -> Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
    Rgb([
        lerp_u8(a.0[0], b.0[0], t),
        lerp_u8(a.0[1], b.0[1], t),
        lerp_u8(a.0[2], b.0[2], t),
    ])
}

pub(super) fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

/// Hypsometric tint at normalised height `t ∈ [0, 1]`: deep sand at the
/// coast, lowland green, foothill brown, mountain brown, rock, snow.
/// Stops are tuned so the sand band is intentionally narrow (≈2 % of max
/// elevation) — it reads as a coastline, not a beach.
pub(super) fn hypso_color(t: f32) -> Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
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

/// Multiply each channel by 2/3. Used by the feature PNGs (roads,
/// settlements, coasts) to dim the hypso background so the overlaid
/// vector features stay readable.
pub(super) fn dim_two_thirds(c: Rgb<u8>) -> Rgb<u8> {
    Rgb([
        (c.0[0] as u16 * 2 / 3) as u8,
        (c.0[1] as u16 * 2 / 3) as u8,
        (c.0[2] as u16 * 2 / 3) as u8,
    ])
}

/// Fill `img` with a muted hypsometric-tint background: `sea` fill for water
/// cells, `hypso_color` run through `dim_land` for land. The caller then
/// overlays whatever content they want (rivers, settlement dots, etc).
pub(super) fn paint_hypso_bg<F: Fn(Rgb<u8>) -> Rgb<u8>>(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    map: &GlobalMap,
    sea: Rgb<u8>,
    dim_land: F,
) {
    let n = map.config.global_res as usize;
    let max_h = map.config.max_elevation_m.max(1.0);
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let px = if map.land_mask[i] == 0 {
                sea
            } else {
                dim_land(hypso_color(map.elevation_m[i] / max_h))
            };
            img.put_pixel(x as u32, y as u32, px);
        }
    }
}

/// Paint a filled disk of `radius` cells at (cx, cy), wrapping X, clamping Y.
pub(super) fn stamp_disk(
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

/// Draw `a → b` as a thick line; if the edge crosses the X seam (cell-coord
/// `|dx| > res/2`), split into two halves meeting at the world's east/west
/// edge so the line doesn't stripe across the image.
pub(super) fn draw_seam_aware_line(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    a: [f32; 2],
    b: [f32; 2],
    color: Rgb<u8>,
    radius: i32,
) {
    let half = n as f32 * 0.5;
    if (b[0] - a[0]).abs() <= half {
        draw_thick_line(img, n, a, b, color, radius);
        return;
    }
    let res_f = n as f32;
    let (lo, hi) = if a[0] < b[0] { (a, b) } else { (b, a) };
    let span = lo[0] + (res_f - hi[0]);
    if span <= 0.0 {
        return;
    }
    let t = lo[0] / span;
    let y_seam = lo[1] + (hi[1] - lo[1]) * t;
    draw_thick_line(img, n, lo, [0.0, y_seam], color, radius);
    draw_thick_line(img, n, [res_f, y_seam], hi, color, radius);
}

/// Stamp a disk of `radius` cells along the line `a → b` at half-cell
/// intervals. Thickness scaling lets the preview survive aggressive
/// downscaling in PNG viewers; for 1-pixel rasterization a per-sample
/// `put_pixel` would do.
fn draw_thick_line(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    a: [f32; 2],
    b: [f32; 2],
    color: Rgb<u8>,
    radius: i32,
) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    let steps = ((len * 2.0).ceil() as i32).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = a[0] + dx * t;
        let y = a[1] + dy * t;
        stamp_disk(img, n, x.round() as i32, y.round() as i32, radius, color);
    }
}
