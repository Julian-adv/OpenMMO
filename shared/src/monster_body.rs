//! Solid bodies for monsters too large to treat as a point.
//!
//! A body is a rectangle in the monster's own frame (front = facing). It
//! seals the cells under it in the passability cache, so players walk around
//! it, and every reach to or from the monster is measured at its
//! [`engage_point`] rather than its centre.

use crate::pathfinding::{build_furniture_passability, FurniturePiece, RuntimePassability};
use crate::world::{shortest_world_delta_x, wrap_world_x, Position};

/// How far outside the body the engage point sits. A sealed cell's centre can
/// lie up to √2/2 m inside the rectangle's edge, so this clears it.
const ENGAGE_OFFSET_M: f32 = 0.8;
/// Vertical band the body blocks, around the monster's own height. Wide enough
/// for a body standing on a slope.
const BODY_BELOW_M: f32 = 6.0;
const BODY_HEIGHT_M: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyFootprint {
    pub half_width: f32,
    pub front: f32,
    pub back: f32,
}

impl BodyFootprint {
    /// `monsters.csv` `footprint`: `halfWidth|front|back` in meters.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('|').map(|p| p.trim().parse::<f32>().ok());
        let (half_width, front, back) = (parts.next()??, parts.next()??, parts.next()??);
        // NaN fails every comparison; infinity is not a body.
        (parts.next().is_none()
            && half_width > 0.0
            && front >= 0.0
            && back >= 0.0
            && (half_width + front + back).is_finite()
            && front + back > 0.0)
            .then_some(Self {
                half_width,
                front,
                back,
            })
    }

    /// World offset from the centre → (across, along) in the body frame.
    /// `(s, c)` is the sin/cos of the facing bearing `atan2(dx, dz)`.
    fn to_local((s, c): (f32, f32), dx: f32, dz: f32) -> (f32, f32) {
        (dx * c - dz * s, dx * s + dz * c)
    }

    fn to_world((s, c): (f32, f32), lx: f32, lz: f32) -> (f32, f32) {
        (lx * c + lz * s, -lx * s + lz * c)
    }

    fn contains_local(&self, lx: f32, lz: f32) -> bool {
        lx.abs() <= self.half_width && (-self.back..=self.front).contains(&lz)
    }

    /// Integer world cells whose centres fall inside the body.
    pub fn cells(&self, cx: f32, cz: f32, rotation: f32) -> Vec<(i32, i32)> {
        let r = self.half_width.hypot(self.front.max(self.back)).ceil() as i32 + 1;
        let (bx, bz) = (cx.floor() as i32, cz.floor() as i32);
        let facing = rotation.sin_cos();
        let mut cells = Vec::new();
        for gz in bz - r..=bz + r {
            for gx in bx - r..=bx + r {
                let dx = shortest_world_delta_x(cx, gx as f32 + 0.5);
                let dz = gz as f32 + 0.5 - cz;
                let (lx, lz) = Self::to_local(facing, dx, dz);
                if self.contains_local(lx, lz) {
                    cells.push((gx, gz));
                }
            }
        }
        cells
    }

    /// The farthest the engage point gets from the centre.
    pub fn reach_radius(&self) -> f32 {
        self.half_width.hypot(self.front.max(self.back)) + ENGAGE_OFFSET_M
    }

    /// The point just outside the body nearest `(px, pz)`: where reach and
    /// line of sight to or from the monster are measured.
    pub fn engage_point(&self, cx: f32, cz: f32, rotation: f32, px: f32, pz: f32) -> (f32, f32) {
        let facing = rotation.sin_cos();
        let (lx, lz) = Self::to_local(facing, shortest_world_delta_x(cx, px), pz - cz);
        let qx = lx.clamp(-self.half_width, self.half_width);
        let qz = lz.clamp(-self.back, self.front);
        let (ox, oz) = if self.contains_local(lx, lz) {
            // Inside: leave through the nearest edge.
            let edges = [
                (
                    self.half_width - lx,
                    (self.half_width + ENGAGE_OFFSET_M, lz),
                ),
                (
                    lx + self.half_width,
                    (-self.half_width - ENGAGE_OFFSET_M, lz),
                ),
                (self.front - lz, (lx, self.front + ENGAGE_OFFSET_M)),
                (lz + self.back, (lx, -self.back - ENGAGE_OFFSET_M)),
            ];
            edges
                .into_iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, p)| p)
                .unwrap_or((qx, qz))
        } else {
            let (nx, nz) = (lx - qx, lz - qz);
            let len = nx.hypot(nz);
            (
                qx + nx / len * ENGAGE_OFFSET_M,
                qz + nz / len * ENGAGE_OFFSET_M,
            )
        };
        let (wx, wz) = Self::to_world(facing, ox, oz);
        (wrap_world_x(cx + wx), cz + wz)
    }

    /// The sealed-cell entry for a body standing at `y` on the overworld.
    pub fn passability(
        &self,
        cx: f32,
        y: f32,
        cz: f32,
        rotation: f32,
    ) -> Option<RuntimePassability> {
        build_furniture_passability(&[FurniturePiece {
            cells: self.cells(cx, cz, rotation),
            floor_level: 0,
            y_base: y - BODY_BELOW_M,
            wall_height: BODY_HEIGHT_M,
        }])
    }
}

