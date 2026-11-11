use super::*;
use crate::auth::AuthService;
use crate::auth::BoardKey;
use onlinerpg_shared::family::{
    permission, FamilyInfo, FAMILY_FOUND_STAMPS, FAMILY_RANK_ELDER, FAMILY_RANK_GUEST,
    FAMILY_RANK_HEAD, FAMILY_RANK_MEMBER, FAMILY_STAMP_ITEM,
};
use onlinerpg_shared::messages::BagLineItem;
use onlinerpg_shared::notice_board::{NoticeBoardRef, NoticeBoardRefusal};

const REGISTRAR: &str = "npc_registrar";

async fn setup(tag: &str) -> (GameState, AuthService, PlayerId) {
    let game = make_test_game_state(tag);
    let auth = make_test_auth(tag);
    let mut npc = make_player(REGISTRAR, 1.0, 1.0);
    npc.name = "Aldwin".into();
    npc.is_official_npc = true;
    game.add_player(npc).await;
    (game, auth, pid(REGISTRAR))
}

/// A character in the game on its own account, standing by the registrar.
async fn enter(game: &GameState, auth: &AuthService, name: &str, stamps: u32) -> (i64, DirectRx) {
    let account = auth.login_google(&format!("family-{name}")).unwrap();
    let mut bag = vec![bag_item(1, "land_deed", 1)];
    if stamps > 0 {
        bag.push(bag_item(2, FAMILY_STAMP_ITEM, stamps));
    }
    let id = estate_owner(game, auth, &account, name, pos3(1.0, 0.0, 1.0), bag).await;
    game.player_gold.write().await.insert(pid(name), 500);
    (id, game.register_direct_channel(&pid(name)).await)
}

async fn stamps(game: &GameState, name: &str) -> u32 {
    bag_count(game, &pid(name), FAMILY_STAMP_ITEM).await
}

/// Claims the 32 m plot at column `plot` of row 0, then walks back.
async fn claim(game: &GameState, auth: &AuthService, name: &str, plot: u8) {
    let x = f32::from(plot) * 32.0 + 1.0;
    game.players
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .position = pos3(x, 0.0, 1.0);
    claim_plot_at(game, auth, name, x, 1.0).await;
    game.players
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .position = pos3(1.0, 0.0, 1.0);
}

async fn found(
    game: &GameState,
    auth: &AuthService,
    registrar: &PlayerId,
    name: &str,
    family: &str,
) {
    game.create_family(&pid(name), registrar, family, auth)
        .await;
}

async fn join(game: &GameState, auth: &AuthService, inviter: &str, target: &str) {
    game.invite_to_family(&pid(inviter), target, auth).await;
    game.respond_to_family_invite(&pid(target), &pid(inviter), true, auth)
        .await;
}

fn family_state(msgs: &[ServerMessage]) -> Option<Option<FamilyInfo>> {
    msgs.iter().rev().find_map(|msg| match msg {
        ServerMessage::FamilyState { family } => Some(family.clone()),
        _ => None,
    })
}

fn has_line(msgs: &[ServerMessage], needle: &str) -> bool {
    msgs.iter().any(|msg| {
        matches!(msg, ServerMessage::SystemMessage { message, .. } if message.contains(needle))
    })
}

async fn shown_family(game: &GameState, name: &str) -> Option<String> {
    game.players.read().await[&pid(name)].family.clone()
}

