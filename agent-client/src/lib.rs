/// This build's release version; tags are `agent-client-v<VERSION>`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod bgm_defs;
pub mod claude;
pub mod codex;
pub mod driver;
pub mod dungeon;
pub mod geom;
pub mod google_auth;
pub mod item_defs;
pub mod llm_scheduler;
pub mod minimax;
pub mod monster_defs;
pub mod openai;
pub mod openrouter;
pub mod orchestrator;
pub mod process_group;
pub mod session;
pub mod shop_info;
pub mod splat;
pub mod state;
pub mod tales;
pub mod terrain_http;
pub mod terrain_snapshots;
pub mod title_defs;
pub mod transcript;
pub mod update;
pub mod watch;
pub mod weather;
pub mod ws;

use serde::Deserialize;

/// Which LLM backend to use for the agent driver.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmType {
    /// No LLM driver: schedule and monster AI drive the NPC on their own.
    #[default]
    None,
    /// Claude CLI (stdio subprocess)
    Claude,
    /// OpenRouter API (HTTP)
    Openrouter,
    /// Codex CLI (stdio subprocess)
    Codex,
    /// Any OpenAI-compatible chat completions endpoint (HTTP)
    Openai,
    /// MiniMax API (HTTP)
    Minimax,
}

pub fn is_http_source(terrain: &str) -> bool {
    terrain.starts_with("http://") || terrain.starts_with("https://")
}

pub fn default_min_interval_secs() -> u64 {
    5
}

pub fn default_urgent_min_interval_secs() -> u64 {
    2
}

// No debounce by default: the pacing floors already batch a stream of
// events (and an in-flight call batches whatever lands during it), so a
// leading debounce only delayed the first response. Set debounce_secs per
// NPC to re-merge sub-second bursts if an NPC ever needs it.
pub fn default_debounce_secs() -> u64 {
    0
}

pub fn default_idle_interval_secs() -> u64 {
    3600
}

pub fn default_activity_window_secs() -> u64 {
    30
}

