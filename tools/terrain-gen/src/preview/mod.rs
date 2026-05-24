//! `preview` command: generate the global map up to the currently-implemented
//! phase and write PNGs for visual inspection.
//!
//! Submodule layout:
//! - `canvas`: drawing primitives + the region-grid overlay every PNG ends with
//! - `text`: 5×7 bitmap font for settlement IDs and edge-region labels
//! - `terrain`: elevation/potential/land-sea PNGs + the coast-distance BFS
//! - `features`: roads/settlements/rivers/coasts PNGs

mod canvas;
mod features;
mod terrain;
mod text;

use anyhow::{Context, Result};
use onlinerpg_shared::worldgen::{
    coasts, continent, elevation, erosion, rivers, roads, settlements, GlobalMap, WorldGenConfig,
};
use onlinerpg_shared::worldgen::{rivers::RiverMap, roads::RoadNetwork, settlements::Settlement};
use std::path::Path;
use std::time::Instant;

/// Subdirectory name used by both `preview` and `bake` to scope a run's
/// preview PNGs by seed (`worldgen_preview/<seed-hex>/`).
pub fn seed_subdir(seed: u64) -> String {
    format!("{:016x}", seed)
}

pub fn run(config: &WorldGenConfig, out_root: &Path) -> Result<()> {
    let seed_dir = out_root.join(seed_subdir(config.seed));
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

    // --- Phase 3: hydraulic erosion (dandrino sim) --------------------------
    let t_ph3 = Instant::now();
    erosion::erode_hydraulic(&mut map);
    let max_post = map
        .elevation_m
        .iter()
        .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    eprintln!(
        "Phase 3 (erosion):        {:.2}s  sim_res={}, max = {:.0}m",
        t_ph3.elapsed().as_secs_f32(),
        if config.erosion_sim_res == 0 {
            config.global_res
        } else {
            config.erosion_sim_res
        },
        max_post
    );

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

    // --- Phase 4c: small-island hill stamp ---------------------------------
    let t_ph4c = Instant::now();
    let island_out = elevation::seed_small_island_hills(&mut map);
    if !island_out.hotspots.is_empty() {
        // Mainland elevation didn't change (hotspots only land on small
        // components), so mainland downstream/flow are still valid.
        // Preserve the existing rivers across `compute_flow` so we don't
        // have to re-extract the whole network just to add island streams.
        let saved_rivers = std::mem::take(&mut river_map.rivers);
        river_map = rivers::compute_flow(&map);
        river_map.rivers = saved_rivers;
        let before = river_map.rivers.len();
        rivers::extract_small_island_rivers(
            &map,
            &mut river_map,
            &island_out.island_cells,
            config.small_island_river_min_peak_m,
            config.small_island_river_min_length as usize,
            config.small_island_river_peak_spacing_cells,
        );
        eprintln!(
            "Phase 4c (island hills):  {:.2}s  +{} hotspots, +{} island rivers",
            t_ph4c.elapsed().as_secs_f32(),
            island_out.hotspots.len(),
            river_map.rivers.len() - before,
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
    let coast_dist = terrain::coast_distance(&map.land_mask, map.config.global_res as usize);
    // Hypso color cache: 5 of the 8 PNGs share the same per-cell hypsometric
    // tint, so compute it once here and thread through to every consumer.
    // At 4096² this is ~50 MB held until `write_pngs` returns.
    let hypso_cache = canvas::build_hypso_cache(map);
    terrain::write_potential_png(map, &dir.join("01_potential.png"))?;
    terrain::write_land_sea_png(map, &coast_dist, &dir.join("01_land_sea.png"))?;
    terrain::write_land_sea_shifted_png(map, &coast_dist, &dir.join("01_land_sea_shifted.png"))?;
    terrain::write_elevation_grayscale_png(map, &dir.join("02_elevation.png"))?;
    terrain::write_elevation_hypso_png(map, &hypso_cache, &dir.join("02_elevation_hypso.png"))?;
    features::write_rivers_png(map, river_map, &hypso_cache, &dir.join("03_rivers.png"))?;
    features::write_settlements_png(
        map,
        river_map,
        settlements_list,
        &hypso_cache,
        &dir.join("04_settlements.png"),
    )?;
    features::write_roads_png(
        map,
        river_map,
        road_net,
        settlements_list,
        &hypso_cache,
        &dir.join("05_roads.png"),
    )?;
    let coast_polys = coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
    features::write_coasts_png(map, &coast_polys, &hypso_cache, &dir.join("06_coasts.png"))?;
    eprintln!(
        "  wrote PNGs: {:.2}s ({} coast polylines)",
        t.elapsed().as_secs_f32(),
        coast_polys.len()
    );
    Ok(())
}
