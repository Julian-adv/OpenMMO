//! `inspect-tile` command: dump river segment data for a single tile.
//! Mirrors the bake pipeline up through `BakeContext::new`, then prints
//! every river segment that influences the requested tile.

use anyhow::Result;
use onlinerpg_shared::worldgen::{
    coasts, continent, elevation, erosion, rivers, roads, settlements,
    tile_bake::{
        self, probe_point,
        settlement_flatten::{self, SETTLEMENT_FLATTEN_BLEND_M, SETTLEMENT_FLAT_RADIUS_M},
        TILE_DIM,
    },
    vector_features::river_segments_near_tile,
    WorldGenConfig,
};

pub fn run(config: &WorldGenConfig, tile_x: i32, tile_z: i32) -> Result<()> {
    eprintln!(
        "Generating world (seed={:#x}, res={}) — same pipeline as bake…",
        config.seed, config.global_res
    );
    let mut map = continent::generate_continent_mask(config);
    elevation::generate_elevation(&mut map);
    erosion::erode_hydraulic(&mut map);

    let mut river_map = rivers::compute_flow(&map);
    let min_peak = config.max_elevation_m * rivers::RIVER_PEAK_ELEVATION_FRAC;
    rivers::extract_rivers(&map, &mut river_map, min_peak, 20);
    let added = elevation::seed_river_gap_mountains(&mut map, &river_map);
    if !added.is_empty() {
        river_map = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut river_map, min_peak, 20);
    }
    let island_out = elevation::seed_small_island_hills(&mut map);
    if !island_out.hotspots.is_empty() {
        let saved_rivers = std::mem::take(&mut river_map.rivers);
        river_map = rivers::compute_flow(&map);
        river_map.rivers = saved_rivers;
        rivers::extract_small_island_rivers(
            &map,
            &mut river_map,
            &island_out.island_cells,
            config.small_island_river_min_peak_m,
            config.small_island_river_min_length as usize,
            config.small_island_river_peak_spacing_cells,
        );
    }

    let fields = settlements::compute_habitability_fields(&map, &river_map);
    let settlements_list = settlements::place_settlements_with_fields(&map, &river_map, &fields);
    let mut road_net = roads::compute_roads(&map, &settlements_list, &river_map);
    roads::merge_parallel_runs(&mut road_net, map.config.global_res as usize);
    roads::merge_parallel_interiors(&mut road_net, map.config.global_res as usize);
    roads::snap_crossings_to_grid(
        &mut road_net,
        &mut river_map,
        map.config.global_res as usize,
    );

    let coast_polys = coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
    let ctx = tile_bake::BakeContext::new(&map, &river_map, &road_net, &coast_polys);

    // Tile origin: tile (tx, tz) covers world [tx*64-32, tx*64+32) per axis.
    let tile_dim = TILE_DIM as f32;
    let tile_origin_x = tile_x as f32 * tile_dim - tile_dim * 0.5;
    let tile_origin_z = tile_z as f32 * tile_dim - tile_dim * 0.5;
    let tile_max_x = tile_origin_x + tile_dim;
    let tile_max_z = tile_origin_z + tile_dim;

    // Per-polyline dump: any polyline with at least one point inside the
    // expanded tile bbox.
    let margin_poly = 20.0;
    println!(
        "=== Per-polyline dump (any point within {}m of tile) ===",
        margin_poly
    );
    let mut hit = 0usize;
    for (pi, poly) in ctx.rivers_world.iter().enumerate() {
        let touches = poly.points.iter().any(|p| {
            p[0] >= tile_origin_x - margin_poly
                && p[0] <= tile_max_x + margin_poly
                && p[1] >= tile_origin_z - margin_poly
                && p[1] <= tile_max_z + margin_poly
        });
        if !touches {
            continue;
        }
        hit += 1;
        println!(
            "\n  Polyline #{} — {} points (only points within {}m of tile shown):",
            pi,
            poly.points.len(),
            margin_poly
        );
        for (k, p) in poly.points.iter().enumerate() {
            let near = p[0] >= tile_origin_x - margin_poly
                && p[0] <= tile_max_x + margin_poly
                && p[1] >= tile_origin_z - margin_poly
                && p[1] <= tile_max_z + margin_poly;
            if !near {
                continue;
            }
            let dup = if k > 0 && poly.points[k - 1] == *p {
                " [DUP]"
            } else {
                ""
            };
            println!("    {:4}: ({:8.2}, {:8.2}){}", k, p[0], p[1], dup);
        }
    }
    println!("\nTotal polylines touching tile region: {}\n", hit);

    // Use a margin similar to the heightmap carve query (a few meters).
    let margin = 10.0;
    let segs = river_segments_near_tile(
        &ctx.rivers_world,
        tile_origin_x,
        tile_origin_z,
        tile_max_x,
        tile_max_z,
        margin,
    );

    println!("=== Tile ({}, {}) ===", tile_x, tile_z);
    println!(
        "  world range: x=[{:.1}, {:.1}]  z=[{:.1}, {:.1}]",
        tile_origin_x, tile_max_x, tile_origin_z, tile_max_z
    );
    println!("  river polylines (world): {}", ctx.rivers_world.len());
    println!(
        "  segments near tile (margin={:.1}m): {}",
        margin,
        segs.len()
    );
    println!();
    println!(
        "{:>4}  {:>10} {:>10}  {:>10} {:>10}  {:>8} {:>8}  {:>9}  {:>7} {:>7}",
        "idx", "ax", "az", "bx", "bz", "len_m", "ang°", "flow→", "wA", "wB"
    );
    for (i, s) in segs.iter().enumerate() {
        let dx = s.bx - s.ax;
        let dz = s.bz - s.az;
        let len = (dx * dx + dz * dz).sqrt();
        let ang = dz.atan2(dx).to_degrees();
        let in_tile_a = s.ax >= tile_origin_x
            && s.ax <= tile_max_x
            && s.az >= tile_origin_z
            && s.az <= tile_max_z;
        let in_tile_b = s.bx >= tile_origin_x
            && s.bx <= tile_max_x
            && s.bz >= tile_origin_z
            && s.bz <= tile_max_z;
        let mark = match (in_tile_a, in_tile_b) {
            (true, true) => "[IN  ]",
            (true, false) | (false, true) => "[CROS]",
            (false, false) => "[OUT ]",
        };
        println!(
            "{:>4}  {:>10.2} {:>10.2}  {:>10.2} {:>10.2}  {:>8.2} {:>+8.1}  {} {:>+5.1}°  {:>7.2} {:>7.2}",
            i, s.ax, s.az, s.bx, s.bz, len, ang, mark, ang, s.width_a, s.width_b
        );
    }
    Ok(())
}