pub fn msg_name(msg: &onlinerpg_shared::ServerMessage) -> &'static str {
    use onlinerpg_shared::ServerMessage;
    match msg {
        ServerMessage::DungeonPropState { .. } => "DungeonPropState",
        ServerMessage::TerrainTileVersion { .. } => "TerrainTileVersion",
        ServerMessage::WorldUpdate { .. } => "WorldUpdate",
        ServerMessage::EquipmentEnchantSucceeded { .. } => "EquipmentEnchantSucceeded",
        ServerMessage::PlayerLeveledUp { .. } => "PlayerLeveledUp",
        ServerMessage::AdminFollowChanged { .. } => "AdminFollowChanged",
        ServerMessage::AuthSuccess { .. } => "AuthSuccess",
        ServerMessage::AuthError { .. } => "AuthError",
        ServerMessage::JoinSuccess { .. } => "JoinSuccess",
        ServerMessage::WatchWaiting => "WatchWaiting",
        ServerMessage::WatchTarget { .. } => "WatchTarget",
        ServerMessage::CharacterCreated { .. } => "CharacterCreated",
        ServerMessage::CharacterStatsRolled { .. } => "CharacterStatsRolled",
        ServerMessage::CharacterDeleted { .. } => "CharacterDeleted",
        ServerMessage::CharacterDeletionScheduled { .. } => "CharacterDeletionScheduled",
        ServerMessage::CharacterDeletionCancelled { .. } => "CharacterDeletionCancelled",
        ServerMessage::CharacterError { .. } => "CharacterError",
        ServerMessage::PlayerJoined { .. } => "PlayerJoined",
        ServerMessage::PlayerLeft { .. } => "PlayerLeft",
        ServerMessage::PlayerAppeared { .. } => "PlayerAppeared",
        ServerMessage::PlayerDisappeared { .. } => "PlayerDisappeared",
        ServerMessage::PlayerMovePath { .. } => "PlayerMovePath",
        ServerMessage::PlayerMoveProgress { .. } => "PlayerMoveProgress",
        ServerMessage::PlayerMoved { .. } => "PlayerMoved",
        ServerMessage::PlayerTeleported { .. } => "PlayerTeleported",
        ServerMessage::PlayerTeleportEffect { .. } => "PlayerTeleportEffect",
        ServerMessage::DungeonChestOpened { .. } => "DungeonChestOpened",
        ServerMessage::DungeonPropBroken { .. } => "DungeonPropBroken",
        ServerMessage::DungeonCartPushed { .. } => "DungeonCartPushed",
        ServerMessage::CartStruckPlayer { .. } => "CartStruckPlayer",
        ServerMessage::DungeonPropOpened { .. } => "DungeonPropOpened",
        ServerMessage::DungeonPropsState { .. } => "DungeonPropsState",
        ServerMessage::DungeonStatues { .. } => "DungeonStatues",
        ServerMessage::DoorState { .. } => "DoorState",
        ServerMessage::DungeonDiscoveries { .. } => "DungeonDiscoveries",
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::Recital { .. } => "Recital",
        ServerMessage::WhisperMessage { .. } => "WhisperMessage",
        ServerMessage::ShoutMessage { .. } => "ShoutMessage",
        ServerMessage::PartyChatMessage { .. } => "PartyChatMessage",
        ServerMessage::SystemMessage { .. } => "SystemMessage",
        ServerMessage::GameState { .. } => "GameState",
        ServerMessage::GameTimeSync { .. } => "GameTimeSync",
        ServerMessage::WeatherSync { .. } => "WeatherSync",
        ServerMessage::PricingNotice(_) => "PricingNotice",
        ServerMessage::MonsterSpawned { .. } => "MonsterSpawned",
        ServerMessage::MonsterMoved { .. } => "MonsterMoved",
        ServerMessage::MonsterRemoved { .. } => "MonsterRemoved",
        ServerMessage::MonsterDead { .. } => "MonsterDead",
        ServerMessage::MonsterHealthUpdate { .. } => "MonsterHealthUpdate",
        ServerMessage::PlayerAttacked { .. } => "PlayerAttacked",
        ServerMessage::DaggerDoubleSlashStarted { .. } => "DaggerDoubleSlashStarted",
        ServerMessage::DaggerDoubleSlashRejected { .. } => "DaggerDoubleSlashRejected",
        ServerMessage::DaggerDoubleSlashSkipped { .. } => "DaggerDoubleSlashSkipped",
        ServerMessage::PlayerAttackRejected { .. } => "PlayerAttackRejected",
        ServerMessage::PlayerAttackStopped { .. } => "PlayerAttackStopped",
        ServerMessage::MonsterSkillStarted { .. } => "MonsterSkillStarted",
        ServerMessage::MonsterSkillStruck { .. } => "MonsterSkillStruck",
        ServerMessage::MonsterSkillReflected { .. } => "MonsterSkillReflected",
        ServerMessage::MonsterStared { .. } => "MonsterStared",
        ServerMessage::MonsterDormant { .. } => "MonsterDormant",
        ServerMessage::MonsterGroundStrike { .. } => "MonsterGroundStrike",
        ServerMessage::MonsterAttackedPlayer { .. } => "MonsterAttackedPlayer",
        ServerMessage::PlayerDead { .. } => "PlayerDead",
        ServerMessage::PlayerRespawned { .. } => "PlayerRespawned",
        ServerMessage::PlayerHealthUpdate { .. } => "PlayerHealthUpdate",
        ServerMessage::XpGained { .. } => "XpGained",
        ServerMessage::SkillsUpdate { .. } => "SkillsUpdate",
        ServerMessage::FishingCasted { .. } => "FishingCasted",
        ServerMessage::FishingBite { .. } => "FishingBite",
        ServerMessage::FishingFight { .. } => "FishingFight",
        ServerMessage::FishingEnded { .. } => "FishingEnded",
        ServerMessage::FishingError { .. } => "FishingError",
        ServerMessage::Kicked { .. } => "Kicked",
        ServerMessage::ServerNotice { .. } => "ServerNotice",
        ServerMessage::ServerAnnouncement { .. } => "ServerAnnouncement",
        ServerMessage::OperatorNotice { .. } => "OperatorNotice",
        ServerMessage::PlayerMountChanged { .. } => "PlayerMountChanged",
        ServerMessage::PlayerWetToggled { .. } => "PlayerWetToggled",
        ServerMessage::PlayerPetrifiedToggled { .. } => "PlayerPetrifiedToggled",
        ServerMessage::PlayerTitleChanged { .. } => "PlayerTitleChanged",
        ServerMessage::PlayerAppearanceChanged { .. } => "PlayerAppearanceChanged",
        ServerMessage::AppearanceChangeResult { .. } => "AppearanceChangeResult",
        ServerMessage::TitleEarned { .. } => "TitleEarned",
        ServerMessage::PlayerTitles { .. } => "PlayerTitles",
        ServerMessage::PlayerGearChanged { .. } => "PlayerGearChanged",
        ServerMessage::Bookmarks { .. } => "Bookmarks",
        ServerMessage::BookmarkRefused { .. } => "BookmarkRefused",
        ServerMessage::BlessResult { .. } => "BlessResult",
        ServerMessage::CapeDyePrompt { .. } => "CapeDyePrompt",
        ServerMessage::LandClaimPrompt { .. } => "LandClaimPrompt",
        ServerMessage::LandscapingMode { .. } => "LandscapingMode",
        ServerMessage::LandscapingPaletteUnlocked { .. } => "LandscapingPaletteUnlocked",
        ServerMessage::LandscapeChanged { .. } => "LandscapeChanged",
        ServerMessage::LandscapeInvalidated { .. } => "LandscapeInvalidated",
        ServerMessage::LandscapeEditResult { .. } => "LandscapeEditResult",
        ServerMessage::FenceVisibility { .. } => "FenceVisibility",
        ServerMessage::FenceEditResult { .. } => "FenceEditResult",
        ServerMessage::PatrolSweepResult { .. } => "PatrolSweepResult",
        ServerMessage::PatrolScouted { .. } => "PatrolScouted",
        ServerMessage::PatrolShovelled { .. } => "PatrolShovelled",
        ServerMessage::PlayerShovelled { .. } => "PlayerShovelled",
        ServerMessage::RoadSnowCleared { .. } => "RoadSnowCleared",
        ServerMessage::SeasonalEvents { .. } => "SeasonalEvents",
        ServerMessage::ObjectRegionsChanged { .. } => "ObjectRegionsChanged",
        ServerMessage::PatrolBrokeOut { .. } => "PatrolBrokeOut",
        ServerMessage::PlayerSwung { .. } => "PlayerSwung",
        ServerMessage::ImpoundState { .. } => "ImpoundState",
        ServerMessage::LandClaimed { .. } => "LandClaimed",
        ServerMessage::LandRejected { .. } => "LandRejected",
        ServerMessage::LandAccountState { .. } => "LandAccountState",
        ServerMessage::CapeTexturePrompt { .. } => "CapeTexturePrompt",
        ServerMessage::HouseSpawned { .. } => "HouseSpawned",
        ServerMessage::HousePlacementStarted { .. } => "HousePlacementStarted",
        ServerMessage::HousePlacementResult { .. } => "HousePlacementResult",
        ServerMessage::HouseDemolitionResult { .. } => "HouseDemolitionResult",
        ServerMessage::HouseUpdated { .. } => "HouseUpdated",
        ServerMessage::HeightTilesInvalidated { .. } => "HeightTilesInvalidated",
        ServerMessage::TreeTilesInvalidated { .. } => "TreeTilesInvalidated",
        ServerMessage::GrassTilesInvalidated { .. } => "GrassTilesInvalidated",
        ServerMessage::HouseRemoved { .. } => "HouseRemoved",
        ServerMessage::HousesInArea { .. } => "HousesInArea",
        ServerMessage::PlayerInteractionChanged { .. } => "PlayerInteractionChanged",
        ServerMessage::PlayerMusicStarted { .. } => "PlayerMusicStarted",
        ServerMessage::PlayerInstrumentStarted { .. } => "PlayerInstrumentStarted",
        ServerMessage::PlayerDanceSync { .. } => "PlayerDanceSync",
        ServerMessage::PlayerInstrumentNotes { .. } => "PlayerInstrumentNotes",
        ServerMessage::InteractionRejected { .. } => "InteractionRejected",
        ServerMessage::AltarStatus { .. } => "AltarStatus",
        ServerMessage::NoticeBoard { .. } => "NoticeBoard",
        ServerMessage::NoticePostView { .. } => "NoticePostView",
        ServerMessage::NoticeBoardRefused { .. } => "NoticeBoardRefused",
        ServerMessage::NoticeBoardPapers { .. } => "NoticeBoardPapers",
        ServerMessage::NoticesTakenDown { .. } => "NoticesTakenDown",
        ServerMessage::NoticePostRemoved { .. } => "NoticePostRemoved",
        ServerMessage::Mailbox { .. } => "Mailbox",
        ServerMessage::MailLetter { .. } => "MailLetter",
        ServerMessage::MailSent { .. } => "MailSent",
        ServerMessage::MailRefused { .. } => "MailRefused",
        ServerMessage::MailNotice { .. } => "MailNotice",
        ServerMessage::FieldRaidNotice { .. } => "FieldRaidNotice",
        ServerMessage::FieldBossWaking { .. } => "FieldBossWaking",
        ServerMessage::FieldBossSleeping { .. } => "FieldBossSleeping",
        ServerMessage::InventoryState { .. } => "InventoryState",
        ServerMessage::InventoryUpdated { .. } => "InventoryUpdated",
        ServerMessage::GroundItemSpawned { .. } => "GroundItemSpawned",
        ServerMessage::GroundItemAppeared { .. } => "GroundItemAppeared",
        ServerMessage::GroundItemRemoved { .. } => "GroundItemRemoved",
        ServerMessage::GroundItemQuantityChanged { .. } => "GroundItemQuantityChanged",
        ServerMessage::GroundItemClaimEnded { .. } => "GroundItemClaimEnded",
        ServerMessage::ShopState { .. } => "ShopState",
        ServerMessage::GoldUpdate { .. } => "GoldUpdate",
        ServerMessage::EffectiveStatsUpdated { .. } => "EffectiveStatsUpdated",
        ServerMessage::GoldGained { .. } => "GoldGained",
        ServerMessage::TradeError { .. } => "TradeError",
        ServerMessage::DealUpdated { .. } => "DealUpdated",
        ServerMessage::BuybackUpdated { .. } => "BuybackUpdated",
        ServerMessage::DealResult { .. } => "DealResult",
        ServerMessage::OutfitStatus { .. } => "OutfitStatus",
        ServerMessage::OutfitOffered { .. } => "OutfitOffered",
        ServerMessage::TradeNotice { .. } => "TradeNotice",
        ServerMessage::TradeDeclined { .. } => "TradeDeclined",
        ServerMessage::TradeBusy { .. } => "TradeBusy",
        ServerMessage::PartyInviteReceived { .. } => "PartyInviteReceived",
        ServerMessage::PartyInviteResult { .. } => "PartyInviteResult",
        ServerMessage::PartySummonReceived { .. } => "PartySummonReceived",
        ServerMessage::PartyState { .. } => "PartyState",
        ServerMessage::PartyPositions { .. } => "PartyPositions",
        ServerMessage::PartyVitals { .. } => "PartyVitals",
        ServerMessage::FriendList { .. } => "FriendList",
        ServerMessage::FriendsOnline { .. } => "FriendsOnline",
        ServerMessage::FriendRequestReceived { .. } => "FriendRequestReceived",
        ServerMessage::FamilyState { .. } => "FamilyState",
        ServerMessage::FamilyInviteReceived { .. } => "FamilyInviteReceived",
        ServerMessage::PlayerFamilyChanged { .. } => "PlayerFamilyChanged",
        ServerMessage::FamilyChatMessage { .. } => "FamilyChatMessage",
        ServerMessage::FamilyStorageLog { .. } => "FamilyStorageLog",
        ServerMessage::FamilyMergeProposal { .. } => "FamilyMergeProposal",
        ServerMessage::HungerUpdate { .. } => "HungerUpdate",
        ServerMessage::ManaUpdate { .. } => "ManaUpdate",
        ServerMessage::TempHealthUpdate { .. } => "TempHealthUpdate",
        ServerMessage::DebuffUpdate { .. } => "DebuffUpdate",
        ServerMessage::AbilityCooldowns { .. } => "AbilityCooldowns",
        ServerMessage::BuffUpdate { .. } => "BuffUpdate",
        ServerMessage::PlayerRadianceToggled { .. } => "PlayerRadianceToggled",
        ServerMessage::PlayerRageToggled { .. } => "PlayerRageToggled",
        ServerMessage::MonsterStunned { .. } => "MonsterStunned",
        ServerMessage::MonsterSlowed { .. } => "MonsterSlowed",
        ServerMessage::PlayerCharged { .. } => "PlayerCharged",
        ServerMessage::HealingPrayed { .. } => "HealingPrayed",
        ServerMessage::BattleHymnStarted { .. } => "BattleHymnStarted",
        ServerMessage::BowMarkUpdate { .. } => "BowMarkUpdate",
        ServerMessage::InvisibilityChanged { .. } => "InvisibilityChanged",
        ServerMessage::InspectionResult { .. } => "InspectionResult",
        ServerMessage::AbilityRejected { .. } => "AbilityRejected",
        ServerMessage::AbilityUsed { .. } => "AbilityUsed",
        ServerMessage::CampfireSpawned { .. } => "CampfireSpawned",
        ServerMessage::CampfireAppeared { .. } => "CampfireAppeared",
        ServerMessage::CampfireRemoved { .. } => "CampfireRemoved",
        ServerMessage::StallPlaced { .. } => "StallPlaced",
        ServerMessage::StallAppeared { .. } => "StallAppeared",
        ServerMessage::StallRemoved { .. } => "StallRemoved",
        ServerMessage::StallState { .. } => "StallState",
        ServerMessage::StallSignChanged { .. } => "StallSignChanged",
        ServerMessage::TipHatPlaced { .. } => "TipHatPlaced",
        ServerMessage::TipHatAppeared { .. } => "TipHatAppeared",
        ServerMessage::TipHatRemoved { .. } => "TipHatRemoved",
        ServerMessage::SongRequested { .. } => "SongRequested",
        ServerMessage::MealPlaced { .. } => "MealPlaced",
        ServerMessage::MealAppeared { .. } => "MealAppeared",
        ServerMessage::MealEaten { .. } => "MealEaten",
        ServerMessage::MealRemoved { .. } => "MealRemoved",
        ServerMessage::GrillStarted => "GrillStarted",
        ServerMessage::MiningStarted { .. } => "MiningStarted",
        ServerMessage::MiningEnded { .. } => "MiningEnded",
        ServerMessage::GrillEnded { .. } => "GrillEnded",
        ServerMessage::PlayerTradeRequested { .. } => "PlayerTradeRequested",
        ServerMessage::PlayerTradeRequestResult { .. } => "PlayerTradeRequestResult",
        ServerMessage::PlayerTradeUpdate { .. } => "PlayerTradeUpdate",
        ServerMessage::PlayerTradeEnded { .. } => "PlayerTradeEnded",
        ServerMessage::PlayerTradeError { .. } => "PlayerTradeError",
        ServerMessage::DungeonReset => "DungeonReset",
        ServerMessage::DungeonLayoutVariants { .. } => "DungeonLayoutVariants",
        ServerMessage::CharacterRenameRequired { .. } => "CharacterRenameRequired",
        ServerMessage::CharacterRenamed { .. } => "CharacterRenamed",
        ServerMessage::EstateChestMode { .. } => "EstateChestMode",
        ServerMessage::EstateFurnitureMoveMode { .. } => "EstateFurnitureMoveMode",
        ServerMessage::EstateChestVisibility { .. } => "EstateChestVisibility",
        ServerMessage::EstateChestEditResult { .. } => "EstateChestEditResult",
        ServerMessage::EstateChestState { .. } => "EstateChestState",
        ServerMessage::FurniturePurchaseResult { .. } => "FurniturePurchaseResult",
        ServerMessage::FurnitureSelectionNotice { .. } => "FurnitureSelectionNotice",
        ServerMessage::MergerRiteCue { .. } => "MergerRiteCue",
        ServerMessage::MergerCelebrated { .. } => "MergerCelebrated",
        ServerMessage::MergerPledgeOffered => "MergerPledgeOffered",
        ServerMessage::BlessingOffered { .. } => "BlessingOffered",
        ServerMessage::ImpoundOffered { .. } => "ImpoundOffered",
        ServerMessage::PatrolRollCalled { .. } => "PatrolRollCalled",
        ServerMessage::WatchOffered { .. } => "WatchOffered",
        ServerMessage::WatchVerdict { .. } => "WatchVerdict",
        ServerMessage::WatchRackState { .. } => "WatchRackState",
        ServerMessage::WatchDuty { .. } => "WatchDuty",
        ServerMessage::FamilyEmblemShown { .. } => "FamilyEmblemShown",
    }
}
