//! Letters posted and collected at mailboxes (doc/MAIL.md).

use super::inventory::hold_one;
use super::{auth_db, GameState};
use crate::auth::{AuthService, NewLetter};
use crate::types::PlayerId;
use onlinerpg_shared::mail::{
    is_mailbox, is_mailbox_furniture, validate_letter_body, MailRefusal, MailboxRef, LETTER_ITEM,
    MAIL_SEND_COOLDOWN_SECS,
};
use onlinerpg_shared::ServerMessage;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{error, info};

#[derive(Default)]
pub(super) struct MailCooldowns(HashMap<i64, Instant>);

impl MailCooldowns {
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    fn try_start(&mut self, character_id: i64, now: Instant) -> bool {
        let cooldown = Duration::from_secs(MAIL_SEND_COOLDOWN_SECS);
        if self
            .0
            .get(&character_id)
            .is_some_and(|last| now.duration_since(*last) < cooldown)
        {
            return false;
        }
        if self.0.len() >= 4096 {
            self.0
                .retain(|_, last| now.duration_since(*last) < cooldown);
        }
        self.0.insert(character_id, now);
        true
    }
}

impl GameState {
    async fn refuse_mail(&self, player_id: &PlayerId, reason: MailRefusal) {
        self.send_direct_message(player_id, ServerMessage::MailRefused { reason })
            .await;
    }

    /// The reader's character id when they stand at `mailbox`.
    async fn at_mailbox(&self, player_id: &PlayerId, mailbox: MailboxRef) -> Option<i64> {
        let reached = match mailbox {
            MailboxRef::Village { object_id } => self
                .reachable_placement(player_id, object_id, |p| is_mailbox(&p.type_id))
                .await
                .is_some(),
            MailboxRef::Estate { furniture_id } => self
                .reachable_furniture(player_id, furniture_id)
                .await
                .is_ok_and(|f| is_mailbox_furniture(&f.item_def_id)),
        };
        if !reached {
            self.refuse_mail(player_id, MailRefusal::OutOfReach).await;
            return None;
        }
        self.character_id_of(player_id).await
    }

    pub async fn open_mailbox(
        &self,
        player_id: &PlayerId,
        mailbox: MailboxRef,
        auth: &AuthService,
    ) {
        let Some(reader) = self.at_mailbox(player_id, mailbox).await else {
            return;
        };
        self.send_inbox(player_id, mailbox, reader, auth).await;
    }

    async fn send_inbox(
        &self,
        player_id: &PlayerId,
        mailbox: MailboxRef,
        reader: i64,
        auth: &AuthService,
    ) {
        let auth = auth.clone();
        match auth_db(move || auth.mail_inbox(reader)).await {
            Ok(letters) => {
                self.send_direct_message(player_id, ServerMessage::Mailbox { mailbox, letters })
                    .await
            }
            Err(err) => {
                error!("Failed to load mail for {reader}: {err}");
                self.refuse_mail(player_id, MailRefusal::ServerError).await;
            }
        }
    }

    pub async fn read_mail(
        &self,
        player_id: &PlayerId,
        mailbox: MailboxRef,
        letter_id: i64,
        auth: &AuthService,
    ) {
        let Some(reader) = self.at_mailbox(player_id, mailbox).await else {
            return;
        };
        let db = auth.clone();
        match auth_db(move || db.read_mail(reader, letter_id)).await {
            Ok(Some(letter)) => {
                self.send_direct_message(player_id, ServerMessage::MailLetter { mailbox, letter })
                    .await
            }
            Ok(None) => {
                self.refuse_mail(player_id, MailRefusal::LetterMissing)
                    .await
            }
            Err(err) => {
                error!("Failed to read letter {letter_id}: {err}");
                self.refuse_mail(player_id, MailRefusal::ServerError).await;
            }
        }
    }

