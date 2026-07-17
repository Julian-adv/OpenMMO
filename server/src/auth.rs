use crate::types::{CharacterAttributes, GameDateTime};
use crate::world_config::world_config;
use onlinerpg_shared::{CharacterClass, Gender};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// New characters start with no gold: anything redeemable granted at creation
/// would let abusers mint wealth by recycling characters (see doc/ECONOMY.md).
/// Starter gear instead uses item defs without a basePrice, which merchants
/// refuse to buy. (item_def_id, quantity, equip_slot)
const STARTER_ITEMS: &[(&str, u32, Option<&str>)] = &[("worn_iron_sword", 1, Some("main_hand"))];

/// Reserved account-name prefix for headless NPC/bot accounts.
pub const NPC_ACCOUNT_PREFIX: &str = "npc_";

/// One persisted inventory row: a bag stack (`equip_slot: None`) or an
/// equipped item.
#[derive(Debug, Clone)]
pub struct ItemRow {
    pub item_def_id: String,
    pub quantity: u32,
    pub equip_slot: Option<String>,
    pub enchant: i32,
}

#[derive(Debug, Clone)]
pub struct AuthService {
    pool: r2d2::Pool<SqliteConnectionManager>,
}

#[derive(Debug, Clone)]
pub struct CharacterRecord {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub level: u32,
    pub xp: u64,
    pub max_hp: u32,
    pub attributes: CharacterAttributes,
    pub class: CharacterClass,
    pub gender: Gender,
    pub last_x: f32,
    pub last_y: f32,
    pub last_z: f32,
    pub last_rotation: f32,
    pub health: Option<u32>,
    pub floor_level: i8,
    pub gold: i64,
    /// Nonzero unlocks admin for ADMIN_EMAILS-allowlisted accounts (tiers reserved).
    pub admin_role: i64,
}

pub struct CharacterSaveData {
    pub character_id: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
    pub xp: u64,
    pub level: u32,
    pub max_hp: u32,
    pub health: u32,
    pub floor_level: i8,
    pub gold: i64,
}

/// Column list shared between queries that return full CharacterRecord rows.
const CHARACTER_COLUMNS: &str = "id, character_name, created_at, level, xp, max_hp, attr_str, attr_dex, attr_con, attr_int, attr_wis, attr_cha, attr_guard, class, last_x, last_y, last_z, last_rotation, health, floor_level, gender, gold, admin_role";

fn character_record_from_row(row: &rusqlite::Row) -> rusqlite::Result<CharacterRecord> {
    Ok(CharacterRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        level: row.get(3)?,
        xp: row.get::<_, i64>(4)? as u64,
        max_hp: row.get(5)?,
        attributes: CharacterAttributes {
            r#str: row.get(6)?,
            dex: row.get(7)?,
            con: row.get(8)?,
            int: row.get(9)?,
            wis: row.get(10)?,
            cha: row.get(11)?,
            guard: row.get(12)?,
        },
        class: {
            let class_str: String = row.get(13)?;
            class_str.parse::<CharacterClass>().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    13,
                    rusqlite::types::Type::Text,
                    format!("unknown character class: {class_str}").into(),
                )
            })?
        },
        last_x: row.get::<_, f64>(14).unwrap_or(0.0) as f32,
        last_y: row.get::<_, f64>(15).unwrap_or(0.0) as f32,
        last_z: row.get::<_, f64>(16).unwrap_or(0.0) as f32,
        last_rotation: row.get::<_, f64>(17).unwrap_or(0.0) as f32,
        health: row
            .get::<_, Option<i64>>(18)
            .ok()
            .flatten()
            .map(|v| v as u32),
        floor_level: row.get::<_, i64>(19).unwrap_or(0) as i8,
        gender: match row
            .get::<_, String>(20)
            .unwrap_or_else(|_| "male".to_string())
            .as_str()
        {
            "female" => Gender::Female,
            _ => Gender::Male,
        },
        gold: row.get::<_, i64>(21).unwrap_or(0),
        admin_role: row.get::<_, i64>(22).unwrap_or(0),
    })
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    AccountNotFound,
    InvalidCharacterName,
    CharacterLimitReached,
    CharacterNameAlreadyExists,
    CharacterNotFound,
    Database(String),
}

