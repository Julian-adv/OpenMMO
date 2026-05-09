//! Snap-axis classification shared by A* (river-perpendicularity gate) and
//! the road↔river grid-snap pass. The 8-way step neighbourhood collapses
//! onto four axes: the two cardinals and the two diagonals; both passes
//! need to ask "which one fits this direction best?" so the helpers live
//! together rather than getting duplicated per consumer.

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(super) enum SnapAxis {
    Horizontal,
    Vertical,
    /// `dy = dx` line (running NW↔SE).
    DiagonalNwSe,
    /// `dy = -dx` line (running NE↔SW).
    DiagonalNeSw,
}

impl SnapAxis {
    pub(super) fn perpendicular(self) -> SnapAxis {
        match self {
            SnapAxis::Horizontal => SnapAxis::Vertical,
            SnapAxis::Vertical => SnapAxis::Horizontal,
            SnapAxis::DiagonalNwSe => SnapAxis::DiagonalNeSw,
            SnapAxis::DiagonalNeSw => SnapAxis::DiagonalNwSe,
        }
    }
}

/// Pick the snap axis whose unit direction is most aligned with `(dx, dy)`.
/// Compares squared projections — the four axes form a 45°-step rosette,
/// so the dominant projection wins by ≥ cos²(22.5°) margin in the generic
/// case. Used both to classify river tangents (A* perpendicularity gate)
/// and to drive the grid-snap pass.
pub(super) fn pick_river_axis(dx: f32, dy: f32) -> SnapAxis {
    let h = dx * dx;
    let v = dy * dy;
    let nw_se = (dx + dy).powi(2) * 0.5;
    let ne_sw = (dx - dy).powi(2) * 0.5;
    if h >= v && h >= nw_se && h >= ne_sw {
        SnapAxis::Horizontal
    } else if v >= nw_se && v >= ne_sw {
        SnapAxis::Vertical
    } else if nw_se >= ne_sw {
        SnapAxis::DiagonalNwSe
    } else {
        SnapAxis::DiagonalNeSw
    }
}

/// Snap-axis classification of a single A* step. The 8-way step neighbourhood
/// maps onto the 4 axes: `(±1, 0) → Horizontal`, `(0, ±1) → Vertical`,
/// `(±1, ±1) same sign → NW-SE`, opposite sign `→ NE-SW`.
pub(super) fn step_axis(dx: i32, dy: i32) -> SnapAxis {
    match (dx, dy) {
        (_, 0) => SnapAxis::Horizontal,
        (0, _) => SnapAxis::Vertical,
        (a, b) if a == b => SnapAxis::DiagonalNwSe,
        _ => SnapAxis::DiagonalNeSw,
    }
}
