use super::*;
use onlinerpg_shared::{
    dungeon::{cell_center, dungeon_origin, generate_dungeon_for, GRID},
    pathfinding::{RuntimeFloorGrid, RuntimePassability},
    WORLD_MAX_X, WORLD_MIN_X,
};
use rand::RngCore;
use std::collections::VecDeque;

const SCROLL: &str = "scroll_of_teleportation";
const NEAR_DUNGEONS: Position = Position {
    x: -1400.0,
    y: 5.0,
    z: 4500.0,
};

#[tokio::test]
async fn teleport_scroll_request_over_websocket_returns_arrival_without_replaying_departure() {
    scroll_request_over_websocket(SCROLL).await;
}

#[tokio::test]
async fn return_scroll_request_over_websocket_returns_arrival_without_replaying_departure() {
    scroll_request_over_websocket("scroll_of_return").await;
}

async fn scroll_request_over_websocket(item_def_id: &str) {
    use crate::connection::{handle_connection, AuthContext, ServerContext};
    use futures_util::{SinkExt, StreamExt};
    use onlinerpg_shared::ClientMessage;
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::watch;
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    async fn receive(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> ServerMessage {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = socket.next().await.unwrap().unwrap();
                if let Message::Binary(bytes) = message {
                    return onlinerpg_shared::deserialize_server_msg(&bytes).unwrap();
                }
            }
        })
        .await
        .expect("server message timed out")
    }

    let label = &format!("{item_def_id}_websocket");
    let game = Arc::new(make_flat_world_game_state(label));
    let auth = Arc::new(make_test_auth(label));
    let account = auth.login_npc("npc_teleport_reader").unwrap();
    let character = create_test_character(&auth, &account, "Reader");
    let context = Arc::new(ServerContext {
        geoip: Default::default(),
        game_state: game.clone(),
        auth_service: auth,
        auth_ctx: Arc::new(AuthContext {
            google: None,
            npc_token: "test-token".into(),
            admin_emails: vec![],
        }),
        connect_limiter: Default::default(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (_shutdown_tx, shutdown) = watch::channel(());
    let server = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        handle_connection(stream, peer, context, shutdown.clone(), shutdown).await;
    });
    let (mut socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
    for message in [
        ClientMessage::ClientInfo {
            protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
            client_kind: "web".into(),
            client_version: onlinerpg_shared::stamp_layout_version("test"),
        },
        ClientMessage::AuthenticateNpc {
            account_name: "npc_teleport_reader".into(),
            npc_token: "test-token".into(),
        },
        ClientMessage::EnterGame {
            character_id: character.id,
        },
    ] {
        socket
            .send(Message::Binary(
                onlinerpg_shared::serialize_client_msg(&message)
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }
    let id = loop {
        if let ServerMessage::JoinSuccess { player, .. } = receive(&mut socket).await {
            break player.id;
        }
    };
    let origin = Position {
        x: 800.0,
        y: 5.0,
        z: 800.0,
    };
    game.teleport_player(&id, origin, 0.0, 0).await;
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(100, item_def_id, 1));
    socket
        .send(Message::Binary(
            onlinerpg_shared::serialize_client_msg(&ClientMessage::UseTeleportScroll {
                instance_id: 100,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    loop {
        if let ServerMessage::PlayerTeleportEffect { phase, .. } = receive(&mut socket).await {
            assert_eq!(phase, onlinerpg_shared::TeleportPhase::Arriving);
            break;
        }
    }
    assert!(!game
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .iter()
        .any(|item| item.instance_id == 100));
    if item_def_id == "scroll_of_return" {
        assert_eq!(
            game.players.read().await[&id].position,
            crate::world_config::world_config()
                .spawn_position
                .position()
        );
    }
    loop {
        let message = receive(&mut socket).await;
        let messages = match message {
            ServerMessage::WorldUpdate { events, .. } => events
                .into_iter()
                .flat_map(|event| event.messages)
                .collect(),
            message => vec![message],
        };
        if messages.iter().any(|message| {
            matches!(message, ServerMessage::PlayerTeleported { player_id, position, .. }
            if *player_id == id && *position != origin)
        }) {
            break;
        }
    }
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

struct ScriptedRng(VecDeque<u64>);

impl RngCore for ScriptedRng {
    fn next_u64(&mut self) -> u64 {
        self.0.pop_front().unwrap_or(1 << 63)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes()[..chunk.len()]);
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

fn roll(fraction: f64) -> u64 {
    ((fraction * (1_u64 << 52) as f64) as u64) << 12
}

fn rolls_for_dungeon(game: &GameState, origin: Position, target: Position, floor: i8) -> Vec<u64> {
    let areas = game.teleport_dungeon_areas(&origin);
    let total = std::f64::consts::PI * (2_000.0_f64.powi(2) - 32.0_f64.powi(2))
        + areas
            .iter()
            .map(|a| f64::from(a.width) * f64::from(a.depth))
            .sum::<f64>();
    let mut preceding = 0.0;
    for area in areas {
        let size = f64::from(area.width) * f64::from(area.depth);
        if area.floor_level == floor
            && (area.origin.x..area.origin.x + area.width).contains(&target.x)
            && (area.origin.z..area.origin.z + area.depth).contains(&target.z)
        {
            return vec![
                roll((preceding + size * 0.5) / total),
                roll(f64::from((target.x - area.origin.x) / area.width)),
                roll(f64::from((target.z - area.origin.z) / area.depth)),
            ];
        }
        preceding += size;
    }
    panic!("target floor missing from teleport candidates");
}

async fn open_dungeon_cell(game: &GameState, dungeon: &str, depth: u8) -> Position {
    let entrance = game.dungeon_defs.get(dungeon).unwrap();
    let layout = &generate_dungeon_for(dungeon)[usize::from(depth) - 1];
    for (i, carved) in layout.carved.iter().enumerate() {
        if *carved {
            let at = cell_center(
                &entrance.position(),
                depth,
                (i as i32 % GRID, i as i32 / GRID),
            );
            if game
                .teleport_dungeon_landing(at, -(depth as i8))
                .await
                .is_some()
            {
                return at;
            }
        }
    }
    panic!("no open dungeon cell");
}

struct OceanTiles;

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for OceanTiles {
    async fn read_heightmap(&self, _tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        Ok(uniform_heightmap(-5.0))
    }
}

struct DistantLandTiles;

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for DistantLandTiles {
    async fn read_heightmap(&self, tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        Ok(uniform_heightmap(if tx.abs() >= 16 { 5.0 } else { -5.0 }))
    }
}

async fn give_scrolls(game: &GameState, player: Player, quantity: u32) -> PlayerId {
    let id = player.id;
    game.add_player(player).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(100, SCROLL, quantity)],
            ..Default::default()
        },
    );
    id
}

async fn quantity(game: &GameState, id: &PlayerId) -> u32 {
    game.get_player_inventory(id)
        .await
        .unwrap()
        .bag
        .iter()
        .filter(|item| item.item_def_id == SCROLL)
        .map(|item| item.quantity)
        .sum()
}

#[tokio::test]
async fn teleport_scroll_moves_in_combat_spends_one_and_clears_movement() {
    let game = make_flat_world_game_state("teleport_scroll_surface");
    let mut player = make_player("Reader", 800.0, 800.0);
    player.position.y = 5.0;
    player.rotation = 1.25;
    player.last_combat_at = GameState::now_ms();
    let origin = player.position;
    let id = give_scrolls(&game, player, 2).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.update_player_position(&id, move_cmd(Position { x: 805.0, ..origin }, false), false)
        .await;
    assert!(game.movement_intents.read().await.contains_key(&id));
    drain(&mut rx);

    game.use_item(&id, 100).await;

    let player = game.players.read().await[&id].clone();
    assert!((32.0..=2_000.01).contains(&origin.dist_xz_sq(&player.position).sqrt()));
    assert_eq!(player.position.y, 5.0);
    assert_eq!(player.floor_level, 0);
    assert_eq!(player.rotation, 1.25);
    assert_eq!(quantity(&game, &id).await, 1);
    assert!(!game.movement_intents.read().await.contains_key(&id));
    assert!(game.dirty_inventories.read().await.contains(&id));
    let messages = drain(&mut rx);
    let effects: Vec<_> = messages
        .iter()
        .filter_map(|msg| match msg {
            ServerMessage::PlayerTeleportEffect {
                phase,
                position,
                floor_level,
                ..
            } => Some((*phase, *position, *floor_level)),
            _ => None,
        })
        .collect();
    assert_eq!(
        effects,
        vec![(
            onlinerpg_shared::TeleportPhase::Arriving,
            player.position,
            0
        ),]
    );
    assert!(messages.iter().any(|msg| matches!(
        msg, ServerMessage::PlayerTeleported { player_id, position, floor_level: 0, .. }
            if *player_id == id && *position == player.position
    )));
    game.use_item(&id, 100).await;
    assert_eq!(quantity(&game, &id).await, 0);
    let last = game.players.read().await[&id].position;
    game.use_item(&id, 100).await;
    assert_eq!(game.players.read().await[&id].position, last);
}

#[tokio::test(start_paused = true)]
async fn teleport_scroll_rejects_a_request_if_the_reader_died_during_the_client_effect() {
    let game = make_flat_world_game_state("teleport_scroll_cancelled");
    let player = make_player("Reader", 800.0, 800.0);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.players.write().await.get_mut(&id).unwrap().health = 0;
    game.use_teleport_scroll(&id, 100).await;
    assert_eq!(quantity(&game, &id).await, 1);
    assert_eq!(game.players.read().await[&id].position, origin);
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|msg| matches!(
        msg,
        ServerMessage::PlayerTeleportEffect {
            phase: onlinerpg_shared::TeleportPhase::Cancelled,
            ..
        }
    )));
    assert!(!messages
        .iter()
        .any(|msg| matches!(msg, ServerMessage::PlayerTeleported { .. })));
}

#[tokio::test(start_paused = true)]
async fn teleport_scroll_has_no_server_animation_delay() {
    let game = make_flat_world_game_state("teleport_scroll_no_delay");
    let id = give_scrolls(&game, make_player("Reader", 800.0, 800.0), 1).await;
    let started = tokio::time::Instant::now();
    game.use_teleport_scroll(&id, 100).await;
    assert_eq!(tokio::time::Instant::now(), started);
    assert_eq!(quantity(&game, &id).await, 0);
}

#[tokio::test]
async fn teleport_scroll_rejects_missing_or_wrong_items_and_cancels_the_client_effect() {
    let game = make_flat_world_game_state("teleport_scroll_invalid_request");
    let player = make_player("Reader", 800.0, 800.0);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(101, "torch", 1));
    let mut rx = game.register_direct_channel(&id).await;
    for instance in [101, 999] {
        game.use_teleport_scroll(&id, instance).await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::PlayerTeleportEffect {
                phase: onlinerpg_shared::TeleportPhase::Cancelled,
                ..
            }
        )));
        assert_eq!(game.players.read().await[&id].position, origin);
        assert_eq!(quantity(&game, &id).await, 1);
    }
    assert!(game.inventories.read().await[&id]
        .bag
        .iter()
        .any(|item| item.instance_id == 101 && item.quantity == 1));
}

