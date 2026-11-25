#[cfg(debug_assertions)]
use super::auth_context::parse_dev_login;
use super::auth_context::{token_matches, AuthContext};
use super::state::{ConnectionState, Watcher};
use crate::auth::AuthService;
use crate::game::character_attributes::roll_character_attributes;
use crate::game::character_hp::{level_one_max_hp, DEFAULT_CHARACTER_RACE};
use crate::game_state::GameState;
use crate::types::{Character, CharacterAttributes, CharacterClass, ServerMessage};
use onlinerpg_shared::VisibleEquipment;
use onlinerpg_shared::{CharacterAppearance, Gender};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const FALLBACK_DEFAULT_MAX_HP: u32 = 13;

/// Shared tail of both auth paths: replace any account session, load
/// characters, and build the AuthSuccess reply.
pub(super) async fn finish_auth(
    game_state: &GameState,
    auth_service: &AuthService,
    state: &mut ConnectionState,
    account_name: String,
    is_official_npc: bool,
) -> Vec<ServerMessage> {
    let (kick_tx, kick_rx) = mpsc::unbounded_channel();

    // The single gate both login paths pass through: a ban stops the session
    // here, not at character select, because the account carries it. Checked
    // and registered under the one lock `/ban` also takes, so a ban landing
    // in between cannot admit a session it can no longer see to evict.
    let account_session_id = {
        let _sessions = game_state.lock_character_sessions().await;
        if let Some(refusal) = ban_refusal(auth_service, &account_name).await {
            return refusal;
        }
        game_state
            .register_account_session_locked(&account_name, kick_tx, auth_service)
            .await
    };

    state.account_name = Some(account_name.clone());
    state.account_session_id = Some(account_session_id);
    state.account_rx = Some(kick_rx);
    state.is_official_npc = is_official_npc;
    state.pending_character_attributes = None;

    // Read only after the session gate: replacing an account's previous
    // session persists that player first, so returning to character select
    // from the game shows the gear and level the character just had.
    let listed = {
        let auth = auth_service.clone();
        let account = account_name.clone();
        crate::game_state::auth_db(move || auth.list_characters_with_equipment(&account)).await
    };
    let characters = match listed {
        Ok(records) => records
            .into_iter()
            .map(character_listing_to_shared)
            .collect::<Vec<Character>>(),
        Err(err) => {
            warn!(
                "Failed to load character list for account '{}': {}",
                account_name, err
            );
            return vec![ServerMessage::AuthError {
                message: err.client_message().to_string(),
            }];
        }
    };

    info!(
        ip = %state.client_ip, account = account_name, character_count = characters.len(),
        "Account authenticated successfully"
    );
    // Replaces any token an earlier auth on this connection handed out, so
    // the store never holds two for one socket.
    if let Some(previous) = state.cape_upload_token.take() {
        game_state.cape_textures().close_session(&previous).await;
    }
    let cape_upload_token = game_state.cape_textures().open_session(&account_name).await;
    state.cape_upload_token = Some(cape_upload_token.clone());

    vec![ServerMessage::AuthSuccess {
        account_name,
        characters,
        cape_upload_token,
    }]
}

/// Checked under `lock_character_sessions`, which `/ban` also takes, so a
/// ban cannot land between the check and the session it would have evicted.
async fn ban_refusal(auth_service: &AuthService, account_name: &str) -> Option<Vec<ServerMessage>> {
    let ban = {
        let auth = auth_service.clone();
        let account = account_name.to_owned();
        crate::game_state::auth_db(move || auth.active_ban(&account)).await
    };
    let message = match ban {
        Ok(None) => return None,
        Ok(Some(ban)) => {
            info!("Rejected banned account '{}'", account_name);
            ban.message()
        }
        Err(err) => {
            // Fail closed: an unreadable ban table must not become a way in.
            error!("Ban check failed for '{}': {}", account_name, err);
            "Could not verify the account. Try again shortly.".to_string()
        }
    };
    Some(vec![ServerMessage::AuthError { message }])
}

