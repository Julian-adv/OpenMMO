use super::*;
use onlinerpg_shared::messages::OutfitGrant;

const RICA_OUTFIT: [&str; 3] = ["shopkeeper_top", "shopkeeper_pants", "shopkeeper_boots"];

struct Scene {
    game: GameState,
    auth: crate::auth::AuthService,
    buyer: PlayerId,
    rica: PlayerId,
    buyer_rx: DirectRx,
    rica_rx: DirectRx,
}

async fn scene(name: &str, gender: Gender, gold: i64) -> Scene {
    let game = make_test_game_state(name);
    let auth = make_test_auth(name);
    let account = auth.login_google(&format!("sub-{name}")).unwrap();
    let character = create_test_character(&auth, &account, "Mina");
    game.add_player(make_npc("npc_rica", "Rica", 0.0, 0.0))
        .await;
    let mut buyer = make_player("buyer", 1.0, 0.0);
    buyer.gender = gender;
    game.add_player(buyer).await;
    let (buyer, rica) = (pid("buyer"), pid("npc_rica"));
    game.register_player_character(&buyer, character.id, 0, attrs_with_cha(10), gold, None)
        .await;
    game.inventories
        .write()
        .await
        .insert(buyer, PlayerInventory::default());
    let outfits = auth.load_npc_outfits(&account).unwrap();
    game.set_outfit_ledger(&buyer, account, character.id, outfits)
        .await;
    let buyer_rx = game.register_direct_channel(&buyer).await;
    let rica_rx = game.register_direct_channel(&rica).await;
    Scene {
        game,
        auth,
        buyer,
        rica,
        buyer_rx,
        rica_rx,
    }
}

impl Scene {
    async fn second(&mut self, gold: i64) -> PlayerId {
        let account = self.auth.login_google("sub-outfit-second").unwrap();
        let character = create_test_character(&self.auth, &account, "Lia");
        let mut second = make_player("second", 1.0, 1.0);
        second.gender = Gender::Female;
        self.game.add_player(second).await;
        let second = pid("second");
        self.game
            .register_player_character(&second, character.id, 0, attrs_with_cha(10), gold, None)
            .await;
        self.game
            .inventories
            .write()
            .await
            .insert(second, PlayerInventory::default());
        self.game
            .set_outfit_ledger(&second, account, character.id, Default::default())
            .await;
        second
    }

    async fn pieces(&self) -> Vec<u32> {
        let mut counts = Vec::new();
        for id in RICA_OUTFIT {
            counts.push(bag_count(&self.game, &self.buyer, id).await);
        }
        counts
    }

    async fn offer(&mut self, gift: bool) {
        self.game
            .offer_outfit(&self.rica, &self.buyer, gift, &self.auth)
            .await;
    }

    async fn answer(&mut self, accept: bool) {
        self.game
            .respond_outfit_offer(&self.buyer, &self.rica, accept, &self.auth)
            .await;
    }

    fn rica_statuses(&mut self) -> Vec<(OutfitGrant, String)> {
        drain(&mut self.rica_rx)
            .into_iter()
            .filter_map(|msg| match msg {
                ServerMessage::OutfitStatus { status, note, .. } => {
                    Some((status, note.unwrap_or_default()))
                }
                _ => None,
            })
            .collect()
    }

    fn offered_to_buyer(&mut self) -> bool {
        drain(&mut self.buyer_rx)
            .iter()
            .any(|msg| matches!(msg, ServerMessage::OutfitOffered { price: 30_000, .. }))
    }
}

#[tokio::test]
async fn a_sale_hands_over_every_piece_once() {
    let mut s = scene("outfit_sale", Gender::Female, 50_000).await;
    s.offer(false).await;
    assert!(s.offered_to_buyer());
    s.answer(true).await;
    s.answer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    assert_eq!(s.game.get_player_gold(&s.buyer).await, 20_000);
    let statuses = s.rica_statuses();
    assert_eq!(statuses.last().unwrap().0, OutfitGrant::Received);

    s.offer(false).await;
    s.offer(true).await;
    assert!(!s.offered_to_buyer());
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    assert!(s
        .rica_statuses()
        .iter()
        .all(|(status, note)| *status == OutfitGrant::Received && note.contains("once")));
}

#[tokio::test]
async fn a_declined_sale_leaves_one_gift() {
    let mut s = scene("outfit_decline", Gender::Female, 50_000).await;
    s.offer(false).await;
    assert!(s.offered_to_buyer());
    s.answer(false).await;
    assert_eq!(s.rica_statuses().last().unwrap().0, OutfitGrant::Declined);

    s.offer(false).await;
    assert!(!s.offered_to_buyer());
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    assert_eq!(s.game.get_player_gold(&s.buyer).await, 50_000);
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
}