#[tokio::test(start_paused = true)]
async fn teleport_scroll_effects_reach_both_locations_on_the_correct_floor() {
    let game = make_flat_world_game_state("teleport_scroll_effect_observers");
    let id = give_scrolls(&game, make_player("Reader", 800.0, 800.0), 1).await;
    let from = make_player("Departure", 800.0, 800.0);
    let to = make_player("Arrival", 1800.0, 800.0);
    let mut upstairs = make_player("Upstairs", 800.0, 800.0);
    upstairs.floor_level = 1;
    for observer in [&from, &to, &upstairs] {
        game.add_player(observer.clone()).await;
    }
    let mut from_rx = game.register_direct_channel(&from.id).await;
    let mut to_rx = game.register_direct_channel(&to.id).await;
    let mut upstairs_rx = game.register_direct_channel(&upstairs.id).await;
    let radius_roll = (1_000_000.0 - 32.0_f64.powi(2)) / (4_000_000.0 - 32.0_f64.powi(2));
    let mut rng = ScriptedRng(vec![roll(0.5), roll(0.0), roll(radius_roll)].into());
    game.use_teleport_scroll_with_rng(&id, 100, &mut rng).await;
    let phases = |messages: Vec<ServerMessage>| {
        messages
            .into_iter()
            .filter_map(|msg| match msg {
                ServerMessage::PlayerTeleportEffect { phase, .. } => Some(phase),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        phases(drain(&mut from_rx)),
        vec![onlinerpg_shared::TeleportPhase::Departing]
    );
    let arrival_messages = drain(&mut to_rx);
    assert_eq!(
        phases(arrival_messages.clone()),
        vec![onlinerpg_shared::TeleportPhase::Arriving]
    );
    let effect_at = arrival_messages
        .iter()
        .position(|msg| matches!(msg, ServerMessage::PlayerTeleportEffect { .. }))
        .unwrap();
    let appeared_at = arrival_messages
        .iter()
        .position(|msg| matches!(msg, ServerMessage::PlayerAppeared { player } if player.id == id))
        .unwrap();
    assert!(effect_at < appeared_at);
    assert!(phases(drain(&mut upstairs_rx)).is_empty());
}

#[tokio::test]
async fn teleport_scroll_lands_on_dry_ground_from_an_upper_floor() {
    let game = make_test_game_state("teleport_scroll_coast");
    let mut player = make_player("Reader", 0.0, 0.0);
    player.position.y = 12.0;
    player.floor_level = 2;
    let id = give_scrolls(&game, player, 1).await;

    game.use_item(&id, 100).await;

    let player = game.players.read().await[&id].clone();
    let (_, depth) = game
        .ground_and_depth_at(player.position.x, player.position.z)
        .await
        .unwrap();
    assert!(depth < 0.0);
    assert_eq!(player.position.y, 5.0);
    assert_eq!(player.floor_level, 0);
    assert_eq!(quantity(&game, &id).await, 0);
}

#[tokio::test]
async fn teleport_scroll_wraps_at_the_world_seam() {
    let game = make_flat_world_game_state("teleport_scroll_seam");
    let mut player = make_player("Reader", WORLD_MAX_X - 0.5, 0.0);
    player.position.y = 5.0;
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;

    game.use_item(&id, 100).await;

    let at = game.players.read().await[&id].position;
    assert!((WORLD_MIN_X..WORLD_MAX_X).contains(&at.x));
    assert!((32.0..=2_000.01).contains(&origin.dist_xz_sq(&at).sqrt()));
    assert_eq!(quantity(&game, &id).await, 0);
}

#[tokio::test]
async fn teleport_scroll_can_reach_land_a_kilometer_away() {
    let game = make_game_state_with(
        "teleport_scroll_distant_land",
        DistantLandTiles,
        SeaOnlyWater,
    );
    let player = make_player("Reader", 0.0, 0.0);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;

    game.use_item(&id, 100).await;

    let at = game.players.read().await[&id].position;
    assert!((990.0..=2_000.01).contains(&origin.dist_xz_sq(&at).sqrt()));
    assert_eq!(at.y, 5.0);
    assert_eq!(quantity(&game, &id).await, 0);
}

#[tokio::test]
async fn teleport_scroll_preserves_item_when_defeated_or_trade_reserved() {
    let game = make_flat_world_game_state("teleport_scroll_guards");
    let mut player = make_player("Reader", 0.0, 0.0);
    player.health = 0;
    let origin = player.position;
    let id = give_scrolls(&game, player, 2).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.use_item(&id, 100).await;
    assert_eq!(quantity(&game, &id).await, 2);
    assert_eq!(game.players.read().await[&id].position, origin);
    assert!(drain(&mut rx).iter().any(|msg| matches!(
        msg, ServerMessage::SystemMessage { message, .. } if message.contains("defeated")
    )));

    game.players.write().await.get_mut(&id).unwrap().health = 10;
    give_scrolls(&game, make_player("Buyer", 1.0, 0.0), 0).await;
    game.request_player_trade(&id, "Buyer").await;
    game.respond_player_trade(&pid("Buyer"), &id, true).await;
    game.set_player_trade_offer(
        &id,
        vec![onlinerpg_shared::messages::PlayerTradeSlot {
            instance_id: 100,
            quantity: 1,
        }],
        0,
    )
    .await;
    assert_eq!(game.trade_reserved_quantity(&id, 100).await, 1);
    game.use_item(&id, 100).await;
    assert_eq!(quantity(&game, &id).await, 2);
    assert_eq!(game.players.read().await[&id].position, origin);
}

#[tokio::test]
async fn teleport_scroll_preserves_item_when_surrounded_by_water() {
    let game = make_game_state_with("teleport_scroll_no_land", OceanTiles, SeaOnlyWater);
    let player = make_player("Reader", -1000.0, 0.0);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    let mut rx = game.register_direct_channel(&id).await;

    game.use_item(&id, 100).await;

    assert_eq!(quantity(&game, &id).await, 1);
    assert_eq!(game.players.read().await[&id].position, origin);
    assert!(drain(&mut rx).iter().any(|msg| matches!(
        msg, ServerMessage::SystemMessage { message, .. } if message.contains("No safe place")
    )));
}

#[tokio::test]
async fn teleport_scroll_rejects_buildings_and_cliffs() {
    let game = make_test_game_state("teleport_scroll_landings");
    assert!(game.teleport_surface_landing(0.0, 0.0).await.is_some());
    assert!(game.teleport_surface_landing(-100.0, 0.0).await.is_none());
    assert!(game.teleport_surface_landing(-31.6, 0.0).await.is_none());
    game.passability_write().insert(
        "house".into(),
        RuntimePassability {
            house_origin_x: 0.0,
            house_origin_z: 0.0,
            min_x: 0.0,
            max_x: 4.0,
            min_z: 0.0,
            max_z: 4.0,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 4,
                depth: 4,
                y_base: 5.0,
                wall_height: 3.0,
                cells: vec![0; 16],
            }],
            stairwells: vec![],
            yields_to_trapped_mover: false,
            allows_projectiles: false,
            is_ground: true,
        },
    );
    assert!(game.teleport_surface_landing(2.0, 2.0).await.is_none());
    assert!(game.teleport_surface_landing(6.0, 6.0).await.is_some());
}

