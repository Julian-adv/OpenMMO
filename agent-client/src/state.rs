use std::collections::{HashMap, HashSet};

use crate::monster_ai::MonsterAiManager;
use onlinerpg_shared::housing::{HouseData, WallDirection};
use onlinerpg_shared::pathfinding::{self, PassabilityCache, PathResult};
use onlinerpg_shared::{Character, ClientMessage, Monster, Player, ServerMessage};
use onlinerpg_shared::{NoSpawnZone, Position};
use onlinerpg_terrain::height::HeightSampler;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

const MAX_EVENTS: usize = 200;
/// Distance threshold for "player appeared nearby" agent events (in game units).
const NEARBY_PLAYER_RADIUS: f32 = 10.0;
/// Don't notify LLM about monster movement beyond 30 game units.
const MONSTER_MOVE_NOTIFY_RADIUS_SQ: f32 = 30.0 * 30.0;

/// How urgently an event needs LLM attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventUrgency {
    /// Must be processed immediately (combat damage to self, death, direct chat, kicked)
    Urgent,
    /// Can wait and be batched with next prompt (world state changes, xp, spawns)
    Routine,
    /// Don't send to LLM at all (high-frequency movement, time sync)
    Noise,
}

/// Shared world data: passability cache and house state.
/// Wrapped in `Arc<RwLock<WorldCache>>` so multiple NPC connections can share it.
pub struct WorldCache {
    passability_cache: PassabilityCache,
    houses: HashMap<String, HouseData>,
}

impl WorldCache {
    pub fn new() -> Self {
        Self {
            passability_cache: PassabilityCache::new(),
            houses: HashMap::new(),
        }
    }

    pub fn passability_cache(&self) -> &PassabilityCache {
        &self.passability_cache
    }

    pub fn add_house(&mut self, house: HouseData) {
        let rp = pathfinding::build_runtime_passability(&house);
        self.passability_cache.insert(house.id.clone(), rp);
        pathfinding::apply_door_overlays(&mut self.passability_cache, &house);
        self.houses.insert(house.id.clone(), house);
    }

    pub fn remove_house(&mut self, house_id: &str) {
        self.houses.remove(house_id);
        self.passability_cache.remove(house_id);
    }

    pub fn update_door(
        &mut self,
        house_id: &str,
        room_index: u32,
        wall_dir: WallDirection,
        segment_index: usize,
        is_open: bool,
    ) {
        if let Some(house) = self.houses.get(house_id) {
            if let Some(room) = house.rooms.get(room_index as usize) {
                pathfinding::update_door_edge(
                    &mut self.passability_cache,
                    house_id,
                    room,
                    wall_dir,
                    segment_index,
                    is_open,
                );
            }
        }
    }
}

/// Shared state between WebSocket reader and Claude driver tasks.
pub struct SharedState {
    pub characters: Vec<Character>,
    pub in_game: bool,
    /// Our own player ID (set on JoinSuccess)
    pub self_player_id: Option<String>,
    /// Our own player state (updated from JoinSuccess, GameState, health updates, etc.)
    pub self_player: Option<Player>,
    /// Known nearby players
    pub nearby_players: HashMap<String, Player>,
    /// Known nearby monsters
    pub nearby_monsters: HashMap<String, Monster>,
    events: Vec<ServerMessage>,
    /// Latest position per monster -- deduplicates high-frequency MonsterMoved events
    latest_monster_moves: HashMap<String, ServerMessage>,
    /// Latest position per player -- deduplicates high-frequency PlayerMoved events
    latest_player_moves: HashMap<String, ServerMessage>,
    /// Latest game time -- only the most recent matters
    latest_time: Option<ServerMessage>,
    /// Players we've already seen within NEARBY_PLAYER_RADIUS -- prevents duplicate events
    seen_nearby_players: HashSet<String>,
    /// Synthetic agent-side events (e.g. "player appeared nearby")
    agent_events: Vec<String>,
    /// Terrain height sampler (shared across NPC connections)
    pub height_sampler: Arc<HeightSampler>,
    /// Shared world cache: passability + houses (shared across NPC connections)
    pub world_cache: Arc<std::sync::RwLock<WorldCache>>,
    /// Current game time: is_night flag from server
    pub is_night: Option<bool>,
    /// Current game hour (0-23)
    pub game_hour: Option<u32>,
    /// Current game minute (0-59)
    pub game_minute: Option<u32>,
    /// Current floor level for the agent
    pub self_floor_level: u8,
    cmd_tx: mpsc::Sender<ClientMessage>,
    /// Notified when an urgent event arrives
    pub urgent_notify: Arc<Notify>,
    /// Monster AI manager for server-assigned monsters
    pub monster_ai: MonsterAiManager,
    /// Pending commands from monster AI and spawn requests
    pending_commands: Vec<ClientMessage>,
    /// No-spawn zones received from server on join
    no_spawn_zones: Vec<NoSpawnZone>,
}

