//! Rail carts in Blackvein Mine corridors: a push from one end slides the
//! cart along its rail until something stops it, up to
//! `CART_ROLL_MAX_CELLS`; sunset puts it back.

use std::collections::HashMap;
use std::time::Duration;

use onlinerpg_shared::dungeon::{
    cart_push_target, current_spec, world_to_cell, CartPush, PropKind, CART_ROLL_SPEED,
};
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::{PlayerId, Position};

use super::combat::StruckMonster;
use super::dungeon::{check_prop_reach, DungeonRuntime};
use super::GameState;

/// What a rolling cart deals to the body that stops it.
const CART_DAMAGE_ROLL: &str = "4d10";

/// A body standing in a cart's path.
enum Occupant {
    Player(PlayerId),
    Monster(String),
}

/// What the push does: `Ok(None)` when the cart cannot budge, `None` when
/// `prop_id` is no rail cart.
fn push_target(
    rt: &DungeonRuntime,
    origin: &Position,
    depth: u8,
    prop_id: u32,
    pusher: (Position, i8),
    occupied: &HashMap<(i32, i32), Occupant>,
) -> Option<Result<Option<CartPush>, String>> {
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
        |cell| occupied.contains_key(&cell),
    )))
}

impl GameState {
    pub async fn push_dungeon_cart(
        &self,
        player_id: &PlayerId,
        entrance_id: &str,
        depth: u8,
        prop_id: u32,
        auth: Option<&crate::auth::AuthService>,
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
        let cell_of = |at: &Position| world_to_cell(&origin, at.x, at.z);
        let (pusher, pusher_name, mut occupied) = {
            let players = self.players.read().await;
            let Some(p) = players.get(player_id).filter(|p| p.health > 0) else {
                return;
            };
            let others: HashMap<(i32, i32), Occupant> = players
                .values()
                .filter(|o| o.id != *player_id && o.floor_level == floor && o.health > 0)
                .map(|o| (cell_of(&o.position), Occupant::Player(o.id)))
                .collect();
            ((p.position, p.floor_level), p.name.clone(), others)
        };
        {
            let monsters = self.monsters.read().await;
            occupied.extend(
                monsters
                    .near(&pusher.0)
                    .filter(|m| m.floor_level == floor && m.health > 0)
                    .map(|m| (cell_of(&m.position), Occupant::Monster(m.id.clone()))),
            );
        }

        let pushed = {
            let mut dungeons = self.dungeons.write().await;
            let Some(rt) = dungeons.get_mut(entrance_id) else {
                return;
            };
            let pushed = push_target(rt, &origin, depth, prop_id, pusher, &occupied);
            if let Some(Ok(Some(CartPush { to, .. }))) = pushed {
                let floor_carts = rt.cart_offsets.entry(depth).or_default();
                if to == 0 {
                    floor_carts.remove(&prop_id);
                } else {
                    floor_carts.insert(prop_id, to);
                }
            }
            pushed
        };
        let push = match pushed {
            Some(Ok(Some(push))) => push,
            Some(Err(reason)) => {
                self.send_direct_message(player_id, ServerMessage::InteractionRejected { reason })
                    .await;
                return;
            }
            _ => return,
        };

        self.rebuild_dungeon_floor_passability(entrance_id, depth)
            .await;
        let victim = push
            .struck
            .and_then(|cell| occupied.remove(&cell).map(|o| (cell, o)));
        let damage = victim
            .as_ref()
            .map_or(0, |_| crate::game::combat::roll_dice(CART_DAMAGE_ROLL));
        let arrival = Duration::from_secs_f32(push.rolled as f32 / CART_ROLL_SPEED);
        let struck_monster = match &victim {
            Some((_, Occupant::Monster(id))) => Some(id.clone()),
            _ => None,
        };
        self.publish_nearby(
            &pusher.0,
            pusher.1,
            ServerMessage::DungeonCartPushed {
                entrance_id: entrance_id.to_string(),
                depth,
                prop_id,
                offset: push.to,
                pusher_id: *player_id,
                damage: if struck_monster.is_some() { damage } else { 0 },
                struck_monster,
            },
            None,
        )
        .await;
        match victim {
            // A player can still step aside before the cart arrives.
            Some((cell, Occupant::Player(id))) => {
                let game_state = self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(arrival).await;
                    game_state
                        .cart_strikes_player(id, cell, origin, floor, damage)
                        .await;
                });
            }
            Some((_, Occupant::Monster(id))) => {
                let monster = self.monsters.read().await.get(&id).cloned();
                if let Some(m) = monster {
                    let target = StruckMonster {
                        monster_id: &m.id,
                        monster_type: &m.monster_type,
                        position: m.position,
                        floor_level: m.floor_level,
                        level_override: m.level_override,
                    };
                    self.strike_monster(player_id, &pusher_name, target, damage, arrival, auth)
                        .await;
                }
            }
            None => {}
        }
    }

    /// The cart arrives: `player_id` takes `damage` only if still standing
    /// in `cell`, the spot it struck them at.
    async fn cart_strikes_player(
        &self,
        player_id: PlayerId,
        cell: (i32, i32),
        origin: Position,
        floor: i8,
        damage: u32,
    ) {
        let in_path = |p: &onlinerpg_shared::Player| {
            p.health > 0
                && p.floor_level == floor
                && world_to_cell(&origin, p.position.x, p.position.z) == cell
        };
        let Some(before) = self
            .players
            .read()
            .await
            .get(&player_id)
            .filter(|p| in_path(p))
            .map(|p| p.health)
        else {
            return;
        };
        self.apply_debuff_damage(vec![(player_id, damage, "mine cart".to_string())])
            .await;
        let Some((at, health)) = self
            .players
            .read()
            .await
            .get(&player_id)
            .map(|p| (p.position, p.health))
        else {
            return;
        };
        self.publish_nearby(
            &at,
            floor,
            ServerMessage::CartStruckPlayer {
                player_id,
                damage: before.saturating_sub(health),
                health,
            },
            None,
        )
        .await;
    }
}
