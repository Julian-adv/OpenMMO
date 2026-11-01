use super::*;

/// Each variant wears its own model and fights like its Sundered Bastion
/// counterpart on the same floors.
#[tokio::test]
async fn shadow_guild_variants_match_their_bastion_counterparts() {
    let game_state = make_test_game_state("shadow_guild_variants");
    assert_variants(
        &game_state,
        &[
            ("orc_bruiser", "orc", "stone_golem"),
            ("lizardfolk_assassin", "lizardfolk", "lizardfolk"),
            ("ogre_smuggler", "ogre", "cyclop"),
        ],
    );
}

#[tokio::test]
async fn shadow_guild_floors_spawn_their_own_pool() {
    let game_state = make_test_game_state("shadow_guild_floors");
    assert_floor_pool(
        &game_state,
        "shadow_guild",
        1,
        &["orc_bruiser", "lizardfolk_assassin"],
    )
    .await;
    assert_floor_pool(
        &game_state,
        "shadow_guild",
        13,
        &[
            "ogre_smuggler",
            "hobgoblin_thug",
            "gnoll_stalker",
            "bugbear_enforcer",
        ],
    )
    .await;
    assert_floor_pool(
        &game_state,
        "shadow_guild",
        24,
        &["orc_bruiser", "ogre_smuggler", "bugbear_enforcer"],
    )
    .await;
}

#[tokio::test]
async fn shadow_guild_spawns_the_shadow_master_and_rolls_its_own_dagger() {
    let (boss, items, gold) = open_final_chest("shadow_guild_rewards", "shadow_guild").await;
    assert_eq!(boss.monster_type, "shadow_master");
    assert_eq!(boss.level_override, Some(30));
    assert_eq!(boss.health, 600);
    assert!(boss.aggressive);
    assert!(!items
        .iter()
        .any(|id| id == "rune_blade" || id == "earth_amulet"));
    assert!((12_500..=37_500).contains(&gold));

    let game_state = make_test_game_state("shadow_guild_table");
    let guild = chest_chances(&game_state, "shadow_guild");
    assert_eq!(guild["shadowfang"], 0.2);
    assert_eq!(guild["ring_of_protection"], 0.2);
    assert_eq!(guild["cutthroat_dirk"], 0.1);
    assert_eq!(guild["great_sword"], 0.1);
    for id in ["rune_blade", "earth_amulet"] {
        assert!(!guild.contains_key(id), "{id}");
    }
    assert!(guild.values().all(|chance| *chance > 0.0));
    assert!(!chest_chances(&game_state, "sundered_bastion").contains_key("shadowfang"));
}
