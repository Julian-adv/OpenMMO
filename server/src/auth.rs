use rusqlite::{params, Connection, OptionalExtension};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AuthService {
    db_path: Arc<PathBuf>,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    AccountAlreadyExists,
    AccountNotFound,
    InvalidPassword,
    Database(String),
}

impl AuthError {
    pub fn client_message(&self) -> &'static str {
        match self {
            AuthError::InvalidInput(_) => "Player name and password are required",
            AuthError::AccountAlreadyExists => "Account already exists",
            AuthError::AccountNotFound => "Account not found",
            AuthError::InvalidPassword => "Invalid password",
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
            AuthError::Database(message) => write!(f, "Database error: {message}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl AuthService {
    pub fn default_db_path() -> PathBuf {
        if Path::new("data").is_dir() {
            Path::new("data").join("accounts.db")
        } else {
            Path::new("../data").join("accounts.db")
        }
    }

    pub fn new(db_path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                player_name TEXT PRIMARY KEY,
                password_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        Ok(Self {
            db_path: Arc::new(db_path),
        })
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

        let conn = Connection::open(self.db_path.as_ref())
            .map_err(|e| AuthError::Database(e.to_string()))?;

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
}
