use super::*;
use onlinerpg_shared::skills::SkillId;

#[tokio::test]
async fn learning_notifies_once_and_saves_without_xp() {
    let game = make_test_game_state("skill_learning");
    let player = pid("learner");
    game.add_player(make_player("learner", 0.0, 0.0)).await;
    game.register_player_character(&player, 42, 0, attrs_with_cha(10), 0, None)
        .await;
    game.register_player_skills(&player, Default::default())
        .await;
    let mut rx = game.register_direct_channel(&player).await;

    assert!(!game.has_skill(&player, SkillId::Fishing).await);
    assert!(game.learn_skill(&player, SkillId::Fishing).await);
    assert!(
        matches!(rx.try_recv().unwrap(), ServerMessage::SkillsUpdate { skills } if skills.has(SkillId::Fishing))
    );
    assert!(!game.learn_skill(&player, SkillId::Fishing).await);
    assert!(matches!(rx.try_recv(), Err(MpscTryRecvError::Empty)));

    let (dirty_ids, dirty) = game.collect_dirty_skill_states().await;
    assert_eq!(dirty_ids, vec![player]);
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].0, 42);
    assert_eq!(dirty[0].1.len(), 1);
    assert_eq!(dirty[0].1[0].skill_id, "fishing");
    assert!(game.collect_dirty_skill_states().await.1.is_empty());
    game.restore_dirty_skills(dirty_ids).await;
    assert_eq!(game.collect_dirty_skill_states().await.1.len(), 1);
}

#[tokio::test]
async fn take_player_skills_snapshots_and_detaches() {
    let game = make_test_game_state("skill_detach");
    let player = pid("learner");
    game.add_player(make_player("learner", 0.0, 0.0)).await;
    game.register_player_character(&player, 7, 0, attrs_with_cha(10), 0, None)
        .await;
    game.register_player_skills(&player, Default::default())
        .await;
    assert!(game.learn_skill(&player, SkillId::Fishing).await);

    let (character_id, rows) = game.take_player_skills(&player).await.unwrap();
    assert_eq!(character_id, 7);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].skill_id, "fishing");
    assert!(crate::game_state::skills::skills_from_rows(&rows).has(SkillId::Fishing));
    assert!(game.take_player_skills(&player).await.is_none());
    assert!(game.collect_dirty_skill_states().await.1.is_empty());
}

#[test]
fn stored_skill_ids_load_as_learned_and_ignore_unknown_ids() {
    let skills = crate::game_state::skills::skills_from_rows(&[
        crate::auth::SkillRow {
            skill_id: "fishing".into(),
        },
        crate::auth::SkillRow {
            skill_id: "unknown_skill".into(),
        },
    ]);
    assert!(skills.has(SkillId::Fishing));
    assert_eq!(
        serde_json::to_value(skills).unwrap(),
        serde_json::json!({ "learned": ["fishing"] })
    );
}
