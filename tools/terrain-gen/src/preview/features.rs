//! Vector-feature PNGs: roads, settlement dots, river polylines, and the
//! extracted coast outlines. All four share the same hypsometric-tinted
//! background and the region-grid overlay; the differences are the
//! features stamped on top and (for roads) the extra edge-coordinate
//! labels that make this the reference image used in conversation.

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use onlinerpg_shared::worldgen::{
    coasts::CoastPolyline, rivers::RiverMap, roads::RoadNetwork, settlements::Settlement, GlobalMap,
};
use std::path::Path;

use super::canvas::{
    dim_two_thirds, draw_seam_aware_line, finish_png, overlay_region_grid, overlay_region_labels,
    paint_hypso_bg, stamp_disk,
};
use super::text::draw_text_centered;
use onlinerpg_shared::worldgen::settlements;

pub(super) fn write_roads_png(
    map: &GlobalMap,
    river_map: &RiverMap,
    road_net: &RoadNetwork,
    settlements_list: &[Settlement],
    path: &Path,
) -> Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), dim_two_thirds);
    for poly in &river_map.rivers {
        for &(x, y) in &poly.points {
            stamp_disk(&mut img, n, x as i32, y as i32, 1, Rgb([70, 140, 210]));
        }
    }
    for road in &road_net.roads {
        for &(x, y) in &road.points {
            stamp_disk(&mut img, n, x as i32, y as i32, 1, Rgb([220, 210, 150]));
        }
    }
    // Dot size scales with map resolution so the dots remain visible when
    // the PNG is downscaled in a viewer (e.g. IDE preview).
    let outer = (map.config.global_res as i32 / 200).max(5);
    let inner = outer - 2;
    // Label scale: at 4096-res `outer` ≈ 20 px; "a0" at scale 2 = 22×14 px,
    // fits inside the 36 px inner disk with a couple px breathing room. Scale
    // tracks `outer` so smaller previews still render readable text.
    let label_scale = (outer / 7).max(1);
    for (idx, s) in settlements_list.iter().enumerate() {
        stamp_disk(
            &mut img,
            n,
            s.cell_x as i32,
            s.cell_y as i32,
            outer,
            Rgb([25, 20, 10]),
        );
        stamp_disk(
            &mut img,
            n,
            s.cell_x as i32,
            s.cell_y as i32,
            inner,
            Rgb([240, 200, 60]),
        );
        let id = settlements::settlement_label(idx);
        draw_text_centered(
            &mut img,
            n,
            s.cell_x as i32,
            s.cell_y as i32,
            &id,
            Rgb([20, 15, 5]),
            label_scale,
        );
    }
    // Roads PNG is the reference image used in conversation, so it gets
    // region-coordinate labels along all four edges on top of the standard
    // grid overlay. Other previews keep the grid only.
    overlay_region_grid(&mut img, map);
    overlay_region_labels(&mut img, map);
    img.save(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(super) fn write_settlements_png(
    map: &GlobalMap,
    river_map: &RiverMap,
    settlements_list: &[Settlement],
    path: &Path,
) -> Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), dim_two_thirds);
    for poly in &river_map.rivers {
        for &(x, y) in &poly.points {
            stamp_disk(&mut img, n, x as i32, y as i32, 1, Rgb([70, 140, 210]));
        }
    }

    // Dot radius encodes relative score so top-scoring cities read bigger.
    let max_score = settlements_list
        .iter()
        .map(|s| s.score)
        .fold(0.0f32, f32::max)
        .max(1e-6);
    for s in settlements_list {
        let t = s.score / max_score;
        let inner = (2.0 + t * 3.0).round() as i32;
        let outer = inner + 1;
        stamp_disk(
            &mut img,
            n,
            s.cell_x as i32,
            s.cell_y as i32,
            outer,
            Rgb([25, 20, 10]),
        );
        stamp_disk(
            &mut img,
            n,
            s.cell_x as i32,
            s.cell_y as i32,
            inner,
            Rgb([240, 200, 60]),
        );
    }

    finish_png(&mut img, map, path)?;
    Ok(())
}

pub(super) fn write_rivers_png(map: &GlobalMap, river_map: &RiverMap, path: &Path) -> Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    // Land is desaturated toward gray so the blue river overlay reads.
    paint_hypso_bg(&mut img, map, Rgb([35, 80, 140]), |c| {
        Rgb([
            ((c.0[0] as u16 * 3 + 128) / 4) as u8,
            ((c.0[1] as u16 * 3 + 128) / 4) as u8,
            ((c.0[2] as u16 * 3 + 128) / 4) as u8,
        ])
    });

    // Polyline thickness scales with log flow at the mouth so major rivers
    // read visibly thicker than trickles.
    for poly in &river_map.rivers {
        if poly.points.is_empty() {
            continue;
        }
        let mouth = *poly.points.last().unwrap();
        let mouth_idx = (mouth.1 as usize) * n + (mouth.0 as usize);
        let f = river_map.flow[mouth_idx];
        let thickness = ((f.ln().max(1.0) * 0.6) as i32).clamp(1, 4);
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

    finish_png(&mut img, map, path)?;
    Ok(())
}

/// Render the extracted coast polylines from `coasts::extract_coasts` over a
/// dimmed land/sea background. Edges crossing the X seam are detected by
/// `|dx| > res/2` and split so the line doesn't stripe across the image.
pub(super) fn write_coasts_png(
    map: &GlobalMap,
    coast_polys: &[CoastPolyline],
    path: &Path,
) -> Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), dim_two_thirds);
    let coast_color = Rgb([255, 110, 40]);
    // 4096-res map → 4 px disk, 1024-res → 1 px. Matches `overlay_region_grid`.
    let radius = ((n as f32 / 1024.0).round() as i32).max(1);
    for poly in coast_polys {
        for w in poly.points.windows(2) {
            draw_seam_aware_line(&mut img, n, w[0], w[1], coast_color, radius);
        }
    }
    finish_png(&mut img, map, path)?;
    Ok(())
}
