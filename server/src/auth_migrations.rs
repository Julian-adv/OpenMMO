use super::AuthService;
use crate::world_config::world_config;
use onlinerpg_shared::xp;
use rusqlite::{params, Connection};

/// Item defs renamed after release, applied to stored inventories at startup.
/// (old_id, new_id) — new_id must exist in items.csv.
const RENAMED_ITEM_IDS: &[(&str, &str)] = &[
    ("leather_cap", "leather_helmet"),
    ("iron_chestplate", "breastplate"),
];

/// Reserved account-name prefix for headless NPC/bot accounts.
/// The level curve before doc/LEVEL_CURVE.md: 20 * 2^(n-2). Kept only for
/// `migrate_level_curve`.
fn legacy_xp_for_level(level: u32) -> u64 {
    if level <= 1 {
        return 0;
    }
    let shift = level - 2;
    if shift >= 64 {
        return u64::MAX;
    }
    20u64.saturating_mul(1u64 << shift)
}

/// Same level, same progress through the level band, on the new curve.
fn migrate_legacy_xp(old_xp: u64) -> u64 {
    let level = if old_xp < 20 {
        1
    } else {
        ((old_xp / 20).ilog2() + 2).min(61)
    };
    let old_start = legacy_xp_for_level(level);
    let old_band = legacy_xp_for_level(level + 1) - old_start;
    let new_start = xp::xp_for_level(level);
    let new_band = xp::xp_for_level(level + 1) - new_start;
    let into = u128::from(old_xp - old_start);
    new_start + (into * u128::from(new_band) / u128::from(old_band)) as u64
}

impl AuthService {
    /// Migrate pre-Google-auth databases: the FNV password hashes are dropped
    /// (worthless as credentials) and accounts become reachable only via
    /// `google_sub` (browser) or the NPC token path.
    pub(super) fn ensure_accounts_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
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

    pub(super) fn ensure_characters_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
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
        if !Self::table_columns(conn, "characters")?.contains("deletion_due_at") {
            conn.execute(
                "ALTER TABLE characters ADD COLUMN deletion_due_at INTEGER",
                [],
            )?;
        }
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_deletion_due ON characters(deletion_due_at) WHERE deletion_due_at IS NOT NULL",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_account_name ON characters(account_name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_level_ranking
             ON characters(level DESC, xp DESC, id ASC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_gold_ranking
             ON characters(gold DESC, id ASC)",
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
                cape_color TEXT,
                cape_texture TEXT,
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Self::ensure_character_item_columns(conn)?;
        Self::ensure_land_schema(conn)?;
        Self::ensure_fence_schema(conn)?;
        Self::ensure_landscaping_schema(conn)?;
        Self::ensure_bookmark_schema(conn)?;
        Self::ensure_statue_schema(conn)?;
        Self::ensure_seasonal_schema(conn)?;
        Self::ensure_estate_storage_schema(conn)?;
        Self::ensure_family_schema(conn)?;
        Self::ensure_notice_board_schema(conn)?;
        Self::ensure_mail_schema(conn)?;
        Self::ensure_road_patrol_schema(conn)?;
        Self::ensure_watch_shift_schema(conn)?;

        // Every inventory read and every save's DELETE filters on character_id;
        // without this SQLite full-scans the table once per character.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_character_items_character_id \
             ON character_items(character_id)",
            [],
        )?;

