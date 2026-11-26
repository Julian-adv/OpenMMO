use super::state::ConnectionState;
use crate::auth::AuthService;
use crate::game_state::{restored_floor_level, GameState, GOD_ON_MESSAGE};
use crate::types::{new_player, ClientMessage, PlayerId, Position, ServerMessage};
use std::sync::Arc;
use tracing::{info, warn};

pub(super) const LOGINS_PAUSED_MESSAGE: &str = "서버 업데이트를 준비 중입니다. 업데이트가 끝난 뒤 다시 접속해 주세요.\nThe server is preparing an update. Please reconnect after it finishes.";

/// What the web client sends on its own under the modal loading dialog;
/// anything else means the player acts while untouchable.
pub(super) fn ends_entry_grace(msg: &ClientMessage) -> bool {
    !matches!(
        msg,
        ClientMessage::WorldReady
            | ClientMessage::Heartbeat
            | ClientMessage::ResyncWorld
            | ClientMessage::EnvReport(_)
            | ClientMessage::RequestFriendsOnline
            | ClientMessage::PeekNoticeBoard { .. }
    )
}

pub(super) async fn end_entry_grace(
    game_state: &GameState,
    state: &mut ConnectionState,
    id: PlayerId,
    msg: &ClientMessage,
) {
    state.awaiting_world_ready = false;
    if game_state.mark_world_ready(&id).await {
        let action = format!("{msg:?}");
        let action = action.split([' ', '{', '(']).next().unwrap_or_default();
        warn!(
            ip = %state.client_ip, account = state.account_name.as_deref(),
            character = state.character_name.as_deref(), player_id = id.get(),
            kind = state.client_kind.unwrap_or_default().as_str(), version = %state.reported_client_version,
            action, "Acted before WorldReady; entry grace ended"
        );
    }
}

async fn bind_entered_player(
    game_state: &GameState,
    state: &mut ConnectionState,
    auth_service: &Arc<AuthService>,
    id: PlayerId,
    account: &str,
    name: &str,
) {
    state.player_id = Some(id);
    let watch = game_state.account_watch(account);
    watch.entered(id);
    game_state.watch_player(id, &watch);
    if watch.is_watched() {
        if let Some(target) = super::account::watch_target(auth_service, account, name).await {
            watch.tee_message(&target);
        }
    }
    if let Some(previous) = state.owner_watch.replace(watch) {
        game_state.release_account_watch(account, previous);
    }
    state.last_furniture_tip.clear();
    state.character_name = Some(name.to_owned());
    if !state.is_official_npc {
        state.next_ip_notice = Some(std::time::Instant::now());
    }
    game_state
        .begin_account_activity(id, account, &state.country, auth_service)
        .await;
}

async fn entry_notices(
    game_state: &GameState,
    state: &ConnectionState,
    auth_service: &Arc<AuthService>,
    character_id: i64,
) -> Vec<ServerMessage> {
    let mut notices = Vec::new();
    if let Some(notice) = game_state.server_notice().await {
        notices.push(ServerMessage::ServerNotice {
            message: Some(notice),
        });
    }
    if state.is_official_npc {
        notices.push(game_state.pricing_notice(auth_service).await);
    } else if let Some(mail) = game_state
        .unread_mail_notice(character_id, auth_service)
        .await
    {
        notices.push(mail);
    }
    notices
}