pub(super) async fn authenticate(
    game_state: &GameState,
    auth_service: &AuthService,
    auth_ctx: &AuthContext,
    state: &mut ConnectionState,
    google_id_token: String,
) -> Vec<ServerMessage> {
    match account_from_token(auth_service, auth_ctx, state, &google_id_token).await {
        Ok((account_name, admin_eligible)) => {
            state.admin_eligible = admin_eligible;
            finish_auth(game_state, auth_service, state, account_name, false).await
        }
        Err(responses) => responses,
    }
}

/// Verifies a browser token (or a debug build's dev login): the account and
/// whether its email is on the admin allowlist.
async fn account_from_token(
    auth_service: &AuthService,
    auth_ctx: &AuthContext,
    state: &ConnectionState,
    google_id_token: &str,
) -> Result<(String, bool), Vec<ServerMessage>> {
    #[cfg(debug_assertions)]
    if let Some(account) = parse_dev_login(google_id_token, &auth_ctx.npc_token)
        .filter(|_| state.client_ip.is_loopback())
    {
        let account_name = auth_service.dev_login(account).map_err(|err| {
            vec![ServerMessage::AuthError {
                message: err.client_message().to_string(),
            }]
        })?;
        warn!(account = account_name, "Dev login (debug build)");
        return Ok((account_name, true));
    }

    let Some(verifier) = &auth_ctx.google else {
        warn!(ip = %state.client_ip, "Google login attempted but no --google-client-id is configured");
        return Err(vec![ServerMessage::AuthError {
            message: "Google sign-in is not configured on this server".to_string(),
        }]);
    };

    let claims = match verifier.verify(google_id_token).await {
        Ok(claims) => claims,
        Err(err) => {
            warn!(ip = %state.client_ip, error = %err, "Google token verification failed");
            return Err(vec![ServerMessage::AuthError {
                message: "Google sign-in verification failed".to_string(),
            }]);
        }
    };

    let account_name = match auth_service.login_google(&claims.sub) {
        Ok(name) => name,
        Err(err) => {
            warn!("Google login failed for sub '{}': {}", claims.sub, err);
            return Err(vec![ServerMessage::AuthError {
                message: err.client_message().to_string(),
            }]);
        }
    };
    info!("Google sub '{}' -> account '{}'", claims.sub, account_name);

    Ok((account_name, auth_ctx.is_admin(&claims)))
}

/// Joins the account's watchers without touching its session, so the
/// playing connection stays. Shows the character in the world now, if any.
pub(super) async fn watch_account(
    game_state: &GameState,
    auth_service: &AuthService,
    auth_ctx: &AuthContext,
    state: &mut ConnectionState,
    google_id_token: String,
) -> Vec<ServerMessage> {
    let account_name =
        match account_from_token(auth_service, auth_ctx, state, &google_id_token).await {
            Ok((name, _)) => name,
            Err(responses) => return responses,
        };
    let watch = game_state.account_watch(&account_name);
    let rx = {
        let _sessions = game_state.lock_character_sessions().await;
        if let Some(refusal) = ban_refusal(auth_service, &account_name).await {
            drop(_sessions);
            game_state.release_account_watch(&account_name, watch);
            return refusal;
        }
        watch.attach()
    };
    let Some(rx) = rx else {
        game_state.release_account_watch(&account_name, watch);
        return vec![ServerMessage::AuthError {
            message: "This account already has the most watchers allowed".to_string(),
        }];
    };
    info!(ip = %state.client_ip, account = account_name, "Watcher attached");
    state.account_name = Some(account_name.clone());
    let player = game_state.watched_player(&watch).await;
    state.watcher = Some(Watcher { watch, rx });
    let Some(player) = player else {
        return vec![ServerMessage::WatchWaiting];
    };
    let id = player.id;
    let mut responses: Vec<ServerMessage> = watch_target(auth_service, &account_name, &player.name)
        .await
        .into_iter()
        .collect();
    responses.push(ServerMessage::JoinSuccess {
        player,
        is_admin: false,
    });
    responses.extend(game_state.owner_state_messages(&id).await);
    // The snapshot reaches the watcher through the player's own stream.
    game_state.reset_world_view(&id).await;
    responses
}