#[tokio::test]
async fn founding_needs_an_estate_and_the_fee_and_a_free_name() {
    let (game, auth, registrar) = setup("family_founding").await;
    let (_, mut rx) = enter(&game, &auth, "Landless", FAMILY_FOUND_STAMPS).await;
    game.create_family(&pid("Landless"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "Own an estate"));
    assert_eq!(stamps(&game, "Landless").await, FAMILY_FOUND_STAMPS);

    let (owner, mut rx) = enter(&game, &auth, "Owner", FAMILY_FOUND_STAMPS - 1).await;
    claim(&game, &auth, "Owner", 0).await;
    game.create_family(&pid("Owner"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "you need 10 revenue stamp"));
    give(&game, &pid("Owner"), bag_item(3, FAMILY_STAMP_ITEM, 3)).await;
    game.create_family(&pid("Owner"), &registrar, "Landless", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "A character has that name"));
    assert_eq!(stamps(&game, "Owner").await, FAMILY_FOUND_STAMPS + 2);

    game.create_family(&pid("Owner"), &registrar, "Ironwood", &auth)
        .await;
    let family = family_state(&drain(&mut rx)).flatten().expect("roster");
    assert_eq!(family.name, "Ironwood");
    assert_eq!(family.my_rank, FAMILY_RANK_HEAD);
    assert_eq!(family.ranks[0].permissions, permission::ALL);
    assert_eq!(family.estates.len(), 1);
    assert!(family.estates[0].mine);
    assert_eq!(stamps(&game, "Owner").await, 2);
    assert_eq!(
        shown_family(&game, "Owner").await.as_deref(),
        Some("Ironwood")
    );
    assert_eq!(
        auth.family_of(owner)
            .unwrap()
            .map(|(_, name)| name)
            .as_deref(),
        Some("Ironwood")
    );
    let account = auth.login_google("family-Owner").unwrap();
    let listed = auth.list_characters_with_equipment(&account).unwrap();
    assert_eq!(listed[0].family.as_deref(), Some("Ironwood"));

    game.rename_family(&pid("Owner"), &registrar, "Oakheart", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "you need 10 revenue stamp"));
    give(&game, &pid("Owner"), bag_item(4, FAMILY_STAMP_ITEM, 8)).await;
    game.rename_family(&pid("Owner"), &pid("Landless"), "Oakheart", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "with the Land Registrar"));
    game.rename_family(&pid("Owner"), &registrar, "Oakheart", &auth)
        .await;
    assert_eq!(
        shown_family(&game, "Owner").await.as_deref(),
        Some("Oakheart")
    );
    assert_eq!(stamps(&game, "Owner").await, 0);
    give(&game, &pid("Owner"), bag_item(5, FAMILY_STAMP_ITEM, 10)).await;
    game.rename_family(&pid("Owner"), &registrar, "Elmwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "once every 30 days"));
    assert_eq!(stamps(&game, "Owner").await, 10);

    let (_, mut rx) = enter(&game, &auth, "Rival", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Rival", 1).await;
    game.create_family(&pid("Rival"), &registrar, "oakheart", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "Another family has that name"));

    let account = auth.login_google("family-namesake").unwrap();
    assert!(auth
        .create_character(
            &account,
            "IRONWOOD",
            &attrs_with_cha(12),
            16,
            CharacterClass::Knight,
            Gender::Male,
        )
        .is_err());
}

#[tokio::test]
async fn invites_add_members_who_share_the_name_and_leaving_starts_a_cooldown() {
    let (game, auth, registrar) = setup("family_invite").await;
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (_, mut guest_rx) = enter(&game, &auth, "Guest", 0).await;

    game.invite_to_family(&pid("Head"), "Guest", &auth).await;
    assert!(drain(&mut guest_rx).iter().any(|msg| matches!(msg,
        ServerMessage::FamilyInviteReceived { inviter_name, family_name, .. }
            if inviter_name == "Head" && family_name == "Ironwood")));
    game.respond_to_family_invite(&pid("Guest"), &pid("Head"), true, &auth)
        .await;
    let family = family_state(&drain(&mut guest_rx))
        .flatten()
        .expect("roster");
    assert_eq!(family.my_rank, FAMILY_RANK_MEMBER);
    assert_eq!(family.members.len(), 2);
    assert!(family.members.iter().all(|m| m.online));
    assert_eq!(
        shown_family(&game, "Guest").await.as_deref(),
        Some("Ironwood")
    );
    assert_eq!(
        family_state(&drain(&mut head_rx))
            .flatten()
            .unwrap()
            .members
            .len(),
        2
    );

    game.remove_from_family(&pid("Head"), None, &auth).await;
    assert!(has_line(&drain(&mut head_rx), "Hand the family"));

    game.remove_from_family(&pid("Guest"), None, &auth).await;
    assert_eq!(family_state(&drain(&mut guest_rx)), Some(None));
    assert_eq!(shown_family(&game, "Guest").await, None);
    game.invite_to_family(&pid("Head"), "Guest", &auth).await;
    assert!(has_line(&drain(&mut head_rx), "less than a day ago"));
}

