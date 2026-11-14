//! Worldgen road centrelines, bucketed for distance queries.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use onlinerpg_shared::shortest_world_delta_x;
use onlinerpg_shared::worldgen::vector_features::{point_segment_distance, Segment};
use serde::Deserialize;

/// Fences whose centre lies this close to a road centreline block the road.
pub const ROAD_CLEARANCE_M: f32 = 3.0;
/// Walkers this close to a road centreline draw no ambient monsters.
pub const ROAD_QUIET_M: f32 = 8.0;

const CELL: f32 = 16.0;
/// Widest query the buckets answer.
const BUCKET_REACH_M: f32 = ROAD_QUIET_M;
const _: () = assert!(ROAD_CLEARANCE_M <= BUCKET_REACH_M);

#[derive(Deserialize)]
struct WorldgenRoads {
    roads: Vec<Road>,
}

#[derive(Deserialize)]
struct Road {
    points: Vec<[f32; 2]>,
}

#[derive(Default)]
pub struct RoadIndex {
    cells: HashMap<(i32, i32), Vec<Segment>>,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl RoadIndex {
    pub fn load(terrain_dir: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(terrain_dir.join("worldgen.json"))?;
        let parsed: WorldgenRoads = serde_json::from_slice(&bytes)?;
        Ok(Self::from_polylines(
            parsed.roads.iter().map(|road| road.points.as_slice()),
        ))
    }

    pub fn from_polylines<'a>(roads: impl IntoIterator<Item = &'a [[f32; 2]]>) -> Self {
        let mut seen = HashSet::new();
        let mut cells: HashMap<(i32, i32), Vec<Segment>> = HashMap::new();
        for points in roads {
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let key = |p: [f32; 2]| (p[0].to_bits(), p[1].to_bits());
                if !seen.insert((key(a).min(key(b)), key(a).max(key(b)))) {
                    continue;
                }
                let span = shortest_world_delta_x(a[0], b[0]);
                let wrapped = [[a[0], a[1]], [a[0] + span, b[1]]];
                let mirrored = [[b[0] - span, a[1]], [b[0], b[1]]];
                let crosses_seam = (b[0] - a[0] - span).abs() > 1.0;
                for [a, b] in std::iter::once(wrapped).chain(crosses_seam.then_some(mirrored)) {
                    let reach = BUCKET_REACH_M;
                    for cx in cell(a[0].min(b[0]) - reach)..=cell(a[0].max(b[0]) + reach) {
                        for cz in cell(a[1].min(b[1]) - reach)..=cell(a[1].max(b[1]) + reach) {
                            cells
                                .entry((cx, cz))
                                .or_default()
                                .push([a[0], a[1], b[0], b[1]]);
                        }
                    }
                }
            }
        }
        Self { cells }
    }

    /// Whether `(x, z)` lies within `ROAD_CLEARANCE_M` of a road centreline.
    pub fn on_road(&self, x: f32, z: f32) -> bool {
        self.within(x, z, ROAD_CLEARANCE_M)
    }

    /// Whether `(x, z)` lies within `ROAD_QUIET_M` of a road centreline.
    pub fn near_road(&self, x: f32, z: f32) -> bool {
        self.within(x, z, ROAD_QUIET_M)
    }

    fn within(&self, x: f32, z: f32, reach: f32) -> bool {
        self.cells.get(&(cell(x), cell(z))).is_some_and(|segments| {
            segments
                .iter()
                .any(|segment| point_segment_distance(x, z, segment) <= reach)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_within_three_metres_of_a_diagonal_road_are_on_it() {
        let road = [[-1228.0, 4380.0], [-1236.0, 4388.0]];
        let index = RoadIndex::from_polylines([road.as_slice(), road.as_slice()]);
        assert!(index.on_road(-1232.5, 4384.0));
        assert!(index.on_road(-1230.0, 4384.0));
        assert!(!index.on_road(-1226.0, 4384.0));
        assert!(!index.on_road(-1300.0, 4384.0));
    }

    #[test]
    fn the_quiet_band_reaches_across_a_cell_border() {
        let road = [[15.0, 0.0], [15.0, 100.0]];
        let index = RoadIndex::from_polylines([road.as_slice()]);
        assert!(index.near_road(22.5, 50.0));
        assert!(!index.on_road(22.5, 50.0));
        assert!(!index.near_road(23.5, 50.0));
    }

    #[test]
    fn a_road_over_the_world_seam_stays_at_the_seam() {
        let road = [[16380.0, 2892.0], [-16380.0, 2892.0]];
        let index = RoadIndex::from_polylines([road.as_slice()]);
        assert!(index.on_road(16382.0, 2892.0));
        assert!(index.on_road(-16382.0, 2892.0));
        assert!(!index.on_road(0.0, 2892.0));
    }
}
