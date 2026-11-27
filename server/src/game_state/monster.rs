use crate::types::{MonsterLifecycle, MonsterState, PlayerId, Position, ServerMessage};
use tracing::{debug, info};

pub(super) const NO_SPAWN_MARGIN: f32 = 30.0;
const AMBIENT_SPAWN_METERS_PER_LEVEL: f32 = 70.0;
const DESPAWN_SCAN_CHUNK: usize = 4_000;

#[derive(Default)]
pub(crate) struct MonsterRegistry {
    monsters: std::collections::HashMap<String, crate::types::Monster>,
    /// Canonical X positions keep queries correct across the world seam.
    cells: super::SpatialIndex<String>,
    /// Damage per player, in first-hit order, for the kill XP split. Kept off
    /// `Monster` so its frequent clones don't copy it.
    damage: std::collections::HashMap<String, Vec<(PlayerId, u32)>>,
    died_at: std::collections::HashMap<String, u64>,
}

impl MonsterRegistry {
    /// Nearby candidates; callers check exact distance.
    pub(crate) fn near<'a>(
        &'a self,
        position: &'a Position,
    ) -> impl Iterator<Item = &'a crate::types::Monster> {
        self.near_within(position, super::EVENT_DELIVERY_RADIUS)
    }

    /// As `near`, for a smaller `radius`.
    pub(crate) fn near_within<'a>(
        &'a self,
        position: &'a Position,
        radius: f32,
    ) -> impl Iterator<Item = &'a crate::types::Monster> {
        self.cells
            .keys_near(position, radius)
            .filter_map(|id| self.monsters.get(id.as_str()))
    }

    pub(crate) fn insert(
        &mut self,
        id: String,
        monster: crate::types::Monster,
    ) -> Option<crate::types::Monster> {
        // Remove the old index entry before replacing the monster.
        let replaced = self.remove(&id);
        let position = monster.position;
        self.monsters.insert(id.clone(), monster);
        self.cells.insert(id, &position);
        replaced
    }

    pub(crate) fn remove(&mut self, id: &str) -> Option<crate::types::Monster> {
        let removed = self.monsters.remove(id);
        self.damage.remove(id);
        self.died_at.remove(id);
        if let Some(monster) = &removed {
            self.cells.remove(id, &monster.position);
        }
        removed
    }

    pub(crate) fn get(&self, id: &str) -> Option<&crate::types::Monster> {
        self.monsters.get(id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut crate::types::Monster> {
        self.monsters.get_mut(id)
    }

    pub(crate) fn values(
        &self,
    ) -> std::collections::hash_map::Values<'_, String, crate::types::Monster> {
        self.monsters.values()
    }

    /// Monsters on the server, corpses included.
    pub(crate) fn len(&self) -> usize {
        self.monsters.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.monsters.is_empty()
    }

    pub(crate) fn alive_near(&self, position: &Position, floor: i8) -> usize {
        self.alive_within(position, floor, super::EVENT_DELIVERY_RADIUS)
            .count()
    }

    pub(crate) fn alive_within<'a>(
        &'a self,
        position: &'a Position,
        floor: i8,
        radius: f32,
    ) -> impl Iterator<Item = &'a crate::types::Monster> {
        self.near_within(position, radius).filter(move |monster| {
            monster.floor_level == floor
                && monster.state != MonsterState::Dead
                && monster.position.dist_xz_sq(position) <= radius * radius
        })
    }

    pub(crate) fn mark_dead(&mut self, id: &str) {
        if let Some(monster) = self.monsters.get_mut(id) {
            monster.state = MonsterState::Dead;
            self.died_at
                .entry(id.to_string())
                .or_insert_with(super::GameState::now_ms);
        }
    }

    /// `None` for a corpse that never went through `mark_dead`.
    pub(crate) fn dead_for_ms(&self, id: &str, now: u64) -> Option<u64> {
        self.died_at.get(id).map(|at| now.saturating_sub(*at))
    }

    pub(crate) fn record_damage(&mut self, id: &str, player_id: PlayerId, dealt: u32) {
        let log = self.damage.entry(id.to_string()).or_default();
        match log.iter_mut().find(|(dealer, _)| *dealer == player_id) {
            Some((_, total)) => *total += dealt,
            None => log.push((player_id, dealt)),
        }
    }

    pub(crate) fn take_damage(&mut self, id: &str) -> Vec<(PlayerId, u32)> {
        self.damage.remove(id).unwrap_or_default()
    }

    /// Move the monster and its spatial index entry together.
    pub(crate) fn set_position(
        &mut self,
        id: &str,
        position: Position,
    ) -> Option<&crate::types::Monster> {
        let monster = self.monsters.get_mut(id)?;
        let old_position = monster.position;
        monster.position = position;
        self.cells.moved(id, &old_position, &position);
        Some(monster)
    }

    /// Verify the spatial index against the registry.
    #[cfg(test)]
    pub(crate) fn cell_index_matches_map(&self) -> bool {
        let mut expected = super::SpatialIndex::default();
        for (id, monster) in &self.monsters {
            expected.insert(id.clone(), &monster.position);
        }
        self.cells.matches(&expected)
    }
}

