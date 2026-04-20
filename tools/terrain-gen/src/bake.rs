//! `bake` command: runs the full worldgen pipeline, then for every tile in
//! the chosen region range writes heightmap + splatmap bin files, per-region
//! palette meta files, and a top-level `worldgen.json` index with the seed,
//! config, settlements, and road polylines in world coordinates.
//!
//! The file layout matches `terrain::TerrainIO` so the runtime can load the
//! output without any format conversion.

use anyhow::{Context, Result};
use onlinerpg_shared::worldgen::{
    continent, elevation, erosion, rivers, roads, settlements, tile_bake, GlobalMap, WorldGenConfig,
};
use onlinerpg_terrain::coords;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Tiles per region side (16 → 256 tiles per region, 1024 m square).
const TILES_PER_REGION: i32 = 16;

pub fn run(
    config: &WorldGenConfig,
    out: &Path,
    region_min: (i32, i32),
    region_max: (i32, i32),
) -> Result<()> {
    let overall = Instant::now();

    // --- Phase 1–6 pipeline: exactly mirrors `preview::run` so the same
    // seed produces the same macro world in both commands. ---------------
    eprintln!(
        "Generating {}×{} global map (seed={:#x})…",
        config.global_res, config.global_res, config.seed
    );
    let t = Instant::now();
    let mut map = continent::generate_continent_mask(config);
    eprintln!("  Phase 1 (continent): {:.2}s", t.elapsed().as_secs_f32());

    let t = Instant::now();
    elevation::generate_elevation(&mut map);
    eprintln!("  Phase 2 (elevation): {:.2}s", t.elapsed().as_secs_f32());

    if config.erosion_droplet_count > 0 {
        let t = Instant::now();
        erosion::erode_hydraulic(&mut map);
        eprintln!(
            "  Phase 3 (erosion):   {:.2}s  ({} droplets)",
            t.elapsed().as_secs_f32(),
            config.erosion_droplet_count
        );
    }

    let t = Instant::now();
    let mut river_map = rivers::compute_flow(&map);
    let min_peak = config.max_elevation_m * 0.3;
    rivers::extract_rivers(&map, &mut river_map, min_peak, 20);
    eprintln!(
        "  Phase 4 (rivers):    {:.2}s  ({} polylines)",
        t.elapsed().as_secs_f32(),
        river_map.rivers.len()
    );

    let t = Instant::now();
    let fields = settlements::compute_habitability_fields(&map, &river_map);
    let mut settlements_list =
        settlements::place_settlements_with_fields(&map, &river_map, &fields);
    eprintln!(
        "  Phase 5a (cities):   {:.2}s  ({} cities)",
        t.elapsed().as_secs_f32(),
        settlements_list.len()
    );

    let t = Instant::now();
    let road_net = roads::compute_roads(&map, &settlements_list);
    eprintln!(
        "  Phase 6 (roads):     {:.2}s  ({} roads)",
        t.elapsed().as_secs_f32(),
        road_net.roads.len()
    );

    let t = Instant::now();
    let extras = settlements::place_settlements_along_roads_with_fields(
        &map,
        &road_net,
        &settlements_list,
        config.settlement_along_road_count as usize,
        &fields,
    );
    let added = extras.len();
    settlements_list.extend(extras);
    eprintln!(
        "  Phase 5b (villages): {:.2}s  (+{} along-road villages, total {})",
        t.elapsed().as_secs_f32(),
        added,
        settlements_list.len()
    );

    // --- Phase 7 prep: build shared per-cell context. -------------------
    let t = Instant::now();
    let ctx = tile_bake::BakeContext::new(&map, &river_map, &road_net);
    eprintln!(
        "  Phase 7 prep:        {:.2}s  (coast/river/road fields)",
        t.elapsed().as_secs_f32()
    );

    // --- Directory scaffolding + per-region palette meta. ---------------
    let region_xs: Vec<i32> = (region_min.0..=region_max.0).collect();
    let region_zs: Vec<i32> = (region_min.1..=region_max.1).collect();
    let region_count = region_xs.len() * region_zs.len();

    std::fs::create_dir_all(out.join("height"))
        .with_context(|| format!("create {}/height", out.display()))?;
    std::fs::create_dir_all(out.join("splat"))
        .with_context(|| format!("create {}/splat", out.display()))?;
    std::fs::create_dir_all(out.join("meta"))
        .with_context(|| format!("create {}/meta", out.display()))?;

    let meta_json_str = serde_json::to_string_pretty(&tile_bake::default_palette_meta())?;
    for &rx in &region_xs {
        for &rz in &region_zs {
            std::fs::create_dir_all(coords::height_region_dir(out, rx, rz))?;
            std::fs::create_dir_all(coords::splat_region_dir(out, rx, rz))?;
            let meta_path = coords::meta_path(out, rx, rz);
            std::fs::write(&meta_path, &meta_json_str)
                .with_context(|| format!("write {}", meta_path.display()))?;
        }
    }

    // --- Tile enumeration + parallel bake. -------------------------------
    let mut tile_coords: Vec<(i32, i32)> = Vec::with_capacity(region_count * 256);
    for &rx in &region_xs {
        for &rz in &region_zs {
            for j in 0..TILES_PER_REGION {
                for i in 0..TILES_PER_REGION {
                    tile_coords.push((rx * TILES_PER_REGION + i, rz * TILES_PER_REGION + j));
                }
            }
        }
    }
    let total_tiles = tile_coords.len();
    eprintln!(
        "Baking {} regions × 256 tiles = {} tiles (x=[{:+},{:+}] z=[{:+},{:+}])",
        region_count, total_tiles, region_min.0, region_max.0, region_min.1, region_max.1
    );

    let done = AtomicUsize::new(0);
    let next_report = AtomicUsize::new(4096);
    let t_bake = Instant::now();

    tile_coords
        .par_iter()
        .try_for_each(|&(tx, tz)| -> Result<()> {
            let baked = tile_bake::bake_tile(&map, &ctx, tx, tz);
            let hpath = coords::heightmap_path(out, tx, tz);
            let spath = coords::splatmap_path(out, tx, tz);
            std::fs::write(&hpath, &baked.heightmap)
                .with_context(|| format!("write {}", hpath.display()))?;
            std::fs::write(&spath, &baked.splatmap)
                .with_context(|| format!("write {}", spath.display()))?;

            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            let report_at = next_report.load(Ordering::Relaxed);
            if n >= report_at || n == total_tiles {
                // Best-effort CAS so only one thread logs per threshold.
                if next_report
                    .compare_exchange(
                        report_at,
                        report_at + 4096,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    eprintln!(
                        "  {}/{} tiles ({:.1}%)",
                        n,
                        total_tiles,
                        n as f32 * 100.0 / total_tiles as f32
                    );
                }
            }
            Ok(())
        })?;

    eprintln!(
        "Baked {} tiles in {:.2}s",
        total_tiles,
        t_bake.elapsed().as_secs_f32()
    );

    // --- Top-level worldgen.json index. ----------------------------------
    let worldgen_json = build_worldgen_json(
        config,
        &map,
        &settlements_list,
        &road_net,
        region_min,
        region_max,
    );
    let wpath = out.join("worldgen.json");
    std::fs::write(&wpath, serde_json::to_string_pretty(&worldgen_json)?)
        .with_context(|| format!("write {}", wpath.display()))?;

    let preview_dir = out.join("worldgen_preview");
    std::fs::create_dir_all(&preview_dir)
        .with_context(|| format!("create {}", preview_dir.display()))?;
    crate::preview::write_pngs(&preview_dir, &map, &river_map, &road_net, &settlements_list)?;

    eprintln!(
        "Wrote {} (bake total {:.2}s)",
        wpath.display(),
        overall.elapsed().as_secs_f32()
    );
    Ok(())
}

