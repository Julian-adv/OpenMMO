use super::*;
use crate::auth::{AuthService, CharacterRecord};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::mail::{MailRefusal, MailboxRef, LETTER_ITEM, MAILBOX_CAPACITY};

const BOX_ID: u32 = 9;
const MAILBOX: MailboxRef = MailboxRef::Village { object_id: BOX_ID };

fn character(auth: &AuthService, name: &str) -> CharacterRecord {
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    create_test_character(auth, &account, name)
}

async fn enter(game: &GameState, record: &CharacterRecord, x: f32, letters: u32) -> DirectRx {
    let name = record.name.as_str();
    game.add_player(make_player(name, x, 0.0)).await;
    game.register_player_character(&pid(name), record.id, 0, attrs_with_cha(10), 0, None)
        .await;
    let mut inv = PlayerInventory::default();
    if letters > 0 {
        inv.bag.push(bag_item(1, LETTER_ITEM, letters));
    }
    game.inventories.write().await.insert(pid(name), inv);
    game.register_direct_channel(&pid(name)).await
}

async fn letters_left(game: &GameState, name: &str) -> u32 {
    game.inventories.read().await[&pid(name)]
        .bag
        .iter()
        .filter(|i| i.item_def_id == LETTER_ITEM)
        .map(|i| i.quantity)
        .sum()
}

fn mailbox_game(name: &str) -> GameState {
    let game = make_test_game_state(name);
    game.sync_region_furniture(
        0,
        0,
        &[FurniturePlacement {
            id: BOX_ID,
            type_id: "mailbox".into(),
            x: 10.0,
            y: 0.0,
            z: 0.0,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    game
}

fn refusals(msgs: &[ServerMessage]) -> Vec<MailRefusal> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::MailRefused { reason } => Some(*reason),
            _ => None,
        })
        .collect()
}

fn inbox(msgs: &[ServerMessage]) -> Option<Vec<onlinerpg_shared::mail::LetterSummary>> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMessage::Mailbox { letters, .. } => Some(letters.clone()),
        _ => None,
    })
}

fn unread_notice(msgs: &[ServerMessage]) -> Option<u32> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMessage::MailNotice { unread } => Some(*unread),
        _ => None,
    })
}

