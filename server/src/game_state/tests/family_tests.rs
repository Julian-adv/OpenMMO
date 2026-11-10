use super::*;
use crate::auth::AuthService;
use onlinerpg_shared::family::{
    permission, FamilyInfo, FAMILY_CREATE_COST, FAMILY_RANK_ELDER, FAMILY_RANK_GUEST,
    FAMILY_RANK_HEAD, FAMILY_RANK_MEMBER,
};

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
async fn enter(game: &GameState, auth: &AuthService, name: &str, gold: i64) -> (i64, DirectRx) {
    let account = auth.login_google(&format!("family-{name}")).unwrap();
    let deeds = vec![bag_item(1, "land_deed", 1)];
    let id = estate_owner(game, auth, &account, name, pos3(1.0, 0.0, 1.0), deeds).await;
    game.player_gold.write().await.insert(pid(name), gold);
    (id, game.register_direct_channel(&pid(name)).await)
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
    game.player_entered_family(&pid(name), auth).await;
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
    let (_, mut rx) = enter(&game, &auth, "Landless", FAMILY_CREATE_COST).await;
    game.create_family(&pid("Landless"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "Own an estate"));
    assert_eq!(
        game.player_gold.read().await[&pid("Landless")],
        FAMILY_CREATE_COST
    );

    let (owner, mut rx) = enter(&game, &auth, "Owner", FAMILY_CREATE_COST - 1).await;
    claim(&game, &auth, "Owner", 0).await;
    game.create_family(&pid("Owner"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "you need 10000 gold"));
    game.player_gold
        .write()
        .await
        .insert(pid("Owner"), FAMILY_CREATE_COST + 5);
    game.create_family(&pid("Owner"), &registrar, "Landless", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "A character has that name"));
    assert_eq!(
        game.player_gold.read().await[&pid("Owner")],
        FAMILY_CREATE_COST + 5
    );

    game.create_family(&pid("Owner"), &registrar, "Ironwood", &auth)
        .await;
    let family = family_state(&drain(&mut rx)).flatten().expect("roster");
    assert_eq!(family.name, "Ironwood");
    assert_eq!(family.my_rank, FAMILY_RANK_HEAD);
    assert_eq!(family.my_permissions(), permission::ALL);
    assert_eq!(family.estates.len(), 1);
    assert!(family.estates[0].mine);
    assert_eq!(game.player_gold.read().await[&pid("Owner")], 5);
    assert_eq!(
        shown_family(&game, "Owner").await.as_deref(),
        Some("Ironwood")
    );
    assert_eq!(
        auth.family_name_of(owner).unwrap().as_deref(),
        Some("Ironwood")
    );
    let account = auth.login_google("family-Owner").unwrap();
    let listed = auth.list_characters_with_equipment(&account).unwrap();
    assert_eq!(listed[0].family.as_deref(), Some("Ironwood"));

    let (_, mut rx) = enter(&game, &auth, "Rival", FAMILY_CREATE_COST).await;
    claim(&game, &auth, "Rival", 1).await;
    game.create_family(&pid("Rival"), &registrar, "ironwood", &auth)
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
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_CREATE_COST).await;
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
    let (head, _head_rx) = enter(&game, &auth, "Head", FAMILY_CREATE_COST).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (member, mut member_rx) = enter(&game, &auth, "Member", 0).await;
    let (stranger, _) = enter(&game, &auth, "Stranger", 0).await;
    join(&game, &auth, "Head", "Member").await;

    let plots = |id, bit| auth.fence_plots(id, bit).unwrap().len();
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
    let (_, mut head_rx) = enter(&game, &auth, "Head", FAMILY_CREATE_COST).await;
    claim(&game, &auth, "Head", 0).await;
    found(&game, &auth, &registrar, "Head", "Ironwood").await;
    let (member, mut member_rx) = enter(&game, &auth, "Member", 500).await;
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
    assert_eq!(auth.fence_plots(member, 0).unwrap().len(), 1);
}

#[tokio::test]
async fn a_family_outlives_its_head_and_dissolves_with_its_last_member() {
    let (game, auth, registrar) = setup("family_succession").await;
    let (head, _) = enter(&game, &auth, "Head", FAMILY_CREATE_COST).await;
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
    assert!(has_line(&drain(&mut elder_rx), "dissolved"));
    assert_eq!(shown_family(&game, "Elder").await, None);

    let (_, mut rx) = enter(&game, &auth, "Next", FAMILY_CREATE_COST).await;
    claim(&game, &auth, "Next", 2).await;
    game.create_family(&pid("Next"), &registrar, "Ironwood", &auth)
        .await;
    assert!(has_line(&drain(&mut rx), "held back"));
}
