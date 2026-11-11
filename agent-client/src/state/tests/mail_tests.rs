use super::*;
use onlinerpg_shared::estate_storage::EstateChest;
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::mail::{Letter, LetterSummary, MailRefusal, MailboxRef};
use onlinerpg_shared::world::GameDateTime;
use onlinerpg_shared::Position;

fn at(hour: u8) -> GameDateTime {
    GameDateTime {
        year: 223,
        month: 4,
        day: 13,
        hour,
        minute: 5,
    }
}

const VILLAGE: MailboxRef = MailboxRef::Village { object_id: 9 };

fn mailbox_state() -> SharedState {
    let (mut state, _rx) = test_state();
    state.self_player = Some(test_player(0.0, 0.0));
    state.self_player_id = Some(PlayerId::from(1));
    state.world_cache.write().unwrap().sync_furniture(
        0,
        0,
        vec![FurniturePlacement {
            id: 9,
            type_id: "mailbox".into(),
            x: 3.0,
            y: 0.0,
            z: 0.0,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    state
}

fn summary(id: i64, read: bool) -> LetterSummary {
    LetterSummary {
        id,
        sender: "jake5".into(),
        excerpt: format!("note {id}"),
        written_at: at(5),
        read,
    }
}

#[test]
fn the_nearest_mailbox_and_an_open_letter_reach_the_world_state_clipped() {
    let mut state = mailbox_state();
    assert!(state
        .format_world_state()
        .contains("Mailbox at (3.0, 0.0), 3.0m away — open_mailbox"));

    let letters = (0..12).rev().map(|id| summary(id, id < 11)).collect();
    state.push_event(ServerMessage::Mailbox {
        mailbox: VILLAGE,
        letters,
    });
    let world = state.format_world_state();
    assert!(world.contains("Letter [letter_id 11] from jake5, 0223-04-13 05:05, UNREAD: note 11"));
    assert!(world.contains("Letter [letter_id 2] from jake5"));
    assert!(!world.contains("[letter_id 1]"));
    assert!(world.contains("(2 older letters not shown)"));

    state.push_event(ServerMessage::MailLetter {
        mailbox: VILLAGE,
        letter: Letter {
            id: 11,
            sender: "jake5".into(),
            body: "x".repeat(500),
            written_at: at(5),
        },
    });
    let world = state.format_world_state();
    assert!(world.contains("Reading letter [letter_id 11] from jake5"));
    assert!(world.contains(&format!("  | {}…", "x".repeat(400))));
    assert!(!world.contains("UNREAD"));

    state.self_player = Some(test_player(40.0, 0.0));
    let world = state.format_world_state();
    assert!(!world.contains("Mailbox"));
    assert!(!world.contains("Your mailbox"));
}

#[test]
fn an_estate_mailbox_is_listed_by_distance() {
    let mut state = mailbox_state();
    state.world_cache.write().unwrap().update_estate_chests(
        PlayerId::from(1),
        &[EstateChest {
            id: 44,
            estate_id: 1,
            owner_id: 2,
            item_def_id: "furniture_mailbox".into(),
            position: Position {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            rotation_deg: 0.0,
            floor_level: 0,
            overdue: false,
            revision: 1,
            text: None,
            family_id: None,
            emblem: None,
        }],
        &[],
    );
    let near = state.world_cache.read().unwrap().mailboxes_near(
        &Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        30.0,
    );
    assert_eq!(
        near.iter().map(|(_, m)| m.mailbox).collect::<Vec<_>>(),
        vec![MailboxRef::Estate { furniture_id: 44 }, VILLAGE]
    );
    state.self_player = Some(test_player(0.0, 0.0));
    assert!(state.format_world_state().contains("Mailbox at (1.0, 0.0)"));
}

#[test]
fn notices_and_refusals_become_events() {
    let mut state = mailbox_state();
    state.push_event(ServerMessage::MailNotice { unread: 2 });
    state.push_event(ServerMessage::MailRefused {
        reason: MailRefusal::NoLetterPaper,
    });
    let events = state.drain_agent_events();
    assert!(events
        .iter()
        .any(|e| e.starts_with("[Mail] 2 unread letters wait for you")));
    assert!(events
        .iter()
        .any(|e| e.starts_with("[MailFailed] you have no Letter Paper")));
}