#[tokio::test]
async fn teleport_scroll_leaves_any_dungeon_floor_for_the_surface() {
    let game = make_flat_world_game_state("teleport_scroll_dungeon");
    game.init_passability(&game.terrain_io).await.unwrap();
    let entrance = game.dungeon_defs.get("skeleton_crypt").unwrap();
    game.ensure_dungeon_runtime(&entrance.id).await;

    for depth in [1, 5, 20] {
        let layout =
            game.dungeons.read().await[&entrance.id].layouts[usize::from(depth) - 1].clone();
        let origin = cell_center(&entrance.position(), depth, layout.rooms[0].center());
        let mut player = make_player("Reader", origin.x, origin.z);
        player.position = origin;
        player.floor_level = -(depth as i8);
        let id = give_scrolls(&game, player, 2).await;
        game.handle_player_floor_change(&id, 0, -(depth as i8), &entrance.position(), &origin)
            .await;
        assert!(game.dungeons.read().await[&entrance.id].floors[&depth]
            .players
            .contains_key(&id));

        game.use_teleport_scroll_with_rng(&id, 100, &mut ScriptedRng(VecDeque::new()))
            .await;

        let player = game.players.read().await[&id].clone();
        assert_eq!(player.floor_level, 0);
        assert_eq!(player.position.y, 5.0);
        assert!((32.0..=2_000.01).contains(&origin.dist_xz_sq(&player.position).sqrt()));
        assert!(!entrance.footprint_contains(player.position.x, player.position.z));
        assert!(!game.dungeons.read().await[&entrance.id].floors[&depth]
            .players
            .contains_key(&id));
        assert_eq!(quantity(&game, &id).await, 1);
    }
}