        Ok(())
    }

    /// Columns added to character_items after release; mirrors
    /// `ensure_character_attribute_columns` for the characters table.
    fn ensure_character_item_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
        let columns = Self::table_columns(conn, "character_items")?;
        if !columns.contains("locked") {
            conn.execute(
                "ALTER TABLE character_items ADD COLUMN locked INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !columns.contains("enchant") {
            conn.execute(
                "ALTER TABLE character_items ADD COLUMN enchant INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !columns.contains("cape_color") {
            conn.execute("ALTER TABLE character_items ADD COLUMN cape_color TEXT", [])?;
        }
        if !columns.contains("cape_texture") {
            conn.execute(
                "ALTER TABLE character_items ADD COLUMN cape_texture TEXT",
                [],
            )?;
        }

        Ok(())
    }

    /// One-off data migrations that must not run twice, keyed by name.
    pub(super) fn ensure_migrations_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        Ok(())
    }

    /// Moves stored XP from the 20 * 2^(n-2) curve onto doc/LEVEL_CURVE.md,
    /// keeping every character's level and progress through its level band.
    /// `level` is left alone: it already agrees with the XP.
    pub(super) fn migrate_level_curve(conn: &Connection) -> Result<(), rusqlite::Error> {
        const NAME: &str = "level_curve_2026_08";
        let done: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = ?1)",
            params![NAME],
            |row| row.get(0),
        )?;
        if done {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        let mut update = tx.prepare("UPDATE characters SET xp = ?1 WHERE id = ?2")?;
        let mut migrated = 0;
        for row in tx
            .prepare("SELECT id, xp FROM characters WHERE xp > 0")?
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?)))?
        {
            let (id, old_xp) = row?;
            migrated += update.execute(params![migrate_legacy_xp(old_xp) as i64, id])?;
        }
        drop(update);
        tx.execute("INSERT INTO migrations (name) VALUES (?1)", params![NAME])?;
        tx.commit()?;
        tracing::info!(migrated, "Migrated character XP to the new level curve");
        Ok(())
    }

    pub(super) fn migrate_item_definition_ids(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt =
            conn.prepare("UPDATE character_items SET item_def_id = ?1 WHERE item_def_id = ?2")?;
        let mut migrated = 0;
        for &(old, new) in RENAMED_ITEM_IDS {
            migrated += stmt.execute(params![new, old])?;
        }
        if migrated > 0 {
            tracing::info!(migrated, "Migrated legacy item definition ids");
        }
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
            ("mana", "INTEGER".into()),
            ("floor_level", "INTEGER NOT NULL DEFAULT 0".into()),
            ("dungeon_epoch", "INTEGER".into()),
            ("gender", "TEXT NOT NULL DEFAULT 'male'".into()),
            ("appearance", "TEXT NOT NULL DEFAULT '{}'".into()),
            ("gold", "INTEGER NOT NULL DEFAULT 0".into()),
            ("admin_role", "INTEGER NOT NULL DEFAULT 0".into()),
            // Unix seconds, NULL until first seen since the column shipped.
            ("last_seen_at", "INTEGER".into()),
            // Shown title id, NULL for none (doc/TITLES.md).
            ("active_title", "TEXT".into()),
            // Ammunition the next shot draws from, NULL to take the strongest
            // of the kind (doc/COMBAT.md 원거리 전투).
            ("active_ammo", "TEXT".into()),
            (
                "satiation",
                format!(
                    "INTEGER NOT NULL DEFAULT {}",
                    onlinerpg_shared::hunger::SATIATION_START
                ),
            ),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::create;
    use crate::test_util::temp_auth;

    #[test]
    fn dungeon_epoch_migration_preserves_legacy_character_state() {
        let (auth, db_path) = temp_auth("dungeon_epoch_migration");
        let account = auth.login_npc("npc_legacy_dungeon").unwrap();
        let character = create(&auth, &account, "Delver").unwrap();
        drop(auth);
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE characters DROP COLUMN dungeon_epoch;
                 UPDATE characters SET floor_level = -20, health = 7, gold = 123;",
            )
            .unwrap();
        drop(connection);

        let migrated = AuthService::new(db_path).unwrap();
        let saved = migrated
            .get_character_for_account(&account, character.id)
            .unwrap();
        assert_eq!(saved.dungeon_epoch, None);
        assert_eq!(saved.floor_level, -20);
        assert_eq!(saved.health, Some(7));
        assert_eq!(saved.gold, 123);
    }

    #[test]
    fn startup_migrates_legacy_item_definition_ids() {
        for (old, new, slot, enchant) in [
            ("leather_cap", "leather_helmet", "head", 2),
            ("iron_chestplate", "breastplate", "chest", 3),
        ] {
            let (auth, db_path) = temp_auth("auth_item_ids");
            drop(auth);

            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "INSERT INTO accounts (player_name) VALUES ('legacy');
                 INSERT INTO characters (id, account_name, character_name)
                 VALUES (1, 'legacy', 'LegacyWearer');",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO character_items
                 (character_id, item_def_id, quantity, equip_slot, enchant)
                 VALUES (1, ?1, 1, ?2, ?3)",
                params![old, slot, enchant],
            )
            .unwrap();
            drop(conn);

            let auth = AuthService::new(db_path).unwrap();
            let rows = auth.load_inventory(1).unwrap();
            assert_eq!(rows.len(), 1, "migrating {old}");
            assert_eq!(rows[0].item_def_id, new);
            assert_eq!(rows[0].equip_slot.as_deref(), Some(slot));
            assert_eq!(rows[0].enchant, enchant);
        }
    }

    #[test]
    fn legacy_xp_migrates_to_the_same_level_and_band_progress() {
        assert_eq!(migrate_legacy_xp(0), 0);
        // Lv1 halfway: 10/20 -> 8/17.
        assert_eq!(migrate_legacy_xp(10), 8);
        assert_eq!(migrate_legacy_xp(20), xp::xp_for_level(2));
        // Lv10 halfway: 5120 + 2560 -> 9956 + 3600.
        assert_eq!(migrate_legacy_xp(7680), 13556);
        let mut prev = 0;
        for old in (0u64..1 << 20)
            .step_by(7)
            .chain([1 << 30, 1 << 40, 1 << 50, 1 << 60])
        {
            let level = if old < 20 { 1 } else { (old / 20).ilog2() + 2 };
            let new = migrate_legacy_xp(old);
            assert_eq!(xp::level_from_xp(new), level, "old xp {old}");
            assert!(new >= prev, "old xp {old}");
            prev = new;
        }
    }

    /// The curve migration runs once: a second startup must not re-map XP
    /// that is already on the new table.
    #[test]
    fn startup_migrates_xp_to_the_new_curve_once() {
        let (auth, db_path) = temp_auth("auth_level_curve");
        drop(auth);

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "DELETE FROM migrations;
             INSERT INTO accounts (player_name) VALUES ('legacy');
             INSERT INTO characters (id, account_name, character_name, level, xp)
             VALUES (1, 'legacy', 'Halfway', 10, 7680);",
        )
        .unwrap();
        drop(conn);

        let read_xp = || {
            Connection::open(&db_path)
                .unwrap()
                .query_row("SELECT xp FROM characters WHERE id = 1", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        };
        drop(AuthService::new(db_path.clone()).unwrap());
        assert_eq!(read_xp(), 13556);
        drop(AuthService::new(db_path.clone()).unwrap());
        assert_eq!(read_xp(), 13556);
    }

    /// A typo'd rename target would silently hand players a ghost item:
    /// unknown ids survive inventory load and fall back to the raw id string.
    #[test]
    fn renamed_item_ids_resolve_to_real_defs() {
        let defs = crate::item_defs::ItemDefs::load();
        for &(old, new) in RENAMED_ITEM_IDS {
            assert!(defs.get(new).is_some(), "{old} renamed to unknown id {new}");
        }
    }

    #[test]
    fn startup_rejects_legacy_case_colliding_names() {
        let db_path =
            std::env::temp_dir().join(format!("onlinerpg_auth_legacy_{}.db", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE characters (
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
                    attr_guard INTEGER NOT NULL DEFAULT 10
                );
                INSERT INTO characters (account_name, character_name)
                VALUES ('legacy', 'Bob'), ('legacy', 'bob');",
            )
            .unwrap();
        }

        // The unique NOCASE index cannot be built over colliding rows;
        // startup fails until the offending characters are renamed.
        assert!(AuthService::new(db_path).is_err());
    }
}
