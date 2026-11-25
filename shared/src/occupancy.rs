//! The 1 m standing grid: one body per cell, best effort. Monster chase slots
//! and player side steps both pick their spot here (doc/MONSTER_SEPARATION.md).

use crate::world::{shortest_world_delta_x, wrap_world_x};

pub type Cell = (i32, i32);

/// A character's body radius; two of them touch inside `OVERLAP_M`.
pub const BODY_RADIUS_M: f32 = 0.31;
pub const OVERLAP_M: f32 = 2.0 * BODY_RADIUS_M;

pub fn cell_of(x: f32, z: f32) -> Cell {
    (wrap_world_x(x).floor() as i32, z.floor() as i32)
}

pub fn cell_center(cell: Cell) -> (f32, f32) {
    (wrap_world_x(cell.0 as f32 + 0.5), cell.1 as f32 + 0.5)
}

/// Who holds which cells around a mover, excluding the mover itself.
/// Standers block walking and slots; other bodies only block slots: their own
/// cell, and any cell centre inside their clearance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Occupancy {
    standers: Vec<Cell>,
    others: Vec<Body>,
}

#[derive(Debug, Clone, PartialEq)]
struct Body {
    cell: Cell,
    x: f32,
    z: f32,
    clearance: f32,
}

impl Body {
    fn within(&self, x: f32, z: f32) -> bool {
        let (dx, dz) = (shortest_world_delta_x(self.x, x), z - self.z);
        dx * dx + dz * dz < self.clearance * self.clearance
    }
}

impl Occupancy {
    pub fn clear(&mut self) {
        self.standers.clear();
        self.others.clear();
    }

    pub fn add_stander(&mut self, cell: Cell) {
        self.standers.push(cell);
    }

    /// `(x, z, clearance)` per body; clearance 0 holds only its cell.
    pub fn set_others(&mut self, bodies: impl IntoIterator<Item = (f32, f32, f32)>) {
        self.others.clear();
        self.others
            .extend(bodies.into_iter().map(|(x, z, clearance)| Body {
                cell: cell_of(x, z),
                x,
                z,
                clearance,
            }));
    }

    pub fn standers(&self) -> &[Cell] {
        &self.standers
    }

    pub fn blocks_walk(&self, cell: Cell) -> bool {
        self.standers.contains(&cell)
    }

    pub fn blocks_slot(&self, cell: Cell) -> bool {
        if self.blocks_walk(cell) {
            return true;
        }
        let (x, z) = cell_center(cell);
        self.others
            .iter()
            .any(|body| body.cell == cell || body.within(x, z))
    }

    /// Whether `(x, z)` stands inside another body's clearance.
    pub fn overlaps(&self, x: f32, z: f32) -> bool {
        self.others.iter().any(|body| body.within(x, z))
    }
}

/// Farthest cell centre `ranked_cells(_, radius, _)` can reach from a point in
/// its centre cell.
pub fn window_reach(radius: i32) -> f32 {
    (radius as f32 + 0.5) * std::f32::consts::SQRT_2
}

/// The cells within `radius` cells of `center` that `rank` keeps, lower rank
/// then lower score first; ties keep the x-major scan order.
pub fn ranked_cells(
    center: Cell,
    radius: i32,
    mut rank: impl FnMut(Cell) -> Option<(u8, f32)>,
) -> Vec<Cell> {
    let mut ranked = Vec::new();
    for ox in -radius..=radius {
        for oz in -radius..=radius {
            let (x, z) = cell_center((center.0 + ox, center.1 + oz));
            let cell = cell_of(x, z);
            if let Some(r) = rank(cell) {
                ranked.push((cell, r));
            }
        }
    }
    ranked.sort_by(|a, b| a.1 .0.cmp(&b.1 .0).then(a.1 .1.total_cmp(&b.1 .1)));
    ranked.into_iter().map(|(cell, _)| cell).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_wrap_across_the_world_seam() {
        let edge = crate::world::WORLD_MAX_X - 0.5;
        let cell = cell_of(edge, 3.2);
        let around = ranked_cells(cell, 1, |_| Some((0, 0.0)));
        assert_eq!(around.len(), 9);
        assert!(around.contains(&cell_of(crate::world::WORLD_MIN_X + 0.5, 3.2)));
    }

    #[test]
    fn ranking_is_stable_and_drops_rejected_cells() {
        let ranked = ranked_cells((0, 0), 1, |c| (c != (0, 0)).then_some((0, 1.0)));
        assert_eq!(ranked.len(), 8);
        assert_eq!(ranked[0], (-1, -1));
        assert_eq!(ranked[7], (1, 1));
    }

    #[test]
    fn only_standers_block_walking() {
        let mut occupancy = Occupancy::default();
        occupancy.add_stander((1, 1));
        occupancy.set_others([(2.9, 2.5, 0.0), (5.05, 4.5, 0.62)]);
        assert!(occupancy.blocks_walk((1, 1)) && occupancy.blocks_slot((1, 1)));
        assert!(!occupancy.blocks_walk((2, 2)) && occupancy.blocks_slot((2, 2)));
        assert!(
            !occupancy.blocks_slot((3, 2)),
            "clearance 0 holds only its cell"
        );
        assert!(occupancy.blocks_slot((4, 4)), "centre inside the clearance");
        assert!(occupancy.overlaps(5.5, 4.5) && !occupancy.overlaps(5.7, 4.5));
    }
}
