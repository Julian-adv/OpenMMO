#[path = "auth_bans.rs"]
mod bans;
#[path = "auth_bookmarks.rs"]
mod bookmarks;
#[path = "auth_characters.rs"]
mod characters;
#[path = "auth_dungeon_history.rs"]
mod dungeon_history;
#[path = "auth_friends.rs"]
mod friends;
#[path = "auth_login.rs"]
mod login;
#[path = "auth_migrations.rs"]
mod migrations;
#[path = "auth_npc_outfits.rs"]
mod npc_outfits;
#[path = "auth_persistence.rs"]
mod persistence;
#[path = "auth_pricing.rs"]
mod pricing;
#[path = "auth_starter_items.rs"]
mod starter_items;
#[path = "auth_titles.rs"]
mod titles;
pub use bans::{ban_message, DEFAULT_BAN_REASON};
pub use characters::CharacterListing;
#[cfg(test)]
pub use characters::CharacterRecord;
pub use npc_outfits::{OutfitClaim, OutfitHandover};
pub use persistence::{CharacterSaveData, ItemRow, SkillRow, TradeLedgerEntry};
pub use pricing::{PricingMeeting, PricingState, PRICING_MEASURE_DAYS};
#[path = "auth_character_deletion.rs"]
mod character_deletion;
#[path = "auth_dashboard_sessions.rs"]
mod dashboard_sessions;
#[path = "auth_dungeon_population.rs"]
mod dungeon_population;
#[path = "auth_enchant_failures.rs"]
mod enchant_failures;
pub(crate) use dashboard_sessions::DASHBOARD_SESSION_SECONDS;
#[path = "auth_estate_defense.rs"]
mod estate_defense;
#[path = "auth_estate_storage.rs"]
mod estate_storage;
#[path = "auth_family.rs"]
mod family;
#[path = "auth_family_metrics.rs"]
mod family_metrics;
#[path = "auth_field_raids.rs"]
mod field_raids;
#[path = "auth_seasonal.rs"]
mod seasonal;
#[path = "auth_statues.rs"]
mod statues;
pub(crate) use estate_defense::{DefenseRun, DefenseSite, Disgraces, TrophySite};
pub(crate) use estate_storage::EstateDeposit;
pub(crate) use family::{FamilyDeparture, FamilyEmblem, FamilyRoster};
pub(crate) use statues::StatueRow;
#[path = "auth_fence.rs"]
mod fence;
#[path = "auth_gold_sinks.rs"]
mod gold_sinks;
#[path = "auth_land.rs"]
mod land;
#[path = "auth_road_patrol.rs"]
mod road_patrol;
#[path = "auth_shelter.rs"]
mod shelter;
#[cfg(test)]
pub(crate) use road_patrol::IMPOUND_HOLD_SECONDS;
pub(crate) use road_patrol::{ImpoundedStack, RoadClearance, RoadSeizure};
pub use shelter::{OwnShelters, ShelterSite};
#[path = "auth_watch_shift.rs"]
mod watch_shift;
pub(crate) use watch_shift::{WatchHire, WatchShiftRow};
#[path = "auth_landscaping.rs"]
mod landscaping;
#[path = "auth_loot_tally.rs"]
mod loot_tally;
#[path = "auth_mail.rs"]
mod mail;
#[path = "auth_metrics.rs"]
mod metrics;
#[path = "auth_notice_board.rs"]
mod notice_board;
pub(crate) use land::EstateCleanup;
pub use land::{LandAccount, OwnedLandPlot};
pub use mail::{NewLetter, MESSAGE_RETENTION_SECS};
pub use notice_board::{BoardKey, NewNoticePost, NoticeClock, PostView, StoredPaper};
use onlinerpg_shared::{CharacterClass, Gender};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

pub const NPC_ACCOUNT_PREFIX: &str = "npc_";

const MAX_NAME_CHARS: usize = 32;

/// Names end up in logs, chat and the UI, so allowlist characters instead of
/// chasing Unicode lookalikes/invisibles: ASCII alphanumeric, underscore,
/// Hangul (syllables + modern jamo), kana (+ ー), CJK unified ideographs.
fn valid_name_char(c: char) -> bool {
    matches!(c,
        'a'..='z' | 'A'..='Z' | '0'..='9' | '_'
        | '가'..='힣' | 'ㄱ'..='ㅣ'
        | 'ぁ'..='ゖ' | 'ァ'..='ヺ' | 'ー'
        | '一'..='\u{9fff}')
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= MAX_NAME_CHARS && name.chars().all(valid_name_char)
}

