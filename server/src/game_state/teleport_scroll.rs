use super::{inventory::consume_one, GameState};
use crate::{item_defs::UseEffect, types::PlayerId};
use onlinerpg_shared::{
    dungeon::{
        dungeon_cache_key, floor_level_for_passability, passability_floor_for_level, world_to_cell,
        GRID,
    },
    pathfinding::{get_floor_y_base, is_cell_sealed, is_circle_blocked_on_floor},
    wrap_world_x, Position, ServerMessage, TeleportPhase,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

const MIN_DISTANCE: f32 = 32.0;
const MAX_DISTANCE: f32 = 2_000.0;
const ARRIVAL_RADIUS: f32 = 0.3;
const DESTINATION_ATTEMPTS: usize = 128;
const SURFACE_AREA: f64 = std::f64::consts::PI
    * (MAX_DISTANCE as f64 * MAX_DISTANCE as f64 - MIN_DISTANCE as f64 * MIN_DISTANCE as f64);

#[derive(Clone, Copy)]
pub(super) struct TeleportArea {
    pub origin: Position,
    pub width: f32,
    pub depth: f32,
    pub floor_level: i8,
}

impl TeleportArea {
    fn area(&self) -> f64 {
        f64::from(self.width) * f64::from(self.depth)
    }
}

fn choose_dungeon_area<'a>(
    surface_area: f64,
    dungeons: &'a [TeleportArea],
    rng: &mut impl Rng,
) -> Option<&'a TeleportArea> {
    let total = surface_area + dungeons.iter().map(TeleportArea::area).sum::<f64>();
    let mut ticket = rng.gen_range(0.0..total);
    for area in dungeons {
        if ticket < area.area() {
            return Some(area);
        }
        ticket -= area.area();
    }
    None
}

fn sample_destination(
    origin: &Position,
    dungeons: &[TeleportArea],
    rng: &mut impl Rng,
) -> (Position, i8) {
    if let Some(area) = choose_dungeon_area(SURFACE_AREA, dungeons, rng) {
        return (
            Position {
                x: wrap_world_x(area.origin.x + rng.gen_range(0.0..area.width)),
                y: area.origin.y,
                z: area.origin.z + rng.gen_range(0.0..area.depth),
            },
            area.floor_level,
        );
    }
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let radius = rng
        .gen_range(MIN_DISTANCE.powi(2)..MAX_DISTANCE.powi(2))
        .sqrt();
    (
        Position {
            x: wrap_world_x(origin.x + angle.cos() * radius),
            y: 0.0,
            z: origin.z + angle.sin() * radius,
        },
        0,
    )
}

impl GameState {
    pub(crate) async fn use_teleport_scroll(&self, player_id: &PlayerId, instance_id: u64) {
        self.stop_bed_rest(player_id).await;
        let effect = {
            let inventories = self.inventories.read().await;
            inventories
                .get(player_id)
                .and_then(|inv| {
                    inv.bag
                        .iter()
                        .find(|item| item.instance_id == instance_id && item.quantity > 0)
                })
                .and_then(|item| self.item_defs.get(&item.item_def_id))
                .and_then(|def| def.use_effect())
        };
        let used = if self
            .reject_if_trade_reserved(player_id, instance_id, "use")
            .await
        {
            false
        } else {
            match effect {
                Some(UseEffect::TeleportTown) => {
                    self.use_return_scroll(player_id, instance_id).await
                }
                Some(UseEffect::TeleportRandom) => {
                    self.use_teleport_scroll_with_rng(
                        player_id,
                        instance_id,
                        &mut StdRng::from_entropy(),
                    )
                    .await
                }
                _ => false,
            }
        };
        if !used {
            self.cancel_teleport_effect(player_id).await;
        }
    }

