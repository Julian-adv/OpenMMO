use super::*;

const COMBAT_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const ASSIST_RANGE_M: f32 = 20.0;

impl SharedState {
    pub(crate) fn dialogue_only(&self) -> bool {
        self.self_player.as_ref().is_some_and(|player| {
            player.is_official_npc && crate::shop_info::dialogue_only(&player.name)
        })
    }

    pub(crate) fn dialogue_ignores_event(&self, event: &ServerMessage) -> bool {
        self.dialogue_only()
            && matches!(
                event,
                ServerMessage::MonsterSpawned { .. }
                    | ServerMessage::MonsterMoved { .. }
                    | ServerMessage::MonsterRemoved { .. }
                    | ServerMessage::MonsterDead { .. }
                    | ServerMessage::MonsterAttackedPlayer { .. }
                    | ServerMessage::MonsterSkillStarted { .. }
                    | ServerMessage::MonsterSkillStruck { .. }
                    | ServerMessage::MonsterGroundStrike { .. }
                    | ServerMessage::PlayerAttacked { .. }
                    | ServerMessage::PlayerAttackRejected { .. }
                    | ServerMessage::PlayerAttackStopped { .. }
                    | ServerMessage::PlayerDead { .. }
                    | ServerMessage::PlayerHealthUpdate { .. }
                    | ServerMessage::XpGained { .. }
                    | ServerMessage::DaggerDoubleSlashStarted { .. }
                    | ServerMessage::DaggerDoubleSlashRejected { .. }
                    | ServerMessage::DaggerDoubleSlashSkipped { .. }
            )
    }

    fn record_self_combat(&mut self, monster_id: &str) {
        self.combat_monsters
            .retain(|_, at| at.elapsed() < COMBAT_IDLE_TIMEOUT);
        self.combat_monsters
            .insert(monster_id.into(), tokio::time::Instant::now());
    }

    /// The nearest live monster that recently struck a self-defending
    /// official NPC and is still within weapon reach on its floor.
    pub(crate) fn self_defense_target(&self) -> Option<String> {
        let player = self
            .live_official_npc()
            .filter(|p| crate::shop_info::self_defense(&p.name))?;
        let reach = self.attack_approach_range() + onlinerpg_shared::reach::REACH_SLACK_METERS;
        self.nearest_recent_combat(player, reach)
    }

    /// Whether we join the fights of those around us and stow our weapon.
    pub(crate) fn protects(&self) -> bool {
        self.live_official_npc()
            .is_some_and(|p| crate::shop_info::protects(&p.name))
    }

    /// The nearest monster in a recent fight a protector should run to.
    pub(crate) fn assist_target(&self) -> Option<String> {
        let player = self.live_official_npc().filter(|_| self.protects())?;
        self.nearest_recent_combat(player, ASSIST_RANGE_M)
    }

    fn live_official_npc(&self) -> Option<&Player> {
        self.self_player
            .as_ref()
            .filter(|p| p.is_official_npc && p.health > 0)
    }