/// The select-screen card of the character being watched, for its HUD.
pub(super) async fn watch_target(
    auth_service: &AuthService,
    account_name: &str,
    character_name: &str,
) -> Option<ServerMessage> {
    let listed = {
        let auth = auth_service.clone();
        let account = account_name.to_owned();
        crate::game_state::auth_db(move || auth.list_characters_with_equipment(&account)).await
    };
    let character = listed
        .inspect_err(|err| {
            warn!("Failed to list characters for watcher of '{account_name}': {err}")
        })
        .ok()?
        .into_iter()
        .map(character_listing_to_shared)
        .find(|character| character.name == character_name)?;
    Some(ServerMessage::WatchTarget { character })
}

pub(super) async fn authenticate_npc(
    game_state: &GameState,
    auth_service: &AuthService,
    auth_ctx: &AuthContext,
    state: &mut ConnectionState,
    account_name: String,
    npc_token: String,
) -> Vec<ServerMessage> {
    if !token_matches(&npc_token, &auth_ctx.npc_token) {
        warn!(ip = %state.client_ip, account = account_name, "NPC auth rejected: bad token");
        return vec![ServerMessage::AuthError {
            message: "Invalid NPC token".to_string(),
        }];
    }

    let account_name = match auth_service.login_npc(&account_name) {
        Ok(name) => name,
        Err(err) => {
            warn!(ip = %state.client_ip, account = account_name, error = %err, "NPC login failed");
            return vec![ServerMessage::AuthError {
                message: err.client_message().to_string(),
            }];
        }
    };

    finish_auth(game_state, auth_service, state, account_name, true).await
}

pub(super) async fn create_character(
    game_state: &GameState,
    auth_service: &AuthService,
    state: &mut ConnectionState,
    character_name: String,
    character_class: CharacterClass,
    gender: Gender,
    appearance: CharacterAppearance,
) -> Vec<ServerMessage> {
    if let Err(responses) = state.require_not_in_game("CreateCharacter") {
        return responses;
    }
    let authed_account_name = match state.require_auth("CreateCharacter") {
        Ok(name) => name,
        Err(responses) => return responses,
    };
    if let Err(responses) = state.require_selectable_class(&character_class) {
        return responses;
    }

    let Some(rolled_attributes) = state.pending_character_attributes.clone() else {
        warn!(
            "Character creation requested without rolled stats for account '{}'",
            authed_account_name
        );
        return vec![ServerMessage::CharacterError {
            message: "Roll attributes first".to_string(),
        }];
    };

    let max_hp = default_character_max_hp(&rolled_attributes, &character_class);
    let _sessions = game_state.lock_character_sessions().await;
    let _persistence = game_state.lock_player_persistence().await;
    match auth_service.create_character_with_appearance(
        &authed_account_name,
        &character_name,
        &rolled_attributes,
        max_hp,
        character_class.clone(),
        gender,
        appearance,
    ) {
        Ok(character) => {
            state.pending_character_attributes = None;
            info!(
                "Character '{}' created for account '{}'",
                character.name, authed_account_name
            );
            let worn = visible_equipment_of(auth_service, character.id).await;
            vec![ServerMessage::CharacterCreated {
                character: character_listing_to_shared(crate::auth::CharacterListing::fresh(
                    character, worn,
                )),
            }]
        }
        Err(err) => {
            warn!(
                "Character create failed for account '{}': {}",
                authed_account_name, err
            );
            vec![ServerMessage::CharacterError {
                message: err.client_message().to_string(),
            }]
        }
    }
}

