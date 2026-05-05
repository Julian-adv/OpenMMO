//! `preview` command: generate the global map up to the currently-implemented
//! phase and write PNGs for visual inspection.

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use onlinerpg_shared::worldgen::{
    coasts, continent, elevation, erosion, rivers, roads, settlements, GlobalMap, WorldGenConfig,
};
use onlinerpg_shared::worldgen::{
    coasts::CoastPolyline, rivers::RiverMap, roads::RoadNetwork, settlements::Settlement,
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
    let min_peak = config.max_elevation_m * rivers::RIVER_PEAK_ELEVATION_FRAC;
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

    // --- Phase 4b: gap-fill mountains for riverless lowlands ---------------
    let added_hotspots = elevation::seed_river_gap_mountains(&mut map, &river_map);
    if !added_hotspots.is_empty() {
        let t_ph4b = Instant::now();
        river_map = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut river_map, min_peak, min_length);
        eprintln!(
            "Phase 4b (gap fill):      {:.2}s  +{} mountain hotspots, {} rivers",
            t_ph4b.elapsed().as_secs_f32(),
            added_hotspots.len(),
            river_map.rivers.len()
        );
    }

    // Habitability fields are shared by Phase 5a and 5b — building once
    // avoids recomputing coast BFS, slope, and river-distance BFS.
    let fields = settlements::compute_habitability_fields(&map, &river_map);

    let t_ph5 = Instant::now();
    let mut settlements_list =
        settlements::place_settlements_with_fields(&map, &river_map, &fields);
    let cities_count = settlements_list.len();
    eprintln!(
        "Phase 5a (cities):        {:.2}s  {} cities",
        t_ph5.elapsed().as_secs_f32(),
        cities_count
    );

    let t_ph6 = Instant::now();
    let mut road_net = roads::compute_roads(&map, &settlements_list, &river_map);
    roads::merge_parallel_runs(&mut road_net, map.config.global_res as usize);
    roads::merge_parallel_interiors(&mut road_net, map.config.global_res as usize);
    roads::snap_crossings_to_grid(
        &mut road_net,
        &mut river_map,
        map.config.global_res as usize,
    );
    eprintln!(
        "Phase 6 (roads):          {:.2}s  {} roads",
        t_ph6.elapsed().as_secs_f32(),
        road_net.roads.len()
    );

    let t_ph5b = Instant::now();
    let extra = settlements::place_settlements_along_roads_with_fields(
        &map,
        &road_net,
        &settlements_list,
        config.settlement_along_road_count as usize,
        &fields,
    );
    let added = extra.len();
    settlements_list.extend(extra);
    eprintln!(
        "Phase 5b (villages):      {:.2}s  +{} along-road villages (total {})",
        t_ph5b.elapsed().as_secs_f32(),
        added,
        settlements_list.len()
    );

    write_pngs(&seed_dir, &map, &river_map, &road_net, &settlements_list)?;

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
            "settlement_count": settlements_list.len(),
        },
        "settlements": settlements_list
            .iter()
            .enumerate()
            .map(|(i, s)| serde_json::json!({
                "id": settlements::settlement_label(i),
                "cell_x": s.cell_x,
                "cell_y": s.cell_y,
                "score": s.score,
            }))
            .collect::<Vec<_>>(),
    });
    std::fs::write(
        seed_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    eprintln!("Wrote preview to {}", seed_dir.display());
    Ok(())
}

/// Write all 8 preview PNGs for a fully-populated global map into `dir`.
/// Shared between the `preview` command and `bake` (which dumps the same
/// images alongside its baked tile artifacts so the same directory carries
/// both the runtime-facing tiles and a human-facing overview).
pub fn write_pngs(
    dir: &Path,
    map: &GlobalMap,
    river_map: &RiverMap,
    road_net: &RoadNetwork,
    settlements_list: &[Settlement],
) -> Result<()> {
    let t = Instant::now();
    // Coast distance field: used by the land/sea previews so that sand
    // appears only at the actual coastline (not wherever the independent
    // potential noise happens to be low).
    let coast_dist = coast_distance(&map.land_mask, map.config.global_res as usize);
    write_potential_png(map, &dir.join("01_potential.png"))?;
    write_land_sea_png(map, &coast_dist, &dir.join("01_land_sea.png"))?;
    write_land_sea_shifted_png(map, &coast_dist, &dir.join("01_land_sea_shifted.png"))?;
    write_elevation_grayscale_png(map, &dir.join("02_elevation.png"))?;
    write_elevation_hypso_png(map, &dir.join("02_elevation_hypso.png"))?;
    write_rivers_png(map, river_map, &dir.join("03_rivers.png"))?;
    write_settlements_png(
        map,
        river_map,
        settlements_list,
        &dir.join("04_settlements.png"),
    )?;
    write_roads_png(
        map,
        river_map,
        road_net,
        settlements_list,
        &dir.join("05_roads.png"),
    )?;
    let coast_polys = coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
    write_coasts_png(map, &coast_polys, &dir.join("06_coasts.png"))?;
    eprintln!(
        "  wrote PNGs: {:.2}s ({} coast polylines)",
        t.elapsed().as_secs_f32(),
        coast_polys.len()
    );
    Ok(())
}

