use super::*;

#[test]
fn a_titled_player_resolves_to_the_canonical_name() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.self_player_id = Some(PlayerId::from(1));
    let mut event = test_player(3.0, 0.0);
    event.id = PlayerId::from(2);
    event.name = "Event".to_string();
    event.title = Some("ogre_slayer_solo".to_string());
    s.nearby_players.insert(event.id, event);
    let display_name = "Event \"Who Slew the Ogre Warlord Alone\"";

    for name in ["Event", "event", "2", display_name] {
        assert_eq!(s.resolve_nearby_player(name), Some((2.into(), false)));
    }
    assert_eq!(
        s.resolve_move_target(display_name),
        Ok(MoveTarget::Character {
            id: 2.into(),
            name: "Event".to_string(),
        })
    );
    assert!(s.apply_favor(display_name, 1));
    assert_eq!(s.favor.get("Event"), Some(&1));
    assert!(!s.favor.contains_key(display_name));

    let world = s.format_world_state();
    assert!(world.contains("Player: Event (favor +1) Lv."), "{world}");
    assert!(
        world.contains("\n  Title: \"Who Slew the Ogre Warlord Alone\""),
        "{world}"
    );
    assert!(!world.contains(display_name), "{world}");

    let mut namesake = test_player(4.0, 0.0);
    namesake.id = 3.into();
    namesake.name = display_name.to_string();
    s.nearby_players.insert(namesake.id, namesake);
    assert_eq!(
        s.resolve_nearby_player(display_name),
        Some((3.into(), false))
    );
}

#[test]
fn title_matching_requires_a_unique_visible_player_with_that_title() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.self_player_id = Some(PlayerId::from(1));
    let mut event = test_player(3.0, 0.0);
    event.id = 2.into();
    event.name = "Event".to_string();
    event.title = Some("ogre_slayer_solo".to_string());
    s.nearby_players.insert(event.id, event.clone());
    let display_name = "Event \"Who Slew the Ogre Warlord Alone\"";

    for wrong in ["Event \"Wrong Title\"", "Who Slew the Ogre Warlord Alone"] {
        assert_eq!(s.resolve_nearby_player(wrong), None);
    }
    event.floor_level = 1;
    s.nearby_players.insert(event.id, event.clone());
    assert_eq!(s.resolve_nearby_player(display_name), None);
    event.floor_level = 0;
    event.position.x = EVENT_DELIVERY_RADIUS + 1.0;
    s.nearby_players.insert(event.id, event.clone());
    assert_eq!(
        s.resolve_nearby_player(display_name),
        Some((event.id, false))
    );
    s.nearby_players.remove(&event.id);
    assert_eq!(s.resolve_nearby_player(display_name), None);

    event.position.x = 3.0;
    event.title = None;
    s.nearby_players.insert(event.id, event.clone());
    assert_eq!(s.resolve_nearby_player(display_name), None);
    event.title = Some("ogre_slayer_solo".to_string());
    s.nearby_players.insert(event.id, event.clone());
    assert_eq!(
        s.resolve_nearby_player(display_name),
        Some((2.into(), false))
    );
    event.id = 3.into();
    s.nearby_players.insert(event.id, event);
    assert_eq!(s.resolve_nearby_player(display_name), None);
}

/// Chat history is a capped ring stamped with the game clock — the
/// short-term memory a stateless backend gets replayed each prompt.
#[test]
fn chat_history_stamps_and_caps() {
    let (mut s, _rx) = test_state();
    let chat = |message: String| ServerMessage::ChatMessage {
        player_id: PlayerId::from(2),
        message,
    };
    let mut player = test_player(0.0, 0.0);
    player.id = PlayerId::from(2);
    player.name = "jake1".into();
    s.nearby_players.insert(player.id, player);
    s.push_event(chat("hello".into()));
    s.finish_conversation();
    assert_eq!(s.chat_history()[0], "[Chat] jake1: hello");

    s.game_hour = Some(20);
    s.game_minute = Some(26);
    for i in 0..40 {
        s.push_event(chat(format!("line {i}")));
    }
    assert_eq!(s.pending_chat().len(), 30);
    s.finish_conversation();
    assert_eq!(s.chat_history().len(), 30, "capped at MAX_CHAT_HISTORY");
    assert_eq!(s.chat_history()[0], "[20:26] [Chat] jake1: line 10");
    assert_eq!(s.chat_history()[29], "[20:26] [Chat] jake1: line 39");
}

