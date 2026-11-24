use super::super::passability::region_of;
use super::*;
use onlinerpg_shared::seasonal::{seasonal_prop_defs, MonthDay};

const GROUND: f32 = 4.0;

fn day(month: u8, day: u8) -> MonthDay {
    MonthDay::new(month, day).unwrap()
}

/// The world on `today`, with the region the Aldermark winter tree stands in.
fn setup(today: MonthDay) -> (GameState, (i32, i32)) {
    let game = make_test_game_state("seasonal");
    let sites = seasonal_prop_defs()
        .iter()
        .map(|def| (def, def.placement(GROUND)))
        .collect();
    game.place_seasonal_props(sites, today);
    let tree = seasonal_prop_defs()
        .iter()
        .find(|def| def.id == "winter_tree_aldermark")
        .expect("the Aldermark winter tree");
    (game, region_of(tree.x, tree.z))
}

#[tokio::test]
async fn the_winter_tree_stands_and_blocks_only_in_season() {
    let (game, (rx, rz)) = setup(day(12, 24));

    let standing = game.fixed_placements_in(rx, rz);
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].type_id, "winter_tree");
    assert_eq!(standing[0].y, GROUND);
    assert!(
        game.sync_region_furniture(rx, rz, &[]),
        "the tree seals its cells"
    );

    assert_eq!(game.set_seasonal_date(day(1, 6)), vec![(rx, rz)]);
    assert!(game.fixed_placements_in(rx, rz).is_empty());
    assert!(!game.sync_region_furniture(rx, rz, &[]));
}

#[tokio::test]
async fn a_day_inside_the_season_changes_no_region() {
    let (game, (rx, rz)) = setup(day(12, 24));

    assert!(game.set_seasonal_date(day(12, 25)).is_empty());
    assert!(game.set_seasonal_date(day(1, 5)).is_empty());
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
}

#[tokio::test]
async fn the_tree_goes_up_when_the_season_opens() {
    let (game, (rx, rz)) = setup(day(12, 9));
    assert!(game.fixed_placements_in(rx, rz).is_empty());

    assert_eq!(game.set_seasonal_date(day(12, 10)), vec![(rx, rz)]);
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
}