    pub(super) async fn use_teleport_scroll_with_rng(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        rng: &mut (impl Rng + Send),
    ) -> bool {
        if self
            .reject_if_defeated(player_id, "You can't read while defeated")
            .await
        {
            return false;
        }
        let Some((origin, rotation, _)) = self.get_player_position(player_id).await else {
            return false;
        };
        let Some((destination, floor_level)) = self.random_teleport_destination(&origin, rng).await
        else {
            self.send_system_message(player_id, "No safe place to teleport was found.")
                .await;
            return false;
        };
        if self
            .reject_if_trade_reserved(player_id, instance_id, "use")
            .await
            || self
                .reject_if_defeated(player_id, "You can't read while defeated")
                .await
        {
            return false;
        }
        let valid_scroll = |inv: &onlinerpg_shared::inventory::PlayerInventory| {
            inv.bag.iter().any(|item| {
                item.instance_id == instance_id
                    && item.quantity > 0
                    && matches!(
                        self.item_defs
                            .get(&item.item_def_id)
                            .and_then(|def| def.use_effect()),
                        Some(UseEffect::TeleportRandom)
                    )
            })
        };
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            inventories
                .get_mut(player_id)
                .filter(|inv| valid_scroll(inv))
                .map(|inv| {
                    consume_one(inv, instance_id);
                    inv.clone()
                })
        };
        let Some(snapshot) = snapshot else {
            return false;
        };
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.teleport_player_with_effects(player_id, destination, rotation, floor_level)
            .await;
        if self
            .players
            .read()
            .await
            .get(player_id)
            .is_some_and(|player| {
                player
                    .mount
                    .is_some_and(|mount| floor_level < 0 || mount.floats())
            })
        {
            self.set_mount(player_id, None).await;
        }
        true
    }

    pub(super) async fn cancel_teleport_effect(&self, player_id: &PlayerId) {
        if let Some((position, _, floor_level)) = self.get_player_position(player_id).await {
            self.send_direct_message(
                player_id,
                ServerMessage::PlayerTeleportEffect {
                    player_id: *player_id,
                    position,
                    floor_level,
                    phase: TeleportPhase::Cancelled,
                },
            )
            .await;
        }
    }

    pub(super) async fn teleport_player_with_effects(
        &self,
        player_id: &PlayerId,
        destination: Position,
        rotation: f32,
        floor_level: i8,
    ) {
        let Some((origin, _, origin_floor)) = self.get_player_position(player_id).await else {
            return;
        };
        self.publish_teleport_effect(player_id, origin, origin_floor, TeleportPhase::Departing)
            .await;
        self.publish_teleport_effect(player_id, destination, floor_level, TeleportPhase::Arriving)
            .await;
        self.teleport_player(player_id, destination, rotation, floor_level)
            .await;
    }

    async fn publish_teleport_effect(
        &self,
        player_id: &PlayerId,
        position: Position,
        floor_level: i8,
        phase: TeleportPhase,
    ) {
        let message = ServerMessage::PlayerTeleportEffect {
            player_id: *player_id,
            position,
            floor_level,
            phase,
        };
        self.publish_nearby(&position, floor_level, message.clone(), Some(player_id))
            .await;
        if phase != TeleportPhase::Departing {
            self.send_direct_message(player_id, message).await;
        }
    }

    pub(super) fn teleport_dungeon_areas(&self, origin: &Position) -> Vec<TeleportArea> {
        let cache = self.passability_read();
        let mut areas = Vec::new();
        for entrance in self.dungeon_defs.all() {
            if origin.dist_xz_sq(&entrance.position()) > (MAX_DISTANCE + GRID as f32).powi(2) {
                continue;
            }
            let Some(entry) = cache.get(&dungeon_cache_key(&entrance.id)) else {
                continue;
            };
            for grid in &entry.floors {
                let floor_level = floor_level_for_passability(grid.floor_level);
                if floor_level < 0 {
                    areas.push(TeleportArea {
                        origin: Position {
                            x: entry.house_origin_x + grid.origin_x as f32,
                            y: grid.y_base,
                            z: entry.house_origin_z + grid.origin_z as f32,
                        },
                        width: f32::from(grid.width),
                        depth: f32::from(grid.depth),
                        floor_level,
                    });
                }
            }
        }
        areas
    }

    async fn random_teleport_destination(
        &self,
        origin: &Position,
        rng: &mut impl Rng,
    ) -> Option<(Position, i8)> {
        let areas = self.teleport_dungeon_areas(origin);
        // Retry the whole pool so rejection preserves equal probability per square metre.
        for _ in 0..DESTINATION_ATTEMPTS {
            let (candidate, floor_level) = sample_destination(origin, &areas, rng);
            if !(MIN_DISTANCE.powi(2)..=MAX_DISTANCE.powi(2))
                .contains(&origin.dist_xz_sq(&candidate))
            {
                continue;
            }
            let position = if floor_level < 0 {
                self.teleport_dungeon_landing(candidate, floor_level).await
            } else {
                self.teleport_surface_landing(candidate.x, candidate.z)
                    .await
            };
            if let Some(position) = position {
                return Some((position, floor_level));
            }
        }
        None
    }

    pub(super) async fn teleport_surface_landing(&self, x: f32, z: f32) -> Option<Position> {
        {
            let cache = self.passability_read();
            if get_floor_y_base(&cache, x, z, 0).is_some()
                || is_circle_blocked_on_floor(&cache, x, z, ARRIVAL_RADIUS, 0, None)
            {
                return None;
            }
        }
        let (y, depth) = self.ground_and_depth_at(x, z).await?;
        if !y.is_finite() || !depth.is_finite() || depth >= 0.0 {
            return None;
        }
        for (dx, dz) in [(0.5, 0.0), (-0.5, 0.0), (0.0, 0.5), (0.0, -0.5)] {
            let (near_y, near_depth) = self
                .ground_and_depth_at(wrap_world_x(x + dx), z + dz)
                .await?;
            if !near_y.is_finite()
                || !near_depth.is_finite()
                || near_depth >= 0.0
                || (near_y - y).abs() > 0.5
            {
                return None;
            }
        }
        Some(Position { x, y, z })
    }

    pub(super) async fn teleport_dungeon_landing(
        &self,
        position: Position,
        floor: i8,
    ) -> Option<Position> {
        let entrance = self.dungeon_defs.entrance_at(position.x, position.z)?;
        let depth = floor.checked_neg().filter(|depth| *depth > 0)? as usize;
        self.ensure_dungeon_runtime(&entrance.id).await;
        let dungeons = self.dungeons.read().await;
        let layout = dungeons.get(&entrance.id)?.layouts.get(depth - 1)?;
        let (x, z) = world_to_cell(&entrance.position(), position.x, position.z);
        if !layout.is_carved(x, z)
            || layout.up_shaft.contains(x, z)
            || layout.down_shaft.is_some_and(|shaft| shaft.contains(x, z))
        {
            return None;
        }
        let cache = self.passability_read();
        let cell_floor = passability_floor_for_level(floor);
        let y = get_floor_y_base(&cache, position.x, position.z, cell_floor)?;
        if is_cell_sealed(&cache, position.x, position.z, cell_floor, None)
            || is_circle_blocked_on_floor(
                &cache,
                position.x,
                position.z,
                ARRIVAL_RADIUS,
                cell_floor,
                None,
            )
        {
            return None;
        }
        Some(Position { y, ..position })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teleport_area_weights_are_proportional_to_floor_area() {
        let areas = [
            TeleportArea {
                origin: Position {
                    x: 0.0,
                    y: -4.0,
                    z: 0.0,
                },
                width: 2.0,
                depth: 5.0,
                floor_level: -1,
            },
            TeleportArea {
                origin: Position {
                    x: 0.0,
                    y: -8.0,
                    z: 0.0,
                },
                width: 6.0,
                depth: 5.0,
                floor_level: -2,
            },
        ];
        let mut rng = StdRng::seed_from_u64(42);
        let mut counts = [0_i32; 3];
        for _ in 0..100_000 {
            let floor =
                choose_dungeon_area(60.0, &areas, &mut rng).map_or(0, |area| -area.floor_level);
            counts[floor as usize] += 1;
        }
        for (actual, expected) in counts.into_iter().zip([60_000, 10_000, 30_000]) {
            assert!((actual - expected).abs() < 700, "{actual} vs {expected}");
        }
    }
}
