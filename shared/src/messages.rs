//! WebSocket protocol envelopes between client and server. `ClientMessage`
//! is everything a client can ask for (move, attack, place house, equip
//! item …); `ServerMessage` is everything the server pushes back (world
//! snapshots, combat results, inventory deltas, kicks). Both serialize
//! via MessagePack — convenience helpers at the bottom of the file
//! centralise the `rmp_serde::to_vec` / `from_slice` calls so callers
//! don't have to know the wire format.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::character::{
    Character, CharacterAppearance, CharacterAttributes, CharacterClass, Gender,
};
use crate::entity::{Monster, MonsterState, Player};
use crate::world::{GameDateTime, Position};
use crate::{fishing, housing, inventory, skills};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveWaypoint {
    pub position: Position,
    pub floor_level: i8,
    pub rotation: Option<f32>,
    pub travel_seconds: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveStatus {
    Searching,
    Moving,
    Arrived,
    Partial,
    Blocked,
    Stopped,
    Rejected,
    Busy,
    MapChanged,
    NodeLimit,
}

/// One player a skill's strike hit, as it stands after the blow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillVictim {
    pub player_id: PlayerId,
    pub damage: u32,
    /// Hidden like `Player::health`.
    pub current_health: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedMessage {
    pub code: String,
    pub params: HashMap<String, String>,
}

impl LocalizedMessage {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: HashMap::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Which side of a merchant trade a haggled deal applies to.
/// `Buy` = the player buys from the merchant, `Sell` = the player sells to
/// the merchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DealKind {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeleportPhase {
    Departing,
    Arriving,
    Cancelled,
}

/// What an altar would do with the clicker's offering right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltarStatus {
    Ready {
        offering: String,
        monster: String,
    },
    NoOffering,
    /// Only a worn copy; it must be taken off first.
    OfferingEquipped {
        offering: String,
    },
    NotFullMoon,
    RaidActive,
}

/// How a field raid ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRaidOutcome {
    Defeated,
    MoonSet,
    /// The morning twilight gave way to full daylight.
    Dawn,
    /// No living character left in the battle zone.
    Wiped,
}

/// Why a `PlayerAttack` request was dropped. Deliberately coarse: a stale id
/// must not reveal hidden monster state such as its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackRejectReason {
    InvalidTarget,
    OutOfRange,
    AttackerDead,
    NotInGame,
    /// The wielded weapon spends ammunition and the bag has none of its kind.
    OutOfAmmo,
}

impl std::fmt::Display for AttackRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidTarget => "invalid_target",
            Self::OutOfRange => "out_of_range",
            Self::AttackerDead => "attacker_dead",
            Self::NotInGame => "not_in_game",
            Self::OutOfAmmo => "out_of_ammo",
        })
    }
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

/// Units the player recently sold to a merchant, repurchasable at the exact
/// payout the player received. Sold units normally vanish (merchants keep no
/// stock), so this is the only way to undo a mis-sell. Stackable units sold
/// back to back at one price share an entry and are bought back together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuybackEntry {
    /// Server-issued id the client references in `BuybackItem`; becomes the
    /// restored item's instance id on repurchase.
    pub entry_id: u64,
    pub item_def_id: String,
    pub enchant: i32,
    /// Dye the sold cape carried, so buying it back returns it dyed.
    #[serde(default)]
    pub cape_color: Option<String>,
    /// Same for its texture hash.
    #[serde(default)]
    pub cape_texture: Option<String>,
    /// Gold the player was paid per unit (smallest unit) — buying back
    /// costs exactly this, so the round trip is gold-neutral.
    pub price: i64,
    pub quantity: u32,
}

impl BuybackEntry {
    pub fn total_price(&self) -> i64 {
        self.price * i64::from(self.quantity)
    }

    pub fn same_units(&self, other: &BuybackEntry) -> bool {
        self.item_def_id == other.item_def_id
            && self.enchant == other.enchant
            && self.cape_color == other.cape_color
            && self.cape_texture == other.cape_texture
            && self.price == other.price
    }
}

/// One line of a stall purchase: `quantity` units off one listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StallBuyLine {
    pub instance_id: u64,
    pub quantity: u32,
}

/// One line of a batched `BuyItems` request: buy `qty` units of one item def.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeLineItem {
    pub item_def_id: String,
    pub qty: u32,
}

/// A quantity from one inventory instance; equipped items require qty 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BagLineItem {
    pub instance_id: u64,
    pub qty: u32,
}

/// One party member as listed in `PartyState`. `hp`/`max_hp` are the
/// roster-time snapshot; steady-state updates ride `PartyVitals`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMember {
    pub id: PlayerId,
    pub name: String,
    pub hp: u32,
    pub max_hp: u32,
    pub class: crate::character::CharacterClass,
    #[serde(default)]
    pub family: Option<String>,
}

/// One member's health as listed in `PartyVitals`. No name or class: the
/// roster from `PartyState` already carries them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMemberVitals {
    pub id: PlayerId,
    pub hp: u32,
    pub max_hp: u32,
}

/// Cap on `/announce` text; the web client reads it through `wasm_api`.
pub const ANNOUNCEMENT_MAX_CHARS: usize = 500;

/// How long a party invite stays acceptable. Shared so the server's
/// enforcement and the agent-client's pruning are guaranteed equal; the web
/// client mirrors it (`INVITE_TTL_MS` in `partyStore.ts`).
pub const PARTY_INVITE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// How long a party summon stays acceptable; `PARTY_INVITE_TTL`'s twin. The
/// web client mirrors it (`SUMMON_TTL_MS` in `partyStore.ts`).
pub const PARTY_SUMMON_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// One member's location as listed in `PartyPositions`. No name: the roster
/// from `PartyState` already carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMemberPosition {
    pub id: PlayerId,
    pub x: f32,
    pub z: f32,
    pub floor_level: i8,
}

/// One friend as listed in `FriendList`. Keyed by character id, not the
/// per-session `PlayerId`: a friendship outlives both sessions, and offline
/// friends have no player id at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FriendEntry {
    pub character_id: i64,
    pub name: String,
    pub level: u32,
    pub class: crate::character::CharacterClass,
    #[serde(default)]
    pub family: Option<String>,
}

/// One online friend as listed in `FriendsOnline`. No name — `FriendList`
/// already carries it — but the level and family ride along, so changes show
/// without re-sending the whole roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineFriend {
    pub character_id: i64,
    pub level: u32,
    #[serde(default)]
    pub family: Option<String>,
}

/// How long a friend request stays answerable. Four times
/// `PARTY_INVITE_TTL`: a party invite is an offer to play *now*, a friend
/// request can wait out the fight the target is in. The web client mirrors it
/// (`FRIEND_REQUEST_TTL_MS` in `friendStore.ts`).
pub const FRIEND_REQUEST_TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// How long a player-trade request stays answerable. `PARTY_INVITE_TTL`'s
/// length for the same reason: trading is an offer to meet *now*.
pub const PLAYER_TRADE_REQUEST_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// How long an open trade session survives without either side touching it.
/// Long, because haggling has real pauses — but bounded, because an offered
/// item is reserved out of its owner's other actions (doc/TRADE.md).
pub const PLAYER_TRADE_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(180);

/// One item in a player-trade offer. Carries `enchant` because +0 and +7 are
/// otherwise indistinguishable, which is the cleanest scam in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerTradeItem {
    pub instance_id: u64,
    pub item_def_id: String,
    pub quantity: u32,
    pub enchant: i32,
    #[serde(default)]
    pub cape_color: Option<String>,
    #[serde(default)]
    pub cape_texture: Option<String>,
}

/// What a client asks to put on the table: whole-offer, never a delta, so a
/// dropped packet cannot desync the two windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerTradeSlot {
    pub instance_id: u64,
    pub quantity: u32,
}

/// One side of a live trade as the server sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerTradeSide {
    pub player_id: PlayerId,
    pub name: String,
    pub items: Vec<PlayerTradeItem>,
    pub copper: i64,
    pub locked: bool,
    pub confirmed: bool,
}

/// The whole session, re-sent on every change. `you` is always the recipient's
/// own side, so each client gets its own view of the same revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerTradeState {
    /// Bumped on every offer change. A `PlayerTradeConfirm` naming a stale
    /// revision is refused — this is what defeats the last-second swap.
    pub revision: u32,
    pub you: PlayerTradeSide,
    pub them: PlayerTradeSide,
}

/// Web client's rendering environment, so performance complaints can be
/// matched against actual hardware. The client sends it after entering the
/// game, but only when the environment changed since the last report. Field
/// names mirror the client's `ClientEnvReport` interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEnvReport {
    pub quality: String,
    pub render_budget: String,
    pub antialias: bool,
    pub pixel_ratio: f32,
    pub device_pixel_ratio: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub screen_w: u32,
    pub screen_h: u32,
    pub backend: String,
    pub gpu_vendor: String,
    pub gpu_architecture: String,
    pub gpu_device: String,
    pub gpu_description: String,
    pub user_agent: String,
}

/// Interaction the server stores for a `/play_music` performance. A wire
/// contract, not a private constant: the server sets it, and both clients
/// compare `PlayerInteractionChanged` against it to know a tune is over.
pub const MUSIC_EMOTE: &str = "guitar_playing";
pub const EXCAVATE_EMOTE: &str = "excavate";

pub const INSTRUMENT_NOTE_COUNT: u8 = 22;
pub const INSTRUMENT_BATCH_MS: u16 = 250;
/// Hands top out near ten notes per 250 ms; slack beyond that only serves
/// clients flooding listeners, who build audio nodes per note received.
pub const INSTRUMENT_MAX_EVENTS_PER_BATCH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentNoteEvent {
    pub note: u8,
    pub offset_ms: u16,
}

/// One-shot clips `/emote <name>` may store as the interaction. Same wire
/// contract as [`MUSIC_EMOTE`]: the server validates against this list, and
/// clients start the clip off the broadcast and send `StopInteraction` when
/// it ends. Clip names live in `social.glb`.
pub const ONE_SHOT_EMOTES: &[&str] = &["excited", "clap", "yawn", "shake_head"];

