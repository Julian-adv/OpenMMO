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
                .any(|(body, clearance)| at.dist_xz_sq(body) < clearance * clearance)
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
                (monster.position, BODY_RADIUS + body.max(BODY_RADIUS))
            })
            .collect()
    }
}