pub(super) async fn request_character_deletion(
    game_state: &GameState,
    auth_service: &AuthService,
    state: &mut ConnectionState,
    character_id: i64,
    cancel: bool,
) -> Vec<ServerMessage> {
    let action = if cancel {
        "CancelCharacterDeletion"
    } else {
        "DeleteCharacter"
    };
    let checked = tracing::info_span!("character_deletion", character_id).in_scope(|| {
        state.require_not_in_game(action)?;
        state.require_auth(action)
    });
    let account = match checked {
        Ok(account) => account,
        Err(responses) => return responses,
    };
    let result = game_state
        .change_character_deletion(auth_service, &account, character_id, cancel)
        .await;
    vec![match result {
        Ok(Some(deletion_due_at)) => ServerMessage::CharacterDeletionScheduled {
            character_id,
            deletion_due_at,
        },
        Ok(None) => ServerMessage::CharacterDeletionCancelled { character_id },
        Err(error) => {
            warn!(ip = %state.client_ip, %account, character_id, action, %error,
                "Character deletion request failed");
            ServerMessage::CharacterError {
                message: error.client_message().to_string(),
            }
        }
    }]
}

pub(super) async fn rename_character(
    game_state: &GameState,
    auth_service: &AuthService,
    state: &mut ConnectionState,
    character_id: i64,
    new_name: String,
) -> Vec<ServerMessage> {
    if let Err(responses) = state.require_not_in_game("RenameCharacter") {
        return responses;
    }
    let authed_account_name = match state.require_auth("RenameCharacter") {
        Ok(name) => name,
        Err(responses) => return responses,
    };
    if game_state.character_in_game(character_id).await {
        return vec![ServerMessage::CharacterError {
            message: "Cannot rename a character while it is in game".to_string(),
        }];
    }
    match auth_service.rename_character(&authed_account_name, character_id, &new_name) {
        Ok(name) => {
            info!(
                "Character id={} renamed to '{}' for account '{}'",
                character_id, name, authed_account_name
            );
            vec![ServerMessage::CharacterRenamed { character_id, name }]
        }
        Err(err) => {
            warn!(
                "Character rename failed for account '{}': {}",
                authed_account_name, err
            );
            vec![ServerMessage::CharacterError {
                message: err.client_message().to_string(),
            }]
        }
    }
}

pub(super) fn roll_character_stats(
    state: &mut ConnectionState,
    character_class: CharacterClass,
    gender: Gender,
) -> Vec<ServerMessage> {
    if let Err(responses) = state.require_not_in_game("RollCharacterStats") {
        return responses;
    }
    if let Err(responses) = state.require_auth("RollCharacterStats") {
        return responses;
    }
    if let Err(responses) = state.require_selectable_class(&character_class) {
        return responses;
    }

    let attributes = roll_character_attributes(&character_class, gender);
    let max_hp = default_character_max_hp(&attributes, &character_class);
    state.pending_character_attributes = Some(attributes.clone());
    vec![ServerMessage::CharacterStatsRolled { attributes, max_hp }]
}

fn character_listing_to_shared(listing: crate::auth::CharacterListing) -> Character {
    let crate::auth::CharacterListing {
        record,
        worn,
        titles,
        active_title,
        family,
    } = listing;
    Character {
        id: record.id,
        deletion_due_at: record.deletion_due_at,
        name: record.name,
        created_at: record.created_at,
        level: record.level,
        xp: record.xp,
        max_hp: record.max_hp,
        attributes: record.attributes,
        class: record.class,
        gender: record.gender,
        appearance: record.appearance,
        equipment: worn,
        titles,
        active_title,
        family,
    }
}

/// Preview gear for one character. A failed lookup only costs the select
/// screen its weapons, so it logs and yields nothing worn.
async fn visible_equipment_of(auth_service: &AuthService, character_id: i64) -> VisibleEquipment {
    let auth = auth_service.clone();
    crate::game_state::auth_db(move || auth.load_character_equipment(character_id))
        .await
        .unwrap_or_else(|err| {
            warn!("Failed to load equipped items for character select: {err}");
            VisibleEquipment::default()
        })
}

fn default_character_max_hp(
    attributes: &CharacterAttributes,
    character_class: &CharacterClass,
) -> u32 {
    match level_one_max_hp(DEFAULT_CHARACTER_RACE, character_class, attributes.con) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "Failed to resolve level 1 max HP for race='{}', class='{}', con='{}': {}",
                DEFAULT_CHARACTER_RACE,
                character_class.as_str(),
                attributes.con,
                err
            );
            FALLBACK_DEFAULT_MAX_HP
        }
    }
}