#[tokio::test]
async fn teleport_scroll_dismounts_a_boat_on_arrival() {
    let game = make_flat_world_game_state("teleport_scroll_boat");
    let mut player = make_player("Reader", 0.0, 0.0);
    player.mount = Some(onlinerpg_shared::mount::MountKind::Rowboat);
    let id = give_scrolls(&game, player, 1).await;

    game.use_item(&id, 100).await;

    let player = game.players.read().await[&id].clone();
    assert_eq!(player.mount, None);
    assert_eq!(player.position.y, 5.0);
    assert_eq!(quantity(&game, &id).await, 0);
}

#[tokio::test]
async fn teleport_scroll_includes_every_floor_before_anyone_visits() {
    let game = make_flat_world_game_state("teleport_scroll_all_floors");
    game.init_passability(&game.terrain_io).await.unwrap();
    assert!(game.dungeons.read().await.is_empty());
    let areas = game.teleport_dungeon_areas(&NEAR_DUNGEONS);
    let mut total = 0;
    for entrance in game.dungeon_defs.all() {
        let layouts = generate_dungeon_for(&entrance.id);
        total += layouts.len();
        let (x, z) = dungeon_origin(entrance.x, entrance.z);
        for layout in layouts {
            assert!(areas.iter().any(|area| area.origin.x == x
                && area.origin.z == z
                && area.floor_level == -(layout.depth as i8)));
        }
    }
    assert_eq!(areas.len(), total);
    assert!(game.teleport_dungeon_areas(&pos(0.0)).is_empty());
}