#[tokio::test]
async fn a_letter_reaches_the_recipient_and_is_read_once() {
    let auth = make_test_auth("mail_flow");
    let game = mailbox_game("mail_flow");
    let (alice, bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let mut alice_rx = enter(&game, &alice, 10.5, 2).await;
    let mut bob_rx = enter(&game, &bob, 40.0, 0).await;

    game.send_mail(
        &pid("alice"),
        MAILBOX,
        " BOB ",
        " 내일 저녁 광장에서 ",
        &auth,
    )
    .await;
    let sent = drain(&mut alice_rx);
    assert!(sent
        .iter()
        .any(|m| matches!(m, ServerMessage::MailSent { recipient } if recipient == "bob")));
    assert_eq!(letters_left(&game, "alice").await, 1);
    assert_eq!(unread_notice(&drain(&mut bob_rx)), Some(1));
    assert!(matches!(
        game.unread_mail_notice(bob.id, &auth).await,
        Some(ServerMessage::MailNotice { unread: 1 })
    ));

    game.open_mailbox(&pid("bob"), MAILBOX, &auth).await;
    assert_eq!(refusals(&drain(&mut bob_rx)), vec![MailRefusal::OutOfReach]);

    game.players
        .write()
        .await
        .get_mut(&pid("bob"))
        .unwrap()
        .position
        .x = 9.5;
    game.open_mailbox(&pid("bob"), MAILBOX, &auth).await;
    let letters = inbox(&drain(&mut bob_rx)).unwrap();
    assert_eq!(letters.len(), 1);
    assert_eq!(
        (letters[0].sender.as_str(), letters[0].read),
        ("alice", false)
    );

    game.read_mail(&pid("alice"), MAILBOX, letters[0].id, &auth)
        .await;
    assert_eq!(
        refusals(&drain(&mut alice_rx)),
        vec![MailRefusal::LetterMissing]
    );

    game.read_mail(&pid("bob"), MAILBOX, letters[0].id, &auth)
        .await;
    let body = drain(&mut bob_rx).into_iter().find_map(|m| match m {
        ServerMessage::MailLetter { letter, .. } => Some(letter.body),
        _ => None,
    });
    assert_eq!(body.as_deref(), Some("내일 저녁 광장에서"));
    assert!(game.unread_mail_notice(bob.id, &auth).await.is_none());

    game.delete_mail(&pid("bob"), MAILBOX, letters[0].id, &auth)
        .await;
    assert_eq!(inbox(&drain(&mut bob_rx)), Some(Vec::new()));
}

#[tokio::test]
async fn refused_letters_keep_the_paper() {
    let auth = make_test_auth("mail_refusals");
    let game = mailbox_game("mail_refusals");
    let alice = character(&auth, "alice");
    let mut alice_rx = enter(&game, &alice, 10.5, 1).await;

    game.send_mail(&pid("alice"), MAILBOX, "nobody", "hi", &auth)
        .await;
    assert_eq!(
        refusals(&drain(&mut alice_rx)),
        vec![MailRefusal::NoSuchRecipient]
    );
    assert_eq!(letters_left(&game, "alice").await, 1);

    game.send_mail(&pid("alice"), MAILBOX, "alice", "hi", &auth)
        .await;
    assert_eq!(refusals(&drain(&mut alice_rx)), vec![MailRefusal::TooFast]);

    game.mail_cooldowns.lock().unwrap().clear();
    game.send_mail(&pid("alice"), MAILBOX, "alice", "  ", &auth)
        .await;
    assert_eq!(refusals(&drain(&mut alice_rx)), vec![MailRefusal::Empty]);

    game.send_mail(&pid("alice"), MAILBOX, "alice", "to me", &auth)
        .await;
    assert!(refusals(&drain(&mut alice_rx)).is_empty());
    assert_eq!(letters_left(&game, "alice").await, 0);

    game.mail_cooldowns.lock().unwrap().clear();
    game.send_mail(&pid("alice"), MAILBOX, "alice", "again", &auth)
        .await;
    assert_eq!(
        refusals(&drain(&mut alice_rx)),
        vec![MailRefusal::NoLetterPaper]
    );
}

#[test]
fn a_full_box_refuses_more_letters() {
    let auth = make_test_auth("mail_full");
    let (alice, bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let letter = crate::auth::NewLetter {
        sender_id: alice.id,
        sender: "alice",
        recipient: "bob",
        body: "hi",
        game_seconds: 0,
    };
    for _ in 0..MAILBOX_CAPACITY {
        assert!(auth.send_mail(&letter).unwrap().is_ok());
    }
    assert!(matches!(
        auth.send_mail(&letter).unwrap(),
        Err(MailRefusal::RecipientFull)
    ));
    let first = auth.mail_inbox(bob.id).unwrap().last().unwrap().id;
    assert!(auth.delete_mail(bob.id, first).unwrap());
    assert!(auth.send_mail(&letter).unwrap().is_ok());
}

#[test]
fn old_letters_and_notices_are_pruned_with_their_comments() {
    use crate::auth::{BoardKey, NewNoticePost, NoticeClock};
    let auth = make_test_auth("mail_prune");
    let (alice, _bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let letter = crate::auth::NewLetter {
        sender_id: alice.id,
        sender: "alice",
        recipient: "bob",
        body: "hi",
        game_seconds: 0,
    };
    assert!(auth.send_mail(&letter).unwrap().is_ok());
    let board = BoardKey {
        rx: 0,
        rz: 0,
        object_id: 1,
    };
    let post = auth
        .add_notice_post(
            board,
            &NewNoticePost {
                character_id: alice.id,
                author: "alice",
                title: "t",
                body: "b",
                image: None,
                position: (500, 500),
                game_seconds: 0,
            },
        )
        .unwrap();
    let clock = NoticeClock {
        expired_at: -10,
        hidden_at: -20,
    };
    assert!(auth
        .add_notice_comment(board, post.id, alice.id, "alice", "c", 0, clock)
        .unwrap()
        .is_ok());

    let now = crate::auth::unix_now();
    assert_eq!(auth.prune_old_messages(now - 1000).unwrap(), (0, 0));
    assert_eq!(auth.prune_old_messages(now + 10).unwrap(), (1, 1));
    assert!(auth.notice_post(board, post.id, clock).unwrap().is_none());
}
