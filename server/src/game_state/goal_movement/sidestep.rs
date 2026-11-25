use super::*;
use onlinerpg_shared::occupancy::{
    cell_center, cell_of, ranked_cells, window_reach, Occupancy, BODY_RADIUS_M, OVERLAP_M,
};

/// Cells searched around the arrival; the second ring is for crowds.
const SEARCH_CELLS: i32 = 2;
/// Extra cost of stepping back the way the walker came.
const BACKWARD_PENALTY_M: f32 = 0.3;
const MAX_STEP_RISE_M: f32 = 0.5;
/// Bounds a chain of walkers stepping onto each other's side spots.
pub(super) const MAX_SIDESTEPS: u8 = 2;

impl GameState {
    /// A one-waypoint walk to the nearest free cell centre when the player
    /// came to rest on another body; `None` when clear or nothing is free.
    pub(super) async fn sidestep_waypoints(&self, player: &Player) -> Option<Vec<MoveWaypoint>> {
        let from = player.position;
        let reach = window_reach(SEARCH_CELLS) + OVERLAP_M;
        let mut occupancy = Occupancy::default();
        occupancy.set_others(
            self.visible_bodies_near(player, reach)
                .await
                .into_iter()
                .map(|(at, clearance)| (at.x, at.z, clearance)),
        );
        if !occupancy.overlaps(from.x, from.z) {
            return None;
        }
        let (heading_x, heading_z) = (player.rotation.sin(), player.rotation.cos());
        let candidates = ranked_cells(cell_of(from.x, from.z), SEARCH_CELLS, |cell| {
            if occupancy.blocks_slot(cell) {
                return None;
            }
            let (x, z) = cell_center(cell);
            let (dx, dz) = (shortest_world_delta_x(from.x, x), z - from.z);
            let behind = dx * heading_x + dz * heading_z < -0.1;
            Some((
                0,
                dx.hypot(dz) + if behind { BACKWARD_PENALTY_M } else { 0.0 },
            ))
        });
        for cell in candidates {
            let (x, z) = cell_center(cell);
            let mut to = Position { x, y: from.y, z };
            let floor = {
                let cache = self.passability_read();
                let floor = pathfinding::start_floor_at(&cache, from.x, from.z, from.y);
                let (dx, dz) = (shortest_world_delta_x(from.x, x), z - from.z);
                if swept_blocked(&cache, from, dx, dz, floor) {
                    continue;
                }
                floor
            };
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

    /// Visible living bodies and pending side-step ends, each with its overlap
    /// distance. Cloaked players are skipped so stepping can't reveal them.
    async fn visible_bodies_near(&self, player: &Player, radius: f32) -> Vec<(Position, f32)> {
        let nearby: Vec<(PlayerId, Position)> = {
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
                .map(|other| (other.id, other.position))
                .collect()
        };
        // try_lock: a busy neighbour is mid-settle, and waiting on it could deadlock.
        let claims = nearby.iter().filter_map(|(id, _)| {
            let entry = self.goal_moves.get(*id)?;
            let state = entry.try_lock().ok()?;
            let plan = state.as_ref()?.plan.as_ref()?;
            let end = plan.waypoints.last()?.position;
            (plan.sidesteps > 0).then_some(end)
        });
        let mut bodies: Vec<_> = nearby
            .iter()
            .map(|(_, at)| *at)
            .chain(claims)
            .map(|at| (at, OVERLAP_M))
            .collect();
        bodies.extend(self.monster_bodies_near(player, radius).await);
        bodies
    }

    /// Footprint bodies are walls in passability; this covers the round ones.
    /// Movers count too: server positions are current, unlike a client's.
    async fn monster_bodies_near(&self, player: &Player, radius: f32) -> Vec<(Position, f32)> {
        let reach = radius + self.monster_defs.max_body_radius();
        self.monsters
            .read()
            .await
            .alive_within(&player.position, player.floor_level, reach)
            .map(|monster| {
                let body = self
                    .monster_defs
                    .get(&monster.monster_type)
                    .map_or(0.0, |def| def.body_radius);
                (monster.position, BODY_RADIUS_M + body.max(BODY_RADIUS_M))
            })
            .collect()
    }
}
