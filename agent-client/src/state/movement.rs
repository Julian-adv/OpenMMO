use super::*;

/// A resolved `move` target.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveTarget {
    Character { id: PlayerId, name: String },
    Monster { id: String },
    GroundItem { instance_id: u64, name: String },
    Prop { prop_id: u32 },
    Chest { selector: String },
    Dungeon { id: String, name: String },
}

/// Why a `move` target did not resolve.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveTargetError {
    /// A monster species where an id belongs, with the ids that match and
    /// how far away each one is.
    SpeciesNotId {
        species: String,
        candidates: Vec<(String, f32)>,
    },
    /// A well-formed monster id that is no longer in sight.
    MonsterGone { id: String },
    Unknown {
        asked: String,
        addressable: Vec<String>,
    },
}

fn looks_like_monster_id(s: &str) -> bool {
    s.strip_prefix(['m', 'M'])
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

impl SharedState {
    /// Resolve a visible `move` target by id shape, then by exact name.
    pub fn resolve_move_target(&self, raw: &str) -> Result<MoveTarget, MoveTargetError> {
        let asked = raw.trim();

        if looks_like_monster_id(asked) {
            return match self
                .monsters_on_my_floor()
                .find(|m| m.id.eq_ignore_ascii_case(asked))
            {
                Some(m) => Ok(MoveTarget::Monster { id: m.id.clone() }),
                None => Err(MoveTargetError::MonsterGone {
                    id: asked.to_string(),
                }),
            };
        }

        if let Ok(n) = asked.parse::<u64>() {
            if let Some((_, item)) = self
                .ground_items_in_sight()
                .iter()
                .find(|(_, i)| i.instance_id == n)
            {
                return Ok(MoveTarget::GroundItem {
                    instance_id: item.instance_id,
                    name: item.item_def_id.clone(),
                });
            }
            // Players before props: their id ranges overlap (prop ids are
            // room indexes from 0, player ids count from 1), and arrival
            // events teach the model numeric character ids.
            if let Some((id, p)) = self.players_on_my_floor().find(|(id, _)| id.get() == n) {
                return Ok(MoveTarget::Character {
                    id: *id,
                    name: p.name.clone(),
                });
            }
            if let Some(b) = self
                .breakables_in_sight()
                .iter()
                .find(|b| u64::from(b.prop_id) == n)
            {
                return Ok(MoveTarget::Prop { prop_id: b.prop_id });
            }
            return Err(self.unknown_target(asked));
        }

        if let Some((id, _)) = self.resolve_nearby_player(asked) {
            return Ok(MoveTarget::Character {
                id,
                name: self.player_display_name(&id),
            });
        }

        if let Some(d) = self.dungeon_named(asked) {
            return Ok(MoveTarget::Dungeon {
                id: d.id.clone(),
                name: d.name.clone(),
            });
        }

        if asked.to_lowercase().contains("chest") && !self.chests_in_sight().is_empty() {
            return Ok(MoveTarget::Chest {
                selector: asked.to_string(),
            });
        }

        // A species name, not an id. Monsters are only ever addressed by id,
        // so hand back the ids that match instead of guessing which one.
        // Checked before ground items, whose loose matcher below would
        // otherwise swallow "goblin" for a goblin_sword lying nearby.
        let candidates = self.monster_ids_of_species(asked);
        if !candidates.is_empty() {
            return Err(MoveTargetError::SpeciesNotId {
                species: asked.to_string(),
                candidates,
            });
        }

        if let Some((instance_id, name)) = self.ground_item_named(asked) {
            return Ok(MoveTarget::GroundItem { instance_id, name });
        }

        Err(self.unknown_target(asked))
    }

    /// Ids and distances of the monsters in sight of a given type, nearest
    /// first — what a species-instead-of-id mistake gets told to use.
    fn monster_ids_of_species(&self, species: &str) -> Vec<(String, f32)> {
        let Some(sp) = self.self_player.as_ref() else {
            return Vec::new();
        };
        let mut found: Vec<(String, f32)> = self
            .monsters_on_my_floor()
            .filter(|m| m.monster_type.eq_ignore_ascii_case(species))
            .map(|m| (m.id.clone(), m.position.dist_xz_sq(&sp.position).sqrt()))
            .collect();
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        found
    }

    /// A target that matched nothing, carrying a sample of what would have.
    fn unknown_target(&self, asked: &str) -> MoveTargetError {
        let mut addressable: Vec<String> = self
            .players_on_my_floor()
            .filter(|(_, p)| self.self_player_id.as_ref() != Some(&p.id))
            .map(|(_, p)| p.name.clone())
            .take(4)
            .collect();
        addressable.extend(self.monsters_on_my_floor().map(|m| m.id.clone()).take(4));
        addressable.extend(
            self.ground_items_in_sight()
                .iter()
                .take(3)
                .map(|(_, i)| format!("{} [id {}]", i.item_def_id, i.instance_id)),
        );
        addressable.extend(
            self.world_cache
                .read()
                .unwrap()
                .all_dungeons()
                .iter()
                .map(|d| d.name.clone()),
        );
        MoveTargetError::Unknown {
            asked: asked.to_string(),
            addressable,
        }
    }

    /// Abort a running follow loop, if any. Returns the name that was being
    /// followed. A loop that already ended left its own note, so it does not
    /// count as cancelled.
    pub fn cancel_follow(&mut self) -> Option<String> {
        let (name, handle) = self.follow_task.take()?;
        if handle.is_finished() {
            return None;
        }
        handle.abort();
        Some(name)
    }

    /// This agent's own tip hat, if one is set down.
    pub fn own_tip_hat(&self) -> Option<&onlinerpg_shared::tip_hat::TipHat> {
        self.self_player_id
            .and_then(|id| self.tip_hats.values().find(|h| h.owner == id))
    }

    /// Fold our stall and pick up our tip hat before walking off — the net
    /// for a departure the LLM did not wrap up itself.
    pub async fn pack_up_placeables(&mut self, label: &str) {
        if self.own_stall().is_some() {
            tracing::info!("[{label}] Stall still out — packing it up");
            let pack = ClientMessage::ChatMessage {
                message: "/pack_stall".to_string(),
            };
            if let Err(e) = self.send_command(pack).await {
                tracing::error!("[{label}] Failed to send /pack_stall: {e}");
            }
        }
        if self.own_tip_hat().is_some() {
            let hat = self
                .self_bag
                .iter()
                .find(|i| crate::item_defs::get(&i.item_def_id).is_some_and(|d| d.is_tip_hat()))
                .map(|i| i.instance_id);
            if let Some(instance_id) = hat {
                tracing::info!("[{label}] Tip hat still out — picking it up");
                let cmd = ClientMessage::UseItem { instance_id };
                if let Err(e) = self.send_command(cmd).await {
                    tracing::error!("[{label}] Failed to pick up the tip hat: {e}");
                }
            }
        }
    }

    /// Our floor as a passability cache index, for path queries. Standing on a
    /// stair shaft this is the floor the shaft's cells are keyed to, which is
    /// not always the floor we are nearest — see `pathfinding::start_floor_at`.
    pub fn passability_floor(&self) -> u8 {
        let floor = passability_floor_for_level(self.self_floor_level);
        if self.self_floor_level >= 0 {
            return floor;
        }
        let Some(position) = self.self_player.as_ref().map(|p| p.position) else {
            return floor;
        };
        if onlinerpg_shared::dungeon::entrance_at(position.x, position.z).is_none() {
            return floor;
        }
        let world = self.world_cache.read().unwrap();
        pathfinding::start_floor_at(
            world.passability_cache(),
            position.x,
            position.z,
            position.y,
        )
    }

    /// The action's opt-out over `always_sprint`, then the server's own hunger
    /// gate (shared `can_sprint`) so both sims agree on our speed. With no
    /// hunger data yet, let the server judge.
    pub fn sprint_allowed(&self, requested: Option<bool>) -> bool {
        requested.unwrap_or(self.always_sprint)
            && self
                .self_hunger
                .is_none_or(|(satiation, _)| onlinerpg_shared::hunger::can_sprint(satiation))
    }

    pub async fn request_move(
        &mut self,
        x: f32,
        z: f32,
        background: bool,
        sprint: Option<bool>,
    ) -> anyhow::Result<u32> {
        self.send_flagged_command(
            ClientMessage::PlayerMoveGoal {
                request_id: 0,
                x,
                z,
                sprinting: self.sprint_allowed(sprint),
                stop_at_entrance: false,
            },
            background,
        )
        .await?;
        Ok(self.move_request_id)
    }

    pub(super) fn apply_move_progress(
        &mut self,
        position: Position,
        rotation: f32,
        floor_level: i8,
    ) {
        if let Some(player) = self.self_player.as_mut() {
            player.position = position;
            player.rotation = rotation;
            player.floor_level = floor_level;
        }
        self.adopt_floor_level(floor_level);
        if let Some(id) = self.self_player_id {
            self.latest_player_moves.remove(&id);
        }
    }

    /// Apply the server's monster pose.
    pub(super) fn apply_monster_pose(
        &mut self,
        monster_id: &str,
        position: Position,
        rotation: f32,
        state: MonsterState,
    ) {
        if let Some(m) = self.nearby_monsters.get_mut(monster_id) {
            m.position = position;
            m.rotation = rotation;
            m.state = state;
        }
    }

    /// Apply an authoritative player pose. Supersedes whatever move that player
    /// had buffered, which `drain_events` would otherwise replay after us.
    pub(super) fn apply_player_pose(
        &mut self,
        player_id: &PlayerId,
        position: Position,
        rotation: f32,
        floor_level: i8,
    ) {
        if let Some(p) = self.nearby_players.get_mut(player_id) {
            p.position = position;
            p.rotation = rotation;
            p.floor_level = floor_level;
        }
        self.latest_player_moves.remove(player_id);
    }

    /// Visibility updates remove monsters from the previous floor.
    pub(crate) fn adopt_floor_level(&mut self, floor_level: i8) {
        self.self_floor_level = floor_level;
    }

    /// Clear the monster, pending movement, and sighting on every removal path.
    pub(super) fn forget_monster(&mut self, id: &str) {
        self.nearby_monsters.remove(id);
        self.latest_monster_moves.remove(id);
        self.sighted_pois.remove(&format!("m:{id}"));
    }

    /// Relocation invalidates any active walk.
    pub(super) fn relocate_self(&mut self, position: Position, rotation: f32, floor_level: i8) {
        self.apply_move_progress(position, rotation, floor_level);
        self.relocations = self.relocations.wrapping_add(1);
    }

    /// Our own pose mirror. `send_command` writes it optimistically on
    /// InteractObject/StopInteraction; the server echo and rejection
    /// converge it.
    /// The chair we are sitting on, if any.
    pub fn own_chair(&self) -> Option<u32> {
        self.self_player
            .as_ref()
            .filter(|p| p.object_type.as_deref() == Some(SIT_OBJECT_TYPE))
            .and_then(|p| p.object_id)
    }

    pub(super) fn set_self_pose(&mut self, object_type: Option<String>, object_id: Option<u32>) {
        if object_type.as_deref() != Some(MUSIC_EMOTE) {
            self.recital = None;
        }
        if let Some(p) = self.self_player.as_mut() {
            p.object_type = object_type;
            p.object_id = object_id;
        }
    }

    /// Whether the cell holding `(x, z)` leaves a mover a legal step out.
    pub fn cell_open(&self, x: f32, z: f32, floor: u8) -> bool {
        self.world_cache.read().unwrap().is_walkable(x, z, floor)
    }

    /// World XZ of the `(type_id, object_id)` furniture placement near `(x, z)`.
    pub fn furniture_position(
        &self,
        type_id: &str,
        object_id: u32,
        x: f32,
        z: f32,
    ) -> Option<(f32, f32)> {
        // Covers the gap between a piece and whoever interacts with it, and
        // disambiguates same-id placements from other regions.
        const RESOLVE_RADIUS: f32 = 3.0;
        let world = self.world_cache.read().unwrap();
        world
            .furniture_placement_near(type_id, object_id, x, z, RESOLVE_RADIUS)
            .map(|p| (p.x, p.z))
    }

    /// A goal for walking toward `(x, z)`: the point itself, or — when its
    /// cell is sealed (furniture swallows the cell a bed pose is authored
    /// on) — the centre of the nearest open neighbouring cell.
    pub fn walkable_near(&self, x: f32, z: f32, floor: u8) -> (f32, f32) {
        let world = self.world_cache.read().unwrap();
        let cache = world.passability_cache();
        if !pathfinding::is_cell_sealed(cache, x, z, floor, None) {
            return (x, z);
        }
        let (cx, cz) = (x.floor() + 0.5, z.floor() + 0.5);
        let d2 = |(nx, nz): (f32, f32)| (nx - x).powi(2) + (nz - z).powi(2);
        (-1..=1i32)
            .flat_map(|dz| (-1..=1i32).map(move |dx| (dx, dz)))
            .filter(|&d| d != (0, 0))
            .map(|(dx, dz)| (cx + dx as f32, cz + dz as f32))
            .filter(|&(nx, nz)| !pathfinding::is_cell_sealed(cache, nx, nz, floor, None))
            .min_by(|&a, &b| d2(a).total_cmp(&d2(b)))
            .unwrap_or((x, z))
    }

    /// Whether a wall stands between us and a point, as the server judges
    /// every blow.
    pub fn attack_line_blocked(&self, to_x: f32, to_z: f32) -> bool {
        let Some(from) = self.self_player.as_ref().map(|p| p.position) else {
            return false;
        };
        let floor = self.passability_floor();
        let world = self.world_cache.read().unwrap();
        pathfinding::attack_line_blocked(
            world.passability_cache(),
            from.x,
            from.z,
            to_x,
            to_z,
            floor,
        )
    }

    /// The passability floor a coordinate move walks on: the storey the LLM
    /// named, else the floor we stand on — or why the coordinate is no goal
    /// there. Refusing up front spares a walk that could only end at the
    /// nearest wall.
    pub fn resolve_goal_floor(&self, x: f32, z: f32, storey: Option<i32>) -> Result<u8, String> {
        let here = self.self_floor_level;
        let Some(storey) = storey else {
            if here > 0 && self.storey_at(x, z, here).is_none() {
                return Err(format!(
                    "it is not on the {} you are standing on — {FLOOR_ZERO_HINT}",
                    storey_name(here as u8)
                ));
            }
            if here < 0 {
                if let Some(d) = self.dungeon_here().filter(|d| !d.footprint_contains(x, z)) {
                    return Err(format!(
                        "it is not on floor {} of {} — it lies outside the dungeon. Come back \
                         up with {{\"type\": \"move\", \"depth\": 0}} first",
                        here.unsigned_abs(),
                        d.name
                    ));
                }
            }
            return Ok(self.passability_floor());
        };
        if here < 0 {
            return Err(
                "\"floor\" names a building storey, but you are underground — come back \
                        up with {\"type\": \"move\", \"depth\": 0} first"
                    .to_string(),
            );
        }
        if storey < 0 {
            return Err(
                "a floor below 0 is a dungeon floor — name the dungeon with \"depth\" instead"
                    .to_string(),
            );
        }
        let Ok(floor) = i8::try_from(storey) else {
            return Err(format!("there is no floor {storey} anywhere"));
        };
        if floor > 0 && self.storey_at(x, z, floor).is_none() {
            return Err(format!(
                "there is no {} at that spot",
                storey_name(floor as u8)
            ));
        }
        Ok(passability_floor_for_level(floor))
    }

    /// Find a smoothed path from current position to the goal.
    pub fn find_path_to(&self, goal_x: f32, goal_z: f32, goal_floor: u8) -> PathResult {
        let (start_x, start_z) = match &self.self_player {
            Some(p) => (p.position.x, p.position.z),
            None => {
                return PathResult {
                    waypoints: Vec::new(),
                    found: false,
                    termination: pathfinding::PathTermination::Unreachable,
                }
            }
        };
        let start_floor = self.passability_floor();
        let max_nodes = path_max_nodes(start_floor, goal_floor);
        let world = self.world_cache.read().unwrap();
        pathfinding::find_and_smooth_path(
            start_x,
            start_z,
            start_floor,
            goal_x,
            goal_z,
            goal_floor,
            world.passability_cache(),
            max_nodes,
        )
    }

    /// Build a facing request toward
    /// the monster. Mirrors the web client's pre-attack position-sync, so
    /// the swing animation orients toward the target. Returns `None` if
    /// either the agent or the monster isn't currently known.
    pub fn face_monster_command(&self, monster_id: &str) -> Option<ClientMessage> {
        let target_pos = self.nearby_monsters.get(monster_id)?.position;
        self.face_position_command(target_pos)
    }

    /// Like `face_monster_command`, but toward another player or NPC — a
    /// position-sync that rotates us to face them, e.g. after walking up
    /// to someone for a conversation.
    pub fn face_player_command(&self, player_id: &PlayerId) -> Option<ClientMessage> {
        let target_pos = self.nearby_players.get(player_id)?.position;
        self.face_position_command(target_pos)
    }

    /// Position-sync at the current location, rotated to face `target_pos`.
    fn face_position_command(&self, target_pos: Position) -> Option<ClientMessage> {
        let self_player = self.self_player.as_ref()?;
        let to_target = crate::geom::PlanarDelta::between(&self_player.position, &target_pos);
        Some(ClientMessage::PlayerFace {
            rotation: to_target.rotation(),
        })
    }
}