#[tokio::test]
async fn rank_permissions_open_the_family_estate_and_no_further() {
    let (game, auth, registrar) = setup("family_permissions").await;
    let (head, _head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (member, mut member_rx) = enter(&game, &auth, "Member", 0).await;
    let (stranger, _) = enter(&game, &auth, "Stranger", 0).await;
    join(&game, &auth, "Head", "Member").await;

    let plots = |id, bit| auth.fence_plots(id, Some(bit)).unwrap().len();
    assert_eq!(plots(head, permission::BUILD_HOUSE), 1);
    assert_eq!(plots(member, permission::PLACE_FURNITURE), 1);
    assert_eq!(plots(member, permission::BUILD_HOUSE), 0);
    assert_eq!(plots(member, permission::LANDSCAPE), 0);
    assert_eq!(plots(stranger, permission::PLACE_FURNITURE), 0);
    assert!(auth
        .estate_storage_plots(member, "storage_chest")
        .unwrap()
        .is_empty());
    assert_eq!(
        auth.estate_storage_plots(member, "furniture_hearthbound_rug")
            .unwrap()
            .len(),
        1
    );

    game.set_family_member_rank(&pid("Member"), head, FAMILY_RANK_GUEST, &auth)
        .await;
    assert!(has_line(&drain(&mut member_rx), "may not change ranks"));
    game.set_family_member_rank(&pid("Head"), member, FAMILY_RANK_ELDER, &auth)
        .await;
    assert_eq!(
        family_state(&drain(&mut member_rx))
            .flatten()
            .unwrap()
            .my_rank,
        FAMILY_RANK_ELDER
    );
    assert_eq!(plots(member, permission::BUILD_HOUSE), 1);
    assert_eq!(plots(member, permission::LANDSCAPE), 1);
    assert!(auth.landscaping_owner_ids(member).unwrap().contains(&head));

    game.set_family_rank(
        &pid("Head"),
        FAMILY_RANK_ELDER,
        "Uncle".into(),
        permission::EDIT_RANKS | permission::INVITE,
        &auth,
    )
    .await;
    let family = family_state(&drain(&mut member_rx)).flatten().unwrap();
    assert_eq!(family.ranks[usize::from(FAMILY_RANK_ELDER)].label, "Uncle");
    assert_eq!(plots(member, permission::BUILD_HOUSE), 0);
    game.set_family_rank(
        &pid("Member"),
        FAMILY_RANK_GUEST,
        "Guest".into(),
        permission::BUILD_HOUSE,
        &auth,
    )
    .await;
    assert!(has_line(&drain(&mut member_rx), "do not hold"));
}

#[tokio::test]
async fn a_members_estate_joins_the_family_and_leaves_with_them() {
    let (game, auth, registrar) = setup("family_estates").await;
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (member, mut member_rx) = enter(&game, &auth, "Member", 0).await;
    claim(&game, &auth, "Member", 1).await;
    join(&game, &auth, "Head", "Member").await;
    let family = family_state(&drain(&mut member_rx)).flatten().unwrap();
    assert!(family.can_attach_estate);
    assert_eq!(family.estates.len(), 1);

    game.attach_estate_to_family(&pid("Member"), true, &auth)
        .await;
    let family = family_state(&drain(&mut member_rx)).flatten().unwrap();
    assert!(!family.can_attach_estate);
    assert_eq!(family.estates.len(), 2);
    assert!(auth
        .owned_land_plots()
        .unwrap()
        .iter()
        .all(|p| p.owner_name == "Ironwood"));
    let head_estate = family.estates.iter().find(|e| !e.mine).unwrap().estate_id;

    game.deposit_family_estate(&pid("Member"), head_estate, 300, &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&pid("Member")], 200);
    let head = game.character_id_of(&pid("Head")).await.unwrap();
    assert_eq!(auth.land_account(head).unwrap().treasury, 300);
    game.deposit_family_estate(&pid("Member"), head_estate + 100, 100, &auth)
        .await;
    assert!(has_line(&drain(&mut member_rx), "not attached"));
    assert_eq!(game.player_gold.read().await[&pid("Member")], 200);

    game.remove_from_family(&pid("Head"), Some(member), &auth)
        .await;
    assert_eq!(family_state(&drain(&mut member_rx)), Some(None));
    let family = family_state(&drain(&mut head_rx)).flatten().unwrap();
    assert_eq!(family.estates.len(), 1);
    assert_eq!(family.members.len(), 1);
    assert_eq!(auth.fence_plots(member, None).unwrap().len(), 1);
    let mut labels: Vec<_> = auth
        .owned_land_plots()
        .unwrap()
        .into_iter()
        .map(|p| p.owner_name)
        .collect();
    labels.sort();
    assert_eq!(labels, ["Ironwood", "Member"]);
}

