use crate::types::{CharacterAttributes, GameDateTime};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AuthService {
    db_path: Arc<PathBuf>,
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
    pub class: String,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    AccountAlreadyExists,
    AccountNotFound,
    InvalidPassword,
    InvalidCharacterName,
    CharacterLimitReached,
    CharacterNameAlreadyExists,
    CharacterNotFound,
    Database(String),
}

impl AuthError {
    pub fn client_message(&self) -> &'static str {
        match self {
            AuthError::InvalidInput(_) => "Account name and password are required",
            AuthError::AccountAlreadyExists => "Account already exists",
            AuthError::AccountNotFound => "Account not found",
            AuthError::InvalidPassword => "Invalid password",
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
            AuthError::AccountAlreadyExists => write!(f, "Account already exists"),
            AuthError::AccountNotFound => write!(f, "Account not found"),
            AuthError::InvalidPassword => write!(f, "Invalid password"),
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

impl AuthService {
    fn data_dir() -> PathBuf {
        if Path::new("data").is_dir() {
            PathBuf::from("data")
        } else {
            PathBuf::from("../data")
        }
    }

    pub fn default_db_path() -> PathBuf {
        Self::data_dir().join("game_data.db")
    }

    pub fn new(db_path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                player_name TEXT PRIMARY KEY,
                password_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        Self::ensure_characters_schema(&conn)?;
        Self::ensure_world_time_schema(&conn)?;

        Ok(Self {
            db_path: Arc::new(db_path),
        })
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
        let mut stmt = conn.prepare("PRAGMA table_info(characters)")?;
        let existing_columns: HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;

        let expected_columns = [
            ("level", "INTEGER NOT NULL DEFAULT 1"),
            ("xp", "INTEGER NOT NULL DEFAULT 0"),
            ("max_hp", "INTEGER NOT NULL DEFAULT 16"),
            ("attr_str", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_dex", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_con", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_int", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_wis", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_cha", "INTEGER NOT NULL DEFAULT 12"),
            ("attr_guard", "INTEGER NOT NULL DEFAULT 10"),
            ("class", "TEXT NOT NULL DEFAULT 'knight'"),
        ];

        for (column_name, column_def) in expected_columns {
            if !existing_columns.contains(column_name) {
                let sql = format!(
                    "ALTER TABLE characters ADD COLUMN {} {}",
                    column_name, column_def
                );
                conn.execute(sql.as_str(), [])?;
            }
        }

        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, AuthError> {
        let conn = Connection::open(self.db_path.as_ref())
            .map_err(|e| AuthError::Database(e.to_string()))?;
        conn.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(conn)
    }

    pub fn authenticate(
        &self,
        player_name: &str,
        password_hash: &str,
        create_account: bool,
    ) -> Result<(), AuthError> {
        let player_name = player_name.trim();
        let password_hash = password_hash.trim();

        if player_name.is_empty() || password_hash.is_empty() {
            return Err(AuthError::InvalidInput(
                "Player name and password hash are required",
            ));
        }

        let conn = self.open_connection()?;

        if create_account {
            self.create_account(&conn, player_name, password_hash)
        } else {
            self.verify_login(&conn, player_name, password_hash)
        }
    }

    fn create_account(
        &self,
        conn: &Connection,
        player_name: &str,
        password_hash: &str,
    ) -> Result<(), AuthError> {
        let existing: Option<String> = conn
            .query_row(
                "SELECT player_name FROM accounts WHERE player_name = ?1",
                params![player_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AuthError::Database(e.to_string()))?;

        if existing.is_some() {
            return Err(AuthError::AccountAlreadyExists);
        }

        conn.execute(
            "INSERT INTO accounts (player_name, password_hash) VALUES (?1, ?2)",
            params![player_name, password_hash],
        )
        .map_err(|e| AuthError::Database(e.to_string()))?;

        Ok(())
    }

    fn verify_login(
        &self,
        conn: &Connection,
        player_name: &str,
        password_hash: &str,
    ) -> Result<(), AuthError> {
        let stored_hash: Option<String> = conn
            .query_row(
                "SELECT password_hash FROM accounts WHERE player_name = ?1",
                params![player_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AuthError::Database(e.to_string()))?;

        match stored_hash {
            None => Err(AuthError::AccountNotFound),
            Some(hash) if hash == password_hash => Ok(()),
            Some(_) => Err(AuthError::InvalidPassword),
        }
    }

    pub fn list_characters(&self, account_name: &str) -> Result<Vec<CharacterRecord>, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }

        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, character_name, created_at, level, xp, max_hp, attr_str, attr_dex, attr_con, attr_int, attr_wis, attr_cha, attr_guard, class
                 FROM characters
                 WHERE account_name = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|e| AuthError::Database(e.to_string()))?;

        let characters = stmt
            .query_map(params![account_name], |row| {
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
                    class: row
                        .get::<_, String>(13)
                        .unwrap_or_else(|_| "knight".to_string()),
                })
            })
            .map_err(|e| AuthError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AuthError::Database(e.to_string()))?;

        Ok(characters)
    }

    pub fn create_character(
        &self,
        account_name: &str,
        character_name: &str,
        attributes: &CharacterAttributes,
        max_hp: u32,
        class: &str,
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
            .optional()
            .map_err(|e| AuthError::Database(e.to_string()))?;
        if account_exists.is_none() {
            return Err(AuthError::AccountNotFound);
        }

        let character_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM characters WHERE account_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .map_err(|e| AuthError::Database(e.to_string()))?;
        if character_count >= 3 {
            return Err(AuthError::CharacterLimitReached);
        }

        let existing_character_name: Option<String> = conn
            .query_row(
                "SELECT character_name FROM characters WHERE character_name = ?1",
                params![character_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AuthError::Database(e.to_string()))?;
        if existing_character_name.is_some() {
            return Err(AuthError::CharacterNameAlreadyExists);
        }

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
                class
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                class,
            ],
        )
        .map_err(|e| AuthError::Database(e.to_string()))?;

        let id = conn.last_insert_rowid();
        let (created_at, level, loaded_max_hp, loaded_attributes, loaded_class): (i64, u32, u32, CharacterAttributes, String) =
            conn
            .query_row(
                "SELECT created_at, level, max_hp, attr_str, attr_dex, attr_con, attr_int, attr_wis, attr_cha, attr_guard, class
                 FROM characters
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        CharacterAttributes {
                            r#str: row.get(3)?,
                            dex: row.get(4)?,
                            con: row.get(5)?,
                            int: row.get(6)?,
                            wis: row.get(7)?,
                            cha: row.get(8)?,
                            guard: row.get(9)?,
                        },
                        row.get::<_, String>(10).unwrap_or_else(|_| "knight".to_string()),
                    ))
                },
            )
            .map_err(|e| AuthError::Database(e.to_string()))?;

        let character = CharacterRecord {
            id,
            name: character_name.to_string(),
            created_at,
            level,
            xp: 0,
            max_hp: loaded_max_hp,
            attributes: loaded_attributes,
            class: loaded_class,
        };

        Ok(character)
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
        let rows_affected = conn
            .execute(
                "DELETE FROM characters WHERE id = ?1 AND account_name = ?2",
                params![character_id, account_name],
            )
            .map_err(|e| AuthError::Database(e.to_string()))?;

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
                "SELECT id, character_name, created_at, level, xp, max_hp, attr_str, attr_dex, attr_con, attr_int, attr_wis, attr_cha, attr_guard, class
                 FROM characters
                 WHERE id = ?1 AND account_name = ?2",
                params![character_id, account_name],
                |row| {
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
                        class: row.get::<_, String>(13).unwrap_or_else(|_| "knight".to_string()),
                    })
                },
            )
            .optional()
            .map_err(|e| AuthError::Database(e.to_string()))?;

        character.ok_or(AuthError::CharacterNotFound)
    }

    pub fn update_character_xp_and_level(
        &self,
        character_id: i64,
        xp: u64,
        level: u32,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE characters SET xp = ?1, level = ?2 WHERE id = ?3",
            params![xp as i64, i64::from(level), character_id],
        )
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn update_character_xp_level_and_max_hp(
        &self,
        character_id: i64,
        xp: u64,
        level: u32,
        max_hp: u32,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE characters SET xp = ?1, level = ?2, max_hp = ?3 WHERE id = ?4",
            params![xp as i64, i64::from(level), i64::from(max_hp), character_id],
        )
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn load_world_time(&self) -> Result<Option<GameDateTime>, AuthError> {
        let conn = self.open_connection()?;
        conn.query_row(
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
        .optional()
        .map_err(|e| AuthError::Database(e.to_string()))
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
        )
        .map_err(|e| AuthError::Database(e.to_string()))?;
        Ok(())
    }
}