/// Favor: nearby human players only, one step per call, clamped in
/// total. Crossing the threshold makes a player keepsake-worthy and
/// the world state shows the standing next to their name.
#[test]
fn favor_accumulates_and_gates_keepsakes() {
    let (mut s, _rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);

    let mut jake = test_player(3.0, 0.0);
    jake.id = PlayerId::from(2);
    jake.name = "jake1".to_string();
    s.nearby_players.insert(jake.id, jake);

    let mut wick = test_player(4.0, 0.0);
    wick.id = PlayerId::from(3);
    wick.name = "Wick".to_string();
    wick.is_official_npc = true;
    s.nearby_players.insert(wick.id, wick);

    assert!(!s.apply_favor("Wick", 1), "NPCs never earn favor");
    assert!(!s.apply_favor("stranger", 1), "unknown names are dropped");
    assert!(
        s.apply_favor("jake1", 5),
        "an oversized delta still steps once"
    );
    assert_eq!(s.favor.get("jake1"), Some(&1));
    assert!(s.trade_worthy_players().is_empty());

    for _ in 0..10 {
        s.apply_favor("jake1", 1);
    }
    assert_eq!(s.favor.get("jake1"), Some(&FAVOR_MAX));
    assert_eq!(s.trade_worthy_players(), ["jake1"]);
    assert!(
        s.format_world_state().contains("jake1 (favor +5)"),
        "{}",
        s.format_world_state()
    );
}

/// The wishlist pitch needs an audience with any favor at all, and a
/// waved-off trade window blocks pushes at that player for the
/// cooldown — dropping them from the audience despite their favor.
#[test]
fn a_declined_trade_offer_blocks_further_pushes() {
    let (mut s, _rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);

    let mut jake = test_player(3.0, 0.0);
    jake.id = PlayerId::from(2);
    jake.name = "jake1".to_string();
    s.nearby_players.insert(jake.id, jake);

    assert!(!s.trade_offer_blocked(&PlayerId::from(2)));
    assert!(
        s.trade_worthy_players().is_empty(),
        "a stranger does not earn the shopping list"
    );
    s.apply_favor("jake1", 1);
    assert!(
        s.trade_worthy_players().is_empty(),
        "one kindness does not yet make a regular"
    );
    for _ in 0..2 {
        s.apply_favor("jake1", 1);
    }
    assert_eq!(
        s.trade_worthy_players(),
        ["jake1"],
        "favor at the trade threshold earns the pitch"
    );

    s.push_event(ServerMessage::TradeDeclined {
        player_id: PlayerId::from(2),
        player_name: "jake1".to_string(),
    });

    assert!(s.trade_offer_blocked(&PlayerId::from(2)));
    assert!(
        !s.trade_offer_blocked(&PlayerId::from(3)),
        "the block is per player, not global"
    );
    assert!(
        s.trade_worthy_players().is_empty(),
        "the only regular declined: the section vanishes"
    );
}

/// Friend requests queue like party invites, cap included. The server caps
/// them per requester, not per target, so without this cap a crowd of
/// strangers grows the prompt one line each.
#[test]
fn friend_requests_are_capped_like_party_invites() {
    let (mut s, _rx) = test_state();
    for i in 2..8u64 {
        s.push_event(ServerMessage::FriendRequestReceived {
            requester_id: PlayerId::from(i),
            requester_name: format!("stranger{i}"),
        });
    }
    assert_eq!(s.pending_friend_requests.len(), MAX_PENDING_FRIEND_REQUESTS);

    // A repeat from someone already queued is not a second entry.
    s.push_event(ServerMessage::FriendRequestReceived {
        requester_id: PlayerId::from(2),
        requester_name: "stranger2".to_string(),
    });
    assert_eq!(s.pending_friend_requests.len(), MAX_PENDING_FRIEND_REQUESTS);
}

