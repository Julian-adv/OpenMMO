//! WebSocket protocol envelopes between client and server. `ClientMessage`
//! is everything a client can ask for (move, attack, place house, equip
//! item …); `ServerMessage` is everything the server pushes back (world
//! snapshots, combat results, inventory deltas, kicks). Both serialize
//! via MessagePack — convenience helpers at the bottom of the file
//! centralise the `rmp_serde::to_vec` / `from_slice` calls so callers
//! don't have to know the wire format.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::character::{Character, CharacterAttributes, CharacterClass, Gender};
use crate::entity::{Monster, MonsterState, Player};
use crate::world::{GameDateTime, NoSpawnZone, Position};
use crate::{housing, inventory};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Authenticate {
        account_name: String,
        password_hash: String,
        create_account: bool,
        #[serde(default)]
        is_npc: bool,
    },
    CreateCharacter {
        character_name: String,
        character_class: CharacterClass,
        gender: Gender,
    },
    RollCharacterStats {
        character_class: CharacterClass,
        gender: Gender,
    },
    DeleteCharacter {
        character_id: i64,
    },
    EnterGame {
        character_id: i64,
    },
    PlayerMove {
        position: Position,
        rotation: f32,
        #[serde(default)]
        floor_level: i8,
    },
    ChatMessage {
        message: String,
    },
    RequestSpawnMonster {
        monster_type: String,
        position: Position,
        rotation: f32,
    },
    MonsterMove {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
    },
    PlayerAttack {
        monster_id: String,
    },
    MonsterAttack {
        monster_id: String,
        target_player_id: String,
    },
    RequestRespawn,
    DebugTeleport {
        position: Position,
    },
    DebugDropItem {
        item_def_id: String,
    },
    TorchToggle {
        enabled: bool,
    },
    InteractObject {
        object_type: String,
        object_id: u32,
    },
    StopInteraction,
    Heartbeat,
    PlaceHouse {
        house: housing::HouseData,
    },
    ModifyRoom {
        house_id: String,
        room_index: u32,
        room: housing::RoomData,
    },
    RemoveHouse {
        house_id: String,
    },
    ToggleDoor {
        house_id: String,
        room_index: u32,
        wall_dir: housing::WallDirection,
        segment_index: u32,
    },
    EquipItem {
        instance_id: u64,
    },
    UnequipItem {
        slot: inventory::EquipSlot,
    },
    DropItem {
        instance_id: u64,
    },
    PickupItem {
        instance_id: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    AuthSuccess {
        account_name: String,
        characters: Vec<Character>,
    },
    JoinSuccess {
        player: Player,
    },
    AuthError {
        message: String,
    },
    CharacterCreated {
        character: Character,
    },
    CharacterStatsRolled {
        attributes: CharacterAttributes,
        max_hp: u32,
    },
    CharacterDeleted {
        character_id: i64,
    },
    CharacterError {
        message: String,
    },
    PlayerJoined {
        player: Player,
    },
    PlayerLeft {
        player_id: String,
    },
    PlayerAppeared {
        player: Player,
    },
    PlayerDisappeared {
        player_id: String,
    },
    PlayerMoved {
        player_id: String,
        position: Position,
        rotation: f32,
        #[serde(default)]
        floor_level: i8,
    },
    PlayerTeleported {
        player_id: String,
        position: Position,
        rotation: f32,
    },
    ChatMessage {
        player_id: String,
        message: String,
    },
    GameState {
        players: HashMap<String, Player>,
        monsters: HashMap<String, Monster>,
        #[serde(default)]
        ground_items: Vec<inventory::GroundItem>,
    },
    GameTimeSync {
        datetime: GameDateTime,
        is_night: bool,
    },
    MonsterSpawned {
        monster: Monster,
    },
    /// Server assigns a monster to this client for AI control.
    MonsterAssigned {
        monster: Monster,
    },
    /// Server asks this client to spawn a monster somewhere near the player.
    /// The client picks a valid position (grassland, not water, away from towns)
    /// around its own location and replies with RequestSpawnMonster.
    SpawnMonsterRequest {
        monster_type: String,
    },
    MonsterMoved {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
        owner_id: Option<String>,
    },
    MonsterRemoved {
        monster_id: String,
    },
    MonsterDead {
        monster_id: String,
        dropped_weapon_item_def_id: Option<String>,
    },
    PlayerAttacked {
        player_id: String,
        monster_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
    },
    MonsterAttackedPlayer {
        monster_id: String,
        player_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
        current_health: u32,
    },
    PlayerDead {
        player_id: String,
    },
    PlayerRespawned {
        player: Player,
    },
    PlayerHealthUpdate {
        player_id: String,
        health: u32,
        max_health: u32,
    },
    XpGained {
        player_id: String,
        xp_amount: u32,
        xp_lost: u64,
        total_xp: u64,
        new_level: u32,
        leveled_up: bool,
        max_hp: u32,
        current_hp: u32,
    },
    Kicked {
        player_id: String,
        reason: String,
    },
    PlayerTorchToggled {
        player_id: String,
        enabled: bool,
    },
    PlayerInteractionChanged {
        player_id: String,
        object_type: Option<String>,
    },
    InteractionRejected {
        reason: String,
    },
    HouseSpawned {
        house: housing::HouseData,
    },
    HouseUpdated {
        house: housing::HouseData,
    },
    TreeTilesInvalidated {
        tiles: Vec<(i32, i32)>,
    },
    HouseRemoved {
        house_id: String,
    },
    HousesInArea {
        houses: Vec<housing::HouseData>,
    },
    DoorToggled {
        house_id: String,
        room_index: u32,
        wall_dir: housing::WallDirection,
        segment_index: u32,
        is_open: bool,
    },
    /// Sent once on join: all no-spawn zones so the client can validate spawn positions.
    NoSpawnZones {
        zones: Vec<NoSpawnZone>,
    },
    /// Sent once on join: full inventory state.
    InventoryState {
        inventory: inventory::PlayerInventory,
    },
    /// Sent after any inventory mutation.
    InventoryUpdated {
        inventory: inventory::PlayerInventory,
    },
    /// A new item was created on the ground.
    GroundItemSpawned {
        item: inventory::GroundItem,
        /// Set when this item was dropped by a dying monster, so the client can
        /// hold the drop until that monster's death-impact animation plays out.
        /// `None` for player/debug drops, which spawn immediately.
        #[serde(default)]
        source_monster_id: Option<String>,
    },
    /// An existing ground item became visible to the client.
    GroundItemAppeared {
        item: inventory::GroundItem,
    },
    /// A ground item was picked up or despawned.
    GroundItemRemoved {
        instance_id: u64,
    },
    /// Inventory action failed.
    InventoryError {
        message: String,
    },
}

pub type PlayerId = String;

// Serialization helpers (used by both server and wasm). `#[inline]` so the
// rmp_serde call lands directly at the call site even though the protocol
// types live in their own crate from the consumers' perspective.
#[inline]
pub fn serialize_client_msg(msg: &ClientMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

#[inline]
pub fn deserialize_client_msg(bytes: &[u8]) -> Result<ClientMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

#[inline]
pub fn serialize_server_msg(msg: &ServerMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

#[inline]
pub fn deserialize_server_msg(bytes: &[u8]) -> Result<ServerMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}
