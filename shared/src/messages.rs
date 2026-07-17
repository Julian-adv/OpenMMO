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

/// Which side of a merchant trade a haggled deal applies to.
/// `Buy` = the player buys from the merchant, `Sell` = the player sells to
/// the merchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DealKind {
    Buy,
    Sell,
}

/// A haggled price modifier on one item, as included in `ShopState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveDeal {
    pub item_def_id: String,
    pub kind: DealKind,
    /// Percentage points added to the normal price (negative = discount on
    /// buys, positive = bonus on sells).
    pub modifier_pct: i32,
    pub expires_in_secs: u32,
}

/// One purchasable item in a non-merchant trader's real inventory, as
/// included in `ShopState`. Merchants use `catalog` (unlimited stock)
/// instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockEntry {
    pub item_def_id: String,
    pub quantity: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Browser login: a Google ID token, verified server-side. The account is
    /// looked up (or created) by the token's `sub` claim.
    Authenticate {
        google_id_token: String,
    },
    /// Headless bot login, gated by the server's shared NPC token. The
    /// account is auto-created on first use.
    AuthenticateNpc {
        account_name: String,
        npc_token: String,
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
    /// Open the treasure chest on a dungeon's final floor. The server
    /// validates proximity, boss state and the per-player cooldown.
    OpenDungeonChest {
        entrance_id: String,
    },
    /// Break a destructible dungeon prop (barrel/crate). The server validates
    /// floor, proximity and prop kind, records the break for the dungeon
    /// instance, opens the cell for movement and broadcasts it nearby.
    BreakDungeonProp {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
    },
    /// Open an interactive dungeon chest prop (plays its lid animation). The
    /// server validates floor, proximity and prop kind, records the open for
    /// the dungeon instance and broadcasts it nearby. The chest stays solid
    /// (no passability change) — only the lid animates.
    OpenDungeonProp {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
    },
    /// Toggle a dungeon door's open state. `depth` 0 is the surface entrance
    /// door; ≥1 is an interior room door. `door_id` is the client's opaque
    /// door key (derived from the door's geometry). The server flips the
    /// stored state for (entrance, depth, door_id) and broadcasts it nearby.
    ToggleDungeonDoor {
        entrance_id: String,
        depth: u8,
        door_id: u32,
    },
    /// Ask for the open/closed state of every door in a dungeon (entrance +
    /// interior, all depths). The server replies with DungeonDoorsState. Sent
    /// when the client registers/enters the dungeon so doors others left open
    /// render correctly.
    RequestDungeonDoors {
        entrance_id: String,
    },
    DebugTeleport {
        position: Position,
    },
    DebugDropItem {
        item_def_id: String,
    },
    DebugSetTime {
        hour: u8,
        minute: u8,
    },
    DebugResetDungeonProps {
        entrance_id: String,
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
    /// Consume a usable item from the bag (e.g. drink a healing potion).
    UseItem {
        instance_id: u64,
    },
    /// Ask a merchant NPC to open its shop.
    OpenShop {
        merchant_player_id: String,
    },
    /// Tell the server the player closed a merchant's trade window. The
    /// server tracks open windows so a trading NPC can be held in place (its
    /// LLM movement is suppressed) while a customer is shopping with it.
    CloseShop {
        merchant_player_id: String,
    },
    /// Buy one unit of an item from a merchant's catalog at base price.
    BuyItem {
        merchant_player_id: String,
        item_def_id: String,
    },
    /// Sell one unit of a bag item to a merchant at its sell rate.
    SellItem {
        merchant_player_id: String,
        instance_id: u64,
    },
    /// NPC-only (LLM haggling): offer a price modifier on one item to a
    /// nearby player. The server clamps the modifier to the player's price
    /// band and enforces budgets/cooldowns; see `doc/ECONOMY.md`.
    OfferDeal {
        target_player_id: String,
        item_def_id: String,
        kind: DealKind,
        /// Requested percentage points off/on the normal price
        /// (negative = discount on buys, positive = bonus on sells).
        modifier_pct: i32,
        /// LLM's stated reason for the decision (logged server-side).
        reason: String,
    },
    /// NPC-only: push the sender's trade window (`ShopState`) onto a nearby
    /// player's client — the conversational entry point for trading
    /// ("LLM opens the trade window", doc/ECONOMY.md).
    OpenTrade {
        target_player_id: String,
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
        /// Unlocks debug/cheat UI; resolved per character at EnterGame.
        is_admin: bool,
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
        #[serde(default)]
        floor_level: i8,
    },
    /// A dungeon treasure chest was opened (loot already delivered to the
    /// opener's inventory/wallet; broadcast nearby for the celebration).
    DungeonChestOpened {
        entrance_id: String,
        player_id: String,
        item_def_ids: Vec<String>,
        gold: i64,
    },
    /// A destructible dungeon prop was broken: swap it to its broken variant
    /// and open its cell for movement. Broadcast to nearby players (the
    /// breaker included).
    DungeonPropBroken {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
    },
    /// An interactive dungeon chest prop was opened: play its lid-open
    /// animation. Broadcast to nearby players (the opener included).
    DungeonPropOpened {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
    },
    /// Snapshot of which props on a floor are already broken or opened, sent
    /// directly to a player as they enter that dungeon floor so late arrivals
    /// render the current state (broken debris + chests in the open pose).
    DungeonPropsState {
        entrance_id: String,
        depth: u8,
        broken: Vec<u32>,
        opened: Vec<u32>,
    },
    /// A dungeon door was toggled (surface entrance at depth 0, or an interior
    /// room door at depth ≥1). Broadcast to nearby players (the toggler
    /// included) so everyone sees it swing.
    DungeonDoorToggled {
        entrance_id: String,
        depth: u8,
        door_id: u32,
        is_open: bool,
    },
    /// Snapshot of every open door in a dungeon (entrance + interior), sent in
    /// reply to RequestDungeonDoors so a fresh client renders the doors others
    /// left open. Each entry is (depth, door_id); doors not listed are shut.
    DungeonDoorsState {
        entrance_id: String,
        doors: Vec<(u8, u32)>,
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
    /// Response to OpenShop (or pushed by an NPC's OpenTrade): the trader's
    /// goods. Display prices come from item definitions; the server
    /// re-validates them on Buy/Sell.
    ShopState {
        merchant_player_id: String,
        merchant_name: String,
        /// Merchant catalog (unlimited stock). Empty for non-merchants.
        catalog: Vec<String>,
        /// Percentage of base price paid when the player sells. For
        /// non-merchants this is the wishlist premium rate (can exceed 100).
        sell_rate_percent: u32,
        /// Haggled price modifiers this player currently holds with this
        /// merchant.
        #[serde(default)]
        active_deals: Vec<ActiveDeal>,
        /// Non-merchants only buy these item defs (their wishlist). Empty
        /// for merchants, who buy anything with a base price.
        #[serde(default)]
        wishlist: Vec<String>,
        /// Non-merchant real-inventory stock the player can buy (at base
        /// price). Empty for merchants, who use `catalog`.
        #[serde(default)]
        stock: Vec<StockEntry>,
    },
    /// Direct message: the receiving player's current gold (smallest unit).
    GoldUpdate {
        gold: i64,
    },
    /// Direct message: the receiving player's effective guard — base attribute
    /// plus every equipped item's guard bonus, i.e. the exact number combat
    /// uses to resolve hits. Sent on join and after any equipment change so the
    /// client can display it without duplicating the server formula.
    GuardUpdated {
        guard: i32,
    },
    /// Direct message: the receiving player gained loose currency from a
    /// pickup. `amount` is in the smallest unit (copper).
    GoldGained {
        amount: i64,
    },
    /// A shop request failed.
    TradeError {
        message: String,
    },
    /// Direct to a player: a haggled price modifier changed on one item.
    /// `modifier_pct == 0` means the deal was consumed or cleared.
    DealUpdated {
        merchant_player_id: String,
        item_def_id: String,
        kind: DealKind,
        modifier_pct: i32,
        expires_in_secs: u32,
    },
    /// Direct to a trading NPC: whether at least one player currently has its
    /// trade window open. While `busy` is true the NPC's LLM keeps its place
    /// (movement is suppressed) so it doesn't wander off mid-trade; it can
    /// still talk and haggle.
    TradeBusy {
        busy: bool,
    },
    /// Direct to a trading NPC: a player completed a buy/sell against it,
    /// so its LLM can react in conversation. `kind` is from the player's
    /// perspective (Buy = the player bought from the NPC).
    TradeNotice {
        player_name: String,
        item_def_id: String,
        kind: DealKind,
        /// Gold that changed hands (smallest unit).
        price: i64,
        /// The NPC's wallet after the trade.
        npc_gold: i64,
    },
    /// Direct to the offering NPC: the server's verdict on its `OfferDeal`.
    DealResult {
        target_player_id: String,
        target_player_name: String,
        item_def_id: String,
        kind: DealKind,
        accepted: bool,
        /// The modifier actually in effect (after band clamping); 0 when
        /// rejected.
        applied_modifier_pct: i32,
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
