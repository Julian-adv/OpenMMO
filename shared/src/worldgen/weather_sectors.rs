//! Rain-cell sectors baked from the plot-resolution climate grid: each zone
//! gets one sector per `SECTOR_KM2` of area, spread evenly through the zone,
//! and a sector's spots are the interior plots around its centre.

use std::collections::VecDeque;

use crate::weather::{Sector, WeatherSectors, WEATHER_SECTORS_VERSION};

/// One climate byte per plot over the whole world.
pub struct ClimatePlotGrid {
    pub w: usize,
    pub h: usize,
    pub plot_m: f32,
    /// World metres of plot (0, 0)'s min corner.
    pub x0: f32,
    pub z0: f32,
    pub zones: Vec<u8>,
}

/// Ground area per sector by zone; tuned so the wet coast hosts several cells
/// at once while the rain shadow rarely hosts one.
pub const SECTOR_KM2: [f32; 5] = [0.0, 4.0, 7.0, 36.0, 12.0];
pub const MAX_SPOTS_PER_SECTOR: usize = 16;
/// Spots sit this far inside their zone so a cell's core stays in its own
/// climate; a thin zone falls back to a smaller margin.
const INSET_PLOTS: [usize; 5] = [16, 8, 4, 2, 0];
/// Coastal showers hang over the shoreline: wet-coast spots stay within this
/// many plots of the sea so their cells spill seaward, not inland.
const COAST_SPOT_PLOTS: u32 = 4;
/// Farthest-point sampling runs over at most this many candidate plots.
const MAX_CANDIDATES: usize = 20_000;

impl ClimatePlotGrid {
    fn zone_at(&self, x: isize, z: isize) -> u8 {
        if z < 0 || z >= self.h as isize {
            return 0;
        }
        let x = x.rem_euclid(self.w as isize) as usize;
        self.zones[z as usize * self.w + x]
    }

    /// Plot distance to the nearest plot of another zone (X wraps).
    fn border_distance(&self) -> Vec<u32> {
        self.distance_from(|i| {
            let here = self.zones[i];
            let (x, z) = ((i % self.w) as isize, (i / self.w) as isize);
            [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                .iter()
                .any(|&(nx, nz)| self.zone_at(nx, nz) != here)
        })
    }

    /// Plot distance to the nearest sea plot (X wraps).
    fn sea_distance(&self) -> Vec<u32> {
        self.distance_from(|i| self.zones[i] == 0)
    }

    fn distance_from(&self, is_source: impl Fn(usize) -> bool) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.zones.len()];
        let mut queue = VecDeque::new();
        for (i, d) in dist.iter_mut().enumerate() {
            if is_source(i) {
                *d = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let d = dist[i] + 1;
            let (x, z) = ((i % self.w) as isize, (i / self.w) as isize);
            for (nx, nz) in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
                if nz < 0 || nz >= self.h as isize {
                    continue;
                }
                let n = nz as usize * self.w + nx.rem_euclid(self.w as isize) as usize;
                if dist[n] > d {
                    dist[n] = d;
                    queue.push_back(n);
                }
            }
        }
        dist
    }
}