/// A pushed trade window lapses like the web client's offer toast: it stops
/// prompting, and a later push from the same merchant reads as a new offer
/// rather than being suppressed as a repeat.
#[test]
fn a_pushed_trade_window_lapses_and_the_next_one_is_new() {
    let (mut s, _rx) = test_state();
    let merchant = PlayerId::from(9);
    let offer = || ServerMessage::ShopState {
        merchant_player_id: merchant,
        merchant_name: "Rica".to_string(),
        catalog: Vec::new(),
        sell_rate_percent: 50,
        active_deals: Vec::new(),
        wishlist: Vec::new(),
        stock: Vec::new(),
        buyback: Vec::new(),
        price_index_percent: 100,
    };

    s.push_event(offer());
    assert_eq!(s.drain_agent_events().len(), 1, "the offer reaches the LLM");
    assert!(s
        .format_world_state()
        .contains("Rica's trade window is open"));

    // A re-send while the offer stands is the same window, not a new one.
    s.push_event(offer());
    assert!(s.drain_agent_events().is_empty());

    s.pushed_trade.as_mut().unwrap().expires_at =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    assert!(
        !s.format_world_state().contains("trade window is open"),
        "a lapsed offer stops prompting"
    );
    s.push_event(offer());
    assert_eq!(
        s.drain_agent_events().len(),
        1,
        "an offer after the last one lapsed is a new offer"
    );

    // Trading with them answers the window: the web client's "Open" path.
    s.clear_pushed_trade(&merchant);
    assert!(!s.format_world_state().contains("trade window is open"));
}

pub(crate) fn family_info(my_rank: u8) -> onlinerpg_shared::family::FamilyInfo {
    use onlinerpg_shared::family::*;
    let member = |id: i64, name: &str, rank: u8, online: bool| FamilyMember {
        character_id: id,
        name: name.to_string(),
        level: 10,
        class: onlinerpg_shared::CharacterClass::Rogue,
        rank,
        online,
    };
    FamilyInfo {
        name: "Stark".to_string(),
        ranks: (0..FAMILY_RANK_COUNT)
            .map(|rank| FamilyRank {
                label: default_rank_label(rank).to_string(),
                permissions: default_rank_permissions(rank),
            })
            .collect(),
        members: vec![
            member(1, "Ned", 0, true),
            member(2, "Me", my_rank, true),
            member(3, "Arya", 2, true),
            member(4, "Bran", 2, false),
        ],
        estates: vec![
            FamilyEstate {
                estate_id: 11,
                owner_name: "Ned".to_string(),
                plots: 4,
                missed: 0,
                mine: false,
            },
            FamilyEstate {
                estate_id: 12,
                owner_name: "Me".to_string(),
                plots: 1,
                missed: 1,
                mine: true,
            },
        ],
        my_rank,
        can_attach_estate: false,
        can_claim_head: false,
        rename_at: 0,
        emblem_texture: None,
        emblem_color: Some("#aa2233".to_string()),
        my_cape_emblem: true,
        seat_estate_id: None,
    }
}

#[test]
fn the_family_block_names_rank_powers_and_who_is_online() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.push_event(ServerMessage::FamilyState {
        family: Some(family_info(onlinerpg_shared::family::FAMILY_RANK_MEMBER)),
    });
    let world = s.format_world_state();
    assert!(
        world.contains(
            "Your family: Stark — you are Family (rank 2); you may store in family chests, \
             take from family chests, post on family boards"
        ),
        "{world}"
    );
    assert!(
        world.contains("  4 members; online besides you: Ned, Arya\n"),
        "{world}"
    );
    assert!(
        world.contains("  Family estates: Ned's, yours (overdue)"),
        "{world}"
    );
    assert!(
        world.contains("Your cape shows the family emblem"),
        "{world}"
    );
    assert!(!s.family_may(onlinerpg_shared::family::permission::INVITE));

    s.push_event(ServerMessage::FamilyState {
        family: Some(family_info(onlinerpg_shared::family::FAMILY_RANK_HEAD)),
    });
    assert!(s.is_family_head());
    assert!(s
        .format_world_state()
        .contains("you may invite, store in family chests"));

    s.push_event(ServerMessage::FamilyState { family: None });
    assert!(!s.format_world_state().contains("Your family"));
}

