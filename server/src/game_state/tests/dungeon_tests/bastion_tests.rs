use super::*;

#[tokio::test]
async fn sundered_bastion_floors_spawn_their_own_pool() {
    let game_state = make_test_game_state("sundered_bastion_floors");
    assert_floor_pool(
        &game_state,
        "sundered_bastion",
        1,
        &["stone_golem", "lizardfolk"],
    )
    .await;
    assert_floor_pool(
        &game_state,
        "sundered_bastion",
        24,
        &["stone_golem", "cyclop", "skeleton_warrior"],
    )
    .await;
}

#[tokio::test]
async fn sundered_bastion_spawns_barkan_and_rewards_every_floor_key() {
    let (boss, _, gold) = open_final_chest("sundered_bastion_rewards", "sundered_bastion").await;
    assert_eq!(boss.monster_type, "barkan");
    assert_eq!(boss.level_override, Some(30));
    assert_eq!(boss.health, 600);
    assert!(boss.aggressive);
    assert!((12_500..=37_500).contains(&gold));

    let game_state = make_test_game_state("sundered_bastion_table");
    let table: HashMap<_, _> = game_state
        .item_defs
        .chest_roll_table(5)
        .into_iter()
        .collect();
    assert_eq!(table["ring_of_protection"], 0.2);
    let chances = chest_chances(&game_state, "sundered_bastion");
    assert_eq!(chances["rune_blade"], 0.2);
    assert_eq!(chances["earth_amulet"], 0.1);
    assert_eq!(chances["ring_of_protection"], 0.2);
    assert!(!chances.contains_key("shadowfang"));
}