/// Convert a global-cell center to its world-space position. World origin
/// sits at the center of the map (X wraps, Z clamps).
fn cell_to_world(cell_x: u32, cell_y: u32, cfg: &WorldGenConfig) -> (f32, f32) {
    let mpc = cfg.meters_per_cell();
    let origin_offset = cfg.world_size_m as f32 * 0.5;
    (
        (cell_x as f32 + 0.5) * mpc - origin_offset,
        (cell_y as f32 + 0.5) * mpc - origin_offset,
    )
}

fn build_worldgen_json(
    cfg: &WorldGenConfig,
    map: &GlobalMap,
    settlements_list: &[settlements::Settlement],
    road_net: &roads::RoadNetwork,
    region_min: (i32, i32),
    region_max: (i32, i32),
) -> serde_json::Value {
    let settlements_json: Vec<_> = settlements_list
        .iter()
        .map(|s| {
            let (wx, wz) = cell_to_world(s.cell_x, s.cell_y, cfg);
            serde_json::json!({
                "cell_x": s.cell_x,
                "cell_y": s.cell_y,
                "world_x": wx,
                "world_z": wz,
                "tile_x": coords::world_to_tile(wx),
                "tile_z": coords::world_to_tile(wz),
                "score": s.score,
            })
        })
        .collect();
    let roads_json: Vec<_> = road_net
        .roads
        .iter()
        .map(|r| {
            let points: Vec<_> = r
                .points
                .iter()
                .map(|&(x, y)| {
                    let (wx, wz) = cell_to_world(x, y, cfg);
                    serde_json::json!([wx, wz])
                })
                .collect();
            serde_json::json!({ "points": points })
        })
        .collect();
    serde_json::json!({
        "seed": cfg.seed,
        "config": cfg,
        "measured_sea_ratio": map.measured_sea_ratio(),
        "baked_region_range": {
            "x_min": region_min.0,
            "x_max": region_max.0,
            "z_min": region_min.1,
            "z_max": region_max.1,
        },
        "settlements": settlements_json,
        "roads": roads_json,
    })
}
