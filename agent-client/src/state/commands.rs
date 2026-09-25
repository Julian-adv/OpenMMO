use super::*;

/// A mark of an action's paper trail, taken before it runs and compared
/// after: where its event window starts, and the action-attributed event and
/// command counts.
#[derive(Clone, Copy)]
pub struct ActionProgress {
    pub events_start: usize,
    pub action_events: u64,
    pub commands_sent: u64,
}

impl SharedState {
    pub fn player_attack_wait(&self) -> std::time::Duration {
        self.last_player_attack_at
            .map_or(std::time::Duration::ZERO, |last| {
                self.attack_cooldown.saturating_sub(last.elapsed())
            })
    }

    pub async fn send_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        self.dispatch_command(msg, true).await
    }

    /// Send something the agent did not ask for. The heartbeat, monster-AI
    /// tick, follow steps and height syncs fire mid-action, and counting
    /// their traffic would rob a dropped action of its `[NoResult]`.
    pub async fn send_background_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        self.dispatch_command(msg, false).await
    }

    /// Send on whichever lane the caller's flag names — for the movers that
    /// walk both for actions and for background tasks like the follow.
    pub async fn send_flagged_command(
        &mut self,
        msg: ClientMessage,
        background: bool,
    ) -> anyhow::Result<()> {
        self.dispatch_command(msg, !background).await
    }

    pub(super) fn cancel_mount_recovery(&mut self) {
        self.mount_recovery_id = self.mount_recovery_id.wrapping_add(1);
        self.mount_recovery_result = Some(false);
    }

    async fn dispatch_command(
        &mut self,
        msg: ClientMessage,
        from_action: bool,
    ) -> anyhow::Result<()> {
        if let Some(resync_id) = self.pending_movement_ack {
            self.cmd_tx
                .send(ClientMessage::MovementResyncAck { resync_id })
                .await?;
            self.pending_movement_ack = None;
        }
        if matches!(
            &msg,
            ClientMessage::PlayerMove { .. }
                | ClientMessage::PlayerKeyboardMove { .. }
                | ClientMessage::PlayerMountTurn { .. }
                | ClientMessage::PlayerAttack { .. }
        ) {
            self.cancel_mount_recovery();
        }
        let player_attack = matches!(&msg, ClientMessage::PlayerAttack { .. });
        let fishing_cast = matches!(&msg, ClientMessage::FishingCast { .. });
        let fishing_stop = matches!(&msg, ClientMessage::FishingStop);
        if player_attack && !self.player_attack_wait().is_zero() {
            // The combat loop retries the latest target after the cooldown.
            return Ok(());
        }
        let msg = match msg {
            ClientMessage::PlayerMove {
                position,
                rotation,
                append,
                sprinting,
                ..
            } => {
                // On the entrance stairs the wire floor is still 0 while the Y
                // already follows the ramp, so terrain height must not win there.
                let position = if self.self_floor_level == 0
                    && self.dungeon_ground_y(position.x, position.z, 0).is_none()
                {
                    self.snap_position_to_ground(position, "PlayerMove").await
                } else {
                    position
                };
                // Update local position immediately so subsequent reads don't use stale data
                if let Some(ref mut p) = self.self_player {
                    p.position = position;
                    p.rotation = rotation;
                }
                ClientMessage::PlayerMove {
                    position,
                    rotation,
                    floor_level: self.self_floor_level,
                    append,
                    sprinting,
                }
            }
            ClientMessage::InteractObject {
                object_type,
                object_id,
            } => {
                // Mirror the pose on send, not on the server echo: a stale
                // LLM response can run this same tick, and
                // refuses_play_command must already see the bed under us or
                // its /play_music replaces the pose.
                self.set_self_pose(Some(object_type.clone()), Some(object_id));
                ClientMessage::InteractObject {
                    object_type,
                    object_id,
                }
            }
            ClientMessage::StopInteraction => {
                self.set_self_pose(None, None);
                ClientMessage::StopInteraction
            }
            other => other,
        };
        self.cmd_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Command channel closed: {e}"))?;
        if player_attack {
            self.last_player_attack_at = Some(tokio::time::Instant::now());
        }
        if fishing_cast {
            self.fishing_retry_at = Some(tokio::time::Instant::now() + FISHING_CAST_ACK_TIMEOUT);
        } else if fishing_stop {
            self.set_self_fishing(false);
            self.fishing_retry_at = Some(tokio::time::Instant::now() + FISHING_RECAST_DELAY);
        }
        if from_action {
            self.action_commands_sent += 1;
        }
        Ok(())
    }

    /// How much an action has done so far. Neither counter moving between two
    /// marks means it left no trace — see the `[NoResult]` backstop in
    /// `handle_response`. Ambient events and background commands stay out of
    /// the counters so concurrent traffic cannot pass for a result.
    pub fn action_progress(&self) -> ActionProgress {
        ActionProgress {
            events_start: self.agent_events.len(),
            action_events: self.action_events_pushed,
            commands_sent: self.action_commands_sent,
        }
    }

    /// Drain pending commands (from monster AI reactions, spawn requests, etc.)
    pub fn drain_pending_commands(&mut self) -> Vec<ClientMessage> {
        let mut commands = std::mem::take(&mut self.pending_commands);
        if let Some(resync_id) = self.pending_movement_ack.take() {
            commands.insert(0, ClientMessage::MovementResyncAck { resync_id });
        }
        commands
    }
}
