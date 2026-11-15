use super::*;
use std::net::Ipv4Addr;

#[test]
fn in_game_rejection_identifies_the_connection_without_debug_wrappers() {
    let (subscriber, buffer) = crate::test_util::capture_logs();
    let mut state = ConnectionState::new("203.0.113.7".parse().unwrap());
    state.account_name = Some("account1".into());
    state.character_name = Some("Player One".into());
    state.player_id = Some(PlayerId::from(42));
    tracing::subscriber::with_default(subscriber, || {
        assert!(state.require_not_in_game("RollCharacterStats").is_err());
    });
    let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    for field in [
        "ip=203.0.113.7",
        "account=\"account1\"",
        "character=\"Player One\"",
        "player_id=42",
        "action=\"RollCharacterStats\"",
    ] {
        assert!(logs.contains(field), "{logs}");
    }
    assert!(!logs.contains("Some("), "{logs}");
    assert!(!logs.contains("PlayerId("), "{logs}");
}

#[test]
fn party_positions_poll_clamped_inside_window() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    assert!(state.party_positions_poll_due());
    // Inside the window: dropped, and the drop must not refresh it.
    assert!(!state.party_positions_poll_due());
    state.last_party_positions_poll = Some(Instant::now() - PARTY_POSITIONS_MIN_INTERVAL);
    assert!(state.party_positions_poll_due());
}

#[test]
fn furniture_tips_are_limited_per_customer_and_item_kind() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    assert!(state.furniture_tip_due(FurnitureTip::StorageChest));
    state
        .last_furniture_tip
        .insert(FurnitureTip::StorageChest, Instant::now());
    assert!(!state.furniture_tip_due(FurnitureTip::StorageChest));
    assert!(state.furniture_tip_due(FurnitureTip::Bed));
    assert_eq!(
        FurnitureTip::for_item("furniture_bed"),
        FurnitureTip::for_item("furniture_rustic_bed")
    );
    let other = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    assert!(other.furniture_tip_due(FurnitureTip::StorageChest));
    state.last_furniture_tip.insert(
        FurnitureTip::StorageChest,
        Instant::now() - FURNITURE_TIP_INTERVAL,
    );
    assert!(state.furniture_tip_due(FurnitureTip::StorageChest));
}

#[test]
fn instrument_batch_limiter_allows_four_per_second_with_a_four_batch_burst() {
    let start = Instant::now();
    let mut limiter = InstrumentBatchLimiter {
        tokens: INSTRUMENT_BATCH_BURST,
        last: start,
    };

    for _ in 0..4 {
        assert!(limiter.allow_at(start));
    }
    assert!(!limiter.allow_at(start));
    assert!(!limiter.allow_at(start + Duration::from_millis(249)));
    assert!(limiter.allow_at(start + Duration::from_millis(250)));
    assert!(!limiter.allow_at(start + Duration::from_millis(250)));
    assert!(limiter.allow_at(start + Duration::from_millis(500)));
}