#[tokio::test]
async fn a_gift_needs_a_declined_sale_and_the_record_survives_relog() {
    let mut s = scene("outfit_gift", Gender::Female, 0).await;
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![0, 0, 0]);
    assert!(s
        .rica_statuses()
        .iter()
        .any(|(_, note)| note.contains("offer it before giving it")));
    s.offer(false).await;
    s.answer(false).await;
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    let (account, character_id) = s.game.outfit_account(&s.buyer).await.unwrap();
    let reloaded = s.auth.load_npc_outfits(&account).unwrap();
    s.game
        .set_outfit_ledger(&s.buyer, account, character_id, reloaded)
        .await;
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
}

#[tokio::test]
async fn the_outfit_is_not_offered_to_the_other_gender() {
    let mut s = scene("outfit_gender", Gender::Male, 50_000).await;
    s.offer(false).await;
    s.offer(true).await;
    assert!(!s.offered_to_buyer());
    assert_eq!(s.pieces().await, vec![0, 0, 0]);
    assert!(s
        .rica_statuses()
        .iter()
        .all(|(_, note)| note.contains("would not fit")));
}

#[tokio::test]
async fn a_failed_handover_releases_the_claim() {
    let mut s = scene("outfit_fail", Gender::Female, 10_000).await;
    s.offer(false).await;
    s.answer(true).await;
    assert_eq!(s.pieces().await, vec![0, 0, 0]);
    assert_eq!(s.game.get_player_gold(&s.buyer).await, 10_000);
    assert_eq!(s.rica_statuses().last().unwrap().0, OutfitGrant::Open);
    s.offer(false).await;
    s.answer(false).await;

    s.game.inventories.write().await.insert(
        s.buyer,
        PlayerInventory {
            bag: vec![bag_item(900, "stone_hearth", 1)],
            ..Default::default()
        },
    );
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![0, 0, 0]);

    s.game
        .inventories
        .write()
        .await
        .insert(s.buyer, PlayerInventory::default());
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
}

#[tokio::test]
async fn an_offer_left_unanswered_counts_as_turned_down() {
    let mut s = scene("outfit_lapse", Gender::Female, 50_000).await;
    let (account, character_id) = s.game.outfit_account(&s.buyer).await.unwrap();
    s.offer(false).await;
    assert!(s.offered_to_buyer());
    s.game.remove_outfit_state(&s.buyer).await;
    s.game.query_outfit(&s.rica, &s.buyer, &s.auth).await;
    assert!(s
        .rica_statuses()
        .iter()
        .any(|(status, note)| *status == OutfitGrant::Declined && note.contains("lapse")));

    let reloaded = s.auth.load_npc_outfits(&account).unwrap();
    s.game
        .set_outfit_ledger(&s.buyer, account, character_id, reloaded)
        .await;
    s.offer(false).await;
    assert!(!s.offered_to_buyer());
    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
}

#[tokio::test]
async fn one_outfit_sells_per_game_day() {
    let mut s = scene("outfit_daily", Gender::Female, 50_000).await;
    s.offer(false).await;
    s.answer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    assert!(!s.rica_statuses().is_empty());

    let second = s.second(50_000).await;
    let mut second_rx = s.game.register_direct_channel(&second).await;

    s.game.offer_outfit(&s.rica, &second, false, &s.auth).await;
    assert!(!drain(&mut second_rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::OutfitOffered { .. })));
    let reopen = drain(&mut s.rica_rx).into_iter().find_map(|msg| match msg {
        ServerMessage::OutfitStatus {
            note: Some(note),
            sales_reopen_in_secs,
            ..
        } if note.contains("already sold") => Some(sales_reopen_in_secs),
        _ => None,
    });
    assert!(matches!(reopen, Some(Some(secs)) if secs > 0));
}

#[tokio::test]
async fn one_outfit_is_given_per_game_day() {
    let mut s = scene("outfit_daily_gift", Gender::Female, 0).await;
    let second = s.second(0).await;
    for player in [s.buyer, second] {
        s.game.offer_outfit(&s.rica, &player, false, &s.auth).await;
        s.game
            .respond_outfit_offer(&player, &s.rica, false, &s.auth)
            .await;
    }

    s.offer(true).await;
    assert_eq!(s.pieces().await, vec![1, 1, 1]);
    s.game.offer_outfit(&s.rica, &second, true, &s.auth).await;
    assert_eq!(bag_count(&s.game, &second, "shopkeeper_top").await, 0);
    let refused = drain(&mut s.rica_rx).into_iter().any(|msg| {
        matches!(msg, ServerMessage::OutfitStatus {
            note: Some(note),
            gifts_reopen_in_secs: Some(_),
            ..
        } if note.contains("given your outfit away today"))
    });
    assert!(refused);
}
