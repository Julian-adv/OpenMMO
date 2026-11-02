use super::*;

/// Each variant wears a stand-in model and fights like its Sundered Bastion
/// counterpart on the same floors.
#[tokio::test]
async fn blackvein_mine_variants_match_their_bastion_counterparts() {
    let game_state = make_test_game_state("blackvein_mine_variants");
    assert_variants(
        &game_state,
        &[
            ("mine_remnant", "hobgoblin", "stone_golem"),
            ("mine_crystal", "orc", "cyclop"),
            ("mine_twisted", "troll", "skeleton"),
            ("mine_veinfused", "stone_golem", "skeleton_warrior"),
            ("mine_predator", "scp939", "skeleton_warrior"),
            ("mine_foreman", "cyclop", "shadow_master"),
        ],
    );
}

#[tokio::test]
async fn blackvein_mine_floors_spawn_one_kind_per_band() {
    let game_state = make_test_game_state("blackvein_mine_floors");
    for (depth, kind) in [
        (1, "mine_remnant"),
        (8, "mine_crystal"),
        (13, "mine_twisted"),
        (18, "mine_veinfused"),
        (24, "mine_predator"),
    ] {
        assert_floor_pool(&game_state, "blackvein_mine", depth, &[kind]).await;
    }
}

#[tokio::test]
async fn blackvein_mine_spawns_the_foreman_without_signature_rolls() {
    let (boss, items, gold) = open_final_chest("blackvein_mine_rewards", "blackvein_mine").await;
    assert_eq!(boss.monster_type, "mine_foreman");
    assert_eq!(boss.level_override, Some(30));
    assert_eq!(boss.health, 600);
    assert!(boss.aggressive);
    let signatures = ["rune_blade", "earth_amulet", "shadowfang"];
    assert!(!items.iter().any(|id| signatures.contains(&id.as_str())));
    assert!((12_500..=37_500).contains(&gold));

    let game_state = make_test_game_state("blackvein_mine_table");
    let mine = chest_chances(&game_state, "blackvein_mine");
    assert_eq!(mine["ring_of_protection"], 0.2);
    for id in signatures {
        assert!(!mine.contains_key(id), "{id}");
    }
}
