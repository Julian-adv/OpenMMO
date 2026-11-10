use super::*;
use onlinerpg_shared::mail::{
    letter_excerpt, Letter, LetterSummary, MailRefusal, MailboxRef, LETTER_BODY_MAX_CHARS,
    MAIL_SEND_COOLDOWN_SECS,
};

/// Mailboxes listed in the world state.
const MAILBOX_SIGHT_M: f32 = 30.0;
/// An open mailbox stays in the prompt only while the agent stands at it.
const MAILBOX_PANEL_M: f32 = 6.0;
const LISTED_LETTERS: usize = 10;
const BODY_CHARS: usize = 400;

pub struct MailboxPanel {
    pub mailbox: MailboxRef,
    pub letters: Vec<LetterSummary>,
    pub open_letter: Option<Letter>,
}

pub fn refusal_text(reason: MailRefusal) -> String {
    match reason {
        MailRefusal::OutOfReach => "you are not standing at a mailbox".into(),
        MailRefusal::Empty => "the letter was empty".into(),
        MailRefusal::TooLong => {
            format!("the letter was too long (up to {LETTER_BODY_MAX_CHARS} characters)")
        }
        MailRefusal::TooFast => {
            format!("you sent a letter too recently (one per {MAIL_SEND_COOLDOWN_SECS} s)")
        }
        MailRefusal::NoLetterPaper => {
            "you have no Letter Paper; buy one from Aldwin the land registrar (1 silver)".into()
        }
        MailRefusal::NoSuchRecipient => "no character has that name".into(),
        MailRefusal::RecipientFull => "the recipient's mailbox is full".into(),
        MailRefusal::LetterMissing => "that letter is not in your mailbox".into(),
        MailRefusal::ServerError => "the post office failed; try later".into(),
    }
}

impl SharedState {
    pub(super) fn on_mailbox(&mut self, mailbox: MailboxRef, letters: &[LetterSummary]) {
        let open_letter = self
            .open_mailbox
            .take()
            .and_then(|panel| panel.open_letter)
            .filter(|letter| letters.iter().any(|l| l.id == letter.id));
        let unread = letters.iter().filter(|l| !l.read).count();
        self.open_mailbox = Some(MailboxPanel {
            mailbox,
            letters: letters.to_vec(),
            open_letter,
        });
        self.push_agent_event(format!(
            "[Mail] Opened your mailbox: {} letters, {unread} unread.",
            letters.len()
        ));
    }

    pub(super) fn on_mail_letter(&mut self, mailbox: MailboxRef, letter: &Letter) {
        let mut letters = self
            .open_mailbox
            .take()
            .map(|panel| panel.letters)
            .unwrap_or_default();
        match letters.iter_mut().find(|l| l.id == letter.id) {
            Some(summary) => summary.read = true,
            None => letters.insert(
                0,
                LetterSummary {
                    id: letter.id,
                    sender: letter.sender.clone(),
                    excerpt: letter_excerpt(&letter.body),
                    written_at: letter.written_at.clone(),
                    read: true,
                },
            ),
        }
        self.open_mailbox = Some(MailboxPanel {
            mailbox,
            letters,
            open_letter: Some(letter.clone()),
        });
        self.push_agent_event(format!(
            "[Mail] Reading letter [letter_id {}] from {}.",
            letter.id, letter.sender
        ));
    }

    pub(super) fn format_mailboxes(&self, lines: &mut Vec<String>) {
        let Some(player) = &self.self_player else {
            return;
        };
        let mailboxes = self
            .world_cache
            .read()
            .unwrap()
            .mailboxes_near(&player.position, MAILBOX_SIGHT_M);
        let Some((dist, nearest)) = mailboxes.first() else {
            return;
        };
        lines.push(format!(
            "Mailbox at ({:.1}, {:.1}), {dist:.1}m away — open_mailbox to collect your letters",
            nearest.position.x, nearest.position.z
        ));
        let Some(panel) = &self.open_mailbox else {
            return;
        };
        if !mailboxes
            .iter()
            .any(|(dist, m)| m.mailbox == panel.mailbox && *dist <= MAILBOX_PANEL_M)
        {
            return;
        }
        lines.push("Your mailbox — letters written by players, newest first:".to_string());
        if panel.letters.is_empty() {
            lines.push("  (no letters)".to_string());
        }
        for letter in panel.letters.iter().take(LISTED_LETTERS) {
            lines.push(format!(
                "  Letter [letter_id {}] from {}, {}{}: {}",
                letter.id,
                letter.sender,
                letter.written_at,
                if letter.read { "" } else { ", UNREAD" },
                super::notice_board::clip(&letter.excerpt, BODY_CHARS)
            ));
        }
        if panel.letters.len() > LISTED_LETTERS {
            lines.push(format!(
                "  ({} older letters not shown)",
                panel.letters.len() - LISTED_LETTERS
            ));
        }
        let Some(letter) = &panel.open_letter else {
            return;
        };
        lines.push(format!(
            "Reading letter [letter_id {}] from {}, {}:",
            letter.id, letter.sender, letter.written_at
        ));
        lines.push(format!(
            "  | {}",
            super::notice_board::clip(&letter.body, BODY_CHARS)
        ));
    }
}
