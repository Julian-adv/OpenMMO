use std::collections::HashMap;

use onlinerpg_shared::{Character, ClientMessage, ServerMessage};
use tokio::sync::mpsc;

const MAX_EVENTS: usize = 200;

/// Shared state between MCP server and WebSocket background tasks.
pub struct SharedState {
    pub characters: Vec<Character>,
    pub in_game: bool,
    events: Vec<ServerMessage>,
    /// Latest position per monster — deduplicates high-frequency MonsterMoved events
    latest_monster_moves: HashMap<String, ServerMessage>,
    /// Latest position per player — deduplicates high-frequency PlayerMoved events
    latest_player_moves: HashMap<String, ServerMessage>,
    /// Latest game time — only the most recent matters
    latest_time: Option<ServerMessage>,
    cmd_tx: mpsc::Sender<ClientMessage>,
}

impl SharedState {
    pub fn new(characters: Vec<Character>, cmd_tx: mpsc::Sender<ClientMessage>) -> Self {
        Self {
            characters,
            in_game: false,
            events: Vec::new(),
            latest_monster_moves: HashMap::new(),
            latest_player_moves: HashMap::new(),
            latest_time: None,
            cmd_tx,
        }
    }

    pub async fn send_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        self.cmd_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Command channel closed: {e}"))
    }

    pub fn push_event(&mut self, msg: ServerMessage) {
        if matches!(msg, ServerMessage::JoinSuccess { .. }) {
            self.in_game = true;
        }

        // Deduplicate high-frequency movement events: keep only latest per entity
        match &msg {
            ServerMessage::MonsterMoved { monster_id, .. } => {
                self.latest_monster_moves
                    .insert(monster_id.clone(), msg);
                return;
            }
            ServerMessage::PlayerMoved { player_id, .. } => {
                self.latest_player_moves
                    .insert(player_id.clone(), msg);
                return;
            }
            ServerMessage::GameTimeSync { .. } => {
                self.latest_time = Some(msg);
                return;
            }
            _ => {}
        }

        self.events.push(msg);

        // Cap buffer size: drop oldest events
        if self.events.len() > MAX_EVENTS {
            let overflow = self.events.len() - MAX_EVENTS;
            self.events.drain(..overflow);
        }
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
}
