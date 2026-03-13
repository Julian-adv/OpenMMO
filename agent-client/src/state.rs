use std::collections::HashMap;

use onlinerpg_shared::{ClientMessage, Character, Monster, Player, ServerMessage};
use tokio::sync::{mpsc, Notify};
use std::sync::Arc;

const MAX_EVENTS: usize = 200;

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
    /// Latest position per monster — deduplicates high-frequency MonsterMoved events
    latest_monster_moves: HashMap<String, ServerMessage>,
    /// Latest position per player — deduplicates high-frequency PlayerMoved events
    latest_player_moves: HashMap<String, ServerMessage>,
    /// Latest game time — only the most recent matters
    latest_time: Option<ServerMessage>,
    cmd_tx: mpsc::Sender<ClientMessage>,
    /// Notified when an urgent event arrives
    pub urgent_notify: Arc<Notify>,
}

impl SharedState {
    pub fn new(characters: Vec<Character>, cmd_tx: mpsc::Sender<ClientMessage>) -> Self {
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
            cmd_tx,
            urgent_notify: Arc::new(Notify::new()),
        }
    }

    pub async fn send_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        // Update local position immediately so subsequent reads don't use stale data
        if let ClientMessage::PlayerMove {
            ref position,
            rotation,
        } = msg
        {
            if let Some(ref mut p) = self.self_player {
                p.position = position.clone();
                p.rotation = rotation;
            }
        }
        self.cmd_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Command channel closed: {e}"))
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

            // Routine: world state changes
            ServerMessage::JoinSuccess { .. }
            | ServerMessage::GameState { .. }
            | ServerMessage::PlayerJoined { .. }
            | ServerMessage::PlayerLeft { .. }
            | ServerMessage::MonsterSpawned { .. }
            | ServerMessage::MonsterDead { .. }
            | ServerMessage::MonsterRemoved { .. }
            | ServerMessage::XpGained { .. }
            | ServerMessage::PlayerRespawned { .. }
            | ServerMessage::PlayerHealthUpdate { .. }
            | ServerMessage::PlayerAttacked { .. }
            | ServerMessage::PlayerTorchToggled { .. } => EventUrgency::Routine,

            // Noise: high-frequency or irrelevant
            ServerMessage::PlayerMoved { .. }
            | ServerMessage::PlayerTeleported { .. }
            | ServerMessage::MonsterMoved { .. }
            | ServerMessage::GameTimeSync { .. } => EventUrgency::Noise,

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
                self.nearby_players.insert(player.id.clone(), player.clone());
            }
            ServerMessage::PlayerLeft { player_id } => {
                self.nearby_players.remove(player_id);
            }
            ServerMessage::MonsterSpawned { monster } => {
                self.nearby_monsters
                    .insert(monster.id.clone(), monster.clone());
            }
            ServerMessage::MonsterDead { monster_id } | ServerMessage::MonsterRemoved { monster_id } => {
                self.nearby_monsters.remove(monster_id);
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
            _ => {}
        }

        let urgency = self.classify_event(&msg);

        // Deduplicate high-frequency movement events: keep only latest per entity
        match &msg {
            ServerMessage::MonsterMoved { monster_id, .. } => {
                self.latest_monster_moves
                    .insert(monster_id.clone(), msg);
                return urgency;
            }
            ServerMessage::PlayerMoved { player_id, .. } => {
                self.latest_player_moves
                    .insert(player_id.clone(), msg);
                return urgency;
            }
            ServerMessage::GameTimeSync { .. } => {
                self.latest_time = Some(msg);
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

    /// Build a text summary of current world state for the LLM prompt.
    pub fn format_world_state(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref p) = self.self_player {
            lines.push(format!(
                "You: {} Lv.{} {:?} HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.name, p.level, p.class, p.health, p.max_health,
                p.position.x, p.position.y, p.position.z
            ));
        }

        // Nearby players (exclude self)
        for p in self.nearby_players.values() {
            if self.self_player_id.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            lines.push(format!(
                "Player: {} Lv.{} HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.name, p.level, p.health, p.max_health,
                p.position.x, p.position.y, p.position.z
            ));
        }

        // Nearby monsters
        for m in self.nearby_monsters.values() {
            lines.push(format!(
                "Monster: {} [{}] HP {}/{} state={} at ({:.1}, {:.1}, {:.1})",
                m.monster_type, m.id, m.health, m.max_health, m.state,
                m.position.x, m.position.y, m.position.z
            ));
        }

        if lines.is_empty() {
            "No state available yet.".to_string()
        } else {
            lines.join("\n")
        }
    }
}