#[tokio::test]
async fn teleport_scroll_from_surface_can_enter_the_deepest_floor_and_dismount() {
    let game = make_flat_world_game_state("teleport_scroll_enter_dungeon");
    game.init_passability(&game.terrain_io).await.unwrap();
    let target = open_dungeon_cell(&game, "skeleton_crypt", 20).await;
    let mut player = make_player("Reader", NEAR_DUNGEONS.x, NEAR_DUNGEONS.z);
    player.mount = Some(onlinerpg_shared::mount::MountKind::Horse);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    let mut rx = game.register_direct_channel(&id).await;
    let mut rng = ScriptedRng(rolls_for_dungeon(&game, origin, target, -20).into());

    game.use_teleport_scroll_with_rng(&id, 100, &mut rng).await;

    let player = game.players.read().await[&id].clone();
    assert_eq!(player.floor_level, -20);
    assert!(player.position.dist_xz_sq(&target) < 0.001);
    assert_eq!(player.position.y, target.y);
    assert_eq!(player.mount, None);
    assert_eq!(quantity(&game, &id).await, 0);
    assert!(game.dungeons.read().await["skeleton_crypt"].floors[&20]
        .players
        .contains_key(&id));
    assert!(drain(&mut rx).iter().any(|msg| matches!(
        msg,
        ServerMessage::PlayerTeleported {
            floor_level: -20,
            ..
        }
    )));
}

