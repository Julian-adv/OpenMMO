//! `inspect-tile` command: dump river segment data for a single tile.
//! Mirrors the bake pipeline up through `BakeContext::new`, then prints
//! every river segment that influences the requested tile.

use anyhow::Result;
use onlinerpg_shared::worldgen::{
    coasts, continent, elevation, erosion, rivers, roads, settlements,
    tile_bake::{self, TILE_DIM},
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

    let fields = settlements::compute_habitability_fields(&map, &river_map);
    let settlements_list =
        settlements::place_settlements_with_fields(&map, &river_map, &fields);
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
    println!("=== Per-polyline dump (any point within {}m of tile) ===", margin_poly);
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
            let dup = if k > 0 && poly.points[k - 1] == *p { " [DUP]" } else { "" };
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
    println!("  segments near tile (margin={:.1}m): {}", margin, segs.len());
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
        let in_tile_a =
            s.ax >= tile_origin_x && s.ax <= tile_max_x && s.az >= tile_origin_z && s.az <= tile_max_z;
        let in_tile_b =
            s.bx >= tile_origin_x && s.bx <= tile_max_x && s.bz >= tile_origin_z && s.bz <= tile_max_z;
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
