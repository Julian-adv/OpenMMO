use super::*;

/// Each variant wears its own model and fights like its Skeleton Crypt
/// counterpart on the same floors.
#[tokio::test]
async fn thieves_den_variants_match_their_crypt_counterparts() {
    let game_state = make_test_game_state("thieves_den_variants");
    assert_variants(
        &game_state,
        &[
            ("kobold_cutpurse", "kobold", "skeleton_weak"),
            ("hobgoblin_thug", "hobgoblin", "skeleton"),
            ("gnoll_stalker", "gnoll", "skeleton"),
            ("bugbear_enforcer", "bugbear", "skeleton_warrior"),
        ],
    );
    assert_eq!(
        game_state
            .monster_defs
            .get("gnoll_stalker")
            .unwrap()
            .hit_debuff
            .as_deref(),
        Some("bleed")
    );
}

#[tokio::test]
async fn thieves_den_floors_spawn_their_own_pool() {
    let game_state = make_test_game_state("thieves_den_floors");
    assert_floor_pool(&game_state, "thieves_den", 1, &["kobold_cutpurse"]).await;
    assert_floor_pool(
        &game_state,
        "thieves_den",
        12,
        &["hobgoblin_thug", "gnoll_stalker", "bugbear_enforcer"],
    )
    .await;
    assert_floor_pool(&game_state, "thieves_den", 19, &["bugbear_enforcer"]).await;
}

#[tokio::test]
async fn thieves_den_spawns_the_thief_king_and_guarantees_the_dirk() {
    let (boss, items, gold) = open_final_chest("thieves_den_rewards", "thieves_den").await;
    assert_eq!(boss.monster_type, "thief_king");
    assert_eq!(boss.level_override, Some(25));
    assert_eq!(boss.health, 350);
    assert!(boss.aggressive);
    for signature in ["breastplate", "cutthroat_dirk"] {
        assert_eq!(
            items.iter().filter(|id| id.as_str() == signature).count(),
            1
        );
    }
    assert!(!items.iter().any(|id| id == "great_sword"));
    assert!((10_000..=30_000).contains(&gold));

    let game_state = make_test_game_state("thieves_den_table");
    let den = chest_chances(&game_state, "thieves_den");
    assert_eq!(den["breastplate"], 1.0);
    assert_eq!(den["cutthroat_dirk"], 1.0);
    assert_eq!(den["plate_helmet"], 0.3);
    assert_eq!(den["plate_gauntlets"], 0.3);
    assert!(!den.contains_key("great_sword"));
    assert!(den.values().all(|chance| *chance > 0.0));
    let crypt = chest_chances(&game_state, "skeleton_crypt");
    assert_eq!(crypt["great_sword"], 1.0);
    assert!(!crypt.contains_key("cutthroat_dirk"));
}