#[tokio::test]
async fn teleport_scroll_between_dungeons_at_same_depth_updates_occupancy() {
    let game = make_flat_world_game_state("teleport_scroll_change_dungeon");
    game.init_passability(&game.terrain_io).await.unwrap();
    let origin = open_dungeon_cell(&game, "old_crypt", 5).await;
    let target = open_dungeon_cell(&game, "skeleton_crypt", 5).await;
    let mut player = make_player("Reader", origin.x, origin.z);
    player.position = origin;
    player.floor_level = -5;
    let id = give_scrolls(&game, player, 2).await;
    game.handle_player_floor_change(&id, 0, -5, &origin, &origin)
        .await;
    let mut rng = ScriptedRng(rolls_for_dungeon(&game, origin, target, -5).into());

    game.use_teleport_scroll_with_rng(&id, 100, &mut rng).await;

    let player = game.players.read().await[&id].clone();
    assert_eq!(player.floor_level, -5);
    assert!(player.position.dist_xz_sq(&target) < 0.001);
    assert_eq!(quantity(&game, &id).await, 1);
    let dungeons = game.dungeons.read().await;
    assert!(!dungeons["old_crypt"].floors[&5].players.contains_key(&id));
    assert!(dungeons["skeleton_crypt"].floors[&5]
        .players
        .contains_key(&id));
}