/// Where reach to or from a monster at `at` is measured toward `toward`: its
/// engage point when it has a body, else `at` itself.
pub fn reach_point(
    body: Option<BodyFootprint>,
    at: Position,
    rotation: f32,
    toward: &Position,
) -> Position {
    body.map_or(at, |body| {
        let (x, z) = body.engage_point(at.x, at.z, rotation, toward.x, toward.z);
        Position { x, z, ..at }
    })
}

/// Edge of a round body of `radius` at `at`, toward `toward` (never past it).
pub fn radial_point(at: Position, radius: f32, toward: &Position) -> Position {
    let dx = shortest_world_delta_x(at.x, toward.x);
    let dz = toward.z - at.z;
    let dist = dx.hypot(dz);
    if radius <= 0.0 || dist <= f32::EPSILON {
        return at;
    }
    let t = radius.min(dist) / dist;
    Position {
        x: wrap_world_x(at.x + dx * t),
        z: at.z + dz * t,
        ..at
    }
}

pub fn cache_key(monster_id: &str) -> String {
    format!("monster-body:{monster_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const BODY: BodyFootprint = BodyFootprint {
        half_width: 4.0,
        front: 3.0,
        back: 2.0,
    };

    #[test]
    fn radial_point_stops_short_of_the_centre_toward_the_player() {
        let at = Position {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        };
        let p = radial_point(
            at,
            1.0,
            &Position {
                x: 0.0,
                y: 0.0,
                z: 3.0,
            },
        );
        assert!((p.z - 1.0).abs() < 1e-5 && p.x.abs() < 1e-5 && p.y == 1.0);
        let inside = radial_point(
            at,
            1.0,
            &Position {
                x: 0.5,
                y: 0.0,
                z: 0.0,
            },
        );
        assert!((inside.x - 0.5).abs() < 1e-5);
        assert_eq!(
            radial_point(
                at,
                0.0,
                &Position {
                    x: 3.0,
                    y: 0.0,
                    z: 0.0
                }
            ),
            at
        );
    }

    #[test]
    fn parses_three_nonnegative_lengths() {
        assert_eq!(BodyFootprint::parse("4|3|2"), Some(BODY));
        for bad in [
            "", "4|3", "4|3|2|1", "0|3|2", "4|-1|2", "4|x|2", "4|0|0", "inf|1|1", "NaN|1|1",
        ] {
            assert_eq!(BodyFootprint::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn cells_follow_facing() {
        // Facing +Z: 8 wide in X, 5 deep in Z.
        let north = BODY.cells(100.0, 100.0, 0.0);
        assert_eq!(north.len(), 8 * 5);
        assert!(north.contains(&(96, 102)) && north.contains(&(103, 98)));
        assert!(!north.contains(&(100, 103)) && !north.contains(&(100, 97)));
        // Facing -X (west): the front reaches 3 m west, the width runs along Z.
        let west = BODY.cells(100.0, 100.0, -FRAC_PI_2);
        assert_eq!(west.len(), 8 * 5);
        assert!(west.contains(&(97, 100)) && west.contains(&(101, 103)));
        assert!(!west.contains(&(96, 100)) && !west.contains(&(102, 100)));
    }

    #[test]
    fn engage_point_sits_just_outside_the_nearest_face() {
        let (x, z) = BODY.engage_point(100.0, 100.0, 0.0, 100.0, 120.0);
        assert!((x - 100.0).abs() < 1e-4 && (z - 103.8).abs() < 1e-4);
        let (x, z) = BODY.engage_point(100.0, 100.0, -FRAC_PI_2, 80.0, 100.0);
        assert!((x - 96.2).abs() < 1e-4 && (z - 100.0).abs() < 1e-4);
        // From inside, out through the nearest edge (the back, 1 m away).
        let (x, z) = BODY.engage_point(100.0, 100.0, 0.0, 100.0, 99.0);
        assert!((x - 100.0).abs() < 1e-4 && (z - 97.2).abs() < 1e-4);
    }

    #[test]
    fn engage_point_cell_is_never_sealed() {
        for rotation in [0.0, 0.4, -FRAC_PI_2, 2.5] {
            let cells = BODY.cells(50.3, 70.6, rotation);
            for i in 0..72 {
                let a = i as f32 * std::f32::consts::TAU / 72.0;
                let (x, z) = BODY.engage_point(
                    50.3,
                    70.6,
                    rotation,
                    50.3 + 20.0 * a.sin(),
                    70.6 + 20.0 * a.cos(),
                );
                assert!(
                    !cells.contains(&(x.floor() as i32, z.floor() as i32)),
                    "{rotation} {i}"
                );
            }
        }
    }

    #[test]
    fn passability_blocks_the_body_and_not_beside_it() {
        let mut cache = crate::pathfinding::PassabilityCache::new();
        cache.insert(
            cache_key("m1"),
            BODY.passability(100.0, 20.0, 100.0, 0.0).unwrap(),
        );
        let blocked =
            |x, z| crate::pathfinding::is_circle_blocked_on_floor(&cache, x, z, 0.3, 0, Some(20.0));
        assert!(blocked(100.0, 100.0));
        assert!(!blocked(100.0, 105.0));
        assert!(crate::pathfinding::is_movement_blocked(
            &cache,
            100.0,
            110.0,
            100.0,
            90.0,
            0,
            Some(20.0)
        ));
    }
}