impl SharedState {
    pub fn new(
        characters: Vec<Character>,
        cmd_tx: mpsc::Sender<ClientMessage>,
        height_sampler: Arc<HeightSampler>,
        world_cache: Arc<std::sync::RwLock<WorldCache>>,
    ) -> Self {
        Self {
            characters,
            in_game: false,
            self_player_id: None,
            self_player: None,
            nearby_players: HashMap::new(),
            nearby_monsters: HashMap::new(),
            events: Vec::new(),
            latest_monster_moves: HashMap::new(),
            latest_player_moves: HashMap::new(),
            latest_time: None,
            seen_nearby_players: HashSet::new(),
            agent_events: Vec::new(),
            height_sampler,
            world_cache,
            is_night: None,
            game_hour: None,
            game_minute: None,
            self_floor_level: 0,
            cmd_tx,
            urgent_notify: Arc::new(Notify::new()),
            monster_ai: MonsterAiManager::new(),
            pending_commands: Vec::new(),
            no_spawn_zones: Vec::new(),
        }
    }

    /// Check all nearby players and emit an agent event for any player
    /// that just entered NEARBY_PLAYER_RADIUS for the first time.
    fn check_nearby_player_proximity(&mut self) {
        let self_pos = match self.self_player.as_ref() {
            Some(p) => &p.position,
            None => return,
        };
        let self_id = match self.self_player_id.as_deref() {
            Some(id) => id,
            None => return,
        };

        for (pid, player) in &self.nearby_players {
            if pid.as_str() == self_id {
                continue;
            }
            if self.seen_nearby_players.contains(pid) {
                continue;
            }
            let dx = player.position.x - self_pos.x;
            let dz = player.position.z - self_pos.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist <= NEARBY_PLAYER_RADIUS {
                self.seen_nearby_players.insert(pid.clone());
                self.agent_events.push(format!(
                    "[PlayerNearby] {} Lv.{} appeared {:.1}m away at ({:.1}, {:.1}, {:.1})",
                    player.name,
                    player.level,
                    dist,
                    player.position.x,
                    player.position.y,
                    player.position.z
                ));
                self.urgent_notify.notify_one();
            }
        }
    }

