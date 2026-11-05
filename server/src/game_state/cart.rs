//! Rail carts in Blackvein Mine corridors: a push from one end slides the
//! cart up to `CART_PUSH_CELLS` along its rail; sunset puts it back.

use std::collections::HashSet;

use onlinerpg_shared::dungeon::{cart_push_target, current_spec, world_to_cell, PropKind};
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::{PlayerId, Position};

use super::dungeon::{check_prop_reach, DungeonRuntime};
use super::GameState;

/// Where the push leaves the cart: `Ok(None)` when it cannot budge, `None`
/// when `prop_id` is no rail cart.
fn push_target(
    rt: &DungeonRuntime,
    origin: &Position,
    depth: u8,
    prop_id: u32,
    pusher: (Position, i8),
    occupied: &HashSet<(i32, i32)>,
) -> Option<Result<Option<i32>, String>> {
    let layout = rt.layouts.get((depth - 1) as usize)?;
    let cart = layout.props.get(prop_id as usize)?;
    if cart.kind != PropKind::MineRailCart {
        return None;
    }
    let (broken, carts) = rt.floor_props(depth);
    let offset = rt
        .cart_offsets
        .get(&depth)
        .and_then(|m| m.get(&prop_id))
        .copied()
        .unwrap_or(0);
    let now = current_spec(cart, prop_id, &carts);
    let (position, floor_level) = pusher;
    let center = match check_prop_reach(origin, depth, &now, &position, floor_level, "cart") {
        Ok(center) => center,
        Err(reason) => return Some(Err(reason)),
    };
    let delta = if now.along_x() {
        onlinerpg_shared::shortest_world_delta_x(center.x, position.x)
    } else {
        position.z - center.z
    };
    if delta.abs() < 1.0 {
        return Some(Err("Push the cart from one end".to_string()));
    }
    Some(Ok(cart_push_target(
        layout,
        prop_id,
        offset,
        delta.signum() as i32,
        &broken,
        &carts,
        |cell| occupied.contains(&cell),
    )))
}

impl GameState {
    pub async fn push_dungeon_cart(
        &self,
        player_id: &PlayerId,
        entrance_id: &str,
        depth: u8,
        prop_id: u32,
    ) {
        if depth == 0 {
            return;
        }
        let Some(entrance) = self.dungeon_defs.get(entrance_id) else {
            return;
        };
        let origin = entrance.position();
        self.ensure_dungeon_runtime(entrance_id).await;
        let floor = -(depth as i8);
        // Every player at this floor level, not just recorded floor occupants:
        // a relog rehydrate can skip the floor-entry bookkeeping.
        let (pusher, mut occupied) = {
            let players = self.players.read().await;
            let Some(p) = players.get(player_id).filter(|p| p.health > 0) else {
                return;
            };
            let others: HashSet<(i32, i32)> = players
                .values()
                .filter(|o| o.id != *player_id && o.floor_level == floor && o.health > 0)
                .map(|o| world_to_cell(&origin, o.position.x, o.position.z))
                .collect();
            ((p.position, p.floor_level), others)
        };
        {
            let monsters = self.monsters.read().await;
            occupied.extend(
                monsters
                    .near(&pusher.0)
                    .filter(|m| m.floor_level == floor && m.health > 0)
                    .map(|m| world_to_cell(&origin, m.position.x, m.position.z)),
            );
        }

        let pushed = {
            let mut dungeons = self.dungeons.write().await;
            let Some(rt) = dungeons.get_mut(entrance_id) else {
                return;
            };
            let pushed = push_target(rt, &origin, depth, prop_id, pusher, &occupied);
            if let Some(Ok(Some(to))) = pushed {
                let floor_carts = rt.cart_offsets.entry(depth).or_default();
                if to == 0 {
                    floor_carts.remove(&prop_id);
                } else {
                    floor_carts.insert(prop_id, to);
                }
            }
            pushed
        };

        match pushed {
            Some(Err(reason)) => {
                self.send_direct_message(player_id, ServerMessage::InteractionRejected { reason })
                    .await;
            }
            Some(Ok(Some(offset))) => {
                self.rebuild_dungeon_floor_passability(entrance_id, depth)
                    .await;
                self.publish_nearby(
                    &pusher.0,
                    pusher.1,
                    ServerMessage::DungeonCartPushed {
                        entrance_id: entrance_id.to_string(),
                        depth,
                        prop_id,
                        offset,
                    },
                    None,
                )
                .await;
            }
            _ => {}
        }
    }
}