#[test]
fn family_invites_queue_like_party_invites_and_joining_settles_them() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    let invite = |i: u64| ServerMessage::FamilyInviteReceived {
        inviter_id: PlayerId::from(i),
        inviter_name: format!("head{i}"),
        family_name: format!("House{i}"),
    };
    assert_eq!(s.classify_event(&invite(2)), EventUrgency::Urgent);
    assert_eq!(
        crate::driver::format_event(&s, &invite(2)).as_deref(),
        Some(
            "[FamilyInvite] head2 invited you to join the House2 family. Accept with \
             {\"type\":\"family_accept\"} or decline with family_decline."
        )
    );
    for i in 2..8 {
        s.push_event(invite(i));
    }
    s.push_event(invite(2));
    assert_eq!(
        s.pending_family_invites.len(),
        family::MAX_PENDING_FAMILY_INVITES
    );
    assert!(s.format_world_state().contains(
        "Pending family invite from head2 to join the House2 family — answer with family_accept"
    ));

    s.pending_family_invites[0].expires_at =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    assert!(!s.format_world_state().contains("from head2"));
    assert_eq!(s.live_family_invites().count(), 2);

    s.push_event(ServerMessage::FamilyState {
        family: Some(family_info(2)),
    });
    assert!(s.pending_family_invites.is_empty());
    assert_eq!(
        s.classify_event(&ServerMessage::FamilyState { family: None }),
        EventUrgency::Noise
    );
}

#[test]
fn a_merge_offer_spells_out_the_outcome_and_waits_for_the_head() {
    use onlinerpg_shared::family::{FamilyMergeProposal, FamilyMergeTerms, MergeSide};
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.push_event(ServerMessage::FamilyState {
        family: Some(family_info(0)),
    });
    let offer = |proposer: u64, keep_name| ServerMessage::FamilyMergeProposal {
        proposal: FamilyMergeProposal {
            proposer_id: PlayerId::from(proposer),
            proposer_name: "Robert".to_string(),
            proposer_family: "Baratheon".to_string(),
            target_name: "Eddard".to_string(),
            target_family: "Stark".to_string(),
            terms: FamilyMergeTerms {
                keep_name,
                keep_emblem: MergeSide::Target,
                keep_head: MergeSide::Proposer,
            },
            emblem: Default::default(),
            members: 12,
        },
    };
    let msg = offer(5, MergeSide::Proposer);
    assert_eq!(s.classify_event(&msg), EventUrgency::Urgent);
    let line = crate::driver::format_event(&s, &msg).unwrap();
    assert!(
        line.starts_with(
            "[FamilyMerge] Robert, head of Baratheon, offers to merge with your Stark: the \
             merged family is named Baratheon, shows your emblem, is led by Robert, and has \
             12 members. A merge cannot be undone."
        ),
        "{line}"
    );
    s.push_event(msg);
    s.push_event(offer(5, MergeSide::Target));
    assert_eq!(s.pending_family_merges.len(), 1, "one offer per proposer");
    let world = s.format_world_state();
    assert!(
        world.contains(
            "Merge offer from Robert, head of Baratheon: the merged family is named Stark"
        ),
        "{world}"
    );

    s.push_event(ServerMessage::FamilyState {
        family: Some(family_info(1)),
    });
    assert!(
        s.pending_family_merges.is_empty(),
        "only a head answers a merge"
    );
}

#[test]
fn family_chat_from_others_wakes_but_our_echo_is_dropped() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    let line = |from: &str| ServerMessage::FamilyChatMessage {
        from: from.to_string(),
        message: "hunt?".to_string(),
    };
    assert_eq!(s.classify_event(&line("Arya")), EventUrgency::Urgent);
    assert_eq!(
        crate::driver::format_event(&s, &line("Arya")).as_deref(),
        Some("[Family] Arya: hunt?")
    );
    assert_eq!(s.classify_event(&line("Me")), EventUrgency::Noise);
    assert_eq!(crate::driver::format_event(&s, &line("Me")), None);
}

#[test]
fn a_family_refusal_reaches_the_model_as_a_system_line() {
    let (s, _rx) = test_state();
    let refusal = ServerMessage::SystemMessage {
        message: "Family: no one called jake9 is online.".to_string(),
        localization: None,
    };
    assert_eq!(s.classify_event(&refusal), EventUrgency::Routine);
    assert_eq!(
        crate::driver::format_event(&s, &refusal).as_deref(),
        Some("[System] Family: no one called jake9 is online.")
    );
}