    pub async fn delete_mail(
        &self,
        player_id: &PlayerId,
        mailbox: MailboxRef,
        letter_id: i64,
        auth: &AuthService,
    ) {
        let Some(reader) = self.at_mailbox(player_id, mailbox).await else {
            return;
        };
        let db = auth.clone();
        match auth_db(move || db.delete_mail(reader, letter_id)).await {
            Ok(true) => self.send_inbox(player_id, mailbox, reader, auth).await,
            Ok(false) => {
                self.refuse_mail(player_id, MailRefusal::LetterMissing)
                    .await
            }
            Err(err) => {
                error!("Failed to delete letter {letter_id}: {err}");
                self.refuse_mail(player_id, MailRefusal::ServerError).await;
            }
        }
    }

    pub async fn send_mail(
        &self,
        player_id: &PlayerId,
        mailbox: MailboxRef,
        recipient: &str,
        body: &str,
        auth: &AuthService,
    ) {
        let body = match validate_letter_body(body) {
            Ok(b) => b.to_string(),
            Err(reason) => return self.refuse_mail(player_id, reason).await,
        };
        let recipient = recipient.trim().to_string();
        if recipient.is_empty() {
            return self
                .refuse_mail(player_id, MailRefusal::NoSuchRecipient)
                .await;
        }
        let Some(sender_id) = self.at_mailbox(player_id, mailbox).await else {
            return;
        };
        if !self.holds_item(player_id, LETTER_ITEM).await {
            return self
                .refuse_mail(player_id, MailRefusal::NoLetterPaper)
                .await;
        }
        let Some(sender) = self
            .players
            .read()
            .await
            .get(player_id)
            .map(|p| p.name.clone())
        else {
            return;
        };
        if self.refuse_if_muted(player_id, &sender, "send mail").await {
            return;
        }
        let allowed = self
            .mail_cooldowns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_start(sender_id, Instant::now());
        if !allowed {
            return self.refuse_mail(player_id, MailRefusal::TooFast).await;
        }
        let hold = self
            .inventories
            .write()
            .await
            .get_mut(player_id)
            .and_then(|inv| hold_one(&mut inv.bag, |i| i.item_def_id == LETTER_ITEM));
        let Some(hold) = hold else {
            return self
                .refuse_mail(player_id, MailRefusal::NoLetterPaper)
                .await;
        };
        let game_seconds = Self::datetime_to_total_game_seconds(&self.current_game_datetime());
        let db = auth.clone();
        let author = sender.clone();
        let result = auth_db(move || {
            db.send_mail(&NewLetter {
                sender_id,
                sender: &author,
                recipient: &recipient,
                body: &body,
                game_seconds,
            })
        })
        .await;
        let saved = match &result {
            Ok(Ok(_)) => Ok(()),
            _ => Err("letter not sent"),
        };
        if self
            .finish_bag_write(player_id, hold, &[], saved)
            .await
            .is_ok()
        {
            self.log_consumed(player_id, [LETTER_ITEM]).await;
        }
        match result {
            Ok(Ok(delivered)) => {
                info!(sender = %sender, recipient = %delivered.name, "mail sent");
                self.send_direct_message(
                    player_id,
                    ServerMessage::MailSent {
                        recipient: delivered.name,
                    },
                )
                .await;
                self.notify_unread_mail(delivered.character_id, auth).await;
            }
            Ok(Err(reason)) => self.refuse_mail(player_id, reason).await,
            Err(err) => {
                error!("Failed to send mail from {sender}: {err}");
                self.refuse_mail(player_id, MailRefusal::ServerError).await;
            }
        }
    }

    /// Tells `character_id`, when online, how many letters wait unread.
    async fn notify_unread_mail(&self, character_id: i64, auth: &AuthService) {
        let Some(player_id) = self.player_id_of_character(character_id).await else {
            return;
        };
        if let Some(message) = self.unread_mail_notice(character_id, auth).await {
            self.send_direct_message(&player_id, message).await;
        }
    }

    /// `MailNotice` when the character has unread letters.
    pub async fn unread_mail_notice(
        &self,
        character_id: i64,
        auth: &AuthService,
    ) -> Option<ServerMessage> {
        let auth = auth.clone();
        match auth_db(move || auth.unread_mail(character_id)).await {
            Ok(0) => None,
            Ok(unread) => Some(ServerMessage::MailNotice { unread }),
            Err(err) => {
                error!("Failed to count mail for {character_id}: {err}");
                None
            }
        }
    }
}