/// Character names must also start with a letter; digit/underscore-leading
/// names created before this rule are grandfathered.
fn valid_character_name(name: &str) -> bool {
    valid_name(name)
        && name
            .chars()
            .next()
            .is_some_and(|c| !matches!(c, '0'..='9' | '_'))
}

pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct AuthService {
    pool: r2d2::Pool<SqliteConnectionManager>,
    /// Arc so cloning the service stays a handle copy: callers move clones
    /// into blocking tasks all over the server.
    banned_names: Arc<HashSet<String>>,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    AccountNotFound,
    InvalidCharacterName,
    CharacterLimitReached,
    CharacterNameAlreadyExists,
    BannedCharacterName,
    CharacterNotFound,
    Database(String),
}

impl AuthError {
    pub fn client_message(&self) -> &'static str {
        match self {
            AuthError::InvalidInput(message) => message,
            AuthError::AccountNotFound => "Account not found",
            AuthError::InvalidCharacterName => {
                "Character name must start with a letter and contain only letters, digits, or _"
            }
            AuthError::CharacterLimitReached => {
                "A maximum of 3 characters can be created per account"
            }
            AuthError::CharacterNameAlreadyExists => "Character name already exists",
            AuthError::BannedCharacterName => "That name cannot be used. Please choose another",
            AuthError::CharacterNotFound => "Character not found",
            AuthError::Database(_) => "Server auth database error",
        }
    }
}

impl Display for AuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Database(message) => write!(f, "Database error: {message}"),
            other => write!(f, "{}", other.client_message()),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<rusqlite::Error> for AuthError {
    fn from(e: rusqlite::Error) -> Self {
        AuthError::Database(e.to_string())
    }
}

impl AuthService {
    pub fn new(db_path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let manager = SqliteConnectionManager::file(&db_path)
            .with_init(|conn| conn.execute_batch("PRAGMA foreign_keys = ON"));

        let pool = r2d2::Pool::builder().build(manager)?;

        let conn = pool.get()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                player_name TEXT PRIMARY KEY,
                google_sub TEXT,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        Self::ensure_accounts_columns(&conn)?;
        Self::ensure_characters_schema(&conn)?;
        #[cfg(debug_assertions)]
        {
            let deadline = unix_now() + character_deletion::DELETION_DELAY_SECONDS;
            conn.execute(
                "UPDATE characters SET deletion_due_at=?1 WHERE deletion_due_at>?1",
                [deadline],
            )?;
        }
        Self::migrate_item_definition_ids(&conn)?;
        Self::strip_npc_starter_weapons(&conn)?;
        Self::sync_npc_genders(&conn)?;
        Self::ensure_blocks_schema(&conn)?;
        Self::ensure_friends_schema(&conn)?;
        Self::ensure_bans_schema(&conn)?;
        Self::ensure_character_skills_schema(&conn)?;
        Self::ensure_world_time_schema(&conn)?;
        Self::ensure_dungeon_chest_schema(&conn)?;
        Self::ensure_dungeon_discovery_schema(&conn)?;
        Self::ensure_titles_schema(&conn)?;
        Self::ensure_npc_outfits_schema(&conn)?;
        Self::ensure_trade_ledger_schema(&conn)?;
        Self::ensure_item_id_schema(&conn)?;
        Self::ensure_gold_snapshots_schema(&conn)?;
        Self::ensure_server_starts_schema(&conn)?;
        Self::ensure_item_sales_schema(&conn)?;
        Self::ensure_gold_sinks_schema(&conn)?;
        Self::ensure_concurrent_samples_schema(&conn)?;
        Self::ensure_dungeon_population_schema(&conn)?;
        Self::ensure_account_activity_schema(&conn)?;
        Self::ensure_pricing_schema(&conn)?;
        Self::ensure_migrations_schema(&conn)?;
        Self::migrate_level_curve(&conn)?;
        Self::migrate_starter_outfit(
            &conn,
            "female_rogue_starter_outfit_20261027",
            CharacterClass::Rogue,
            Gender::Female,
        )?;
        Self::migrate_starter_outfit(
            &conn,
            "valkyrie_starter_outfit_20261103",
            CharacterClass::Valkyrie,
            Gender::Female,
        )?;
        Self::migrate_starter_weapon(
            &conn,
            "rogue_starter_dagger_20261027",
            CharacterClass::Rogue,
            "worn_dagger",
            Some("worn_iron_sword"),
        )?;
        Self::migrate_starter_weapon(
            &conn,
            "priest_starter_mace_20261031",
            CharacterClass::Priest,
            "worn_mace",
            None,
        )?;
        Self::ensure_level_history_schema(&conn)?;
        Self::ensure_gold_history_schema(&conn)?;
        Self::ensure_weapon_enchant_history_schema(&conn)?;
        Self::ensure_weapon_enchant_failures_schema(&conn)?;
        Self::ensure_loot_tally_schema(&conn)?;
        Self::ensure_field_raid_events_schema(&conn)?;
        Self::ensure_armor_enchant_history_schema(&conn)?;
        Self::ensure_land_history_schema(&conn)?;
        Self::ensure_dashboard_sessions_schema(&conn)?;

        Ok(Self {
            pool,
            banned_names: Arc::new(HashSet::new()),
        })
    }