/// Draw region-grid lines (1 km spacing) and world-origin axes on top of an
/// already-rendered PNG, then save. Shared wrapper so every preview image
/// gets the same overlay without duplicating save error-context boilerplate.
fn finish_png(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, map: &GlobalMap, path: &Path) -> Result<()> {
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
fn overlay_region_grid(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, map: &GlobalMap) {
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
fn overlay_region_labels(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, map: &GlobalMap) {
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
        (a.0[0] as f32 * (1.0 - t) + b.0[0] as f32 * t) as u8,
        (a.0[1] as f32 * (1.0 - t) + b.0[1] as f32 * t) as u8,
        (a.0[2] as f32 * (1.0 - t) + b.0[2] as f32 * t) as u8,
    ])
}

/// Fill `img` with a muted hypsometric-tint background: `sea` fill for water
/// cells, `hypso_color` run through `dim_land` for land. The caller then
/// overlays whatever content they want (rivers, settlement dots, etc).
fn paint_hypso_bg<F: Fn(Rgb<u8>) -> Rgb<u8>>(
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

fn write_roads_png(
    map: &GlobalMap,
    river_map: &rivers::RiverMap,
    road_net: &roads::RoadNetwork,
    settlements_list: &[settlements::Settlement],
    path: &Path,
) -> anyhow::Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), |c| {
        Rgb([
            (c.0[0] as u16 * 2 / 3) as u8,
            (c.0[1] as u16 * 2 / 3) as u8,
            (c.0[2] as u16 * 2 / 3) as u8,
        ])
    });
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

fn write_settlements_png(
    map: &GlobalMap,
    river_map: &rivers::RiverMap,
    settlements_list: &[settlements::Settlement],
    path: &Path,
) -> anyhow::Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), |c| {
        Rgb([
            (c.0[0] as u16 * 2 / 3) as u8,
            (c.0[1] as u16 * 2 / 3) as u8,
            (c.0[2] as u16 * 2 / 3) as u8,
        ])
    });
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

fn write_rivers_png(
    map: &GlobalMap,
    river_map: &rivers::RiverMap,
    path: &Path,
) -> anyhow::Result<()> {
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
fn write_coasts_png(
    map: &GlobalMap,
    coast_polys: &[CoastPolyline],
    path: &Path,
) -> anyhow::Result<()> {
    let n = map.config.global_res as usize;
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(n as u32, n as u32);
    paint_hypso_bg(&mut img, map, Rgb([28, 65, 115]), |c| {
        Rgb([
            (c.0[0] as u16 * 2 / 3) as u8,
            (c.0[1] as u16 * 2 / 3) as u8,
            (c.0[2] as u16 * 2 / 3) as u8,
        ])
    });
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

/// Draw `a → b` as a thick line; if the edge crosses the X seam (cell-coord
/// `|dx| > res/2`), split into two halves meeting at the world's east/west
/// edge so the line doesn't stripe across the image.
fn draw_seam_aware_line(
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

/// 5×7 bitmap glyphs indexed by base-36 digit (0..=9 then a..=z), top-to-
/// bottom rows, 5 LSB bits per row (bit 4 = leftmost pixel). Hand-rolled
/// to avoid pulling a font crate for a 36-glyph need.
const FONT_GLYPHS: [[u8; 7]; 36] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F], // 2
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C], // 9
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F], // a
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E], // b
    [0x00, 0x00, 0x0E, 0x10, 0x10, 0x10, 0x0E], // c
    [0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F], // d
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E], // e
    [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08], // f
    [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E], // g
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // l
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11], // m
    [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E], // o
    [0x00, 0x00, 0x16, 0x19, 0x1E, 0x10, 0x10], // p
    [0x00, 0x00, 0x0D, 0x13, 0x0F, 0x01, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E], // s
    [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D], // u
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04], // v
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A], // w
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11], // x
    [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E], // y
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F], // z
];