#[tokio::test]
async fn a_family_outlives_its_head_and_dissolves_with_its_last_member() {
    let (game, auth, registrar) = setup("family_succession").await;
    let (head, _) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (_, mut elder_rx) = enter(&game, &auth, "Elder", 0).await;
    let (_, _later_rx) = enter(&game, &auth, "Later", 0).await;
    join(&game, &auth, "Head", "Elder").await;
    join(&game, &auth, "Head", "Later").await;

    game.claim_family_head(&pid("Elder"), &auth).await;
    assert!(has_line(&drain(&mut elder_rx), "not been away long enough"));

    let account = auth.login_google("family-Head").unwrap();
    auth.delete_character(&account, head).unwrap();
    game.forget_player_family(&pid("Head")).await;
    game.settle_families(&auth).await;
    let family = family_state(&drain(&mut elder_rx)).flatten().unwrap();
    assert_eq!(family.my_rank, FAMILY_RANK_HEAD);
    assert_eq!(family.members.len(), 2);
    assert!(family.estates.is_empty());

    game.remove_from_family(&pid("Later"), None, &auth).await;
    game.remove_from_family(&pid("Elder"), None, &auth).await;
    assert!(has_line(&drain(&mut elder_rx), "Visit the Land Registrar"));
    game.dissolve_family(&pid("Elder"), &pid("Later"), &auth)
        .await;
    assert!(has_line(&drain(&mut elder_rx), "with the Land Registrar"));
    game.dissolve_family(&pid("Elder"), &registrar, &auth).await;
    assert!(has_line(&drain(&mut elder_rx), "you need 1 revenue stamp"));
    give(&game, &pid("Elder"), bag_item(9, FAMILY_STAMP_ITEM, 2)).await;
    game.dissolve_family(&pid("Elder"), &registrar, &auth).await;
    assert!(has_line(&drain(&mut elder_rx), "dissolved"));
    assert_eq!(stamps(&game, "Elder").await, 1);
    assert_eq!(shown_family(&game, "Elder").await, None);

    let (_, mut rx) = enter(&game, &auth, "Next", FAMILY_FOUND_STAMPS + 1).await;
    claim(&game, &auth, "Next", 2).await;
    game.create_family(&pid("Next"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "held back"));
}