/// Clips `/emote <name>` loops instead of playing once: the dancer's client
/// repeats the clip until the player moves or presses Escape, then sends
/// `StopInteraction` — the held-pose contract of [`MUSIC_EMOTE`], minus the
/// music. Clip names live in `social.glb`.
pub const LOOPING_EMOTES: &[&str] = &[
    "twist",
    "macarena",
    "chicken",
    "stand_pose2",
    "stand_pose3",
    "stand_pose4",
    "weight_shift",
];

/// `message` is `prefix` as a whole slash-command word; returns the trimmed
/// remainder. Shared because the agent-client types the commands this parses —
/// the two sides must agree on what counts as the command word.
pub fn strip_command<'a>(message: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = message.trim().strip_prefix(prefix)?;
    (rest.is_empty() || rest.starts_with(' ')).then(|| rest.trim())
}

/// The title a `/play_music` argument names: a whole title first, then a
/// fragment of one, ignoring case. Shared for the same reason as
/// `strip_command` — the server resolves the query and the agent-client
/// decides beforehand whether it would resolve at all. An empty query is the
/// server's random pick, which is the caller's business, not this rule's.
pub fn resolve_title<'a>(
    mut titles: impl Iterator<Item = &'a str> + Clone,
    query: &str,
) -> Option<&'a str> {
    let wanted = query.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    titles
        .clone()
        .find(|t| t.to_lowercase() == wanted)
        .or_else(|| titles.find(|t| t.to_lowercase().contains(&wanted)))
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    ResyncWorld,
    /// Mandatory first message: protocol check plus who is connecting. The
    /// server refuses anything else until it arrives, and refuses the
    /// connection outright when `protocol_version` differs from its own.
    ///
    /// This shape is frozen. Together with `ServerMessage::AuthError` it is
    /// the only channel that can tell an out-of-date client why it was
    /// refused, so extra data goes in a new message, never in here.
    ClientInfo {
        protocol_version: u32,
        /// Which client program: "web" (browser) or "cli" (agent-client).
        /// Self-reported, used only for the `/who` breakdown — never for
        /// permissions, or clients would have a reason to lie.
        client_kind: String,
        client_version: String,
    },
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
        #[serde(default)]
        appearance: CharacterAppearance,
    },
    RollCharacterStats {
        character_class: CharacterClass,
        gender: Gender,
    },
    DeleteCharacter {
        character_id: i64,
    },
    CancelCharacterDeletion {
        character_id: i64,
    },
    /// New name for one of the account's characters, sent from character
    /// select after the server refused entry with `CharacterRenameRequired`.
    RenameCharacter {
        character_id: i64,
        new_name: String,
    },
    EnterGame {
        character_id: i64,
    },
    /// The scene has finished compiling, so the player can be hit again. See
    /// `entity::WORLD_LOADING_GRACE_MS`.
    WorldReady,
    PlayerMoveGoal {
        request_id: u32,
        x: f32,
        z: f32,
        sprinting: bool,
        #[serde(default)]
        stop_at_entrance: bool,
    },
    PlayerMoveStop {
        request_id: u32,
    },
    PlayerMoveDirection {
        request_id: u32,
        rotation: f32,
        forward: i8,
        turn: i8,
        sprinting: bool,
    },
    PlayerFace {
        rotation: f32,
    },
    ChatMessage {
        message: String,
    },
    PlayerAttack {
        monster_id: String,
    },
    StartPlayerAttack {
        monster_id: String,
        dagger_skill: bool,
        request_id: u32,
    },
    StopPlayerAttack {
        request_id: u32,
    },
    SetPlayerAttackSkill {
        request_id: u32,
        dagger_skill: bool,
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
    /// Start mining an ore heap or crystal cluster with a pickaxe in the bag.
    /// The server checks it like `BreakDungeonProp`, then runs a
    /// `MINING_CAST_MS` cast that movement or combat cancels.
    MineDungeonProp {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
    },
    /// Push a rail cart from one of its ends; it slides away along the rail.
    PushDungeonCart {
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
    InteractObject {
        object_type: String,
        object_id: u32,
    },
    /// Clicked an offering altar; answered with `AltarStatus`.
    InspectAltar {
        object_id: u32,
    },
    /// Confirmed the offering after a `Ready` `AltarStatus`.
    OfferAtAltar {
        object_id: u32,
    },
    /// Clicked a notice board; answered with `NoticeBoard`.
    OpenNoticeBoard {
        board: crate::notice_board::NoticeBoardRef,
    },
    ReadNoticePost {
        board: crate::notice_board::NoticeBoardRef,
        post_id: i64,
    },
    /// Spends one notice paper. `position` is the paper centre (see
    /// `notice_board::PAPER_SCALE`); without one the server picks a free spot.
    WriteNoticePost {
        board: crate::notice_board::NoticeBoardRef,
        title: String,
        body: String,
        #[serde(default)]
        image: Option<String>,
        #[serde(default)]
        position: Option<(u16, u16)>,
    },
    /// Papers on a board in sight, for the board model; answered with
    /// `NoticeBoardPapers`.
    PeekNoticeBoard {
        board: crate::notice_board::NoticeBoardRef,
    },
    /// Land registrar only: removes posts past their lifetime.
    TakeDownNotices {
        object_id: u32,
    },
    /// Answered with `Mailbox`: the reader's own letters.
    OpenMailbox {
        mailbox: crate::mail::MailboxRef,
    },
    ReadMail {
        mailbox: crate::mail::MailboxRef,
        letter_id: i64,
    },
    /// Spends one letter paper.
    SendMail {
        mailbox: crate::mail::MailboxRef,
        recipient: String,
        body: String,
    },
    DeleteMail {
        mailbox: crate::mail::MailboxRef,
        letter_id: i64,
    },
    WriteNoticeComment {
        board: crate::notice_board::NoticeBoardRef,
        post_id: i64,
        body: String,
    },
    /// Writer only; the paper is not returned. Answered with
    /// `NoticePostRemoved`, then the refreshed `NoticeBoard`.
    RemoveNoticePost {
        board: crate::notice_board::NoticeBoardRef,
        post_id: i64,
    },
    StartInstrument,
    InstrumentNotes {
        events: Vec<InstrumentNoteEvent>,
    },
    StopInteraction,
    Heartbeat,
    PlaceHouse {
        instance_id: u64,
        origin: Position,
        quarter_turns: u8,
    },
    RemoveHouse {
        house_id: String,
    },
    /// Open or shut the door named by a `door::DoorId` within reach.
    ToggleDoor {
        door: String,
    },
    EquipItem {
        instance_id: u64,
    },
    UnequipItem {
        slot: inventory::EquipSlot,
    },
    SetItemLocked {
        instance_id: u64,
        locked: bool,
    },
    /// Draw from this pile instead of the strongest. `None` clears the choice
    /// and puts the next shot back on the best round in the bag. Ammunition
    /// is stackable and so cannot occupy an equip slot; this is how it is
    /// "equipped" (doc/COMBAT.md 원거리 전투).
    SelectAmmo {
        item_def_id: Option<String>,
    },
    /// Drop bag quantities and equipped items in one all-or-nothing operation.
    DropItems {
        items: Vec<BagLineItem>,
    },
    /// The pickup crouch started. Sent at the clip's first frame, whereas
    /// `PickupItem` waits for the grab moment ~35% in, so nearby players see
    /// the whole animation instead of joining it late.
    PickupStarted,
    PickupItem {
        instance_id: u64,
    },
    PickupNearbyItems,
    /// Consume a usable item from the bag (e.g. drink a healing potion).
    UseItem {
        instance_id: u64,
    },
    UseLandDocument {
        instance_id: u64,
        tile_x: i32,
        tile_z: i32,
        quadrant: u8,
    },
    EditFence {
        edge: crate::fence::FenceEdge,
        place: bool,
    },
    StartLandscapingMode {
        tool: crate::landscaping::LandscapingTool,
    },
    EditLandscape {
        stroke: crate::landscaping::LandscapingStroke,
    },
    PlaceEstateChest {
        instance_id: u64,
        position: Position,
        rotation_deg: f32,
        floor_level: i8,
    },
    StartEstateFurnitureMove {
        furniture_id: i64,
    },
    MoveEstateFurniture {
        furniture_id: i64,
        expected_revision: u64,
        position: Position,
        rotation_deg: f32,
        floor_level: i8,
    },
    OpenEstateChest {
        chest_id: i64,
    },
    SetEstateFurnitureText {
        furniture_id: i64,
        text: String,
    },
    SelectFurnitureDisplay {
        display_id: u32,
    },
    CheckoutFurniture {
        items: Vec<crate::furniture_shop::FurnitureOrderLine>,
        expected_gold: i64,
        expected_total: i64,
    },
    TransferEstateItems {
        chest_id: i64,
        deposits: Vec<BagLineItem>,
        withdrawals: Vec<BagLineItem>,
        expected_revision: u64,
    },
    RecoverEstateChest {
        chest_id: i64,
    },
    LandAccount {
        merchant_player_id: PlayerId,
    },
    LandDeposit {
        merchant_player_id: PlayerId,
        amount: i64,
    },
    LandWithdraw {
        merchant_player_id: PlayerId,
        amount: i64,
    },
    /// Dye the worn cape with `color` (`#rrggbb`), spending the dye at
    /// `instance_id`. Answers a `CapeDyePrompt`; the server re-checks
    /// everything (doc/CAPE_CUSTOMIZATION.md).
    DyeCape {
        instance_id: u64,
        color: String,
    },
    /// Put the already-uploaded texture `texture` (a content hash) on the worn
    /// cape, spending the transfer kit at `instance_id`. Answers a
    /// `CapeTexturePrompt`; the server re-checks everything.
    ApplyCapeTexture {
        instance_id: u64,
        texture: String,
    },
    /// Report the cape texture another player is wearing. The server records
    /// the hash, the reporter and the target for an admin to review.
    ReportCapeTexture {
        player_id: PlayerId,
    },
    /// Drop `amount` copper into a nearby tip hat. The server checks the
    /// wallet, the distance and that the hat isn't the sender's own.
    TipHat {
        hat_id: u64,
        amount: i64,
        #[serde(default)]
        song: Option<String>,
    },
    /// Step up to a stall. An NPC's stall is its shop front and opens the
    /// priced shop instead; a player's opens the consignment panel.
    OpenStall {
        stall_id: u64,
    },
    /// Stop watching the stall panel, so listing changes stop being pushed.
    CloseStall,
    /// Write the sender's stall sign, or clear it with an empty string.
    SetStallSign {
        sign: String,
    },
    /// Put `quantity` units of a bag item on the sender's stall at
    /// `unit_price` copper each. Listing the same instance again re-prices it.
    ListStallItem {
        instance_id: u64,
        quantity: u32,
        unit_price: i64,
    },
    /// Take a listing back off the sender's stall.
    UnlistStallItem {
        instance_id: u64,
    },
    /// Buy off one stall. All-or-nothing over every line, like `BuyItems`:
    /// the server re-checks the stock, the wallet, the carried weight and the
    /// distance to the table, then moves the lot in one swap.
    BuyFromStall {
        stall_id: u64,
        lines: Vec<StallBuyLine>,
    },
    SetStallBuyOrder {
        item_def_id: String,
        quantity: u32,
        enchant: i32,
        unit_price: i64,
    },
    RemoveStallBuyOrder {
        order_id: u64,
    },
    SellToStall {
        stall_id: u64,
        order_id: u64,
        instance_id: u64,
        quantity: u32,
    },
    /// Official NPC only: set `item_def_id` on the table in front of the
    /// occupied chair `chair_object_id`. The server resolves the table top.
    ServeMeal {
        chair_object_id: u32,
        item_def_id: String,
    },
    /// Eat the plate served to the chair the sender is sitting on.
    EatMeal {
        meal_id: u64,
    },
    /// Official NPC only: take an abandoned plate away.
    ClearMeal {
        meal_id: u64,
    },
    /// Show `title` above the name, or nothing. Ignored unless the character
    /// has earned it (doc/TITLES.md).
    SetActiveTitle {
        title: Option<String>,
    },
    /// Ask a merchant NPC to open its shop.
    OpenShop {
        merchant_player_id: PlayerId,
    },
    /// Tell the server the player closed a merchant's trade window. The
    /// server tracks open windows so a trading NPC can be held in place (its
    /// LLM movement is suppressed) while a customer is shopping with it.
    CloseShop {
        merchant_player_id: PlayerId,
    },
    /// Buy one unit of an item from a merchant's catalog at base price.
    BuyItem {
        merchant_player_id: PlayerId,
        item_def_id: String,
    },
    /// Sell one unit of a bag item to a merchant at its sell rate.
    SellItem {
        merchant_player_id: PlayerId,
        instance_id: u64,
    },
    /// Repurchase a whole `BuybackEntry` previously sold to this merchant, at
    /// its recorded per-unit payout times its quantity.
    BuybackItem {
        merchant_player_id: PlayerId,
        entry_id: u64,
    },
    /// Buy multiple units, possibly of different items, in one all-or-nothing
    /// transaction (see `SellItems` for the mirror).
    BuyItems {
        merchant_player_id: PlayerId,
        items: Vec<TradeLineItem>,
    },
    /// Sell multiple bag stacks (partial quantities allowed) in one
    /// all-or-nothing transaction.
    SellItems {
        merchant_player_id: PlayerId,
        items: Vec<BagLineItem>,
    },
    /// Repurchase multiple buyback entries at once, all-or-nothing.
    BuybackItems {
        merchant_player_id: PlayerId,
        entry_ids: Vec<u64>,
    },
    /// NPC-only (LLM haggling): offer a price modifier on one item to a
    /// nearby player. The server clamps the modifier to the player's price
    /// band and enforces budgets/cooldowns; see `doc/ECONOMY.md`.
    OfferDeal {
        target_player_id: PlayerId,
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
        target_player_id: PlayerId,
    },
    /// Wave off an NPC-pushed trade offer ("Not now" on the toast, or the
    /// toast timing out unanswered). Relayed to the NPC as `TradeDeclined`
    /// so its agent stops pushing trade windows at that player for a while.
    DeclineTrade {
        merchant_player_id: PlayerId,
    },
    /// Ask a named online player to trade. Name-based like `PartyInvite`, but
    /// unlike it the target must also be within `MAX_TRADE_DISTANCE`.
    PlayerTradeRequest {
        target_name: String,
    },
    /// Accept or decline a pending trade request from `requester_id`.
    PlayerTradeRespond {
        requester_id: PlayerId,
        accept: bool,
    },
    /// Replace the sender's whole side of the table. Whole-offer rather than
    /// add/remove so the server never has to reconcile a partial view.
    PlayerTradeSetOffer {
        items: Vec<PlayerTradeSlot>,
        copper: i64,
    },
    /// Freeze the sender's side at `revision`. Refused if the revision moved.
    PlayerTradeLock {
        revision: u32,
    },
    /// Reopen the sender's side for edits, clearing both confirmations.
    PlayerTradeUnlock,
    /// Commit the sender's side. Both sides confirmed at the same revision
    /// executes the swap; a stale revision is refused.
    PlayerTradeConfirm {
        revision: u32,
    },
    /// Abandon the session, releasing both sides' reservations.
    PlayerTradeCancel,
    /// Invite a named player to the sender's party. Name-based like whisper:
    /// the target may be outside the sender's AOI.
    PartyInvite {
        target_name: String,
    },
    /// Accept or decline a pending party invite from `inviter_id`.
    PartyRespond {
        inviter_id: PlayerId,
        accept: bool,
    },
    /// Accept or decline a pending party summon from `caster_id`.
    PartySummonRespond {
        caster_id: PlayerId,
        accept: bool,
    },
    /// Leave the current party. The leader leaving promotes the earliest
    /// remaining member; a party reduced to one member disbands.
    PartyLeave,
    /// Leader-only: remove `target_id` from the sender's party. A party
    /// reduced to one member disbands, like `PartyLeave`.
    PartyKick {
        target_id: PlayerId,
    },
    /// Leader-only: hand party leadership to `target_id`.
    PartyPromote {
        target_id: PlayerId,
    },
    /// Leader-only: let mates take each other's claimed loot at once.
    PartySetLoot {
        shared: bool,
    },
    /// Say something to the sender's party. Delivered to every online member
    /// wherever they are (no AOI cut), echoed to the sender included.
    PartyChat {
        message: String,
    },
    /// Accept or decline a pending friend request from `requester_id`.
    FriendRespond {
        requester_id: PlayerId,
        accept: bool,
    },
    /// Drop a friendship, both directions. Name-based like `PartyInvite`: the
    /// friend may be offline, so no player id exists to name them by.
    FriendRemove {
        name: String,
    },
    /// Ask which of the sender's friends are online right now. Polled by the
    /// client (faster while the panel is open); there is no presence push.
    RequestFriendsOnline,
    /// Ask where the sender's party members are right now (map open). A
    /// one-shot snapshot: steady-state updates are pushed by the server's
    /// party-position tick whenever a member relocates.
    RequestPartyPositions,
    /// Cast the equipped fishing rod at a water point. The server validates
    /// rod, range, floor and water (water-field depth at the point) and
    /// answers with a `FishingCasted` broadcast or a direct `FishingError`.
    FishingCast {
        position: Position,
    },
    /// Respond to the fish (currently only `Hook`, on a bite). Timing is
    /// judged server-side against the bite deadline plus latency grace.
    FishingRespond {
        action: fishing::FishingAction,
    },
    /// Reel in deliberately. Also implied by moving, attacking or
    /// disconnecting — any of them ends the session as `Aborted`.
    FishingStop,
    /// Logged server-side only; accepted once per connection.
    EnvReport(ClientEnvReport),
    UseAbility {
        ability: crate::ability::AbilityId,
        monster_id: Option<String>,
        #[serde(default)]
        target_player_id: Option<PlayerId>,
    },
    /// Found a family at the land registrar (doc/FAMILY_SYSTEM.md). Failures
    /// come back as a system message; success as `FamilyState`.
    CreateFamily {
        merchant_player_id: PlayerId,
        name: String,
    },
    /// Invite an online character without a family.
    InviteToFamily {
        name: String,
    },
    RespondFamilyInvite {
        inviter_id: PlayerId,
        accept: bool,
    },
    LeaveFamily,
    /// The head, as the last member, dissolves the family at the land
    /// registrar.
    DissolveFamily {
        merchant_player_id: PlayerId,
    },
    KickFamilyMember {
        character_id: i64,
    },
    SetFamilyMemberRank {
        character_id: i64,
        rank: u8,
    },
    TransferFamilyHead {
        character_id: i64,
    },
    /// Succeed a head who has been away `FAMILY_HEAD_INACTIVE_SECS`.
    ClaimFamilyHead,
    /// Change a rank's label and permission bits.
    SetFamilyRank {
        rank: u8,
        label: String,
        permissions: u32,
    },
    /// The head renames the family at the land registrar.
    RenameFamily {
        merchant_player_id: PlayerId,
        name: String,
    },
    /// Attach the sender's account's estate to their family, or detach it.
    AttachEstateToFamily {
        attach: bool,
    },
    /// Pay into a family estate's tax account.
    DepositFamilyEstate {
        estate_id: i64,
        amount: i64,
    },
    /// Say something to every online member of the sender's family.
    FamilyChat {
        message: String,
    },
    /// Answered with `FamilyStorageLog`.
    RequestFamilyStorageLog,
    /// Needs `EDIT_EMBLEM`. `texture` is an uploaded cape texture hash.
    SetFamilyEmblem {
        texture: Option<String>,
        color: Option<String>,
    },
    /// The head picks which attached estate a Scroll of Family Return reaches.
    SetFamilySeat {
        estate_id: i64,
    },
    /// Show the family emblem on the sender's own cape, or their own print.
    SetFamilyCapeEmblem {
        show: bool,
    },
    /// A head offers to merge with the family `head_name` leads.
    ProposeFamilyMerge {
        head_name: String,
        terms: crate::family::FamilyMergeTerms,
    },
    RespondFamilyMerge {
        proposer_id: PlayerId,
        accept: bool,
    },
    /// The proposing head's own answer when the officiant asks them first.
    ConfirmFamilyMerge {
        accept: bool,
    },
    /// Ask for the emblem of a named online player's family, for the pledge
    /// form; answered with `FamilyEmblemShown` when they have a family.
    InspectFamilyEmblem {
        player_name: String,
    },
    /// NPC-only: the merger officiant hands a nearby family head the pledge
    /// form, the spoken way into `ProposeFamilyMerge`.
    OfferMergerPledge {
        target_player_id: PlayerId,
    },
}

impl ClientMessage {
    /// Acts a player turned to stone may not take (doc/DEBUFF.md
    /// `petrifies`). Movement and facing check `Player.petrified` where they
    /// apply, off this per-message path. Admins keep their slash commands.
    pub fn blocked_while_petrified(&self, is_admin: bool) -> bool {
        use ClientMessage::*;
        match self {
            ChatMessage { message } => message.starts_with('/') && !is_admin,
            PlayerAttack { .. }
            | StartPlayerAttack { .. }
            | UseAbility { .. }
            | OpenDungeonChest { .. }
            | BreakDungeonProp { .. }
            | MineDungeonProp { .. }
            | PushDungeonCart { .. }
            | OpenDungeonProp { .. }
            | InteractObject { .. }
            | InspectAltar { .. }
            | OfferAtAltar { .. }
            | WriteNoticePost { .. }
            | WriteNoticeComment { .. }
            | RemoveNoticePost { .. }
            | TakeDownNotices { .. }
            | SendMail { .. }
            | StartInstrument
            | InstrumentNotes { .. }
            | PlaceHouse { .. }
            | RemoveHouse { .. }
            | ToggleDoor { .. }
            | EquipItem { .. }
            | UnequipItem { .. }
            | DropItems { .. }
            | PickupStarted
            | PickupItem { .. }
            | PickupNearbyItems
            | UseItem { .. }
            | UseLandDocument { .. }
            | EditFence { .. }
            | StartLandscapingMode { .. }
            | EditLandscape { .. }
            | PlaceEstateChest { .. }
            | StartEstateFurnitureMove { .. }
            | MoveEstateFurniture { .. }
            | OpenEstateChest { .. }
            | TransferEstateItems { .. }
            | DyeCape { .. }
            | ApplyCapeTexture { .. }
            | TipHat { .. }
            | OpenStall { .. }
            | BuyFromStall { .. }
            | SellToStall { .. }
            | ServeMeal { .. }
            | EatMeal { .. }
            | OpenShop { .. }
            | BuyItem { .. }
            | SellItem { .. }
            | BuybackItem { .. }
            | BuyItems { .. }
            | SellItems { .. }
            | BuybackItems { .. }
            | OfferDeal { .. }
            | OpenTrade { .. }
            | PlayerTradeRequest { .. }
            | PlayerTradeRespond { .. }
            | PartySummonRespond { .. }
            | FishingCast { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    DungeonPropState {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
        active: bool,
        broken: bool,
        opened: bool,
        /// Cells a rail cart stands pushed along its rail.
        offset: i32,
    },
    TerrainTileVersion {
        tile_x: i32,
        tile_z: i32,
        files: crate::terrain_files::TerrainFiles,
    },
    WorldUpdate {
        world_epoch: String,
        generation: u64,
        sequence: u64,
        position: Position,
        floor_level: i8,
        reset: bool,
        ready: bool,
        events: Vec<crate::interest::WorldEvent>,
    },
    AuthSuccess {
        account_name: String,
        characters: Vec<Character>,
        /// Bearer credential for the player's own REST calls (cape texture
        /// upload). Separate from the Google id token, which expires inside
        /// an hour while a session runs all evening.
        #[serde(default)]
        cape_upload_token: String,
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
    CharacterDeletionScheduled {
        character_id: i64,
        deletion_due_at: i64,
    },
    CharacterDeletionCancelled {
        character_id: i64,
    },
    /// Entry refused: the character's name is on the banned list. NPC
    /// accounts never see this, having no one to answer the prompt.
    CharacterRenameRequired {
        character_id: i64,
    },
    CharacterRenamed {
        character_id: i64,
        name: String,
    },
    CharacterError {
        message: String,
    },
    PlayerJoined {
        player: Player,
    },
    PlayerLeft {
        player_id: PlayerId,
    },
    PlayerAppeared {
        player: Player,
    },
    PlayerDisappeared {
        player_id: PlayerId,
    },
    PlayerMovePath {
        request_id: u32,
        server_time_ms: u64,
        position: Position,
        rotation: f32,
        floor_level: i8,
        waypoints: Vec<MoveWaypoint>,
        speed: f32,
        termination: crate::pathfinding::PathTermination,
    },
    PlayerMoveProgress {
        request_id: u32,
        server_time_ms: u64,
        position: Position,
        rotation: f32,
        floor_level: i8,
        next_waypoint: u32,
        speed: f32,
        status: MoveStatus,
    },
    PlayerMoved {
        player_id: PlayerId,
        position: Position,
        rotation: f32,
        #[serde(default)]
        floor_level: i8,
        #[serde(default)]
        sprinting: bool,
    },
    PlayerTeleported {
        player_id: PlayerId,
        position: Position,
        rotation: f32,
        #[serde(default)]
        floor_level: i8,
    },
    /// Valhalla Charge. The server already has the charger at `position`;
    /// viewers run the model there from `from` over `duration_ms`.
    PlayerCharged {
        player_id: PlayerId,
        monster_id: String,
        from: Position,
        position: Position,
        rotation: f32,
        floor_level: i8,
        duration_ms: u64,
    },
    PlayerTeleportEffect {
        player_id: PlayerId,
        position: Position,
        floor_level: i8,
        phase: TeleportPhase,
    },
    /// A dungeon treasure chest was opened. The rolled items burst out of
    /// the chest as ground drops moments later; the gold goes straight to
    /// the opener's wallet. Broadcast nearby — except when `item_def_ids`
    /// is empty and `gold` is 0: that is a re-open of a chest the opener
    /// already claimed tonight (real opens always pay gold), sent to the
    /// opener alone so their lid swings on an empty box.
    DungeonChestOpened {
        entrance_id: String,
        player_id: PlayerId,
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
    /// Nearby: `pusher_id` rolled a rail cart to `offset` cells from its spot.
    /// A monster in its path stops it and takes `damage`; a player is struck
    /// on arrival with `CartStruckPlayer`.
    DungeonCartPushed {
        entrance_id: String,
        depth: u8,
        prop_id: u32,
        offset: i32,
        pusher_id: PlayerId,
        struck_monster: Option<String>,
        damage: u32,
    },
    /// Nearby: a rolling cart reached `player_id`, still in its path.
    CartStruckPlayer {
        player_id: PlayerId,
        damage: u32,
        health: u32,
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
    /// Every dungeon entrance this character has discovered (world-map
    /// markers): full snapshot at join and after each new discovery. Only
    /// ids travel — both sides embed the entrance registry, so the client
    /// resolves names and positions locally.
    DungeonDiscoveries {
        entrance_ids: Vec<String>,
    },
    ChatMessage {
        player_id: PlayerId,
        message: String,
    },
    /// A private message, delivered only to the target and echoed to the
    /// sender. Carries names instead of ids: whisper ignores distance, so
    /// either end may be outside the other's AOI where an id resolves to
    /// nobody.
    WhisperMessage {
        from: String,
        to: String,
        message: String,
        #[serde(default)]
        from_family: Option<String>,
        #[serde(default)]
        to_family: Option<String>,
    },
    /// Private server reply to the player's own command (`/who`, `/escape`,
    /// whisper errors). Not `ChatMessage`: clients must render it as a system
    /// line, not the player's own speech.
    SystemMessage {
        message: String,
        #[serde(default)]
        localization: Option<LocalizedMessage>,
    },
    /// A party-channel line, sent to every online member (the sender's echo
    /// included). Carries the name like `WhisperMessage`: party chat ignores
    /// distance, so the sender may be outside a member's AOI.
    PartyChatMessage {
        from: String,
        message: String,
        #[serde(default)]
        from_family: Option<String>,
    },
    /// One verse of a recital, paced over a song. `logged` (`/recite`) puts
    /// it in the chat log as well as the bubble — the first pass of a lyric;
    /// `/recite_quiet` repeats are bubble-only so the loop does not flood
    /// the log. Same reach as speech.
    Recital {
        player_id: PlayerId,
        line: String,
        logged: bool,
    },
    /// Direct to the invitee: a party invite to answer with `PartyRespond`
    /// before it expires server-side.
    PartyInviteReceived {
        inviter_id: PlayerId,
        inviter_name: String,
    },
    /// Direct to the inviter: the invite's outcome. Kept distinct from
    /// `SystemMessage` like `TradeError` so the agent-client reacts to it.
    PartyInviteResult {
        target_name: String,
        accepted: bool,
        message: String,
    },
    /// Direct to the target: a trade request to answer with
    /// `PlayerTradeRespond` before it expires server-side.
    PlayerTradeRequested {
        requester_id: PlayerId,
        requester_name: String,
    },
    /// Direct to the requester: the request's outcome.
    PlayerTradeRequestResult {
        target_name: String,
        accepted: bool,
        message: String,
    },
    /// The full session after any change, to both sides. `you` is the
    /// recipient's own side; the offered items double as the client's
    /// authoritative reservation list for greying out bag slots.
    PlayerTradeUpdate {
        state: PlayerTradeState,
    },
    /// The session is over. `completed` separates a finished swap from a
    /// cancel, an expiry, or a failed commit.
    PlayerTradeEnded {
        completed: bool,
        message: String,
    },
    /// A rejected action inside a live session (stale revision, overweight,
    /// untradeable item). The session survives; the window shows the reason.
    PlayerTradeError {
        message: String,
    },
    /// Direct to each other party member when one reads a summoning scroll:
    /// a consent request to answer with `PartySummonRespond` before it
    /// expires server-side.
    PartySummonReceived {
        caster_id: PlayerId,
        caster_name: String,
    },
    /// Direct to each member after any roster change. Empty `members` means
    /// the receiver is no longer in a party.
    PartyState {
        leader_id: PlayerId,
        members: Vec<PartyMember>,
        /// Mates may take each other's claimed loot at once.
        loot_shared: bool,
    },
    /// The whole friend roster, offline friends included. Sent at login and
    /// re-sent after any change, to both sides of it.
    FriendList {
        friends: Vec<FriendEntry>,
    },
    /// Answer to `RequestFriendsOnline`: which friends are online right now.
    /// Absence from the list is the offline signal, so a shrinking list needs
    /// no separate message.
    FriendsOnline {
        friends: Vec<OnlineFriend>,
    },
    /// Direct to the target: a friend request to answer with `FriendRespond`
    /// before it expires server-side.
    FriendRequestReceived {
        requester_id: PlayerId,
        requester_name: String,
    },
    /// Party member locations with no AOI cut — the point is members beyond
    /// it. Pushed to the whole party when a member relocates, and sent
    /// directly as the answer to `RequestPartyPositions`. Includes the
    /// recipient (one payload serves every member; clients filter
    /// themselves); empty when the requester is not in a party.
    PartyPositions {
        members: Vec<PartyMemberPosition>,
    },
    /// Party member health with no AOI cut, `PartyPositions`' twin: pushed to
    /// the whole party when a member's health changes. The roster snapshot in
    /// `PartyState` seeds the panel; this keeps it current.
    PartyVitals {
        members: Vec<PartyMemberVitals>,
    },
    GameState {
        /// A list, not a map keyed by id: `PlayerId` is numeric and
        /// `wasm_api`'s `serialize_maps_as_objects` refuses non-string keys,
        /// which would fail the whole frame. Each `Player` carries its own id
        /// anyway, so the key was redundant.
        players: Vec<Player>,
        monsters: HashMap<String, Monster>,
        #[serde(default)]
        ground_items: Vec<inventory::GroundItem>,
        #[serde(default)]
        campfires: Vec<crate::hunger::Campfire>,
        #[serde(default)]
        stalls: Vec<crate::stall::Stall>,
        #[serde(default)]
        tip_hats: Vec<crate::tip_hat::TipHat>,
        #[serde(default)]
        meals: Vec<crate::meal::Meal>,
    },
    GameTimeSync {
        datetime: GameDateTime,
        is_night: bool,
    },
    /// Sent on join, every 30 s, and when an admin changes the override.
    WeatherSync {
        seed: u64,
        bias: f32,
        sectors_tag: String,
        rain_override: Option<f32>,
        /// The forced `rain_override` falls as snow.
        snow_override: bool,
    },
    /// NPC clients only (doc/PRICING.md).
    PricingNotice(crate::pricing::PricingNotice),
    MonsterSpawned {
        monster: Monster,
    },
    MonsterMoved {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        /// Where a remote view walks the model until the next sync — a point on
        /// the mover's own path, not its destination. Aiming a viewer's straight
        /// line at the destination walks the model through the walls the path
        /// goes around. See `MonsterBrain::current_leg_target`.
        target_position: Position,
        /// Set on chase legs; viewers aim the walk at the chased player's
        /// live local position instead of the sync-old `target_position`,
        /// stopping at the carried radius.
        chasing: Option<crate::monster_ai::ChaseAim>,
    },
    MonsterRemoved {
        monster_id: String,
    },
    MonsterDead {
        monster_id: String,
        dropped_weapon_item_def_id: Option<String>,
    },
    /// HP left after a hit that did not kill.
    MonsterHealthUpdate {
        monster_id: String,
        health: u32,
        max_health: u32,
    },
    PlayerAttacked {
        player_id: PlayerId,
        monster_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
        critical: bool,
        /// The round spent, for a weapon that spends one — the clients draw
        /// this arrow rather than guessing from the shooter's bag, which they
        /// cannot see for anyone but themselves.
        ammo_item_def_id: Option<String>,
        dagger_strike: Option<u8>,
        /// Valhalla Charge's arrival blow, landing as the dash does.
        #[serde(default)]
        charge: bool,
    },
    DaggerDoubleSlashStarted {
        player_id: PlayerId,
        monster_id: String,
        cooldown_ms: u64,
    },
    DaggerDoubleSlashRejected {
        monster_id: String,
        reason: String,
        cooldown_ms: u64,
    },
    DaggerDoubleSlashSkipped {
        player_id: PlayerId,
        monster_id: String,
        strike: u8,
        reason: String,
    },
    EquipmentEnchantSucceeded {
        player_id: PlayerId,
        weapon: bool,
    },
    /// Nearby celebration; the leveling player learns from `XpGained`.
    PlayerLeveledUp {
        player_id: PlayerId,
    },
    /// Direct ack to the attacker for a dropped `PlayerAttack` request, so a
    /// rejection is distinguishable from packet loss.
    PlayerAttackRejected {
        monster_id: String,
        reason: AttackRejectReason,
    },
    PlayerAttackStopped {
        monster_id: String,
        request_id: u32,
    },
    /// A telegraphed skill began; its damage arrives later in one
    /// `MonsterSkillStruck` for the players still in its area. A zone
    /// skill sends a `half_angle` of pi.
    MonsterSkillStarted {
        monster_id: String,
        skill: crate::monster_ai::MonsterSkillId,
        rotation: f32,
        range: f32,
        half_angle: f32,
        /// Ground the cast marked, each `spot_radius` across; empty for most.
        spots: Vec<crate::Position>,
        spot_radius: f32,
    },
    /// A skill's strike, once for everyone it hit; misses are not listed.
    /// Each victim's `PlayerHealthUpdate` and death still go out on their own.
    /// `burn` is a tick of the magma it left behind.
    MonsterSkillStruck {
        monster_id: String,
        skill: crate::monster_ai::MonsterSkillId,
        victims: Vec<SkillVictim>,
        burn: bool,
    },
    /// A plain swing at a distant target: the ground where it stood erupts
    /// after `impact_ms`, striking whoever is still within `radius`.
    MonsterGroundStrike {
        monster_id: String,
        target_player_id: PlayerId,
        position: Position,
        floor_level: i8,
        radius: f32,
        impact_ms: u32,
    },
    MonsterAttackedPlayer {
        monster_id: String,
        player_id: PlayerId,
        hit: bool,
        roll: u8,
        damage: u32,
        /// Hidden like `Player::health`.
        current_health: u32,
        /// Struck by a `MonsterGroundStrike` eruption, already shown.
        ranged: bool,
    },
    PlayerDead {
        player_id: PlayerId,
    },
    PlayerRespawned {
        player: Player,
    },
    ManaUpdate {
        mana: u32,
        max_mana: u32,
    },
    /// The owner's Primal Roar shield; 0 when it is spent or gone.
    TempHealthUpdate {
        amount: u32,
    },
    /// Viewers without HP access get `health` 1 while alive (0 when dead) and
    /// `max_health` 0; see `Player::health`.
    PlayerHealthUpdate {
        player_id: PlayerId,
        health: u32,
        max_health: u32,
    },
    XpGained {
        player_id: PlayerId,
        xp_amount: u32,
        xp_lost: u64,
        total_xp: u64,
        new_level: u32,
        leveled_up: bool,
        max_hp: u32,
        current_hp: u32,
        /// The kill this XP came from, so a client can hold the gain until that
        /// monster starts its death animation. `None` for the death penalty.
        monster_id: Option<String>,
    },
    /// Private learned skills, sent on login and acquisition.
    SkillsUpdate {
        skills: skills::Skills,
    },
    /// A player's cast landed: render their bobber at `position`. Broadcast
    /// nearby (the caster included) so fishing is visible to passers-by.
    /// `rotation` preserves the rowboat heading; on foot it faces the cast.
    /// Carried here so bystanders need not wait for a face-turn packet.
    FishingCasted {
        player_id: PlayerId,
        position: Position,
        rotation: f32,
    },
    /// The bobber dipped — the angler has the shared bite window (plus
    /// latency grace, judged server-side) to send `Hook`.
    FishingBite {
        player_id: PlayerId,
    },
    /// Public 250 ms fight beat, including the angler's reel motion.
    FishingFight {
        player_id: PlayerId,
        bobber: Position,
        fish_state: fishing::FishState,
        stance: fishing::FishingAction,
        tension_pct: u32,
        stamina_pct: u32,
        /// Rolled at the bite; trophy fish need sustained high tension.
        trophy: bool,
    },
    /// The session is over: despawn the bobber and, for the angler, show the
    /// outcome. A caught fish also arrives via the normal `InventoryUpdated`
    /// (or `GroundItemSpawned` when the bag couldn't take the weight).
    FishingEnded {
        player_id: PlayerId,
        outcome: fishing::FishingOutcome,
    },
    /// Direct: a fishing request was refused (no rod, not water, too far…).
    /// Mirrors `InventoryError`.
    FishingError {
        message: String,
    },
    Kicked {
        player_id: PlayerId,
        reason: String,
    },
    ServerNotice {
        message: Option<String>,
    },
    ServerAnnouncement {
        message: String,
    },
    /// Direct operator notice (see doc/IP_NOTICES.md); clients keep it on
    /// screen until the player confirms it.
    OperatorNotice {
        message: String,
    },
    PlayerTorchToggled {
        player_id: PlayerId,
        enabled: bool,
    },
    PlayerMountChanged {
        player_id: PlayerId,
        mount: Option<crate::mount::MountKind>,
    },
    /// The `wet` soaking went up or came off this player (doc/DEBUFF.md).
    /// Cosmetic — only the footprint trail reads it.
    PlayerWetToggled {
        player_id: PlayerId,
        wet: bool,
    },
    /// This player turned to stone or came out of it (doc/DEBUFF.md
    /// `petrifies`). Sent to the owner too.
    PlayerPetrifiedToggled {
        player_id: PlayerId,
        petrified: bool,
    },
    /// A nearby player's shown title changed (doc/TITLES.md).
    PlayerTitleChanged {
        player_id: PlayerId,
        title: Option<String>,
    },
    /// The recipient earned a title; sent to them alone.
    TitleEarned {
        title: String,
    },
    /// The recipient's own title list and active pick, on entry and after
    /// every change.
    PlayerTitles {
        titles: Vec<String>,
        active: Option<String>,
    },
    /// The client asked to use a cape dye and may open its colour picker:
    /// there is a cape on to dye and the dye is in the bag. The server keeps
    /// no pending state — `DyeCape` re-checks everything.
    CapeDyePrompt {
        instance_id: u64,
    },
    LandClaimPrompt {
        instance_id: u64,
        tile_x: i32,
        tile_z: i32,
        quadrant: u8,
        reason: Option<String>,
    },
    LandscapingMode {
        owner_id: i64,
        owned_character_ids: Vec<i64>,
        plots: Vec<crate::fence::FencePlot>,
        palette: Vec<u8>,
        has_toolbox: bool,
        tool: crate::landscaping::LandscapingTool,
    },
    LandscapingPaletteUnlocked {
        palette: Vec<u8>,
    },
    LandscapeChanged {
        tiles: Vec<crate::landscaping::LandscapingTile>,
    },
    LandscapeInvalidated {
        tiles: Vec<(i32, i32)>,
    },
    LandscapeEditResult {
        error: Option<String>,
    },
    FenceVisibility {
        added: Vec<crate::fence::Fence>,
        removed: Vec<crate::fence::FenceEdge>,
    },
    FenceEditResult {
        error: Option<String>,
    },
    EstateChestMode {
        instance_id: u64,
        item_def_id: String,
        owner_id: i64,
        plots: Vec<crate::fence::FencePlot>,
    },
    EstateFurnitureMoveMode {
        furniture: crate::estate_storage::EstateChest,
        plots: Vec<crate::fence::FencePlot>,
    },
    EstateChestVisibility {
        added: Vec<crate::estate_storage::EstateChest>,
        removed: Vec<i64>,
    },
    EstateChestEditResult {
        error: Option<String>,
    },
    EstateChestState {
        state: Option<crate::estate_storage::EstateChestState>,
        error: Option<String>,
    },
    FurniturePurchaseResult {
        error: Option<String>,
    },
    LandClaimed {
        estate_id: i64,
        tile_x: i32,
        tile_z: i32,
        quadrant: u8,
    },
    LandRejected {
        reason: String,
    },
    LandAccountState {
        merchant_player_id: PlayerId,
        treasury: i64,
        plots: u32,
        monthly_tax: i64,
        next_tax: i64,
        next_due: crate::GameDateTime,
        due_in_seconds: u64,
        missed: u32,
        recovery_cost: i64,
        free_months: u32,
        error: Option<String>,
    },
    /// Same for a cape transfer kit: a cape is on and the kit is in the bag,
    /// so the client may open its image picker. Nothing is spent until
    /// `ApplyCapeTexture`.
    CapeTexturePrompt {
        instance_id: u64,
    },
    /// A player's equipped main-hand item changed; `None` reverts remote
    /// rendering to the class default weapon.
    PlayerMainHandChanged {
        player_id: PlayerId,
        item_def_id: Option<String>,
    },
    /// A player's equipped back item changed; `None` removes the cape from
    /// remote rendering. `cape_color` is the dye on that instance, if any —
    /// re-dyeing sends this with an unchanged `item_def_id`.
    PlayerBackChanged {
        player_id: PlayerId,
        item_def_id: Option<String>,
        #[serde(default)]
        cape_color: Option<String>,
        #[serde(default)]
        cape_texture: Option<String>,
    },
    PlayerArmorChanged {
        player_id: PlayerId,
        armor: crate::character::ArmorEquipment,
    },
    PlayerInteractionChanged {
        player_id: PlayerId,
        object_type: Option<String>,
        /// Furniture placement ID; None for emotes.
        #[serde(default)]
        object_id: Option<u32>,
        position: Position,
        rotation: f32,
        floor_level: i8,
    },
    /// A player started a `/play_music` performance; nearby clients play the
    /// named BGM track. `track` is the title the server resolved from its
    /// registry — receivers play it only if their own BGM list has it.
    /// The performance ends with the emote (`PlayerInteractionChanged` /
    /// [`MUSIC_EMOTE`] giving way to anything else). Also sent to a player
    /// who comes into earshot mid-performance, with `elapsed_secs` saying
    /// how far in the tune already is.
    PlayerMusicStarted {
        player_id: PlayerId,
        track: String,
        #[serde(default)]
        elapsed_secs: f32,
    },
    PlayerInstrumentStarted {
        player_id: PlayerId,
    },
    PlayerInstrumentNotes {
        player_id: PlayerId,
        position: Position,
        floor_level: i8,
        events: Vec<InstrumentNoteEvent>,
    },
    InteractionRejected {
        reason: String,
    },
    AltarStatus {
        object_id: u32,
        status: AltarStatus,
    },
    NoticeBoard {
        board: crate::notice_board::NoticeBoardRef,
        posts: Vec<crate::notice_board::NoticePostSummary>,
    },
    NoticePostView {
        board: crate::notice_board::NoticeBoardRef,
        post: crate::notice_board::NoticePost,
        comments: Vec<crate::notice_board::NoticeComment>,
    },
    NoticeBoardRefused {
        reason: crate::notice_board::NoticeBoardRefusal,
    },
    NoticeBoardPapers {
        board: crate::notice_board::NoticeBoardRef,
        papers: Vec<crate::notice_board::NoticePaper>,
    },
    /// Sent to the registrar after a take-down, then the refreshed `NoticeBoard`.
    NoticesTakenDown {
        object_id: u32,
        count: u32,
    },
    NoticePostRemoved {
        board: crate::notice_board::NoticeBoardRef,
        post_id: i64,
    },
    Mailbox {
        mailbox: crate::mail::MailboxRef,
        letters: Vec<crate::mail::LetterSummary>,
    },
    MailLetter {
        mailbox: crate::mail::MailboxRef,
        letter: crate::mail::Letter,
    },
    MailSent {
        recipient: String,
    },
    MailRefused {
        reason: crate::mail::MailRefusal,
    },
    /// Unread letters waiting; sent on entering the game and when one arrives.
    MailNotice {
        unread: u32,
    },
    FieldRaidNotice {
        monster: String,
        outcome: FieldRaidOutcome,
        /// Item granted to this recipient for the raid.
        reward: Option<String>,
        /// Raid token item and how many this recipient got.
        tokens: Option<(String, u32)>,
    },
    /// An offering took; the boss rises from its dormant stand-in at
    /// `position` once `wake_in_ms` has passed.
    FieldBossWaking {
        monster: String,
        position: Position,
        wake_in_ms: u32,
    },
    /// The raid failed and the boss at `position` sinks back into its
    /// dormant stand-in; its removal follows.
    FieldBossSleeping {
        monster_id: String,
        monster: String,
        position: Position,
    },
    HouseSpawned {
        house: housing::HouseData,
    },
    HousePlacementStarted {
        instance_id: u64,
        item_name: String,
        house: housing::HouseData,
        plots: Vec<crate::fence::FencePlot>,
    },
    HousePlacementResult {
        error: Option<String>,
    },
    HouseDemolitionResult {
        house_id: String,
        error: Option<String>,
    },
    HouseUpdated {
        house: housing::HouseData,
    },
    HeightTilesInvalidated {
        tiles: Vec<(i32, i32)>,
    },
    TreeTilesInvalidated {
        tiles: Vec<(i32, i32)>,
    },
    GrassTilesInvalidated {
        tiles: Vec<(i32, i32)>,
    },
    HouseRemoved {
        house_id: String,
    },
    HousesInArea {
        houses: Vec<housing::HouseData>,
    },
    /// Live state of the door named by a `door::DoorId`; `None` once it is
    /// out of view or gone. Doors are shut until told otherwise.
    DoorState {
        door: String,
        is_open: Option<bool>,
    },
    /// Sent once on join: full inventory state.
    InventoryState {
        inventory: inventory::PlayerInventory,
    },
    /// Sent after any inventory mutation.
    InventoryUpdated {
        inventory: inventory::PlayerInventory,
    },
    /// A new item was created on the ground. Sent when the item becomes real,
    /// so a client spawns it on arrival: a dying monster's loot is held back
    /// server-side until the killing blow lands.
    GroundItemSpawned {
        item: inventory::GroundItem,
    },
    /// An existing ground item became visible to the client.
    GroundItemAppeared {
        item: inventory::GroundItem,
    },
    /// A ground item was picked up, despawned, or left the client's view.
    GroundItemRemoved {
        instance_id: u64,
        /// Who picked it up, when someone did — `None` for a despawn or an
        /// item that merely dropped out of range.
        picked_up_by: Option<PlayerId>,
    },
    /// A pile shrank without emptying: someone took part of it, having been
    /// able to carry only some of the units.
    GroundItemQuantityChanged {
        instance_id: u64,
        quantity: u32,
        /// Who took the units, for the loot line in chat; clients derive the
        /// taken count from the quantity they had cached.
        picked_up_by: Option<PlayerId>,
    },
    /// A claimed item's 30 seconds ran out: anyone may pick it up now.
    GroundItemClaimEnded {
        instance_id: u64,
    },
    /// Response to OpenShop (or pushed by an NPC's OpenTrade): the trader's
    /// goods. Display prices come from item definitions; the server
    /// re-validates them on Buy/Sell.
    ShopState {
        merchant_player_id: PlayerId,
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
        /// Units this player recently sold to this merchant, repurchasable
        /// at the recorded payout. Empty for non-merchants, whose bought
        /// units stay visible in `stock`.
        #[serde(default)]
        buyback: Vec<BuybackEntry>,
        /// Consumable buy-price index, 100 = base; residents send 100.
        #[serde(default = "default_price_index_percent")]
        price_index_percent: u32,
    },
    /// Direct message: the receiving player's current gold (smallest unit).
    GoldUpdate {
        gold: i64,
    },
    /// Direct message: the receiving player's effective stats (base attribute
    /// plus equipped-gear bonuses) — the exact numbers combat and haggling use.
    /// Sent on join and after any equipment change so the client never
    /// duplicates the server formula.
    EffectiveStatsUpdated {
        guard: i32,
        cha: i32,
        int: i32,
        /// Gear's share of max HP, already counted in `max_health`.
        max_hp_bonus: u32,
        critical_chance_percent: u8,
    },
    /// Direct message: the receiving player gained loose currency from a
    /// pickup. `amount` is in the smallest unit (copper).
    GoldGained {
        amount: i64,
    },
    /// A shop request failed. Kept distinct from `SystemMessage`: the
    /// agent-client reacts urgently to a failed trade.
    TradeError {
        message: String,
        #[serde(default)]
        localization: Option<LocalizedMessage>,
    },
    /// Direct to a player: a haggled price modifier changed on one item.
    /// `modifier_pct == 0` means the deal was consumed or cleared.
    DealUpdated {
        merchant_player_id: PlayerId,
        item_def_id: String,
        kind: DealKind,
        modifier_pct: i32,
        expires_in_secs: u32,
    },
    /// Direct to a player: its buyback list with one merchant changed (a
    /// sell added an entry, or a buyback consumed one).
    BuybackUpdated {
        merchant_player_id: PlayerId,
        buyback: Vec<BuybackEntry>,
    },
    /// Direct to a trading NPC: whether at least one player currently has its
    /// trade window open. While `busy` is true the NPC's LLM keeps its place
    /// (movement is suppressed) so it doesn't wander off mid-trade; it can
    /// still talk and haggle.
    TradeBusy {
        busy: bool,
    },
    /// A step of a family merger rite, delivered only to the officiant NPC.
    MergerRiteCue {
        cue: String,
    },
    /// Direct to a family head: the officiant holds out the pledge form.
    MergerPledgeOffered {
        officiant: String,
    },
    /// Answer to `InspectFamilyEmblem`.
    FamilyEmblemShown {
        player_name: String,
        emblem: crate::family::Emblem,
    },
    /// A merger was sworn at the altar: petals and bells for everyone nearby.
    MergerCelebrated {
        position: Position,
        floor_level: i8,
    },
    /// An unpaid showroom selection, delivered only to the clerk.
    FurnitureSelectionNotice {
        player_id: PlayerId,
        player_name: String,
        item_def_id: String,
    },
    /// Completed trade; Buy means the player bought from the NPC.
    TradeNotice {
        player_name: String,
        item_def_id: String,
        kind: DealKind,
        /// Price in the merchant's currency; gold uses the smallest unit.
        price: i64,
        /// The NPC's wallet after the trade.
        npc_gold: i64,
    },
    /// Direct to a merchant NPC: the named player waved off its pushed
    /// trade window ("Not now", or the offer toast expired). The agent
    /// suppresses trade pushes at them for a cooldown.
    TradeDeclined {
        player_id: PlayerId,
        player_name: String,
    },
    /// Direct to the offering NPC: the server's verdict on its `OfferDeal`.
    DealResult {
        target_player_id: PlayerId,
        target_player_name: String,
        item_def_id: String,
        kind: DealKind,
        accepted: bool,
        /// The modifier actually in effect (after band clamping); 0 when
        /// rejected.
        applied_modifier_pct: i32,
        message: String,
    },
    /// Direct to the owner only (exact satiation is private, doc/HUNGER.md).
    /// Sent on band transitions, eating and debuff changes — not on every
    /// decay tick. Carries the effective multipliers (hunger × debuffs) so
    /// the client never re-derives them.
    HungerUpdate {
        satiation: u32,
        state: crate::hunger::HungerState,
        move_mult: f32,
        attack_mult: f32,
        carry_mult: f32,
    },
    /// Direct to the owner only: the full list of active debuffs, sent when
    /// one is applied, refreshed or expires (doc/DEBUFF.md).
    DebuffUpdate {
        debuffs: Vec<crate::debuff::ActiveDebuffState>,
    },
    /// A campfire was just lit nearby (play the ignition, not just appear).
    CampfireSpawned {
        campfire: crate::hunger::Campfire,
    },
    /// An already-burning campfire entered the receiver's AOI.
    CampfireAppeared {
        campfire: crate::hunger::Campfire,
    },
    /// Burned out or left the receiver's AOI.
    CampfireRemoved {
        campfire_id: u64,
    },
    /// A merchant just laid out a stall nearby.
    StallPlaced {
        stall: crate::stall::Stall,
    },
    /// An already-laid stall entered the receiver's AOI.
    StallAppeared {
        stall: crate::stall::Stall,
    },
    /// Packed up or left the receiver's AOI.
    StallRemoved {
        stall_id: u64,
    },
    /// The whole listing state of the stall the receiver has open. Sent on
    /// open and re-sent whole on every change: a stale panel is what makes a
    /// customer click for goods somebody else already took.
    StallState {
        stall_id: u64,
        owner_name: String,
        sign: String,
        listings: Vec<crate::stall::StallListing>,
        /// The receiver owns this stall, so the panel manages instead of buys.
        owned: bool,
        buy_orders: Vec<crate::stall::StallBuyOrder>,
    },
    /// A stall in the receiver's AOI changed its sign board.
    StallSignChanged {
        stall_id: u64,
        sign: String,
    },
    /// A performer just set a tip hat down nearby.
    TipHatPlaced {
        tip_hat: crate::tip_hat::TipHat,
    },
    /// An already-placed tip hat entered the receiver's AOI.
    TipHatAppeared {
        tip_hat: crate::tip_hat::TipHat,
    },
    /// Picked up, left behind by its owner, or left the receiver's AOI.
    TipHatRemoved {
        tip_hat_id: u64,
    },
    SongRequested {
        requester_name: String,
        track: String,
    },
    /// A maid just set a dish down on a table nearby.
    MealPlaced {
        meal: crate::meal::Meal,
    },
    /// An already-served dish entered the receiver's AOI.
    MealAppeared {
        meal: crate::meal::Meal,
    },
    /// The guest finished it; the empty plate stays until cleared.
    MealEaten {
        meal_id: u64,
    },
    /// Cleared, expired, or left the receiver's AOI.
    MealRemoved {
        meal_id: u64,
    },
    /// Direct to the griller: the 3s grill cast began.
    GrillStarted,
    /// Direct to the griller. `grilled_item_def_id` is None when the cast was
    /// cancelled (movement, combat, the fire burning out). The grilled item
    /// itself arrives through the normal `InventoryUpdated`.
    GrillEnded {
        grilled_item_def_id: Option<String>,
    },
    /// Direct to each dungeon occupant before the sunset reset puts them out.
    DungeonReset,
    /// `(entrance id, variant)` for `dungeon::generate_dungeon_variant`. Sent
    /// in full on join and to everyone when a sunset reset redraws a maze;
    /// an unlisted dungeon keeps variant 0.
    DungeonLayoutVariants {
        variants: Vec<(String, u32)>,
    },
    /// Nearby: a player began mining a prop. The mined prop arrives as
    /// `DungeonPropBroken`; `MiningEnded` stops the swing either way.
    MiningStarted {
        player_id: PlayerId,
    },
    /// `found` names what a finished cast turned up from prop `prop_id`
    /// (item id, quantity), so nearby clients float it over the prop.
    MiningEnded {
        player_id: PlayerId,
        prop_id: u32,
        found: Option<(String, u32)>,
    },
    AbilityCooldowns {
        cooldowns: Vec<crate::ability::AbilityTimer>,
    },
    BuffUpdate {
        buffs: Vec<crate::ability::AbilityTimer>,
    },
    AbilityRejected {
        ability: crate::ability::AbilityId,
        reason: crate::ability::AbilityRejectReason,
    },
    InspectionResult {
        inspection: crate::ability::InspectionResult,
    },
    AbilityUsed {
        ability: crate::ability::AbilityId,
        player_id: PlayerId,
        position: Position,
        floor_level: i8,
        targets: Vec<PlayerId>,
    },
    PlayerRadianceToggled {
        player_id: PlayerId,
        enabled: bool,
    },
    PlayerRageToggled {
        player_id: PlayerId,
        enabled: bool,
    },
    /// Primal Roar's stagger, for the dizzy marker over each monster.
    MonsterStunned {
        monster_ids: Vec<String>,
        duration_ms: u64,
    },
    /// Valhalla Charge's slow; viewers walk the monster at `move_mult`.
    MonsterSlowed {
        monster_id: String,
        move_mult: f32,
        duration_ms: u64,
    },
    /// Healing Prayer landed: `amount` HP restored to the player, or damage
    /// dealt to the undead monster. The new HP follows on its own.
    HealingPrayed {
        player_id: PlayerId,
        target_player_id: Option<PlayerId>,
        monster_id: Option<String>,
        amount: u32,
    },
    /// A bard began Battle Hymn's 10 s: viewers stop that bard's tune and
    /// play march `tune`, picked here so everyone hears the same one.
    /// Leaving the strum stops it.
    BattleHymnStarted {
        player_id: PlayerId,
        position: Position,
        floor_level: i8,
        tune: u8,
    },
    BowMarkUpdate {
        monster_id: Option<String>,
        remaining_ms: u64,
    },
    /// Direct to the wearer: the invisibility cloak hid or revealed them.
    InvisibilityChanged {
        invisible: bool,
    },
    /// The recipient's family, or `None` when they have none. Sent on entry
    /// and after every change (doc/FAMILY_SYSTEM.md).
    FamilyState {
        family: Option<crate::family::FamilyInfo>,
    },
    /// Direct to the target: an invite to answer with `RespondFamilyInvite`.
    FamilyInviteReceived {
        inviter_id: PlayerId,
        inviter_name: String,
        family_name: String,
    },
    /// A nearby player's family name changed.
    PlayerFamilyChanged {
        player_id: PlayerId,
        family: Option<String>,
    },
    /// A family-channel line, sent to every online member, the sender included.
    FamilyChatMessage {
        from: String,
        message: String,
    },
    /// The family's newest chest movements, newest first.
    FamilyStorageLog {
        entries: Vec<crate::family::FamilyStorageEntry>,
    },
    /// Direct to a head: a merge offer to answer with `RespondFamilyMerge`.
    FamilyMergeProposal {
        proposal: crate::family::FamilyMergeProposal,
    },
}

pub use crate::entity::PlayerId;

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

fn default_price_index_percent() -> u32 {
    100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryClass {
    NearbyState,
    NearbyEffect,
    Terrain,
    Participants,
    Global,
    Control,
}

impl ServerMessage {
    /// Players whose HP this message carries; see `Player::health`.
    pub fn health_owners(&self) -> Vec<PlayerId> {
        match self {
            Self::PlayerJoined { player }
            | Self::PlayerAppeared { player }
            | Self::PlayerRespawned { player } => vec![player.id],
            Self::PlayerHealthUpdate { player_id, .. }
            | Self::MonsterAttackedPlayer { player_id, .. } => vec![*player_id],
            Self::MonsterSkillStruck { victims, .. } => {
                victims.iter().map(|victim| victim.player_id).collect()
            }
            _ => vec![],
        }
    }

    /// Leave `viewer` only whether other players are alive, not their HP.
    /// Covers the same messages as `health_owners`.
    pub fn hide_health(&mut self, viewer: PlayerId) {
        let alive = |health: &mut u32| *health = (*health).min(1);
        match self {
            Self::PlayerJoined { player }
            | Self::PlayerAppeared { player }
            | Self::PlayerRespawned { player }
                if player.id != viewer =>
            {
                alive(&mut player.health);
                player.max_health = 0;
            }
            Self::PlayerHealthUpdate {
                player_id,
                health,
                max_health,
            } if *player_id != viewer => {
                alive(health);
                *max_health = 0;
            }
            Self::MonsterAttackedPlayer {
                player_id,
                current_health,
                ..
            } if *player_id != viewer => alive(current_health),
            Self::MonsterSkillStruck { victims, .. } => {
                for victim in victims.iter_mut().filter(|v| v.player_id != viewer) {
                    alive(&mut victim.current_health);
                }
            }
            _ => {}
        }
    }

    pub fn delivery_class(&self) -> DeliveryClass {
        match self {
            Self::DoorState { .. }
            | Self::DungeonPropState { .. }
            | Self::PlayerJoined { .. }
            | Self::PlayerLeft { .. }
            | Self::PlayerAppeared { .. }
            | Self::PlayerDisappeared { .. }
            | Self::PlayerMoved { .. }
            | Self::PlayerTeleported { .. }
            | Self::PlayerCharged { .. }
            | Self::DungeonPropBroken { .. }
            | Self::DungeonCartPushed { .. }
            | Self::DungeonPropOpened { .. }
            | Self::DungeonPropsState { .. }
            | Self::GameState { .. }
            | Self::MonsterSpawned { .. }
            | Self::MonsterMoved { .. }
            | Self::MonsterRemoved { .. }
            | Self::MonsterDead { .. }
            | Self::MonsterHealthUpdate { .. }
            | Self::PlayerDead { .. }
            | Self::PlayerRespawned { .. }
            | Self::PlayerHealthUpdate { .. }
            | Self::FishingCasted { .. }
            | Self::FishingBite { .. }
            | Self::FishingFight { .. }
            | Self::FishingEnded { .. }
            | Self::PlayerTorchToggled { .. }
            | Self::PlayerMountChanged { .. }
            | Self::PlayerWetToggled { .. }
            | Self::PlayerPetrifiedToggled { .. }
            | Self::PlayerTitleChanged { .. }
            | Self::PlayerFamilyChanged { .. }
            | Self::FenceVisibility { .. }
            | Self::EstateChestVisibility { .. }
            | Self::PlayerMainHandChanged { .. }
            | Self::PlayerBackChanged { .. }
            | Self::PlayerArmorChanged { .. }
            | Self::PlayerInteractionChanged { .. }
            | Self::PlayerMusicStarted { .. }
            | Self::PlayerInstrumentStarted { .. }
            | Self::HouseSpawned { .. }
            | Self::HouseUpdated { .. }
            | Self::HouseRemoved { .. }
            | Self::HousesInArea { .. }
            | Self::GroundItemSpawned { .. }
            | Self::GroundItemAppeared { .. }
            | Self::GroundItemRemoved { .. }
            | Self::GroundItemQuantityChanged { .. }
            | Self::GroundItemClaimEnded { .. }
            | Self::CampfireSpawned { .. }
            | Self::CampfireAppeared { .. }
            | Self::CampfireRemoved { .. }
            | Self::StallPlaced { .. }
            | Self::StallAppeared { .. }
            | Self::StallRemoved { .. }
            | Self::StallSignChanged { .. }
            | Self::TipHatPlaced { .. }
            | Self::TipHatAppeared { .. }
            | Self::TipHatRemoved { .. }
            | Self::MealPlaced { .. }
            | Self::MealAppeared { .. }
            | Self::MealEaten { .. }
            | Self::MealRemoved { .. }
            | Self::PlayerRadianceToggled { .. }
            | Self::PlayerRageToggled { .. } => DeliveryClass::NearbyState,
            Self::PlayerTeleportEffect { .. }
            | Self::DungeonChestOpened { .. }
            | Self::ChatMessage { .. }
            | Self::Recital { .. }
            | Self::PlayerAttacked { .. }
            | Self::DaggerDoubleSlashStarted { .. }
            | Self::DaggerDoubleSlashSkipped { .. }
            | Self::EquipmentEnchantSucceeded { .. }
            | Self::PlayerLeveledUp { .. }
            | Self::MergerCelebrated { .. }
            | Self::MonsterSkillStarted { .. }
            | Self::MonsterSkillStruck { .. }
            | Self::MonsterGroundStrike { .. }
            | Self::MonsterAttackedPlayer { .. }
            | Self::PlayerInstrumentNotes { .. }
            | Self::AbilityUsed { .. }
            | Self::MonsterStunned { .. }
            | Self::MonsterSlowed { .. }
            | Self::HealingPrayed { .. }
            | Self::BattleHymnStarted { .. }
            | Self::MiningStarted { .. }
            | Self::MiningEnded { .. }
            | Self::CartStruckPlayer { .. } => DeliveryClass::NearbyEffect,
            Self::TerrainTileVersion { .. }
            | Self::LandscapeChanged { .. }
            | Self::LandscapeInvalidated { .. }
            | Self::HeightTilesInvalidated { .. }
            | Self::TreeTilesInvalidated { .. }
            | Self::GrassTilesInvalidated { .. } => DeliveryClass::Terrain,
            Self::AuthSuccess { .. }
            | Self::JoinSuccess { .. }
            | Self::AuthError { .. }
            | Self::CharacterCreated { .. }
            | Self::CharacterStatsRolled { .. }
            | Self::CharacterDeleted { .. }
            | Self::CharacterDeletionScheduled { .. }
            | Self::CharacterDeletionCancelled { .. }
            | Self::CharacterRenameRequired { .. }
            | Self::CharacterRenamed { .. }
            | Self::CharacterError { .. }
            | Self::DungeonDiscoveries { .. }
            | Self::WhisperMessage { .. }
            | Self::SystemMessage { .. }
            | Self::OperatorNotice { .. }
            | Self::SongRequested { .. }
            | Self::PartyChatMessage { .. }
            | Self::FamilyChatMessage { .. }
            | Self::PartyInviteReceived { .. }
            | Self::PartyInviteResult { .. }
            | Self::PlayerTradeRequested { .. }
            | Self::PlayerTradeRequestResult { .. }
            | Self::PlayerTradeUpdate { .. }
            | Self::PlayerTradeEnded { .. }
            | Self::PlayerTradeError { .. }
            | Self::PartySummonReceived { .. }
            | Self::PartyState { .. }
            | Self::FriendList { .. }
            | Self::FriendsOnline { .. }
            | Self::FriendRequestReceived { .. }
            | Self::FamilyState { .. }
            | Self::FamilyStorageLog { .. }
            | Self::FamilyMergeProposal { .. }
            | Self::FamilyInviteReceived { .. }
            | Self::PartyPositions { .. }
            | Self::PartyVitals { .. }
            | Self::PricingNotice(..)
            | Self::DaggerDoubleSlashRejected { .. }
            | Self::PlayerAttackRejected { .. }
            | Self::PlayerAttackStopped { .. }
            | Self::ManaUpdate { .. }
            | Self::TempHealthUpdate { .. }
            | Self::XpGained { .. }
            | Self::SkillsUpdate { .. }
            | Self::FishingError { .. }
            | Self::Kicked { .. }
            | Self::TitleEarned { .. }
            | Self::PlayerTitles { .. }
            | Self::InvisibilityChanged { .. }
            | Self::CapeDyePrompt { .. }
            | Self::LandClaimPrompt { .. }
            | Self::LandscapingMode { .. }
            | Self::LandscapingPaletteUnlocked { .. }
            | Self::LandscapeEditResult { .. }
            | Self::FenceEditResult { .. }
            | Self::EstateChestMode { .. }
            | Self::EstateFurnitureMoveMode { .. }
            | Self::EstateChestEditResult { .. }
            | Self::EstateChestState { .. }
            | Self::FurniturePurchaseResult { .. }
            | Self::LandClaimed { .. }
            | Self::LandRejected { .. }
            | Self::LandAccountState { .. }
            | Self::CapeTexturePrompt { .. }
            | Self::InteractionRejected { .. }
            | Self::AltarStatus { .. }
            | Self::NoticeBoard { .. }
            | Self::NoticePostView { .. }
            | Self::NoticeBoardRefused { .. }
            | Self::NoticeBoardPapers { .. }
            | Self::NoticesTakenDown { .. }
            | Self::NoticePostRemoved { .. }
            | Self::Mailbox { .. }
            | Self::MailLetter { .. }
            | Self::MailSent { .. }
            | Self::MailRefused { .. }
            | Self::MailNotice { .. }
            | Self::FieldRaidNotice { .. }
            | Self::FieldBossWaking { .. }
            | Self::FieldBossSleeping { .. }
            | Self::HousePlacementStarted { .. }
            | Self::HousePlacementResult { .. }
            | Self::HouseDemolitionResult { .. }
            | Self::InventoryState { .. }
            | Self::InventoryUpdated { .. }
            | Self::ShopState { .. }
            | Self::GoldUpdate { .. }
            | Self::EffectiveStatsUpdated { .. }
            | Self::GoldGained { .. }
            | Self::TradeError { .. }
            | Self::DealUpdated { .. }
            | Self::BuybackUpdated { .. }
            | Self::TradeBusy { .. }
            | Self::TradeNotice { .. }
            | Self::FurnitureSelectionNotice { .. }
            | Self::MergerRiteCue { .. }
            | Self::MergerPledgeOffered { .. }
            | Self::FamilyEmblemShown { .. }
            | Self::TradeDeclined { .. }
            | Self::DealResult { .. }
            | Self::PlayerMovePath { .. }
            | Self::PlayerMoveProgress { .. }
            | Self::HungerUpdate { .. }
            | Self::DebuffUpdate { .. }
            | Self::StallState { .. }
            | Self::GrillStarted
            | Self::GrillEnded { .. }
            | Self::DungeonReset
            | Self::AbilityCooldowns { .. }
            | Self::BuffUpdate { .. }
            | Self::AbilityRejected { .. }
            | Self::InspectionResult { .. }
            | Self::BowMarkUpdate { .. } => DeliveryClass::Participants,
            Self::GameTimeSync { .. }
            | Self::DungeonLayoutVariants { .. }
            | Self::WeatherSync { .. }
            | Self::ServerNotice { .. }
            | Self::ServerAnnouncement { .. } => DeliveryClass::Global,
            Self::WorldUpdate { .. } => DeliveryClass::Control,
        }
    }
}