/// Glyph for "?" used for any char outside 0..=9 / a..=z (e.g. the "??"
/// overflow label from `settlement_label`).
const GLYPH_QUESTION: [u8; 7] = [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04];
const GLYPH_PLUS: [u8; 7] = [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00];
const GLYPH_MINUS: [u8; 7] = [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00];

fn font_glyph(c: char) -> [u8; 7] {
    match c {
        '+' => GLYPH_PLUS,
        '-' => GLYPH_MINUS,
        _ => c
            .to_digit(36)
            .map(|d| FONT_GLYPHS[d as usize])
            .unwrap_or(GLYPH_QUESTION),
    }
}

/// Stamp a 5×7 glyph at `(left, top)` scaled by `scale` (each lit bit becomes
/// a `scale×scale` block). X wraps, Y clamps — same conventions as `stamp_disk`.
fn draw_glyph(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    left: i32,
    top: i32,
    bitmap: &[u8; 7],
    color: Rgb<u8>,
    scale: i32,
) {
    for row in 0..7i32 {
        let bits = bitmap[row as usize];
        for col in 0..5i32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let py = top + row * scale + sy;
                    if py < 0 || py >= n as i32 {
                        continue;
                    }
                    let px = (left + col * scale + sx).rem_euclid(n as i32) as u32;
                    img.put_pixel(px, py as u32, color);
                }
            }
        }
    }
}

/// Pixel bbox `(left, top, right, bot)` of `text` rendered by
/// `draw_text_centered` at `scale`, centered on `(cx, cy)`. Right/bot are
/// exclusive. Empty strings collapse to a zero-width rect at the center.
fn text_bbox(text: &str, scale: i32, cx: i32, cy: i32) -> (i32, i32, i32, i32) {
    let char_w = 5 * scale;
    let char_h = 7 * scale;
    let gap = scale;
    let count = text.chars().count() as i32;
    let total_w = if count > 0 {
        count * char_w + (count - 1) * gap
    } else {
        0
    };
    let left = cx - total_w / 2;
    let top = cy - char_h / 2;
    (left, top, left + total_w, top + char_h)
}

/// Render `text` centered on `(cx, cy)` using the 5×7 bitmap font at `scale`.
/// Inter-character gap is 1 scaled pixel so dense IDs ("a0") stay readable.
fn draw_text_centered(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    cx: i32,
    cy: i32,
    text: &str,
    color: Rgb<u8>,
    scale: i32,
) {
    let (mut left, top, _, _) = text_bbox(text, scale, cx, cy);
    let advance = 5 * scale + scale;
    for c in text.chars() {
        draw_glyph(img, n, left, top, &font_glyph(c), color, scale);
        left += advance;
    }
}

/// Draw `text` centered at `(cx, cy)` with a solid background pad behind it.
/// Used by the edge-region labels where text sits on arbitrary terrain colors
/// and needs guaranteed contrast (settlement labels rely on the yellow disk
/// behind them instead). `pad` is in scaled pixels around the glyph block.
fn draw_text_with_bg(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    cx: i32,
    cy: i32,
    text: &str,
    text_color: Rgb<u8>,
    bg_color: Rgb<u8>,
    scale: i32,
    pad: i32,
) {
    if text.is_empty() {
        return;
    }
    let (l, t, r, b) = text_bbox(text, scale, cx, cy);
    let x0 = (l - pad).max(0);
    let x1 = (r + pad).min(n as i32);
    let y0 = (t - pad).max(0);
    let y1 = (b + pad).min(n as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            img.put_pixel(px as u32, py as u32, bg_color);
        }
    }
    draw_text_centered(img, n, cx, cy, text, text_color, scale);
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
    finish_png(&mut img, map, path)?;
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
    finish_png(&mut img, map, path)?;
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
    finish_png(&mut img, map, path)?;
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
    finish_png(&mut img, map, path)?;
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