    fn nearest_recent_combat(&self, player: &Player, range: f32) -> Option<String> {
        self.recent_combat_monsters()
            .filter_map(|(id, monster)| {
                let monster = monster.filter(|m| live_on_floor(m, player.floor_level))?;
                let dist_sq = player.position.dist_xz_sq(&monster.position);
                (dist_sq <= range.powi(2)).then_some((dist_sq, id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id.clone())
    }

    /// A blow traded with `player_id`: ours always counts, and a protector
    /// takes up the fight of anyone within `ASSIST_RANGE_M`.
    pub(super) fn record_combat(&mut self, monster_id: &str, player_id: &PlayerId) {
        let mine = self.self_player_id.as_ref() == Some(player_id);
        let near = || {
            let me = self.live_official_npc()?;
            let other = self.nearby_players.get(player_id)?;
            Some(
                other.floor_level == me.floor_level
                    && other.position.dist_xz_sq(&me.position) <= ASSIST_RANGE_M.powi(2),
            )
        };
        if mine || (self.protects() && near() == Some(true)) {
            self.record_self_combat(monster_id);
        }
    }

    /// A chase that failed leaves the monster alone until it strikes again.
    pub(crate) fn forget_combat(&mut self, monster_id: &str) {
        self.combat_monsters.remove(monster_id);
    }

    /// Draw or put away the main-hand weapon; `None` when already so.
    pub(crate) fn weapon_command(&self, armed: bool) -> Option<onlinerpg_shared::ClientMessage> {
        use onlinerpg_shared::inventory::EquipSlot;
        if armed == self.main_hand_weapon().is_some() {
            return None;
        }
        if !armed {
            return Some(onlinerpg_shared::ClientMessage::UnequipItem {
                slot: EquipSlot::MainHand,
            });
        }
        self.self_bag
            .iter()
            .find(|item| {
                crate::item_defs::get(&item.item_def_id).is_some_and(|def| {
                    def.equip_slot == Some(EquipSlot::MainHand)
                        && def.category.as_deref() == Some("weapon")
                })
            })
            .map(|item| onlinerpg_shared::ClientMessage::EquipItem {
                instance_id: item.instance_id,
            })
    }

    pub(crate) fn schedule_combat_active(&self) -> bool {
        let Some(player) = self.live_official_npc() else {
            return false;
        };
        self.recent_combat_monsters()
            .any(|(_, monster)| monster.is_none_or(|m| live_on_floor(m, player.floor_level)))
    }

    /// Monsters in a fight with us within the idle timeout, with their
    /// current state when still in sight.
    fn recent_combat_monsters(&self) -> impl Iterator<Item = (&String, Option<&Monster>)> {
        self.combat_monsters
            .iter()
            .filter(|(_, at)| at.elapsed() < COMBAT_IDLE_TIMEOUT)
            .map(|(id, _)| (id, self.nearby_monsters.get(id)))
    }

    /// How much farther than us a named player in sight stands from `(x, z)`.
    pub(crate) fn lag_of_named(&self, name: &str, x: f32, z: f32) -> Option<f32> {
        let me = self.self_player.as_ref()?.position;
        let target = onlinerpg_shared::Position { x, y: me.y, z };
        self.players_on_my_floor()
            .map(|(_, player)| player)
            .find(|player| player.name == name)
            .map(|player| {
                player.position.dist_xz_sq(&target).sqrt() - me.dist_xz_sq(&target).sqrt()
            })
    }

    pub(crate) fn is_dead(&self) -> bool {
        self.self_player.as_ref().is_some_and(|p| p.health == 0)
    }

    /// Characters on our own floor. Someone a floor above is a dot straight
    /// overhead, not a neighbour, so nothing the LLM sees or names should
    /// reach them.
    pub(super) fn players_on_my_floor(&self) -> impl Iterator<Item = (&PlayerId, &Player)> {
        self.nearby_players
            .iter()
            .filter(|(_, p)| p.floor_level == self.self_floor_level)
    }

    /// Monsters on our own floor — cross-floor ones read to the LLM as
    /// phantom respawns.
    pub(super) fn monsters_on_my_floor(&self) -> impl Iterator<Item = &Monster> {
        self.nearby_monsters
            .values()
            .filter(|m| m.floor_level == self.self_floor_level)
    }

    /// Emit a [Sighted] event when any point of interest — a monster, dropped
    /// loot, a dungeon entrance — enters EVENT_DELIVERY_RADIUS on our floor, and
    /// forget it once it drifts well past the edge so a re-entry announces
    /// again. Without this the agent walks straight past everything between
    /// scheduled turns. Only an aggressive monster wakes the driver; the rest
    /// ride to the next prompt so a long walk isn't cut every few metres.
    pub(super) fn check_sightings(&mut self) {
        let (self_pos, self_floor) = match self.self_player.as_ref() {
            Some(p) => (p.position, self.self_floor_level),
            None => return,
        };

        // (typed key, description, wakes_driver)
        let mut newly: Vec<(String, String, bool)> = Vec::new();
        // Keys still close enough to stay "seen" — a wider ring than the entry
        // radius so a POI hovering at the edge doesn't announce every tick.
        let mut nearby: HashSet<String> = HashSet::new();
        let dialogue_only = self.dialogue_only();
        let sighted = &self.sighted_pois;
        // Ring bookkeeping shared by every POI kind; hands the key back only
        // when the POI just entered sight.
        let mut track = |key: String| -> Option<String> {
            let new = !sighted.contains(&key);
            nearby.insert(key.clone());
            new.then_some(key)
        };

        // Only aggressive monsters get a sighting event: they are the ones
        // worth waking the driver for, and CURRENT STATE already lists every
        // monster in sight — one event line per grazing mob would flood the
        // prompt in a dense spawn field.
        for (id, m) in &self.nearby_monsters {
            if dialogue_only
                || m.floor_level != self_floor
                || m.state == MonsterState::Dead
                || !m.aggressive
            {
                continue;
            }
            let d = crate::geom::PlanarDelta::to_xz(&self_pos, m.position.x, m.position.z);
            if let Some(key) = track(format!("m:{id}")) {
                newly.push((
                    key,
                    format!(
                        "[Sighted] {} [{id}] HP {}/{} — at ({:.0}, {:.0}), {:.0}m {}.",
                        crate::monster_defs::monster_name(&m.monster_type),
                        m.health,
                        m.max_health,
                        m.position.x,
                        m.position.z,
                        d.dist,
                        compass(d.dx, d.dz),
                    ),
                    true,
                ));
            }
        }

        for (iid, item) in &self.ground_items {
            if item.floor_level != self_floor {
                continue;
            }
            // Our own drop: we put it there, announcing it is pure noise.
            if self.self_player_id.is_some() && item.dropped_by == self.self_player_id {
                continue;
            }
            let d = crate::geom::PlanarDelta::to_xz(&self_pos, item.position.x, item.position.z);
            if let Some(key) = track(format!("i:{iid}")) {
                newly.push((
                    key,
                    format!(
                        "[Sighted] loot on the ground: {} [id {iid}] — at ({:.0}, {:.0}), {:.0}m {}.",
                        item.item_def_id,
                        item.position.x,
                        item.position.z,
                        d.dist,
                        compass(d.dx, d.dz),
                    ),
                    false,
                ));
            }
        }

        // Dungeon entrances only matter above ground.
        if self_floor >= 0 {
            let wc = self.world_cache.read().unwrap();
            for dg in wc.all_dungeons() {
                let d = crate::geom::PlanarDelta::to_xz(&self_pos, dg.entrance.x, dg.entrance.z);
                if d.dist > EVENT_DELIVERY_RADIUS {
                    continue;
                }
                if let Some(key) = track(format!("d:{}", dg.name)) {
                    newly.push((
                        key,
                        format!(
                            "[Sighted] {} entrance ({} floors) — at ({:.0}, {:.0}), {:.0}m {}.",
                            dg.name,
                            dg.max_depth(),
                            dg.entrance.x,
                            dg.entrance.z,
                            d.dist,
                            compass(d.dx, d.dz)
                        ),
                        false,
                    ));
                }
            }
        }

        // Re-entry announces again after the subject leaves the active view.
        self.sighted_pois.retain(|k| nearby.contains(k));

        for (key, note, wake) in newly {
            self.sighted_pois.insert(key);
            if wake {
                self.push_ambient_event(note);
            } else {
                self.push_ambient_event_quiet(note);
            }
        }
    }

    /// Remember an item on the ground.
    pub(crate) fn remember_ground_item(&mut self, item: GroundItem) {
        self.ground_items.insert(item.instance_id, item);
    }

    /// One ground item by instance id.
    pub fn ground_item(&self, instance_id: u64) -> Option<&GroundItem> {
        self.ground_items.get(&instance_id)
    }

    /// Who else alone may pick this ground item up for now.
    pub fn loot_claimant(&self, item: &GroundItem) -> Option<String> {
        let owner = item.claimed_by?;
        (self.self_player_id != Some(owner)).then(|| self.visible_name(&owner))
    }

    /// Active ground items on our floor, closest first.
    pub fn ground_items_in_sight(&self) -> Vec<(f32, &GroundItem)> {
        let Some(sp) = self.self_player.as_ref() else {
            return Vec::new();
        };
        let mut in_sight: Vec<_> = self
            .ground_items
            .values()
            .filter(|item| item.floor_level == self.self_floor_level)
            .map(|item| (item.position.dist_xz_sq(&sp.position), item))
            .collect();
        in_sight.sort_by(|a, b| a.0.total_cmp(&b.0));
        in_sight
    }

    /// The ground item in sight that `asked` names: exact def id or display
    /// name, then substring, nearest first. The one matcher for pickup and
    /// move-by-name, so what pickup would take, move walks to.
    pub fn ground_item_named(&self, asked: &str) -> Option<(u64, String)> {
        let in_sight = self.ground_items_in_sight();
        let ids: Vec<&str> = in_sight
            .iter()
            .map(|(_, i)| i.item_def_id.as_str())
            .collect();
        let def_id = crate::item_defs::resolve_named(&ids, asked)?;
        // The nearest one we may take, else the nearest at all.
        let (_, item) = in_sight
            .iter()
            .filter(|(_, i)| i.item_def_id == def_id)
            .min_by_key(|(_, i)| self.loot_claimant(i).is_some())?;
        Some((item.instance_id, item.item_def_id.clone()))
    }
}

fn live_on_floor(monster: &Monster, floor: i8) -> bool {
    monster.health > 0 && monster.state != MonsterState::Dead && monster.floor_level == floor
}