impl AuthError {
    pub fn client_message(&self) -> &'static str {
        match self {
            AuthError::InvalidInput(message) => message,
            AuthError::AccountNotFound => "Account not found",
            AuthError::InvalidCharacterName => "Character name is required",
            AuthError::CharacterLimitReached => {
                "A maximum of 3 characters can be created per account"
            }
            AuthError::CharacterNameAlreadyExists => "Character name already exists",
            AuthError::CharacterNotFound => "Character not found",
            AuthError::Database(_) => "Server auth database error",
        }
    }
}

impl Display for AuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidInput(message) => write!(f, "{message}"),
            AuthError::AccountNotFound => write!(f, "Account not found"),
            AuthError::InvalidCharacterName => write!(f, "Character name is required"),
            AuthError::CharacterLimitReached => {
                write!(f, "A maximum of 3 characters can be created per account")
            }
            AuthError::CharacterNameAlreadyExists => write!(f, "Character name already exists"),
            AuthError::CharacterNotFound => write!(f, "Character not found"),
            AuthError::Database(message) => write!(f, "Database error: {message}"),
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
    pub fn default_db_path() -> PathBuf {
        PathBuf::from("data/game_data.db")
    }

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
        Self::ensure_world_time_schema(&conn)?;

        Ok(Self { pool })
    }

    /// Migrate pre-Google-auth databases: the FNV password hashes are dropped
    /// (worthless as credentials) and accounts become reachable only via
    /// `google_sub` (browser) or the NPC token path.
    fn ensure_accounts_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
        let columns = Self::table_columns(conn, "accounts")?;
        if columns.contains("password_hash") {
            conn.execute("ALTER TABLE accounts DROP COLUMN password_hash", [])?;
        }
        if !columns.contains("google_sub") {
            conn.execute("ALTER TABLE accounts ADD COLUMN google_sub TEXT", [])?;
        }
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_accounts_google_sub
             ON accounts(google_sub) WHERE google_sub IS NOT NULL",
            [],
        )?;
        Ok(())
    }

    fn ensure_characters_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS characters (
                id INTEGER PRIMARY KEY,
                account_name TEXT NOT NULL,
                character_name TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                level INTEGER NOT NULL DEFAULT 1,
                max_hp INTEGER NOT NULL DEFAULT 16,
                attr_str INTEGER NOT NULL DEFAULT 12,
                attr_dex INTEGER NOT NULL DEFAULT 12,
                attr_con INTEGER NOT NULL DEFAULT 12,
                attr_int INTEGER NOT NULL DEFAULT 12,
                attr_wis INTEGER NOT NULL DEFAULT 12,
                attr_cha INTEGER NOT NULL DEFAULT 12,
                attr_guard INTEGER NOT NULL DEFAULT 10,
                FOREIGN KEY (account_name) REFERENCES accounts(player_name) ON DELETE CASCADE
            )",
            [],
        )?;
        Self::ensure_character_attribute_columns(conn)?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_account_name ON characters(account_name)",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_items (
                id INTEGER PRIMARY KEY,
                character_id INTEGER NOT NULL,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 1,
                equip_slot TEXT,
                enchant INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Self::ensure_character_item_columns(conn)?;

        Ok(())
    }

    /// Column names currently on `table`, for post-release ALTER migrations.
    fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(columns)
    }

    /// Columns added to character_items after release; mirrors
    /// `ensure_character_attribute_columns` for the characters table.
    fn ensure_character_item_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
        if !Self::table_columns(conn, "character_items")?.contains("enchant") {
            conn.execute(
                "ALTER TABLE character_items ADD COLUMN enchant INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_world_time_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS world_time (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                year INTEGER NOT NULL,
                month INTEGER NOT NULL,
                day INTEGER NOT NULL,
                hour INTEGER NOT NULL,
                minute INTEGER NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        Ok(())
    }

    fn ensure_character_attribute_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
        let existing_columns = Self::table_columns(conn, "characters")?;

        let spawn = &world_config().spawn_position;
        let expected_columns: Vec<(&str, String)> = vec![
            ("level", "INTEGER NOT NULL DEFAULT 1".into()),
            ("xp", "INTEGER NOT NULL DEFAULT 0".into()),
            ("max_hp", "INTEGER NOT NULL DEFAULT 16".into()),
            ("attr_str", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_dex", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_con", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_int", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_wis", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_cha", "INTEGER NOT NULL DEFAULT 12".into()),
            ("attr_guard", "INTEGER NOT NULL DEFAULT 10".into()),
            ("class", "TEXT NOT NULL DEFAULT 'knight'".into()),
            ("last_x", format!("REAL NOT NULL DEFAULT {}", spawn.x)),
            ("last_y", format!("REAL NOT NULL DEFAULT {}", spawn.y)),
            ("last_z", format!("REAL NOT NULL DEFAULT {}", spawn.z)),
            (
                "last_rotation",
                format!("REAL NOT NULL DEFAULT {}", spawn.rotation),
            ),
            ("health", "INTEGER".into()),
            ("floor_level", "INTEGER NOT NULL DEFAULT 0".into()),
            ("gender", "TEXT NOT NULL DEFAULT 'male'".into()),
            ("gold", "INTEGER NOT NULL DEFAULT 0".into()),
            ("admin_role", "INTEGER NOT NULL DEFAULT 0".into()),
        ];

        for (column_name, column_def) in &expected_columns {
            if !existing_columns.contains(*column_name) {
                let sql = format!(
                    "ALTER TABLE characters ADD COLUMN {} {}",
                    column_name, column_def
                );
                conn.execute(sql.as_str(), [])?;
            }
        }

        Ok(())
    }

    fn open_connection(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, AuthError> {
        self.pool
            .get()
            .map_err(|e| AuthError::Database(e.to_string()))
    }

    /// Log in with a verified Google subject id, creating the account on
    /// first login. Returns the account's player_name. Account names are
    /// random on purpose — deriving them from token claims (email/name)
    /// would persist personal data.
    pub fn login_google(&self, google_sub: &str) -> Result<String, AuthError> {
        let google_sub = google_sub.trim();
        if google_sub.is_empty() {
            return Err(AuthError::InvalidInput("Google subject id is required"));
        }

        let conn = self.open_connection()?;

        for _ in 0..100 {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT player_name FROM accounts WHERE google_sub = ?1",
                    params![google_sub],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(name) = existing {
                return Ok(name);
            }

            let candidate = format!("player_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
            match conn.execute(
                "INSERT INTO accounts (player_name, google_sub) VALUES (?1, ?2)",
                params![candidate, google_sub],
            ) {
                Ok(_) => return Ok(candidate),
                // Name taken (or lost a same-sub race): retry with a fresh name.
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(AuthError::Database(
            "could not allocate a unique account name".to_string(),
        ))
    }

    /// Log in a headless NPC account (token already checked by the caller),
    /// creating it on first use. Returns the canonical (trimmed) name.
    ///
    /// NPC accounts live in a reserved `npc_` namespace: player accounts are
    /// named `player_*` (Google) or predate this scheme (legacy), so requiring
    /// the prefix stops the shared NPC token from ever binding to a human's
    /// account, even on a config typo.
    pub fn login_npc(&self, account_name: &str) -> Result<String, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if !account_name.starts_with(NPC_ACCOUNT_PREFIX) {
            return Err(AuthError::InvalidInput(
                "NPC account names must start with 'npc_'",
            ));
        }

        let conn = self.open_connection()?;
        let existing_sub: Option<Option<String>> = conn
            .query_row(
                "SELECT google_sub FROM accounts WHERE player_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .optional()?;

        match existing_sub {
            Some(None) => Ok(account_name.to_string()),
            Some(Some(_)) => Err(AuthError::InvalidInput(
                "Account name belongs to a player account",
            )),
            None => {
                conn.execute(
                    "INSERT INTO accounts (player_name) VALUES (?1)",
                    params![account_name],
                )?;
                Ok(account_name.to_string())
            }
        }
    }

    pub fn list_characters(&self, account_name: &str) -> Result<Vec<CharacterRecord>, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }

        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {}
             FROM characters
             WHERE account_name = ?1
             ORDER BY created_at ASC, id ASC",
            CHARACTER_COLUMNS
        ))?;

        let characters = stmt
            .query_map(params![account_name], character_record_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(characters)
    }

    pub fn create_character(
        &self,
        account_name: &str,
        character_name: &str,
        attributes: &CharacterAttributes,
        max_hp: u32,
        class: CharacterClass,
        gender: Gender,
    ) -> Result<CharacterRecord, AuthError> {
        let account_name = account_name.trim();
        let character_name = character_name.trim();

        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }

        if character_name.is_empty() {
            return Err(AuthError::InvalidCharacterName);
        }

        let conn = self.open_connection()?;

        let account_exists: Option<String> = conn
            .query_row(
                "SELECT player_name FROM accounts WHERE player_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .optional()?;
        if account_exists.is_none() {
            return Err(AuthError::AccountNotFound);
        }

        let character_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM characters WHERE account_name = ?1",
            params![account_name],
            |row| row.get(0),
        )?;
        if character_count >= 3 {
            return Err(AuthError::CharacterLimitReached);
        }

        let existing_character_name: Option<String> = conn
            .query_row(
                "SELECT character_name FROM characters WHERE character_name = ?1",
                params![character_name],
                |row| row.get(0),
            )
            .optional()?;
        if existing_character_name.is_some() {
            return Err(AuthError::CharacterNameAlreadyExists);
        }

        let gender_str = match gender {
            Gender::Male => "male",
            Gender::Female => "female",
        };

        conn.execute(
            "INSERT INTO characters (
                account_name,
                character_name,
                level,
                max_hp,
                attr_str,
                attr_dex,
                attr_con,
                attr_int,
                attr_wis,
                attr_cha,
                attr_guard,
                class,
                gender,
                last_x,
                last_y,
                last_z,
                last_rotation,
                gold
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 0)",
            params![
                account_name,
                character_name,
                1_i64,
                i64::from(max_hp),
                i64::from(attributes.r#str),
                i64::from(attributes.dex),
                i64::from(attributes.con),
                i64::from(attributes.int),
                i64::from(attributes.wis),
                i64::from(attributes.cha),
                i64::from(attributes.guard),
                class.as_str(),
                gender_str,
                f64::from(world_config().spawn_position.x),
                f64::from(world_config().spawn_position.y),
                f64::from(world_config().spawn_position.z),
                f64::from(world_config().spawn_position.rotation),
            ],
        )?;

        let id = conn.last_insert_rowid();

        {
            let mut stmt = conn.prepare(
                "INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (item_def_id, quantity, equip_slot) in STARTER_ITEMS {
                stmt.execute(params![id, item_def_id, quantity, equip_slot])?;
            }
        }
        let created_at: i64 = conn.query_row(
            "SELECT created_at FROM characters WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok(CharacterRecord {
            id,
            name: character_name.to_string(),
            created_at,
            level: 1,
            xp: 0,
            max_hp,
            attributes: attributes.clone(),
            class,
            gender,
            last_x: world_config().spawn_position.x,
            last_y: world_config().spawn_position.y,
            last_z: world_config().spawn_position.z,
            last_rotation: world_config().spawn_position.rotation,
            health: None,
            floor_level: 0,
            gold: 0,
            admin_role: 0,
        })
    }

    pub fn delete_character(&self, account_name: &str, character_id: i64) -> Result<(), AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if character_id <= 0 {
            return Err(AuthError::CharacterNotFound);
        }

        let conn = self.open_connection()?;
        let rows_affected = conn.execute(
            "DELETE FROM characters WHERE id = ?1 AND account_name = ?2",
            params![character_id, account_name],
        )?;

        if rows_affected == 0 {
            return Err(AuthError::CharacterNotFound);
        }

        Ok(())
    }

    pub fn get_character_for_account(
        &self,
        account_name: &str,
        character_id: i64,
    ) -> Result<CharacterRecord, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if character_id <= 0 {
            return Err(AuthError::CharacterNotFound);
        }

        let conn = self.open_connection()?;
        let character = conn
            .query_row(
                &format!(
                    "SELECT {}
                     FROM characters
                     WHERE id = ?1 AND account_name = ?2",
                    CHARACTER_COLUMNS
                ),
                params![character_id, account_name],
                character_record_from_row,
            )
            .optional()?;

        character.ok_or(AuthError::CharacterNotFound)
    }

    pub fn save_characters_batch(&self, data: &[CharacterSaveData]) -> Result<(), AuthError> {
        if data.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE characters SET last_x = ?1, last_y = ?2, last_z = ?3, last_rotation = ?4, \
                 xp = ?5, level = ?6, max_hp = ?7, health = ?8, floor_level = ?9, gold = ?10 WHERE id = ?11",
            )?;
            for d in data {
                stmt.execute(params![
                    f64::from(d.x),
                    f64::from(d.y),
                    f64::from(d.z),
                    f64::from(d.rotation),
                    d.xp as i64,
                    i64::from(d.level),
                    i64::from(d.max_hp),
                    i64::from(d.health),
                    i64::from(d.floor_level),
                    d.gold,
                    d.character_id,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_world_time(&self) -> Result<Option<GameDateTime>, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT year, month, day, hour, minute FROM world_time WHERE id = 1",
                [],
                |row| {
                    Ok(GameDateTime {
                        year: row.get(0)?,
                        month: row.get(1)?,
                        day: row.get(2)?,
                        hour: row.get(3)?,
                        minute: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_world_time(&self, datetime: &GameDateTime) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO world_time (id, year, month, day, hour, minute, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, strftime('%s', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                year = excluded.year,
                month = excluded.month,
                day = excluded.day,
                hour = excluded.hour,
                minute = excluded.minute,
                updated_at = excluded.updated_at",
            params![
                i64::from(datetime.year),
                i64::from(datetime.month),
                i64::from(datetime.day),
                i64::from(datetime.hour),
                i64::from(datetime.minute),
            ],
        )?;
        Ok(())
    }

    /// Load all items for a character.
    pub fn load_inventory(&self, character_id: i64) -> Result<Vec<ItemRow>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT item_def_id, quantity, equip_slot, enchant FROM character_items WHERE character_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![character_id], |row| {
                Ok(ItemRow {
                    item_def_id: row.get(0)?,
                    quantity: row.get(1)?,
                    equip_slot: row.get(2)?,
                    enchant: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Save all items for a character, replacing existing data.
    pub fn save_inventory(&self, character_id: i64, items: &[ItemRow]) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "DELETE FROM character_items WHERE character_id = ?1",
            params![character_id],
        )?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot, enchant) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for item in items {
                stmt.execute(params![
                    character_id,
                    item.item_def_id,
                    item.quantity,
                    item.equip_slot,
                    item.enchant
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_role_defaults_to_zero_and_loads_after_update() {
        let db_path =
            std::env::temp_dir().join(format!("onlinerpg_auth_admin_{}.db", uuid::Uuid::new_v4()));
        let auth = AuthService::new(db_path.clone()).unwrap();
        let account = auth.login_npc("npc_admin_role_test").unwrap();

        let attributes = CharacterAttributes {
            r#str: 12,
            dex: 12,
            con: 12,
            int: 12,
            wis: 12,
            cha: 12,
            guard: 10,
        };
        let record = auth
            .create_character(
                &account,
                "AdminRoleTest",
                &attributes,
                16,
                CharacterClass::Knight,
                Gender::Male,
            )
            .unwrap();
        assert_eq!(record.admin_role, 0);

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE characters SET admin_role = 2 WHERE id = ?1",
            params![record.id],
        )
        .unwrap();

        let loaded = auth.get_character_for_account(&account, record.id).unwrap();
        assert_eq!(loaded.admin_role, 2);
    }
}