    pub async fn send_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        // Correct Y coordinate for PlayerMove using terrain height
        let msg = if let ClientMessage::PlayerMove {
            mut position,
            rotation,
            ..
        } = msg
        {
            let original_y = position.y;
            match self
                .height_sampler
                .sample_height(position.x, position.z)
                .await
            {
                Ok(terrain_y) => {
                    tracing::debug!(
                        "Height correction: ({:.1}, {:.1}) y: {:.2} -> {:.2}",
                        position.x,
                        position.z,
                        original_y,
                        terrain_y
                    );
                    position.y = terrain_y;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to sample terrain height at ({:.1}, {:.1}): {e}",
                        position.x,
                        position.z
                    );
                }
            }
            // Update local position immediately so subsequent reads don't use stale data
            if let Some(ref mut p) = self.self_player {
                p.position = position.clone();
                p.rotation = rotation;
            }
            ClientMessage::PlayerMove {
                position,
                rotation,
                floor_level: self.self_floor_level as i8,
            }
        } else {
            msg
        };
        self.cmd_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Command channel closed: {e}"))
    }

    /// Send a position sync to correct Y to terrain height.
    /// Should be called after JoinSuccess or PlayerRespawned to snap to ground.
    pub async fn sync_height(&mut self) -> anyhow::Result<()> {
        let Some(ref p) = self.self_player else {
            return Ok(());
        };
        let pos = p.position.clone();
        let rotation = p.rotation;
        self.send_command(ClientMessage::PlayerMove {
            position: pos,
            rotation,
            floor_level: 0,
        })
        .await
    }

    /// Classify how urgent a server event is for LLM processing.
    pub fn classify_event(&self, msg: &ServerMessage) -> EventUrgency {
        let self_id = self.self_player_id.as_deref();
        match msg {
            // Urgent: we are being attacked or we died
            ServerMessage::MonsterAttackedPlayer { player_id, .. } => {
                if self_id == Some(player_id.as_str()) {
                    EventUrgency::Urgent
                } else {
                    EventUrgency::Routine
                }
            }
            ServerMessage::PlayerDead { player_id } => {
                if self_id == Some(player_id.as_str()) {
                    EventUrgency::Urgent
                } else {
                    EventUrgency::Routine
                }
            }
            // Urgent: someone chats (not ourselves)
            ServerMessage::ChatMessage { player_id, .. } => {
                if self_id != Some(player_id.as_str()) {
                    EventUrgency::Urgent
                } else {
                    EventUrgency::Noise
                }
            }
            // Urgent: kicked
            ServerMessage::Kicked { .. } => EventUrgency::Urgent,

            // Urgent: another player attacks a monster (so we can join in)
            ServerMessage::PlayerAttacked { player_id, .. } => {
                if self_id != Some(player_id.as_str()) {
                    EventUrgency::Urgent
                } else {
                    EventUrgency::Routine
                }
            }

            // Routine: world state changes
            ServerMessage::JoinSuccess { .. }
            | ServerMessage::GameState { .. }
            | ServerMessage::PlayerJoined { .. }
            | ServerMessage::PlayerLeft { .. }
            | ServerMessage::MonsterSpawned { .. }
            | ServerMessage::MonsterAssigned { .. }
            | ServerMessage::SpawnMonsterRequest { .. }
            | ServerMessage::MonsterDead { .. }
            | ServerMessage::MonsterRemoved { .. }
            | ServerMessage::XpGained { .. }
            | ServerMessage::PlayerRespawned { .. }
            | ServerMessage::PlayerHealthUpdate { .. }
            | ServerMessage::PlayerTorchToggled { .. } => EventUrgency::Routine,

            // Noise: high-frequency, irrelevant, or housing updates
            ServerMessage::PlayerMoved { .. }
            | ServerMessage::PlayerTeleported { .. }
            | ServerMessage::MonsterMoved { .. }
            | ServerMessage::GameTimeSync { .. }
            | ServerMessage::HouseSpawned { .. }
            | ServerMessage::HousesInArea { .. }
            | ServerMessage::HouseUpdated { .. }
            | ServerMessage::HouseRemoved { .. }
            | ServerMessage::DoorToggled { .. } => EventUrgency::Noise,

            // Auth/character events: routine (handled before game entry)
            _ => EventUrgency::Routine,
        }
    }

    /// Push an event and update tracked state. Returns the urgency of the event.
    pub fn push_event(&mut self, msg: ServerMessage) -> EventUrgency {
        // Update tracked state from certain messages
        match &msg {
            ServerMessage::JoinSuccess { player } => {
                self.in_game = true;
                self.self_player_id = Some(player.id.clone());
                self.self_player = Some(player.clone());
            }
            ServerMessage::GameState { players, monsters } => {
                self.nearby_players = players.clone();
                self.nearby_monsters = monsters.clone();
                // Update self_player from game state
                if let Some(ref self_id) = self.self_player_id {
                    if let Some(p) = players.get(self_id) {
                        self.self_player = Some(p.clone());
                    }
                }
            }
            ServerMessage::PlayerHealthUpdate {
                player_id,
                health,
                max_health,
            } => {
                if self.self_player_id.as_deref() == Some(player_id.as_str()) {
                    if let Some(ref mut p) = self.self_player {
                        p.health = *health;
                        p.max_health = *max_health;
                    }
                }
            }
            ServerMessage::PlayerJoined { player } => {
                self.nearby_players
                    .insert(player.id.clone(), player.clone());
            }
            ServerMessage::PlayerLeft { player_id } => {
                self.nearby_players.remove(player_id);
                self.seen_nearby_players.remove(player_id);
            }
            ServerMessage::MonsterSpawned { monster } => {
                self.nearby_monsters
                    .insert(monster.id.clone(), monster.clone());
            }
            ServerMessage::SpawnMonsterRequest {
                monster_type,
                min_x,
                min_z,
                max_x,
                max_z,
            } => {
                if let Some(pos) = self.find_valid_spawn_position(*min_x, *min_z, *max_x, *max_z) {
                    let mut rng = rand::thread_rng();
                    let rotation = rng.gen_range(0.0..std::f32::consts::TAU);
                    self.pending_commands
                        .push(ClientMessage::RequestSpawnMonster {
                            monster_type: monster_type.clone(),
                            position: pos,
                            rotation,
                        });
                }
            }
            ServerMessage::NoSpawnZones { zones } => {
                self.no_spawn_zones = zones.clone();
            }
            ServerMessage::MonsterAssigned { monster } => {
                self.nearby_monsters
                    .insert(monster.id.clone(), monster.clone());
                self.monster_ai.add_monster(monster);
            }
            ServerMessage::MonsterDead { monster_id } => {
                self.nearby_monsters.remove(monster_id);
                self.monster_ai.handle_monster_dead(monster_id);
            }
            ServerMessage::MonsterRemoved { monster_id } => {
                self.nearby_monsters.remove(monster_id);
                self.monster_ai.remove_monster(monster_id);
            }
            ServerMessage::CharacterCreated { ref character } => {
                self.characters.push(character.clone());
            }
            ServerMessage::PlayerMoved {
                player_id,
                position,
                ..
            } => {
                // Update tracked position for self and nearby players
                if self.self_player_id.as_deref() == Some(player_id.as_str()) {
                    if let Some(ref mut p) = self.self_player {
                        p.position = position.clone();
                    }
                }
                if let Some(p) = self.nearby_players.get_mut(player_id) {
                    p.position = position.clone();
                }
            }
            ServerMessage::MonsterMoved {
                monster_id,
                position,
                ..
            } => {
                if let Some(m) = self.nearby_monsters.get_mut(monster_id) {
                    m.position = position.clone();
                }
            }
            ServerMessage::HouseSpawned { ref house } => {
                self.world_cache.write().unwrap().add_house(house.clone());
            }
            ServerMessage::HousesInArea { ref houses } => {
                let mut world = self.world_cache.write().unwrap();
                for house in houses {
                    world.add_house(house.clone());
                }
            }
            ServerMessage::HouseUpdated { ref house } => {
                self.world_cache.write().unwrap().add_house(house.clone());
            }
            ServerMessage::HouseRemoved { ref house_id } => {
                self.world_cache.write().unwrap().remove_house(house_id);
            }
            ServerMessage::DoorToggled {
                ref house_id,
                room_index,
                ref wall_dir,
                segment_index,
                is_open,
            } => {
                self.world_cache.write().unwrap().update_door(
                    house_id,
                    *room_index,
                    *wall_dir,
                    *segment_index as usize,
                    *is_open,
                );
            }
            // Notify monster AI when a managed monster is attacked
            ServerMessage::PlayerAttacked {
                player_id,
                monster_id,
                hit,
                damage,
                ..
            } => {
                if self.monster_ai.manages(monster_id) {
                    let world = self.world_cache.read().unwrap();
                    let cmds = self.monster_ai.handle_monster_hit(
                        monster_id,
                        player_id,
                        *hit,
                        *damage,
                        world.passability_cache(),
                    );
                    drop(world);
                    self.pending_commands.extend(cmds);
                }
            }
            _ => {}
        }

        // Check if any player just entered the nearby radius
        match &msg {
            ServerMessage::GameState { .. }
            | ServerMessage::PlayerJoined { .. }
            | ServerMessage::PlayerMoved { .. } => {
                self.check_nearby_player_proximity();
            }
            _ => {}
        }

        let urgency = self.classify_event(&msg);

        // Deduplicate high-frequency movement events: keep only latest per entity
        match &msg {
            ServerMessage::MonsterMoved {
                monster_id,
                position,
                ..
            } => {
                // Only forward to LLM if monster is within notification radius
                let dominated_by_distance = self.self_player.as_ref().is_some_and(|sp| {
                    let dx = position.x - sp.position.x;
                    let dz = position.z - sp.position.z;
                    dx * dx + dz * dz > MONSTER_MOVE_NOTIFY_RADIUS_SQ
                });
                if !dominated_by_distance {
                    self.latest_monster_moves.insert(monster_id.clone(), msg);
                }
                return urgency;
            }
            ServerMessage::PlayerMoved { player_id, .. } => {
                self.latest_player_moves.insert(player_id.clone(), msg);
                return urgency;
            }
            ServerMessage::GameTimeSync { datetime, is_night } => {
                let prev_night = self.is_night;
                let prev_hour = self.game_hour;
                let hour = datetime.hour as u32;
                let minute = datetime.minute as u32;
                let night = *is_night;
                self.is_night = Some(night);
                self.game_hour = Some(hour);
                self.game_minute = Some(minute);
                self.latest_time = Some(msg);
                // Detect day/night transition or hour change → wake driver
                if (prev_night.is_some() && prev_night != self.is_night)
                    || (prev_hour.is_some() && prev_hour != self.game_hour)
                {
                    self.agent_events.push(format!(
                        "[TimeChange] It is now {hour:02}:{minute:02} ({}).",
                        if night { "night" } else { "day" }
                    ));
                    self.urgent_notify.notify_one();
                }
                return urgency;
            }
            _ => {}
        }

        self.events.push(msg);

        // Cap buffer size: drop oldest events
        if self.events.len() > MAX_EVENTS {
            let overflow = self.events.len() - MAX_EVENTS;
            self.events.drain(..overflow);
        }

        // Notify Claude driver if urgent
        if urgency == EventUrgency::Urgent {
            self.urgent_notify.notify_one();
        }

        urgency
    }

    pub fn drain_events(&mut self) -> Vec<ServerMessage> {
        let mut events = std::mem::take(&mut self.events);

        // Append latest snapshots
        if let Some(time) = self.latest_time.take() {
            events.push(time);
        }
        events.extend(self.latest_monster_moves.drain().map(|(_, v)| v));
        events.extend(self.latest_player_moves.drain().map(|(_, v)| v));

        events
    }

    /// Drain pending commands (from monster AI reactions, spawn requests, etc.)
    pub fn drain_pending_commands(&mut self) -> Vec<ClientMessage> {
        std::mem::take(&mut self.pending_commands)
    }

    /// Drain synthetic agent-side events (e.g. player proximity alerts).
    pub fn drain_agent_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.agent_events)
    }

    /// Find a smoothed path from current position to the goal.
    pub fn find_path_to(&self, goal_x: f32, goal_z: f32, goal_floor: u8) -> PathResult {
        let (start_x, start_z) = match &self.self_player {
            Some(p) => (p.position.x, p.position.z),
            None => {
                return PathResult {
                    waypoints: Vec::new(),
                    found: false,
                }
            }
        };
        let world = self.world_cache.read().unwrap();
        pathfinding::find_and_smooth_path(
            start_x,
            start_z,
            self.self_floor_level,
            goal_x,
            goal_z,
            goal_floor,
            world.passability_cache(),
            pathfinding::DEFAULT_MAX_NODES,
        )
    }

    /// Find a valid spawn position within the given area.
    /// Tries random positions, rejecting blocked locations (inside houses).
    /// Y coordinate is set to 0; the monster AI will correct via terrain height on first move.
    fn find_valid_spawn_position(
        &self,
        min_x: f32,
        min_z: f32,
        max_x: f32,
        max_z: f32,
    ) -> Option<Position> {
        let world = self.world_cache.read().unwrap();
        let mut rng = rand::thread_rng();
        for _ in 0..10 {
            let x: f32 = rng.gen_range(min_x..max_x);
            let z: f32 = rng.gen_range(min_z..max_z);

            // Reject if inside a house
            if pathfinding::is_movement_blocked(world.passability_cache(), x, z, x, z, 0.0) {
                continue;
            }

            // Reject if inside a no-spawn zone
            if self.no_spawn_zones.iter().any(|zone| zone.contains(x, z)) {
                continue;
            }

            return Some(Position { x, y: 0.0, z });
        }
        None
    }

    /// Push a synthetic agent event visible to the LLM.
    pub fn push_agent_event(&mut self, event: String) {
        self.agent_events.push(event);
    }

    /// Current game time snapshot for schedule resolution.
    pub fn time_context(&self) -> (Option<bool>, Option<u32>, Option<u32>) {
        (self.is_night, self.game_hour, self.game_minute)
    }

    /// Build a text summary of current world state for the LLM prompt.
    pub fn format_world_state(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref p) = self.self_player {
            lines.push(format!(
                "You: {} Lv.{} {:?} HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.name,
                p.level,
                p.class,
                p.health,
                p.max_health,
                p.position.x,
                p.position.y,
                p.position.z
            ));
        }

        // Nearby players (exclude self)
        for p in self.nearby_players.values() {
            if self.self_player_id.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            lines.push(format!(
                "Player: {} Lv.{} HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.name, p.level, p.health, p.max_health, p.position.x, p.position.y, p.position.z
            ));
        }

        // Exclude monsters beyond LLM notification radius
        let sp = self.self_player.as_ref();
        for m in self.nearby_monsters.values() {
            if let Some(sp) = sp {
                let dx = m.position.x - sp.position.x;
                let dz = m.position.z - sp.position.z;
                if dx * dx + dz * dz > MONSTER_MOVE_NOTIFY_RADIUS_SQ {
                    continue;
                }
            }
            lines.push(format!(
                "Monster: {} [{}] HP {}/{} state={} at ({:.1}, {:.1}, {:.1})",
                m.monster_type,
                m.id,
                m.health,
                m.max_health,
                m.state,
                m.position.x,
                m.position.y,
                m.position.z
            ));
        }

        if lines.is_empty() {
            "No state available yet.".to_string()
        } else {
            lines.join("\n")
        }
    }
}
