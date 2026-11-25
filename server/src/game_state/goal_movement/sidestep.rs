use super::synchronization::BODY_RADIUS;
use super::*;
use std::f32::consts::PI;

pub(super) const OVERLAP_M: f32 = 2.0 * BODY_RADIUS;
/// One body width plus a gap; the second ring is for crowds.
const STEP_RINGS_M: [f32; 2] = [0.8, 1.6];
/// Fractions of a half turn off the heading: sideways first, back last.
const STEP_TURNS: [f32; 8] = [0.5, -0.5, 0.25, -0.25, 0.75, -0.75, 0.0, 1.0];
const MAX_STEP_RISE_M: f32 = 0.5;
/// Bounds a chain of walkers stepping onto each other's side spots.
pub(super) const MAX_SIDESTEPS: u8 = 2;

impl GameState {
    /// A one-waypoint walk off another body the player came to rest on;
    /// `None` when the spot is clear or every neighbour is blocked or taken.
    pub(super) async fn sidestep_waypoints(&self, player: &Player) -> Option<Vec<MoveWaypoint>> {
        let from = player.position;
        let bodies = self
            .visible_bodies_near(player, STEP_RINGS_M[1] + OVERLAP_M)
            .await;
        let taken = |at: &Position| {
            bodies
                .iter()
                .any(|body| at.dist_xz_sq(body) < OVERLAP_M * OVERLAP_M)
        };
        if !taken(&from) {
            return None;
        }
        let side = if player.id.get().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let (floor, open) = {
            let cache = self.passability_read();
            let floor = pathfinding::start_floor_at(&cache, from.x, from.z, from.y);
            let open: Vec<_> = STEP_RINGS_M
                .into_iter()
                .flat_map(|radius| STEP_TURNS.map(|turn| (radius, turn)))
                .map(|(radius, turn)| {
                    let angle = player.rotation + side * turn * PI;
                    Position {
                        x: wrap_world_x(from.x + angle.sin() * radius),
                        y: from.y,
                        z: from.z + angle.cos() * radius,
                    }
                })
                .filter(|to| {
                    !taken(to)
                        && !swept_blocked(
                            &cache,
                            from,
                            shortest_world_delta_x(from.x, to.x),
                            to.z - from.z,
                            floor,
                        )
                })
                .collect();
            (floor, open)
        };
        for mut to in open {
            let Some(y) = self.dash_landing_y(player.floor_level, to).await else {
                continue;
            };
            if (y - from.y).abs() > MAX_STEP_RISE_M {
                continue;
            }
            to.y = y;
            if floor == 0
                && passability::too_steep(
                    &self.passability_read(),
                    &self.bridge_decks,
                    floor,
                    &from,
                    &to,
                )
            {
                continue;
            }
            return Some(vec![MoveWaypoint {
                position: to,
                floor_level: player.floor_level,
                rotation: None,
                travel_seconds: None,
            }]);
        }
        None
    }

    /// Living bodies others can see on the player's floor. Cloaked players are
    /// left out so stepping around them can't give them away.
    async fn visible_bodies_near(&self, player: &Player, radius: f32) -> Vec<Position> {
        let players = self.players.read().await;
        let cells = self.player_spatial_cells.read().await;
        cells
            .keys_near(&player.position, radius)
            .filter(|id| **id != player.id)
            .filter_map(|id| players.get(id))
            .filter(|other| {
                other.floor_level == player.floor_level
                    && other.health > 0
                    && !other.invisible
                    && other.position.dist_xz_sq(&player.position) < radius * radius
            })
            .map(|other| other.position)
            .collect()
    }
}