pub(super) async fn enter_game(
    game_state: &GameState,
    auth_service: &Arc<AuthService>,
    state: &mut ConnectionState,
    character_id: i64,
) -> Vec<ServerMessage> {
    if state.player_id.is_some() {
        warn!(
            ip = %state.client_ip, account = state.account_name.as_deref(),
            character = state.character_name.as_deref(), player_id = state.player_id.map(PlayerId::get),
            requested_character_id = character_id,
            "Client already entered game, ignoring EnterGame request"
        );
        return vec![];
    }

    let authed_account_name = match state.require_auth("EnterGame") {
        Ok(name) => name,
        Err(responses) => return responses,
    };
    let character_sessions = game_state.lock_character_sessions().await;
    let player_persistence = game_state.lock_player_persistence().await;
    let Some(account_session_id) = state.account_session_id else {
        return vec![];
    };
    if !game_state
        .is_current_account_session(&authed_account_name, account_session_id)
        .await
    {
        return vec![];
    }

    let selected_character =
        match auth_service.get_character_for_account(&authed_account_name, character_id) {
            Ok(character) => character,
            Err(err) => {
                warn!(
                    "EnterGame failed for account '{}': {}",
                    authed_account_name, err
                );
                return vec![ServerMessage::CharacterError {
                    message: err.client_message().to_string(),
                }];
            }
        };

    if selected_character.deletion_due_at.is_some() {
        return vec![ServerMessage::CharacterError {
            message: "This character is awaiting deletion. Cancel deletion to enter the game."
                .to_string(),
        }];
    }

    // A name banned after the character was made stops it here, not
    // at login: the client answers with RenameCharacter and retries.
    if auth_service.is_name_banned_for(&authed_account_name, &selected_character.name) {
        info!(
            "Refusing entry for banned character name '{}'",
            selected_character.name
        );
        return vec![ServerMessage::CharacterRenameRequired { character_id }];
    }

    state.is_admin = state.admin_eligible && selected_character.admin_role > 0;
    if state.is_admin {
        info!(
            "Account '{}' entering as admin character '{}' (role {})",
            authed_account_name, selected_character.name, selected_character.admin_role
        );
    }

    if let Some(body) = game_state
        .lingering_body_of_account(&authed_account_name)
        .await
    {
        if body.character_id != character_id {
            return vec![ServerMessage::CharacterError {
                message: format!(
                    "Another of your characters is still in a fight in the world. Try again within {}s.",
                    body.remaining_secs
                ),
            }];
        }
        let id = body.id;
        let Some(player) = game_state
            .resume_lingering_body(id, &authed_account_name, account_session_id)
            .await
        else {
            return vec![];
        };
        let mut responses = vec![ServerMessage::JoinSuccess {
            player,
            is_admin: state.is_admin,
        }];
        responses.extend(game_state.owner_state_messages(&id).await);
        responses.extend(entry_notices(game_state, state, auth_service, character_id).await);
        drop(player_persistence);
        state.direct_rx = Some(game_state.register_connection_channel(&id).await);
        game_state.send_resumed_state(&id).await;
        bind_entered_player(
            game_state,
            state,
            auth_service,
            id,
            &authed_account_name,
            &selected_character.name,
        )
        .await;
        drop(character_sessions);
        info!(
            ip = %state.client_ip, account = authed_account_name, character = selected_character.name,
            player_id = %id, character_id,
            "Account resumed its lingering character"
        );
        return responses;
    }

    if game_state.logins_paused() && !state.is_admin && !state.is_official_npc {
        info!(
            account = authed_account_name,
            character = selected_character.name,
            "Refusing entry while logins are paused for an update"
        );
        return vec![ServerMessage::CharacterError {
            message: LOGINS_PAUSED_MESSAGE.to_string(),
        }];
    }

    // Skills and dungeon history load before any registration: a failed
    // read must refuse the session, or an empty fallback would overwrite
    // trained skills on save and re-grant already-opened chest rewards.
    let (skill_rows, chest_opens, discovered_dungeons, titles, family) = {
        let auth = Arc::clone(auth_service);
        let loaded = crate::game_state::auth_db(move || {
            Ok((
                auth.load_skills(character_id)?,
                auth.load_dungeon_history(character_id)?,
                auth.load_titles(character_id)?,
                auth.family_of(character_id)?,
            ))
        })
        .await;
        match loaded {
            Ok((rows, (opens, ids), titles, family)) => (rows, opens, ids, titles, family),
            Err(err) => {
                warn!(
                    "Failed to load required state for character {}: {} — refusing session",
                    character_id, err
                );
                return vec![ServerMessage::CharacterError {
                    message: err.client_message().to_string(),
                }];
            }
        }
    };

    let npc_def = state
        .is_official_npc
        .then(|| crate::npc_defs::npc_defs().get_by_npc_name(&selected_character.name))
        .flatten();
    let max_hp = selected_character
        .max_hp
        .max(npc_def.map_or(0, |def| def.min_max_hp));
    let character_xp = selected_character
        .xp
        .max(onlinerpg_shared::xp::xp_for_level(
            npc_def.map_or(1, |def| def.min_level),
        ));
    let mut attributes = selected_character.attributes.clone();
    attributes.guard = attributes.guard.max(npc_def.map_or(0, |def| def.min_guard));

    let mut player = new_player(
        selected_character.name.clone(),
        selected_character
            .level
            .max(npc_def.map_or(1, |def| def.min_level)),
        max_hp,
        selected_character.class.clone(),
        selected_character.gender,
        Position {
            x: selected_character.last_x,
            y: selected_character.last_y,
            z: selected_character.last_z,
        },
        selected_character.last_rotation,
        state.is_official_npc,
        state.client_kind.unwrap_or_default(),
    );
    player.appearance = npc_def.map_or(selected_character.appearance, |def| {
        def.appearance(selected_character.appearance)
    });
    player.title = titles.1.clone();
    let family_id = family.as_ref().map(|(id, _)| *id);
    player.family = family.map(|(_, name)| name);
    player.rank = npc_def.and_then(|def| def.rank.clone());
    player.invulnerable = state.is_admin;

    // Health is restored once gear sets max HP, below.
    let saved_floor = selected_character.floor_level;
    player.floor_level = restored_floor_level(saved_floor);
    if player.floor_level != saved_floor {
        let spawn = &crate::world_config::world_config().spawn_position;
        player.position = spawn.position();
        player.rotation = spawn.rotation;
        warn!(
            "Reset out-of-range stored floor {} to the world spawn for character '{}'",
            saved_floor, selected_character.name
        );
    }
    if player.is_official_npc {
        if let Some(entry) = game_state.npc_login_schedule(&player.name) {
            player.position = Position {
                x: entry.pos[0],
                y: entry.pos[1],
                z: entry.pos[2],
            };
            player.rotation = entry.rotation.to_radians();
            player.floor_level = entry.floor_level as i8;
        }
    }
    let mut dungeon_reset_on_login = false;
    if player.floor_level < 0 {
        let ok = game_state
            .rehydrate_dungeon_player(&mut player, selected_character.dungeon_epoch)
            .await;
        dungeon_reset_on_login = ok && player.floor_level == 0;
        if !ok {
            let spawn = &crate::world_config::world_config().spawn_position;
            player.position = spawn.position();
            player.rotation = spawn.rotation;
            player.floor_level = 0;
        }
    }
    if player.floor_level > 0
        && !game_state.has_surface_floor(player.floor_level as u8, &player.position)
    {
        let spawn = &crate::world_config::world_config().spawn_position;
        player.position = spawn.position();
        player.rotation = spawn.rotation;
        player.floor_level = 0;
    }
    // The stored Y was the client's word at logout; re-ground it.
    if player.floor_level >= 0 {
        player.position.y = game_state
            .surface_ground_y(
                player.floor_level as u8,
                &player.position,
                player.position.y,
                player.mount,
            )
            .await;
    }
    let id = player.id;

    state.direct_rx = Some(game_state.register_connection_channel(&id).await);
    game_state
        .register_player_character(
            &id,
            character_id,
            character_xp,
            attributes,
            selected_character.gold,
            (!state.is_official_npc).then_some(selected_character.satiation),
        )
        .await;
    if !game_state
        .attach_player_to_account_session(&authed_account_name, account_session_id, id)
        .await
    {
        warn!(
            "Account session changed during game admission for '{}'",
            authed_account_name
        );
        return vec![];
    }
    let (auth, account) = (Arc::clone(auth_service), authed_account_name.clone());
    let human = !state.is_official_npc;
    match crate::game_state::auth_db(move || {
        Ok((
            auth.load_blocked_names(character_id)?,
            auth.load_friends(character_id)?,
            human.then(|| auth.load_npc_outfits(&account)),
            human.then(|| auth.bookmarks(character_id)).transpose()?,
        ))
    })
    .await
    {
        Ok((blocked, friends, outfits, bookmarks)) => {
            if let Some(bookmarks) = bookmarks {
                game_state
                    .send_direct_message(&id, ServerMessage::Bookmarks { bookmarks })
                    .await;
            }
            game_state.set_player_blocks(&id, blocked).await;
            game_state.set_player_friends(&id, friends).await;
            match outfits {
                Some(Ok(outfits)) => {
                    game_state
                        .set_outfit_ledger(&id, authed_account_name.clone(), character_id, outfits)
                        .await
                }
                Some(Err(err)) => warn!(
                    "Failed to load NPC outfits for character {}: {}",
                    character_id, err
                ),
                None => {}
            }
        }
        Err(err) => warn!(
            "Failed to load block/friend lists for character {}: {}",
            character_id, err
        ),
    }

    game_state.set_chest_opens(character_id, chest_opens).await;
    game_state.set_player_titles(&id, titles.0).await;
    game_state
        .greet_watch_recruit(&id, &player.name, auth_service)
        .await;
    game_state.send_player_titles(&id).await;
    game_state
        .player_entered_family(&id, &player.name, family_id, auth_service)
        .await;
    game_state
        .set_dungeon_discoveries(&id, discovered_dungeons)
        .await;

    if !game_state
        .reconnect_stall_owner(&player, character_id)
        .await
    {
        game_state
            .load_player_inventory(&id, character_id, auth_service)
            .await;
    }
    if state.is_official_npc {
        game_state.seed_npc_loadout(&id, &player.name).await;
        game_state.seed_npc_keepsakes(&id, &player.name).await;
    }

    // Missing rows = never trained.
    let skills = crate::game_state::skills_from_rows(&skill_rows);
    game_state.register_player_skills(&id, skills).await;

    // Worn gear, before add_player builds the late-join GameState snapshot.
    if let Some(inv) = game_state.get_player_inventory(&id).await {
        player.gear = game_state.shown_gear(&id, &inv).await;
        player.max_health_per_level = game_state.equipped_max_health_per_level(&inv);
        player.invisible = game_state.wears_invisibility(&inv);
        game_state.sync_health_viewer(&id, &inv);
    }
    player.set_base_max_health(max_hp);
    player.health = selected_character
        .health
        .map_or(player.max_health, |saved| saved.min(player.max_health));

    game_state
        .register_mana(
            &player,
            selected_character.attributes.wis,
            selected_character.mana,
        )
        .await;
    let mut responses = vec![ServerMessage::JoinSuccess {
        player: player.clone(),
        is_admin: state.is_admin,
    }];
    if player.invisible {
        responses.push(ServerMessage::InvisibilityChanged { invisible: true });
    }
    if player.invulnerable {
        responses.push(ServerMessage::SystemMessage {
            localization: None,
            message: GOD_ON_MESSAGE.to_string(),
        });
    }
    responses.extend(game_state.owner_state_messages(&id).await);
    responses.extend(entry_notices(game_state, state, auth_service, character_id).await);

    let rejoin_floor = player.floor_level;
    let rejoin_pos = player.position;
    info!(target: "player_session",
        ip = %state.client_ip, account = authed_account_name, character = selected_character.name,
        player_id = %id, character_id, account_session_id,
        client_kind = state.client_kind.map(|kind| kind.as_str()),
        reported_version = state.reported_client_version,
        x = player.position.x, y = player.position.y, z = player.position.z,
        rotation = player.rotation, floor = player.floor_level,
        "Movement session joined");
    drop(player_persistence);
    game_state.add_player(player).await;
    // Stamps last_seen_at at the next flush.
    game_state.mark_dirty(&id).await;

    if dungeon_reset_on_login {
        responses.push(ServerMessage::SystemMessage {
            localization: Some(onlinerpg_shared::messages::LocalizedMessage::new(
                "server.dungeonReset",
            )),
            message: "The dungeon has reset. You return to its entrance.".to_string(),
        });
    }
    if rejoin_floor < 0 {
        // Restore floor occupancy and populate empty monster slots.
        game_state
            .handle_player_floor_change(&id, 0, rejoin_floor, &rejoin_pos, &rejoin_pos)
            .await;
    }

    bind_entered_player(
        game_state,
        state,
        auth_service,
        id,
        &authed_account_name,
        &selected_character.name,
    )
    .await;
    state.awaiting_world_ready = true;
    drop(character_sessions);

    info!(
        ip = %state.client_ip, account = authed_account_name, character = selected_character.name,
        player_id = %id, character_id,
        "Account entered game"
    );
    responses
}