#[tokio::test]
async fn teleport_scroll_retries_the_whole_pool_after_a_rejected_surface() {
    let game = make_game_state_with("teleport_scroll_retry_pool", OceanTiles, SeaOnlyWater);
    game.init_passability(&game.terrain_io).await.unwrap();
    let target = open_dungeon_cell(&game, "old_crypt", 1).await;
    let player = make_player("Reader", NEAR_DUNGEONS.x, NEAR_DUNGEONS.z);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    let mut rolls = vec![1 << 63; 3];
    rolls.extend(rolls_for_dungeon(&game, origin, target, -1));
    let mut rng = ScriptedRng(rolls.into());

    game.use_teleport_scroll_with_rng(&id, 100, &mut rng).await;

    assert_eq!(game.players.read().await[&id].floor_level, -1);
    assert_eq!(quantity(&game, &id).await, 0);
    assert!(rng.0.is_empty());
}

#[tokio::test]
async fn teleport_scroll_rejects_dungeon_walls_props_and_stairs() {
    let game = make_flat_world_game_state("teleport_scroll_dungeon_obstacles");
    game.init_passability(&game.terrain_io).await.unwrap();
    let entrance = game.dungeon_defs.get("old_crypt").unwrap();
    let layout = generate_dungeon_for(&entrance.id).pop().unwrap();
    let wall = layout.carved.iter().position(|carved| !carved).unwrap() as i32;
    let mut blocked = vec![
        (wall % GRID, wall / GRID),
        layout.up_shaft.step_cell(2, 0),
        layout.chest.unwrap(),
    ];
    blocked.extend(
        layout
            .props
            .iter()
            .filter(|prop| prop.kind.is_solid())
            .map(|prop| (prop.x, prop.z)),
    );
    for cell in blocked {
        assert!(game
            .teleport_dungeon_landing(
                cell_center(&entrance.position(), layout.depth, cell),
                -(layout.depth as i8)
            )
            .await
            .is_none());
    }
    let player = make_player("Reader", NEAR_DUNGEONS.x, NEAR_DUNGEONS.z);
    let origin = player.position;
    let id = give_scrolls(&game, player, 1).await;
    let wall_position = cell_center(
        &entrance.position(),
        layout.depth,
        (wall % GRID, wall / GRID),
    );
    let mut rng =
        ScriptedRng(rolls_for_dungeon(&game, origin, wall_position, -(layout.depth as i8)).into());

    game.use_teleport_scroll_with_rng(&id, 100, &mut rng).await;

    assert_eq!(game.players.read().await[&id].floor_level, 0);
    assert_eq!(game.players.read().await[&id].position.y, 5.0);
    assert_eq!(quantity(&game, &id).await, 0);
}