fn family_lines(msgs: &[ServerMessage]) -> Vec<(String, String)> {
    msgs.iter()
        .filter_map(|msg| match msg {
            ServerMessage::FamilyChatMessage { from, message } => {
                Some((from.clone(), message.clone()))
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn family_chat_reaches_members_only_and_honours_blocks() {
    let (game, auth, registrar) = setup("family_chat").await;
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (_, mut member_rx) = enter(&game, &auth, "Member", 0).await;
    join(&game, &auth, "Head", "Member").await;
    let (_, mut outsider_rx) = enter(&game, &auth, "Outsider", 0).await;
    drain(&mut head_rx);
    drain(&mut member_rx);

    game.send_family_chat(&pid("Head"), "supper at the hall".into())
        .await;
    let line = ("Head".to_string(), "supper at the hall".to_string());
    assert_eq!(family_lines(&drain(&mut head_rx)), std::slice::from_ref(&line));
    assert_eq!(family_lines(&drain(&mut member_rx)), [line]);
    assert!(family_lines(&drain(&mut outsider_rx)).is_empty());

    game.send_family_chat(&pid("Outsider"), "hello?".into())
        .await;
    assert!(has_line(&drain(&mut outsider_rx), "do not belong"));
    assert!(family_lines(&drain(&mut head_rx)).is_empty());

    game.blocked_names
        .write()
        .await
        .entry(pid("Member"))
        .or_default()
        .insert("Head".into());
    game.send_family_chat(&pid("Head"), "anyone?".into()).await;
    assert_eq!(family_lines(&drain(&mut head_rx)).len(), 1);
    assert!(family_lines(&drain(&mut member_rx)).is_empty());
}

/// Family setup on flat ground, where estate furniture can be placed.
async fn flat_setup(tag: &str) -> (GameState, AuthService, PlayerId) {
    let game = make_flat_world_game_state(tag);
    let auth = make_test_auth(tag);
    let mut npc = make_player(REGISTRAR, 1.0, 1.0);
    npc.name = "Aldwin".into();
    npc.is_official_npc = true;
    game.add_player(npc).await;
    (game, auth, pid(REGISTRAR))
}

async fn stand_at(game: &GameState, name: &str, x: f32, z: f32) {
    game.players
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .position = pos3(x, 5.05, z);
}

fn line(instance_id: u64, qty: u32) -> BagLineItem {
    BagLineItem { instance_id, qty }
}

#[tokio::test]
async fn family_chests_follow_rank_and_stay_with_the_family() {
    use onlinerpg_shared::family::{FAMILY_CHEST_ITEM, FAMILY_CHEST_LIMIT};
    let (game, auth, registrar) = flat_setup("family_chests").await;
    let (head, mut head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (member, _) = enter(&game, &auth, "Member", 0).await;
    join(&game, &auth, "Head", "Member").await;
    let (guest, _) = enter(&game, &auth, "Guest", 0).await;
    join(&game, &auth, "Head", "Guest").await;
    game.set_family_member_rank(&pid("Head"), guest, FAMILY_RANK_GUEST, &auth)
        .await;
    let (outsider, _) = enter(&game, &auth, "Outsider", 0).await;

    give(&game, &pid("Head"), bag_item(10, FAMILY_CHEST_ITEM, 5)).await;
    give(&game, &pid("Member"), bag_item(20, "apple", 5)).await;
    give(&game, &pid("Guest"), bag_item(30, "apple", 5)).await;
    for name in ["Head", "Member", "Guest"] {
        stand_at(&game, name, 2.5, 3.5).await;
    }
    let spots = [2.5, 6.0, 9.5, 13.0, 16.5];
    for x in spots {
        stand_at(&game, "Head", x, 3.5).await;
        game.place_estate_chest(&pid("Head"), 10, pos3(x, 5.0, 2.5), 0.0, 0, &auth)
            .await;
    }
    let chests = auth.load_estate_chests().unwrap();
    assert_eq!(chests.len() as i64, FAMILY_CHEST_LIMIT);
    let chest = chests.iter().find(|c| c.position.x == 2.5).unwrap().id;
    assert_eq!(chests[0].owner_id, head);
    game.load_estate_chests(&auth).await.unwrap();
    stand_at(&game, "Head", 2.5, 3.5).await;

    assert!(auth.estate_chest_state(chest, outsider).unwrap().is_err());
    let state = auth.estate_chest_state(chest, guest).unwrap().unwrap();
    assert!(state.family && state.can_deposit && !state.can_withdraw);

    game.transfer_estate_items(&pid("Member"), chest, vec![line(20, 3)], vec![], 0, &auth)
        .await;
    game.transfer_estate_items(&pid("Guest"), chest, vec![line(30, 2)], vec![], 1, &auth)
        .await;
    let state = auth.estate_chest_state(chest, member).unwrap().unwrap();
    assert_eq!(state.revision, 2);
    let stored = state.items[0].instance_id;
    game.transfer_estate_items(
        &pid("Guest"),
        chest,
        vec![],
        vec![line(stored, 1)],
        2,
        &auth,
    )
    .await;
    assert_eq!(bag_count(&game, &pid("Guest"), "apple").await, 3);
    game.transfer_estate_items(
        &pid("Member"),
        chest,
        vec![],
        vec![line(stored, 1)],
        2,
        &auth,
    )
    .await;
    assert_eq!(bag_count(&game, &pid("Member"), "apple").await, 3);

    let log = auth.family_storage_log(member).unwrap().unwrap();
    let moves: Vec<_> = log
        .iter()
        .map(|e| (e.character_name.as_str(), e.quantity))
        .collect();
    assert_eq!(moves, [("Member", -1), ("Guest", 2), ("Member", 3)]);
    assert!(auth.family_storage_log(outsider).unwrap().is_none());

    drain(&mut head_rx);
    game.attach_estate_to_family(&pid("Head"), false, &auth)
        .await;
    assert!(has_line(&drain(&mut head_rx), "Empty the family chests"));

    game.remove_from_family(&pid("Head"), Some(guest), &auth)
        .await;
    assert!(auth.estate_chest_state(chest, guest).unwrap().is_err());
}

fn refusal(msgs: &[ServerMessage]) -> Option<NoticeBoardRefusal> {
    msgs.iter().rev().find_map(|msg| match msg {
        ServerMessage::NoticeBoardRefused { reason } => Some(*reason),
        _ => None,
    })
}

fn board_posts(msgs: &[ServerMessage]) -> Option<Vec<String>> {
    msgs.iter().rev().find_map(|msg| match msg {
        ServerMessage::NoticeBoard { posts, .. } => {
            Some(posts.iter().map(|p| p.title.clone()).collect())
        }
        _ => None,
    })
}

#[tokio::test]
async fn family_boards_are_for_members_and_free_to_post() {
    use onlinerpg_shared::family::FAMILY_BOARD_ITEM;
    let (game, auth, registrar) = flat_setup("family_board").await;
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_FOUND_STAMPS).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (_, mut member_rx) = enter(&game, &auth, "Member", 0).await;
    join(&game, &auth, "Head", "Member").await;
    let (guest, mut guest_rx) = enter(&game, &auth, "Guest", 0).await;
    join(&game, &auth, "Head", "Guest").await;
    game.set_family_member_rank(&pid("Head"), guest, FAMILY_RANK_GUEST, &auth)
        .await;
    let (_, mut outsider_rx) = enter(&game, &auth, "Outsider", 0).await;

    give(&game, &pid("Head"), bag_item(10, FAMILY_BOARD_ITEM, 1)).await;
    for name in ["Head", "Member", "Guest", "Outsider"] {
        stand_at(&game, name, 4.0, 4.0).await;
    }
    game.place_estate_chest(&pid("Head"), 10, pos3(4.0, 5.0, 2.5), 0.0, 0, &auth)
        .await;
    let furniture_id = auth.load_estate_chests().unwrap()[0].id;
    game.load_estate_chests(&auth).await.unwrap();
    let board = NoticeBoardRef::Estate { furniture_id };

    game.write_notice_post(&pid("Member"), board, "Supper", "at six", None, None, &auth)
        .await;
    assert!(refusal(&drain(&mut member_rx)).is_none());
    game.write_notice_post(&pid("Guest"), board, "Hi", "guest", None, None, &auth)
        .await;
    assert_eq!(
        refusal(&drain(&mut guest_rx)),
        Some(NoticeBoardRefusal::NoFamilyRank)
    );
    game.open_notice_board(&pid("Guest"), board, &auth).await;
    assert_eq!(
        board_posts(&drain(&mut guest_rx)),
        Some(vec!["Supper".into()])
    );
    game.open_notice_board(&pid("Outsider"), board, &auth).await;
    let seen = drain(&mut outsider_rx);
    assert_eq!(refusal(&seen), Some(NoticeBoardRefusal::NotFamily));
    assert!(board_posts(&seen).is_none());

    drain(&mut head_rx);
    let post_id = auth
        .notice_posts(
            BoardKey::estate(furniture_id),
            crate::auth::NoticeClock {
                expired_at: i64::MIN,
                hidden_at: i64::MIN,
            },
            0,
        )
        .unwrap()[0]
        .id;
    game.remove_notice_post(&pid("Head"), board, post_id, &auth)
        .await;
    assert_eq!(board_posts(&drain(&mut head_rx)), Some(vec![]));
}