    pub fn with_banned_names(mut self, names: HashSet<String>) -> Self {
        self.banned_names = Arc::new(names);
        self
    }

    /// Whether an account is barred from using this name. Operator-run NPC
    /// accounts are exempt: headless bots cannot answer a rename prompt.
    pub fn is_name_banned_for(&self, account_name: &str, name: &str) -> bool {
        !account_name.starts_with(NPC_ACCOUNT_PREFIX)
            && self
                .banned_names
                .contains(&crate::banned_names::normalize(name))
    }

    /// Column names currently on `table`, for post-release ALTER migrations.
    fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(columns)
    }

    fn open_connection(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, AuthError> {
        self.pool
            .get()
            .map_err(|e| AuthError::Database(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_auth;
    use crate::types::CharacterAttributes;

    pub(super) fn plain_attributes() -> CharacterAttributes {
        CharacterAttributes {
            r#str: 10,
            dex: 10,
            con: 10,
            int: 10,
            wis: 10,
            cha: 10,
            guard: 10,
        }
    }

    pub(super) fn create(
        auth: &AuthService,
        account: &str,
        name: &str,
    ) -> Result<CharacterRecord, AuthError> {
        auth.create_character(
            account,
            name,
            &plain_attributes(),
            10,
            CharacterClass::Knight,
            Gender::Male,
        )
    }

    #[test]
    fn name_validation_rejects_control_chars_and_long_names() {
        let (auth, _) = temp_auth("auth_names");

        assert!(auth.login_npc("npc_bad\nname").is_err());

        let account = auth.login_npc("npc_name_test").unwrap();
        assert!(create(&auth, &account, "Bad\nName").is_err());
        assert!(create(&auth, &account, "30000").is_err());
        assert!(create(&auth, &account, "_Player").is_err());
        assert!(create(&auth, &account, "김철수").is_ok());
        assert!(create(&auth, &account, "ㅇㅇ_Player1").is_ok());
        assert!(create(&auth, &account, "Player1").is_ok());

        assert!(!valid_character_name("30000"));
        assert!(!valid_character_name("9lives"));
        assert!(!valid_character_name("_x"));
        assert!(valid_character_name("x_9"));
        assert!(valid_character_name("가9"));

        assert!(!valid_name(""));
        assert!(!valid_name("Bad\u{1b}[31mName"));
        assert!(!valid_name("Zero\u{200b}Width"));
        assert!(!valid_name("Emo😀ji"));
        assert!(!valid_name("Rtl\u{202e}Name"));
        assert!(!valid_name("With Space"));
        assert!(!valid_name(&"x".repeat(33)));
        assert!(!valid_name("Ｆｕｌｌ"));
        assert!(!valid_name("ﾊﾝｶｸ"));
        assert!(!valid_name("・"));
        assert!(valid_name("さくら"));
        assert!(valid_name("ローラ"));
        assert!(valid_name("田中太郎"));
        assert!(valid_name("劍聖"));
    }
}