impl std::ops::Index<&str> for MonsterRegistry {
    type Output = crate::types::Monster;

    fn index(&self, id: &str) -> &Self::Output {
        &self.monsters[id]
    }
}

impl super::GameState {
    fn find_ambient_rule(
        monster_type: &str,
    ) -> Option<&'static crate::world_config::AmbientSpawnRule> {
        crate::world_config::world_config()
            .ambient_spawns
            .iter()
            .find(|r| r.monster_type == monster_type)
    }

    /// Spawn and publish a monster; dungeon slots have their own population limit.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_monster(
        &self,
        monster_type: String,
        position: Position,
        rotation: f32,
        floor_level: i8,
        lifecycle: MonsterLifecycle,
        level_override: Option<u8>,
        aggressive: bool,
    ) -> Option<crate::types::Monster> {
        self.spawn_monster_with(
            monster_type,
            position,
            rotation,
            floor_level,
            lifecycle,
            level_override,
            aggressive,
            None,
            None,
        )
        .await
    }

    /// `spawn_monster` with the siege an estate-defense raider carries and a
    /// model scale on top of its type's.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn spawn_monster_with(
        &self,
        monster_type: String,
        position: Position,
        rotation: f32,
        floor_level: i8,
        lifecycle: MonsterLifecycle,
        level_override: Option<u8>,
        aggressive: bool,
        siege: Option<onlinerpg_shared::Siege>,
        scale: Option<f32>,
    ) -> Option<crate::types::Monster> {
        let def = self.monster_defs.get(&monster_type);
        let base_health = def.map(|d| d.max_health()).unwrap_or(10);
        // Depth scaling preserves any higher authored health, including bosses.
        let health = match level_override {
            Some(level) => {
                base_health.max(crate::game::combat::monster_max_health_for_level(level))
            }
            None => base_health,
        };
        let id = {
            let mut ids = self.id_state.write().await;
            ids.next_monster_number += 1;
            format!("m{}", ids.next_monster_number)
        };
        let monster = crate::types::Monster {
            id,
            monster_type: monster_type.clone(),
            position,
            rotation,
            state: MonsterState::Idle,
            health,
            max_health: health,
            floor_level,
            level_override,
            aggressive,
            dormant: def.is_some_and(|d| d.ambush_range > 0.0),
            scale,
            lifecycle,
            last_attack_at: 0,
            siege,
        };

        let mut monsters = self.monsters.write().await;
        if lifecycle == MonsterLifecycle::Ambient
            && Self::find_ambient_rule(&monster_type).is_some()
            && monsters.alive_near(&position, floor_level)
                >= crate::world_config::world_config().max_nearby_monsters as usize
        {
            return None;
        }
        let id = monster.id.clone();
        monsters.insert(id.clone(), monster.clone());
        let total = monsters.len();
        drop(monsters);
        if let Some(body) = def.and_then(|d| d.body).filter(|_| floor_level == 0) {
            if let Some(entry) = body.passability(position.x, position.y, position.z, rotation) {
                self.passability_write()
                    .insert(onlinerpg_shared::monster_body::cache_key(&id), entry);
            }
        }
        info!(
            "Spawned {} {} at ({:.1},{:.1}) (total: {})",
            monster_type, id, position.x, position.z, total
        );

        self.publish_subject_change(ServerMessage::MonsterSpawned {
            monster: monster.clone(),
        });
        Some(monster)
    }

    /// Apply the terrain delta while preserving any spawn height offset.
    pub(super) async fn expected_monster_move_y(
        &self,
        floor_level: i8,
        from: Position,
        to: Position,
    ) -> Option<f32> {
        // Attack cadence reports plenty of unchanged positions.
        if from.x == to.x && from.z == to.z {
            return Some(from.y);
        }
        let (from_ground, to_ground) = if floor_level < 0 {
            let from_entrance = self.dungeon_defs.entrance_at(from.x, from.z)?;
            let to_entrance = self.dungeon_defs.entrance_at(to.x, to.z)?;
            if from_entrance.id != to_entrance.id {
                return None;
            }
            self.ensure_dungeon_runtime(&from_entrance.id).await;
            let dungeons = self.dungeons.read().await;
            let layouts = &dungeons.get(&from_entrance.id)?.layouts;
            let origin = from_entrance.position();
            let depth = floor_level.unsigned_abs();
            let height_at =
                |x, z| onlinerpg_shared::dungeon::floor_height_at(&origin, layouts, depth, x, z);
            (height_at(from.x, from.z)?, height_at(to.x, to.z)?)
        } else {
            (
                self.height_sampler
                    .sample_height(from.x, from.z)
                    .await
                    .ok()?,
                self.height_sampler.sample_height(to.x, to.z).await.ok()?,
            )
        };
        Some(from.y + to_ground - from_ground)
    }

    /// Minimum town distance for an ambient type, derived from its level.
    pub(crate) fn min_ambient_town_distance(&self, monster_type: &str) -> f32 {
        self.monster_defs.get(monster_type).map_or(0.0, |def| {
            f32::from(def.level.saturating_sub(1)) * AMBIENT_SPAWN_METERS_PER_LEVEL
        })
    }

    /// A monster type's level at a dungeon depth; the surface passes its
    /// distance-equivalent depth.
    pub(crate) fn level_at_depth(&self, monster_type: &str, depth: u8) -> u8 {
        onlinerpg_shared::dungeon::monster_level_for_depth(self.def_level(monster_type), depth)
    }

    pub(super) fn def_level(&self, monster_type: &str) -> u8 {
        self.monster_defs
            .get(monster_type)
            .map_or(1, |def| def.level)
    }

    pub(super) async fn despawn_monsters_near(&self, position: &Position) {
        let candidates = self
            .monsters
            .read()
            .await
            .near(position)
            .filter(|monster| monster.lifecycle.despawns_when_unattended())
            .map(|monster| monster.id.clone())
            .collect::<Vec<_>>();
        self.despawn_unwatched_monsters(&candidates).await;
    }

    pub(super) async fn despawn_unwatched_monsters(&self, candidates: &[String]) {
        if candidates.is_empty() {
            return;
        }
        let removed = {
            let players = self.players.read().await;
            let spatial = self.player_spatial_cells.read().await;
            let mut monsters = self.monsters.write().await;
            let expired = candidates
                .iter()
                .filter(|id| {
                    monsters.get(id).is_some_and(|monster| {
                        monster.lifecycle.despawns_when_unattended()
                            && !spatial
                                .keys_near(&monster.position, super::EVENT_DELIVERY_RADIUS)
                                .filter_map(|id| players.get(id))
                                .any(|player| Self::watches(player, monster))
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            expired
                .into_iter()
                .filter_map(|id| {
                    let monster = monsters.remove(&id)?;
                    self.publish_subject_change(ServerMessage::MonsterRemoved {
                        monster_id: id.clone(),
                    });
                    debug!("Despawned unattended monster {}", monster.id);
                    Some((id, monster.monster_type))
                })
                .collect::<Vec<_>>()
        };
        for (id, monster_type) in removed {
            self.release_body(&id, &monster_type);
            self.brain_death(&id).await;
        }
    }

    pub(super) fn watches(player: &crate::types::Player, monster: &crate::types::Monster) -> bool {
        player.floor_level == monster.floor_level
            && monster.position.dist_xz_sq(&player.position) <= super::EVENT_DELIVERY_RADIUS.powi(2)
    }

    pub async fn tick_monster_despawns(&self) {
        let candidates = self
            .monsters
            .read()
            .await
            .values()
            .filter(|monster| monster.lifecycle.despawns_when_unattended())
            .map(|monster| monster.id.clone())
            .collect::<Vec<_>>();
        for chunk in candidates.chunks(DESPAWN_SCAN_CHUNK) {
            self.despawn_unwatched_monsters(chunk).await;
        }
    }

    pub(super) async fn despawn_monsters(&self, expired: Vec<String>) {
        let removed = {
            let mut monsters = self.monsters.write().await;
            expired
                .into_iter()
                .filter_map(|id| {
                    let monster = monsters.remove(&id)?;
                    self.publish_subject_change(ServerMessage::MonsterRemoved {
                        monster_id: id.clone(),
                    });
                    Some((id, monster.monster_type))
                })
                .collect::<Vec<_>>()
        };
        for (id, monster_type) in removed {
            self.release_body(&id, &monster_type);
            self.brain_death(&id).await;
        }
    }

    /// Lift a body's sealed cells once its monster dies or leaves.
    pub(super) fn release_body(&self, monster_id: &str, monster_type: &str) {
        if self
            .monster_defs
            .get(monster_type)
            .is_some_and(|d| d.body.is_some())
        {
            self.passability_write()
                .remove(&onlinerpg_shared::monster_body::cache_key(monster_id));
        }
    }
}