fn pick_hash(seed: u64, zone: u8, plot: usize) -> u64 {
    crate::weather::splitmix64(
        seed ^ (zone as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (plot as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9),
    )
}

pub fn place_sectors(grid: &ClimatePlotGrid, seed: u64) -> WeatherSectors {
    // WeatherSync crosses to JS as a Number; keep the seed exact there.
    let seed = seed & ((1u64 << 53) - 1);
    let dist = grid.border_distance();
    let sea = grid.sea_distance();
    let plot_km2 = (grid.plot_m / 1000.0).powi(2);
    let mut sectors = Vec::new();
    for zone in 1u8..=4 {
        let plots: Vec<usize> = (0..grid.zones.len())
            .filter(|&i| grid.zones[i] == zone)
            .collect();
        if plots.is_empty() {
            continue;
        }
        let count =
            ((plots.len() as f32 * plot_km2 / SECTOR_KM2[zone as usize]).round() as usize).max(1);
        let candidates: Vec<usize> = if zone == 1 {
            plots
                .iter()
                .copied()
                .filter(|&i| (1..=COAST_SPOT_PLOTS).contains(&sea[i]))
                .collect()
        } else {
            INSET_PLOTS
                .iter()
                .map(|&inset| {
                    plots
                        .iter()
                        .copied()
                        .filter(|&i| dist[i] as usize >= inset)
                        .collect::<Vec<usize>>()
                })
                .find(|c| c.len() >= count * MAX_SPOTS_PER_SECTOR)
                .unwrap_or_else(|| plots.clone())
        };
        if candidates.is_empty() {
            continue;
        }
        let stride = candidates.len().div_ceil(MAX_CANDIDATES).max(1);
        let candidates: Vec<usize> = candidates.into_iter().step_by(stride).collect();
        let xz = |i: usize| ((i % grid.w) as f32, (i / grid.w) as f32);
        let dist2 = |a: usize, b: usize| {
            let (ax, az) = xz(a);
            let (bx, bz) = xz(b);
            let dx = (ax - bx).abs().min(grid.w as f32 - (ax - bx).abs());
            dx * dx + (az - bz) * (az - bz)
        };

        let first = candidates
            .iter()
            .copied()
            .max_by_key(|&i| pick_hash(seed, zone, i))
            .expect("non-empty");
        let mut centres = vec![first];
        let mut nearest: Vec<f32> = candidates.iter().map(|&i| dist2(i, first)).collect();
        while centres.len() < count.min(candidates.len()) {
            let (best, _) = nearest
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .expect("non-empty");
            let c = candidates[best];
            centres.push(c);
            for (n, &i) in nearest.iter_mut().zip(&candidates) {
                *n = n.min(dist2(i, c));
            }
        }

        for &c in &centres {
            let mut near: Vec<usize> = candidates.clone();
            near.sort_by(|&a, &b| dist2(a, c).total_cmp(&dist2(b, c)).then(a.cmp(&b)));
            near.truncate(MAX_SPOTS_PER_SECTOR);
            let spots = near
                .iter()
                .map(|&i| {
                    let (x, z) = xz(i);
                    [
                        grid.x0 + (x + 0.5) * grid.plot_m,
                        grid.z0 + (z + 0.5) * grid.plot_m,
                    ]
                })
                .collect();
            sectors.push(Sector { zone, spots });
        }
    }
    WeatherSectors {
        version: WEATHER_SECTORS_VERSION,
        seed,
        sectors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 256 × 256 plots of 32 m (8.2 km square): sea ring, wet coast band,
    /// temperate interior.
    fn grid() -> ClimatePlotGrid {
        let n = 256;
        let mut zones = vec![0u8; n * n];
        for z in 0..n {
            for x in 0..n {
                let edge = x.min(z).min(n - 1 - x).min(n - 1 - z);
                zones[z * n + x] = match edge {
                    0..=7 => 0,
                    8..=39 => 1,
                    _ => 2,
                };
            }
        }
        ClimatePlotGrid {
            w: n,
            h: n,
            plot_m: 32.0,
            x0: -4096.0,
            z0: -4096.0,
            zones,
        }
    }

    fn zone_of(grid: &ClimatePlotGrid, spot: [f32; 2]) -> u8 {
        let x = ((spot[0] - grid.x0) / grid.plot_m) as usize;
        let z = ((spot[1] - grid.z0) / grid.plot_m) as usize;
        grid.zones[z * grid.w + x]
    }

    #[test]
    fn spots_lie_inside_their_own_zone_and_off_its_border() {
        let g = grid();
        let dist = g.border_distance();
        let sea = g.sea_distance();
        let ws = place_sectors(&g, 42);
        assert!(!ws.sectors.is_empty());
        for s in &ws.sectors {
            assert!((1..=MAX_SPOTS_PER_SECTOR).contains(&s.spots.len()));
            for &spot in &s.spots {
                assert_eq!(zone_of(&g, spot), s.zone);
                let x = ((spot[0] - g.x0) / g.plot_m) as usize;
                let z = ((spot[1] - g.z0) / g.plot_m) as usize;
                if s.zone == 1 {
                    assert!(
                        sea[z * g.w + x] <= COAST_SPOT_PLOTS,
                        "coast spot far from sea"
                    );
                } else {
                    assert!(dist[z * g.w + x] >= 2, "spot hugs the zone border");
                }
            }
        }
    }

    #[test]
    fn sector_count_follows_zone_area() {
        let g = grid();
        let ws = place_sectors(&g, 42);
        let wet = ws.sectors.iter().filter(|s| s.zone == 1).count();
        let temperate = ws.sectors.iter().filter(|s| s.zone == 2).count();
        // wet band: 240² − 176² plots; interior: 176² plots
        let km2 = |plots: f32| plots * (32.0f32 / 1000.0).powi(2);
        let expect = |plots: f32, zone: usize| (km2(plots) / SECTOR_KM2[zone]).round() as usize;
        let band = 240.0 * 240.0 - 176.0 * 176.0;
        assert_eq!(
            (wet, temperate),
            (expect(band, 1), expect(176.0 * 176.0, 2))
        );
        assert!(ws.sectors.iter().all(|s| s.zone != 0));
    }

    #[test]
    fn placement_is_deterministic_per_seed() {
        let g = grid();
        assert_eq!(place_sectors(&g, 42), place_sectors(&g, 42));
        let a = place_sectors(&g, 42);
        let b = place_sectors(&g, 43);
        assert_eq!(a.sectors.len(), b.sectors.len());
        assert_eq!((a.seed, b.seed), (42, 43));
        assert_ne!(
            a.sectors, b.sectors,
            "seed changes where the sampling starts"
        );
    }

    #[test]
    fn a_zone_too_thin_for_an_inset_still_gets_spots() {
        let mut g = grid();
        // shrink the wet band to 3 plots so no inset ≥ 2 survives
        for z in 0..g.h {
            for x in 0..g.w {
                let edge = x.min(z).min(g.w - 1 - x).min(g.h - 1 - z);
                g.zones[z * g.w + x] = match edge {
                    0..=7 => 0,
                    8..=10 => 1,
                    _ => 2,
                };
            }
        }
        let ws = place_sectors(&g, 1);
        assert!(ws.sectors.iter().any(|s| s.zone == 1));
    }
}