/// Decompose the height pipeline at each `(wx, wz)` point. Re-runs the full
/// worldgen prefix (continent → erosion → rivers → settlements → roads →
/// coasts → BakeContext), then calls `probe_point` for each probe.
pub fn probe(config: &WorldGenConfig, points: &[(f32, f32)]) -> Result<()> {
    eprintln!(
        "Generating world (seed={:#x}, res={}) — same pipeline as bake…",
        config.seed, config.global_res
    );
    let mut map = continent::generate_continent_mask(config);
    elevation::generate_elevation(&mut map);
    erosion::erode_hydraulic(&mut map);

    let mut river_map = rivers::compute_flow(&map);
    let min_peak = config.max_elevation_m * rivers::RIVER_PEAK_ELEVATION_FRAC;
    rivers::extract_rivers(&map, &mut river_map, min_peak, 20);
    let added = elevation::seed_river_gap_mountains(&mut map, &river_map);
    if !added.is_empty() {
        river_map = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut river_map, min_peak, 20);
    }
    let island_out = elevation::seed_small_island_hills(&mut map);
    if !island_out.hotspots.is_empty() {
        let saved_rivers = std::mem::take(&mut river_map.rivers);
        river_map = rivers::compute_flow(&map);
        river_map.rivers = saved_rivers;
        rivers::extract_small_island_rivers(
            &map,
            &mut river_map,
            &island_out.island_cells,
            config.small_island_river_min_peak_m,
            config.small_island_river_min_length as usize,
            config.small_island_river_peak_spacing_cells,
        );
    }

    let fields = settlements::compute_habitability_fields(&map, &river_map);
    let settlements_list = settlements::place_settlements_with_fields(&map, &river_map, &fields);
    let mut road_net = roads::compute_roads(&map, &settlements_list, &river_map);
    roads::merge_parallel_runs(&mut road_net, map.config.global_res as usize);
    roads::merge_parallel_interiors(&mut road_net, map.config.global_res as usize);
    roads::snap_crossings_to_grid(
        &mut road_net,
        &mut river_map,
        map.config.global_res as usize,
    );

    let coast_polys = coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
    let ctx = tile_bake::BakeContext::new(&map, &river_map, &road_net, &coast_polys);
    let pad_directives =
        settlement_flatten::build_directives(&settlements_list, &map.config, &map, &ctx);

    for &(wx, wz) in points {
        let p = probe_point(&map, &ctx, wx, wz);
        let tile_dim = TILE_DIM as f32;
        let tx = ((wx + tile_dim * 0.5) / tile_dim).floor() as i32;
        let tz = ((wz + tile_dim * 0.5) / tile_dim).floor() as i32;
        let ix = (wx - (tx as f32 * tile_dim - tile_dim * 0.5)).round() as i32;
        let iz = (wz - (tz as f32 * tile_dim - tile_dim * 0.5)).round() as i32;
        println!(
            "\n=== Point ({:.2}, {:.2})  tile({}, {}) cell({}, {}) ===",
            wx, wz, tx, tz, ix, iz
        );
        println!(
            "  global cell: ({}, {})   land_mask={}  dist_to_land={}",
            p.global_cell.0, p.global_cell.1, p.land_mask, p.dist_to_land
        );
        println!(
            "  natural height (no carve):          {:+7.3} m",
            p.natural_height
        );
        println!(
            "  final height (carve):               {:+7.3} m",
            p.final_height
        );
        match p.river {
            None => println!("  nearest river: NONE (no segment within margin)"),
            Some(r) => {
                println!(
                    "  nearest river segment: idx={} t={:.3} d={:.2}m  signed_d={:+.2}m",
                    r.seg_idx, r.t, r.d_m, r.signed_d_m
                );
                println!(
                    "    width={:.2}m  flow_norm={:.3}  half_width={:.2}m  taper={:.2}m",
                    r.width, r.flow_norm, r.half_width, r.taper
                );
                println!(
                    "    depth_uncapped={:.2}m  bed_floor={:+.2}m  max_carve_depth={:.2}m",
                    r.depth_uncapped, r.bed_floor, r.max_carve_depth
                );
                println!("    carve applied: {:.3}m", r.carve);
                let in_channel = r.d_m <= r.half_width;
                let in_taper = !in_channel && r.d_m <= r.half_width + r.taper;
                let zone = if in_channel {
                    "INSIDE CHANNEL (full bed)"
                } else if in_taper {
                    "INSIDE TAPER (sloped bank)"
                } else {
                    "OUTSIDE carve"
                };
                println!("    zone: {}", zone);
            }
        }
        let reach = SETTLEMENT_FLAT_RADIUS_M + 8.0 + SETTLEMENT_FLATTEN_BLEND_M;
        // `build_directives` emits one entry per settlement in the same order,
        // so the i-th directive matches the i-th settlement.
        let mut nearby: Vec<(f32, usize)> = pad_directives
            .iter()
            .enumerate()
            .filter_map(|(i, fl)| {
                let d = ((wx - fl.center_x).powi(2) + (wz - fl.center_z).powi(2)).sqrt();
                (d <= reach + 30.0).then_some((d, i))
            })
            .collect();
        nearby.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        if nearby.is_empty() {
            println!("  settlement pads in reach: NONE");
        } else {
            println!("  settlement pads within {:.0}m+30m:", reach);
            let inner = (SETTLEMENT_FLAT_RADIUS_M - 8.0).max(0.0);
            for &(d, i) in nearby.iter().take(3) {
                let fl = &pad_directives[i];
                let zone = if d <= inner {
                    "INSIDE FLAT (height forced to target_y)"
                } else if d <= reach {
                    "BLEND/EDGE (height pulled toward target_y)"
                } else {
                    "OUTSIDE pad"
                };
                println!(
                    "    settlement center=({:.2}, {:.2})  dist={:.2}m  target_y={:+.3}m  {}",
                    fl.center_x, fl.center_z, d, fl.target_y, zone
                );
            }
            let post = settlement_flatten::flatten_height_at(
                wx,
                wz,
                p.natural_height,
                &pad_directives,
                &ctx.detail_noise,
                map.config.world_size_m as f32,
            );
            println!(
                "  height AFTER settlement flatten (pre-carve): {:+.3}m",
                post
            );
        }
    }
    Ok(())
}
