use crate::types::{ClientKind, Player, PlayerId, ServerMessage};
use crate::world_config::world_config;
use tracing::{info, warn};

/// `/who` breakdown. Splits by client program rather than by "human vs bot":
/// the server cannot tell whether a person or an LLM is driving a web client,
/// and does not try to (`doc/REMOTE_AGENT_CLIENT.md`). Official NPCs are
/// counted separately because that one the server does know for certain.
#[derive(Default)]
struct OnlineCounts {
    web: u32,
    cli: u32,
    other: u32,
    official_npc: u32,
}

impl OnlineCounts {
    fn tally<'a>(players: impl Iterator<Item = &'a Player>) -> Self {
        let mut counts = Self::default();
        for player in players {
            if player.is_official_npc {
                counts.official_npc += 1;
                continue;
            }
            match player.client_kind {
                ClientKind::Web => counts.web += 1,
                ClientKind::Cli => counts.cli += 1,
                ClientKind::Other | ClientKind::Unknown => counts.other += 1,
            }
        }
        counts
    }

    fn describe(&self) -> String {
        let total = self.web + self.cli + self.other + self.official_npc;
        let mut parts = vec![format!("{} web", self.web), format!("{} cli", self.cli)];
        if self.other > 0 {
            parts.push(format!("{} other", self.other));
        }
        parts.push(format!("{} npc", self.official_npc));
        format!("Online: {total} ({})", parts.join(", "))
    }
}

/// `/w <name> <message>` (or `/whisper`). Returns the parts unvalidated so a
/// malformed whisper draws a usage reply instead of leaking into local chat.
pub(crate) fn parse_whisper_command(message: &str) -> Option<(&str, &str)> {
    let trimmed = message.trim();
    let rest = ["/whisper", "/w"].iter().find_map(|prefix| {
        let rest = trimmed.strip_prefix(prefix)?;
        (rest.is_empty() || rest.starts_with(' ')).then(|| rest.trim_start())
    })?;
    Some(match rest.split_once(' ') {
        Some((name, message)) => (name, message.trim()),
        None => (rest, ""),
    })
}

/// `/notice <message>` sets the server banner, bare `/notice` clears it.
/// `requires_admin` gates the command through this same parser, so the syntax
/// is defined once.
pub(crate) fn parse_notice_command(message: &str) -> Option<Option<&str>> {
    let rest = message.trim().strip_prefix("/notice")?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some(Some(rest.trim()).filter(|rest| !rest.is_empty()))
}

impl super::GameState {
    pub async fn send_chat_message(&self, player_id: &PlayerId, message: String) {
        if let Some(notice) = parse_notice_command(&message) {
            info!(
                player = ?player_id,
                cleared = notice.is_none(),
                len = notice.map_or(0, str::len),
                "server notice updated"
            );
            self.set_server_notice(notice.map(str::to_string)).await;
            return;
        }

        if message.trim() == "/escape" {
            self.escape_to_spawn(player_id).await;
            return;
        }

        if message.trim() == "/who" {
            let counts = {
                let players = self.players.read().await;
                OnlineCounts::tally(players.values())
            };
            self.send_system_message(player_id, counts.describe()).await;
            return;
        }

        // Handle /give command
        if let Some(item_id) = message.strip_prefix("/give ") {
            let item_id = item_id.trim();
            if self.give_item(player_id, item_id).await {
                self.send_system_message(player_id, format!("Gave item: {}", item_id))
                    .await;
            } else {
                self.send_system_message(player_id, format!("Unknown item: {}", item_id))
                    .await;
            }
            return;
        }

        if let Some((target_name, whisper)) = parse_whisper_command(&message) {
            self.send_whisper(player_id, target_name, whisper).await;
            return;
        }

        let player_name = {
            let players = self.players.read().await;
            players.get(player_id).map(|player| player.name.clone())
        };

        if let Some(player_name) = player_name {
            // Chat content stays out of logs on purpose (privacy, F-012).
            info!(from = %player_name, len = message.len(), "chat message");
            let recipients = self
                .player_ids_within(player_id, super::EVENT_DELIVERY_RADIUS)
                .await;
            self.send_direct_message_to_players(
                &recipients,
                ServerMessage::ChatMessage {
                    player_id: *player_id,
                    message,
                },
            )
            .await;
        } else {
            warn!("Chat message from non-existent player: {}", player_id);
        }
    }

    /// Deliver a whisper to the named player, wherever they are, and echo it
    /// back so the sender's client can render the outgoing line. Errors go
    /// only to the sender.
    async fn send_whisper(&self, player_id: &PlayerId, target_name: &str, message: &str) {
        if target_name.is_empty() || message.is_empty() {
            self.send_system_message(player_id, "Whisper: /w <name> <message>")
                .await;
            return;
        }

        let (sender_name, target) = {
            let players = self.players.read().await;
            let sender_name = players.get(player_id).map(|player| player.name.clone());
            // Names are UNIQUE only case-sensitively: exact match wins, the
            // case-insensitive convenience applies only while unambiguous.
            let target = match players.values().find(|player| player.name == target_name) {
                Some(player) => Ok((player.id, player.name.clone())),
                None => {
                    let mut matches = players
                        .values()
                        .filter(|player| player.name.eq_ignore_ascii_case(target_name));
                    match (matches.next(), matches.next()) {
                        (Some(player), None) => Ok((player.id, player.name.clone())),
                        (None, _) => Err(format!("Whisper: no one called {target_name} is here.")),
                        (Some(_), Some(_)) => Err(format!(
                            "Whisper: several players match {target_name}; spell the name exactly."
                        )),
                    }
                }
            };
            (sender_name, target)
        };

        let Some(from) = sender_name else {
            warn!("Whisper from non-existent player: {}", player_id);
            return;
        };
        let (target_id, to) = match target {
            Ok(target) => target,
            Err(message) => {
                self.send_system_message(player_id, message).await;
                return;
            }
        };
        if target_id == *player_id {
            self.send_system_message(player_id, "Whisper: that's you.")
                .await;
            return;
        }

        // Content and recipient both stay out of logs (privacy, F-012).
        info!(from = %from, len = message.len(), "whisper");
        let whisper = ServerMessage::WhisperMessage {
            from,
            to,
            message: message.to_string(),
        };
        self.send_direct_message(&target_id, whisper.clone()).await;
        self.send_direct_message(player_id, whisper).await;
    }

    /// Last resort for a player wedged somewhere movement can't undo: return
    /// them to the world spawn on the surface.
    ///
    /// Open to everyone by design — the players who need it are precisely the
    /// ones who cannot reach an admin. The combat lockout is what keeps it from
    /// doubling as a free disengage.
    async fn escape_to_spawn(&self, player_id: &PlayerId) {
        let in_combat = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                warn!("/escape from non-existent player: {}", player_id);
                return;
            };
            Self::now_ms().saturating_sub(player.last_combat_at) < super::OUT_OF_COMBAT_MS
        };
        if in_combat {
            self.send_system_message(player_id, "Escape: not while in combat.")
                .await;
            return;
        }

        // Queued waypoints target the place being escaped from; snapping to one
        // after the teleport would drag the player straight back.
        self.movement_intents.write().await.remove(player_id);

        let spawn = &world_config().spawn_position;
        self.teleport_player(player_id, spawn.position(), spawn.rotation, 0)
            .await;
        info!("Player {} escaped to spawn", player_id);
        self.send_system_message(player_id, "Escape: returned to the starting point.")
            .await;
    }
}
