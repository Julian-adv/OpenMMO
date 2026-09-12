use super::{unix_now, AuthError, AuthService, PricingMeeting, NPC_ACCOUNT_PREFIX};
use crate::metrics::{
    kst_day_start, AccountActivity, ArmorEnchantLeaderboard, ArmorEnchantLeaderboardEntry,
    ArmorEnchantSample, ArmorEnchantSeries, CharacterGoldSample, CharacterGoldSeries,
    CharacterLeaderboard, ConcurrentCounts, ConcurrentHistorySample, GoldHistory,
    GoldHistorySample, GoldLeaderboard, GoldLeaderboardEntry, GoldSample, GoldSource,
    LandLeaderboard, LandLeaderboardEntry, LandSample, LandSeries, LevelLeaderboard,
    LevelLeaderboardEntry, LevelSample, LevelSeries, PerAccountGoldHistory,
    PerAccountGoldHistorySample, PerAccountGoldSample, PriceIndexHistory, PriceMeeting,
    UniqueHistory, UniqueSample, WeaponEnchantLeaderboard, WeaponEnchantLeaderboardEntry,
    WeaponEnchantSample, WeaponEnchantSeries, DAY_SECONDS, SAMPLE_INTERVAL_SECONDS,
};
use rusqlite::{params, types::FromSql, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

enum LeaderboardMetric {
    Level,
    Gold,
    WeaponEnchant,
    ArmorEnchant,
    Land,
}

fn character_metric_source(metric: &str) -> &'static str {
    if metric == "land_plots" {
        "(SELECT c.*, COALESCE(land.land_plots, 0) AS land_plots FROM characters c
          LEFT JOIN (
            SELECT e.owner_id, COUNT(*) AS land_plots
            FROM land_estates e JOIN land_plots p ON p.estate_id = e.id
            GROUP BY e.owner_id
          ) land ON land.owner_id = c.id)"
    } else {
        "characters"
    }
}

fn unique_collection_started_at(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT started_at FROM unique_account_collection WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

impl AuthService {
    pub(super) fn ensure_item_sales_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS item_sale_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO item_sale_collection (id, started_at)
                VALUES (1, unixepoch());
             CREATE TABLE IF NOT EXISTS item_sale_samples (
                timestamp INTEGER NOT NULL,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                gold INTEGER NOT NULL CHECK (gold >= 0),
                PRIMARY KEY (timestamp, item_def_id)
             );",
        )?;
        if !Self::table_columns(&transaction, "item_sale_collection")?.contains("hourly") {
            transaction.execute_batch(
                "ALTER TABLE item_sale_collection ADD COLUMN hourly INTEGER NOT NULL DEFAULT 1;
                 CREATE TEMP TABLE hourly_item_sales AS
                    SELECT timestamp - timestamp % 3600 AS timestamp, item_def_id,
                           SUM(quantity) AS quantity, SUM(gold) AS gold
                    FROM item_sale_samples GROUP BY timestamp - timestamp % 3600, item_def_id;
                 DELETE FROM item_sale_samples;
                 INSERT INTO item_sale_samples (timestamp, item_def_id, quantity, gold)
                    SELECT * FROM hourly_item_sales;
                 DROP TABLE hourly_item_sales;",
            )?;
        }
        if !Self::table_columns(&transaction, "item_sale_samples")?.contains("source") {
            transaction.execute_batch(
                "ALTER TABLE item_sale_samples RENAME TO legacy_item_sale_samples;
                 CREATE TABLE item_sale_samples (
                    timestamp INTEGER NOT NULL,
                    source TEXT NOT NULL DEFAULT 'item_sale',
                    item_def_id TEXT NOT NULL,
                    quantity INTEGER NOT NULL CHECK (quantity > 0),
                    gold INTEGER NOT NULL CHECK (gold >= 0),
                    PRIMARY KEY (timestamp, source, item_def_id),
                    CHECK ((source = 'item_sale' AND item_def_id != '') OR
                           (source IN ('dungeon_chest', 'coin_pile', 'coin_pouch', 'npc_salary')
                            AND item_def_id = ''))
                 );
                 INSERT INTO item_sale_samples (timestamp, item_def_id, quantity, gold)
                    SELECT timestamp, item_def_id, quantity, gold FROM legacy_item_sale_samples;
                 DROP TABLE legacy_item_sale_samples;
                 ALTER TABLE item_sale_collection ADD COLUMN rewards_started_at INTEGER NOT NULL DEFAULT 0;
                 UPDATE item_sale_collection SET rewards_started_at = unixepoch();",
            )?;
        }
        transaction.commit()
    }

    pub fn record_gold_sources(
        &self,
        sales: &[crate::metrics::GoldSourceRecord],
    ) -> Result<(), AuthError> {
        if sales.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        let mut statement = transaction.prepare_cached(
            "INSERT INTO item_sale_samples (timestamp, source, item_def_id, quantity, gold)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(timestamp, source, item_def_id) DO UPDATE SET
                quantity = quantity + excluded.quantity, gold = gold + excluded.gold",
        )?;
        for sale in sales {
            let (source, item_def_id) = sale.source.storage_key();
            statement.execute(params![
                sale.timestamp - sale.timestamp.rem_euclid(SAMPLE_INTERVAL_SECONDS),
                source,
                item_def_id,
                sale.quantity,
                sale.gold
            ])?;
        }
        drop(statement);
        transaction.commit()?;
        Ok(())
    }

    pub fn item_gold_sources(
        &self,
        until: i64,
        hours: u32,
    ) -> Result<crate::metrics::ItemGoldSources, AuthError> {
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        let until = until - until.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        let from = until - i64::from(hours) * 3600;
        let (collection_started_at, rewards_started_at) = transaction.query_row(
            "SELECT started_at, rewards_started_at FROM item_sale_collection WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let defs = crate::item_defs::item_defs();
        let mut statement = transaction.prepare(
            "SELECT source, item_def_id, SUM(quantity), SUM(gold) AS total_gold
             FROM item_sale_samples WHERE timestamp >= ?1 AND timestamp < ?2
             GROUP BY source, item_def_id ORDER BY total_gold DESC, source ASC, item_def_id ASC",
        )?;
        let entries = statement
            .query_map(params![from, until], |row| {
                let kind: String = row.get(0)?;
                let item_def_id: String = row.get(1)?;
                let (source, name) = match kind.as_str() {
                    "item_sale" => {
                        let name = defs
                            .get(&item_def_id)
                            .map_or_else(|| item_def_id.clone(), |item| item.name.clone());
                        (GoldSource::ItemSale { item_def_id }, name)
                    }
                    "dungeon_chest" => (GoldSource::DungeonChest, "던전 보상 상자".into()),
                    "coin_pile" => (GoldSource::CoinPile, "동전 더미 획득".into()),
                    "coin_pouch" => (GoldSource::CoinPouch, "동전 주머니 개봉".into()),
                    "npc_salary" => (GoldSource::NpcSalary, "주민 NPC 급여".into()),
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok(crate::metrics::ItemGoldSource {
                    source,
                    name,
                    quantity: row.get(2)?,
                    gold: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let total_gold = entries
            .iter()
            .try_fold(0i64, |total, entry| total.checked_add(entry.gold))
            .ok_or_else(|| AuthError::Database("Gold source total overflow".into()))?;
        Ok(crate::metrics::ItemGoldSources {
            from,
            until,
            collection_started_at,
            rewards_started_at,
            total_gold,
            entries,
        })
    }

    pub(super) fn ensure_level_history_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::ensure_character_history_schema(conn, "level", 1)
    }

    pub(super) fn ensure_gold_history_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::ensure_character_history_schema(conn, "gold", 0)
    }

    pub(super) fn ensure_land_history_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::ensure_character_history_schema(conn, "land_plots", 0)
    }

    pub(super) fn ensure_weapon_enchant_history_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        let transaction = conn.unchecked_transaction()?;
        if !Self::table_columns(&transaction, "characters")?.contains("weapon_enchant") {
            transaction.execute(
                "ALTER TABLE characters ADD COLUMN weapon_enchant INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let weapon_ids: Vec<_> = crate::item_defs::item_defs()
            .all()
            .filter(|def| def.is_weapon())
            .map(|def| &def.id)
            .collect();
        let weapon_ids = serde_json::to_string(&weapon_ids)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        transaction.execute(
            "UPDATE characters SET weapon_enchant = MAX(0, COALESCE((
                SELECT MAX(enchant) FROM character_items WHERE character_id = characters.id
                    AND quantity > 0 AND item_def_id IN (SELECT value FROM json_each(?1))
             ), 0))",
            [weapon_ids],
        )?;
        Self::ensure_character_history_schema(&transaction, "weapon_enchant", 0)?;
        transaction.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_weapon_enchant_ranking
             ON characters(weapon_enchant DESC, id ASC)",
            [],
        )?;
        transaction.commit()
    }

    pub(super) fn ensure_armor_enchant_history_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        let transaction = conn.unchecked_transaction()?;
        if !Self::table_columns(&transaction, "characters")?.contains("armor_enchant") {
            transaction.execute(
                "ALTER TABLE characters ADD COLUMN armor_enchant INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let armor_slots: HashMap<_, _> = crate::item_defs::item_defs()
            .all()
            .filter(|def| def.is_armor())
            .filter_map(|def| def.equip_slot.map(|slot| (&def.id, slot)))
            .collect();
        let armor_slots = serde_json::to_string(&armor_slots)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        transaction.execute(
            "UPDATE characters SET armor_enchant = COALESCE((
                SELECT SUM(enchant) FROM (
                    SELECT MAX(0, MAX(items.enchant)) AS enchant
                    FROM character_items items
                    JOIN json_each(?1) definitions ON items.item_def_id = definitions.key
                    WHERE items.character_id = characters.id AND items.quantity > 0
                    GROUP BY definitions.value
                )
             ), 0)",
            [armor_slots],
        )?;
        Self::ensure_character_history_schema(&transaction, "armor_enchant", 0)?;
        transaction.execute(
            "CREATE INDEX IF NOT EXISTS idx_characters_armor_enchant_ranking
             ON characters(armor_enchant DESC, id ASC)",
            [],
        )?;
        transaction.commit()
    }

    fn ensure_character_history_schema(
        conn: &Connection,
        metric: &str,
        minimum: u32,
    ) -> Result<(), rusqlite::Error> {
        let source = character_metric_source(metric);
        let initial_value = if metric == "land_plots" {
            "0".to_owned()
        } else {
            format!("NEW.{metric}")
        };
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS character_{metric}_history (
                character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                timestamp INTEGER NOT NULL,
                {metric} INTEGER NOT NULL CHECK ({metric} >= {minimum}),
                PRIMARY KEY (character_id, timestamp)
             );
             INSERT INTO character_{metric}_history (character_id, timestamp, {metric})
             SELECT id, CAST(strftime('%s', 'now') AS INTEGER), {metric} FROM {source} c
             WHERE account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*'
                AND NOT EXISTS (SELECT 1 FROM character_{metric}_history WHERE character_id = c.id);
             CREATE TRIGGER IF NOT EXISTS character_{metric}_created AFTER INSERT ON characters
             WHEN NEW.account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*'
             BEGIN
                INSERT INTO character_{metric}_history VALUES (NEW.id, CAST(strftime('%s', 'now') AS INTEGER), {initial_value});
             END;
             DROP TRIGGER IF EXISTS character_{metric}_changed;
             CREATE TABLE IF NOT EXISTS character_metrics_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                sampled_at INTEGER NOT NULL
             );"
        ))
    }

    pub fn record_hourly_character_metrics(&self, now: i64) -> Result<bool, AuthError> {
        let mut conn = self.open_connection()?;
        let transaction =
            conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous: Option<i64> = transaction
            .query_row(
                "SELECT sampled_at FROM character_metrics_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let boundary = now - now.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        if previous.is_some_and(|previous| previous >= boundary) {
            return Ok(false);
        }
        Self::write_character_metric_samples(&transaction, now)?;
        transaction.execute(
            "INSERT INTO character_metrics_state (id, sampled_at) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET sampled_at = excluded.sampled_at",
            [now],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    fn write_character_metric_samples(conn: &Connection, now: i64) -> Result<(), rusqlite::Error> {
        for metric in [
            "level",
            "gold",
            "weapon_enchant",
            "armor_enchant",
            "land_plots",
        ] {
            let source = character_metric_source(metric);
            conn.execute(
                &format!(
                    "INSERT INTO character_{metric}_history (character_id, timestamp, {metric})
                 SELECT c.id, ?1, c.{metric} FROM {source} c
                 WHERE account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*' AND c.{metric} IS NOT (
                    SELECT h.{metric} FROM character_{metric}_history h
                    WHERE h.character_id = c.id ORDER BY timestamp DESC LIMIT 1
                 )
                 ON CONFLICT(character_id, timestamp) DO UPDATE SET {metric} = excluded.{metric}"
                ),
                [now],
            )?;
        }
        Ok(())
    }

    pub fn level_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<LevelLeaderboard, AuthError> {
        self.character_leaderboard(
            LeaderboardMetric::Level,
            hours,
            interval,
            |name, level, account_first_rank| LevelLeaderboardEntry {
                name,
                level,
                account_first_rank,
            },
            |name, started_at, samples| LevelSeries {
                name,
                started_at,
                samples: samples
                    .into_iter()
                    .map(|(timestamp, level)| LevelSample { timestamp, level })
                    .collect(),
            },
        )
    }

    pub fn gold_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<GoldLeaderboard, AuthError> {
        self.character_leaderboard(
            LeaderboardMetric::Gold,
            hours,
            interval,
            |name, gold, account_first_rank| GoldLeaderboardEntry {
                name,
                gold,
                account_first_rank,
            },
            |name, started_at, samples| CharacterGoldSeries {
                name,
                started_at,
                samples: samples
                    .into_iter()
                    .map(|(timestamp, gold)| CharacterGoldSample { timestamp, gold })
                    .collect(),
            },
        )
    }

    pub fn weapon_enchant_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<WeaponEnchantLeaderboard, AuthError> {
        self.character_leaderboard(
            LeaderboardMetric::WeaponEnchant,
            hours,
            interval,
            |name, weapon_enchant, account_first_rank| WeaponEnchantLeaderboardEntry {
                name,
                weapon_enchant,
                account_first_rank,
            },
            |name, started_at, samples| WeaponEnchantSeries {
                name,
                started_at,
                samples: samples
                    .into_iter()
                    .map(|(timestamp, weapon_enchant)| WeaponEnchantSample {
                        timestamp,
                        weapon_enchant,
                    })
                    .collect(),
            },
        )
    }

    pub fn armor_enchant_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<ArmorEnchantLeaderboard, AuthError> {
        self.character_leaderboard(
            LeaderboardMetric::ArmorEnchant,
            hours,
            interval,
            |name, armor_enchant, account_first_rank| ArmorEnchantLeaderboardEntry {
                name,
                armor_enchant,
                account_first_rank,
            },
            |name, started_at, samples| ArmorEnchantSeries {
                name,
                started_at,
                samples: samples
                    .into_iter()
                    .map(|(timestamp, armor_enchant)| ArmorEnchantSample {
                        timestamp,
                        armor_enchant,
                    })
                    .collect(),
            },
        )
    }

    pub fn land_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<LandLeaderboard, AuthError> {
        self.character_leaderboard(
            LeaderboardMetric::Land,
            hours,
            interval,
            |name, land_plots, account_first_rank| LandLeaderboardEntry {
                name,
                land_plots,
                account_first_rank,
            },
            |name, started_at, samples| LandSeries {
                name,
                started_at,
                samples: samples
                    .into_iter()
                    .map(|(timestamp, land_plots)| LandSample {
                        timestamp,
                        land_plots,
                    })
                    .collect(),
            },
        )
    }

    fn character_leaderboard<Value: FromSql, Entry, Series>(
        &self,
        metric: LeaderboardMetric,
        hours: u32,
        interval: i64,
        make_entry: fn(String, Value, usize) -> Entry,
        make_series: fn(String, i64, Vec<(i64, Value)>) -> Series,
    ) -> Result<CharacterLeaderboard<Entry, Series>, AuthError> {
        let (metric, order_by) = match metric {
            LeaderboardMetric::Level => ("level", "level DESC, xp DESC, id ASC"),
            LeaderboardMetric::Gold => ("gold", "gold DESC, id ASC"),
            LeaderboardMetric::WeaponEnchant => ("weapon_enchant", "weapon_enchant DESC, id ASC"),
            LeaderboardMetric::ArmorEnchant => ("armor_enchant", "armor_enchant DESC, id ASC"),
            LeaderboardMetric::Land => ("land_plots", "land_plots DESC, id ASC"),
        };
        let source = character_metric_source(metric);
        let filter = if metric == "land_plots" {
            "AND land_plots > 0"
        } else {
            ""
        };
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let mut statement = transaction.prepare(&format!(
            "SELECT character_name, {metric}, account_name, id FROM {source}
             WHERE account_name NOT GLOB ?1 {filter}
             ORDER BY {order_by} LIMIT 10",
        ))?;
        let rows = statement
            .query_map([format!("{NPC_ACCOUNT_PREFIX}*")], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Value>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let until = unix_now();
        let from = until - i64::from(hours) * 3600;
        let mut first_ranks = HashMap::new();
        let mut entries = Vec::new();
        let mut series = Vec::new();
        let mut history = transaction.prepare(&format!(
            "SELECT timestamp, {metric} FROM character_{metric}_history
             WHERE character_id = ?1 AND timestamp IN (
                SELECT MIN(timestamp) FROM character_{metric}_history WHERE character_id = ?1 AND timestamp <= ?3
                UNION SELECT MAX(timestamp) FROM character_{metric}_history WHERE character_id = ?1 AND timestamp <= ?2
                UNION SELECT MAX(timestamp) FROM character_{metric}_history
                    WHERE character_id = ?1 AND timestamp > ?2 AND timestamp <= ?3 GROUP BY timestamp / ?4
             ) ORDER BY timestamp",
        ))?;
        for (index, (name, value, account, id)) in rows.into_iter().enumerate() {
            let recorded = history
                .query_map(params![id, from, until, interval], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Value>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(&(started_at, _)) = recorded.first() {
                let baseline = recorded
                    .iter()
                    .rposition(|(timestamp, _)| *timestamp <= from)
                    .unwrap_or(0);
                let samples = recorded
                    .into_iter()
                    .skip(baseline)
                    .map(|(timestamp, value)| (timestamp.max(from), value))
                    .collect();
                series.push(make_series(name.clone(), started_at, samples));
            }
            entries.push(make_entry(
                name,
                value,
                *first_ranks.entry(account).or_insert(index + 1),
            ));
        }
        Ok(CharacterLeaderboard {
            timestamp: until,
            from,
            sample_interval_seconds: interval,
            entries,
            series,
        })
    }

    pub fn per_account_gold_history(
        &self,
        until: i64,
        hours: u32,
        interval: i64,
        active_days: u32,
    ) -> Result<PerAccountGoldHistory, AuthError> {
        let from = until - i64::from(hours) * 3600;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        let joined = "WITH normalized AS (
            SELECT g.ts, g.total_gold,
                CASE ?1 WHEN 1 THEN u.day_accounts WHEN 7 THEN u.week_accounts
                    WHEN 30 THEN u.month_accounts WHEN 180 THEN u.half_year_accounts
                    WHEN 365 THEN u.year_accounts END AS accounts
            FROM gold_snapshots g
            JOIN unique_account_daily_samples u
                ON u.timestamp = g.ts - (g.ts + 9 * 3600) % 86400
            WHERE g.ts >= ?2 AND g.ts <= ?3
        )";
        let latest_timestamp: Option<i64> = transaction.query_row(
            "SELECT MAX(ts) FROM gold_snapshots WHERE ts <= ?1",
            [until],
            |row| row.get(0),
        )?;
        let latest = transaction
            .query_row(
                &format!(
                    "{joined}
                SELECT ts, total_gold, accounts, total_gold * 1.0 / accounts
                FROM normalized WHERE accounts > 0"
                ),
                params![active_days, latest_timestamp, latest_timestamp],
                |row| {
                    Ok(PerAccountGoldSample {
                        timestamp: row.get(0)?,
                        total_gold: row.get(1)?,
                        accounts: row.get(2)?,
                        gold_per_account: row.get(3)?,
                    })
                },
            )
            .optional()?;
        let mut statement = transaction.prepare(&format!(
            "{joined}
            SELECT MAX((ts / ?4) * ?4, ?2), AVG(total_gold * 1.0 / accounts),
                MAX(total_gold * 1.0 / accounts), COUNT(*)
            FROM normalized WHERE accounts > 0
            GROUP BY ts / ?4 ORDER BY ts / ?4"
        ))?;
        let samples = statement
            .query_map(params![active_days, from, until, interval], |row| {
                Ok(PerAccountGoldHistorySample {
                    timestamp: row.get(0)?,
                    gold_per_account: row.get(1)?,
                    peak_gold_per_account: row.get(2)?,
                    sample_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PerAccountGoldHistory {
            from,
            until,
            sample_interval_seconds: interval,
            window_seconds: i64::from(active_days) * DAY_SECONDS,
            collection_started_at,
            latest,
            samples,
        })
    }

    pub fn price_index_history(
        &self,
        until: i64,
        hours: u32,
    ) -> Result<PriceIndexHistory, AuthError> {
        let from = until - i64::from(hours) * 3600;
        let current_index_percent = self.load_pricing_state()?.index_percent;
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT ts, game_day, m_prev, m_now, growth, index_before, index_after
             FROM pricing_history WHERE ts > ?1 AND ts <= ?2 ORDER BY ts, rowid",
        )?;
        let meetings = statement
            .query_map(params![from, until], |row| {
                Ok(PriceMeeting {
                    timestamp: row.get(0)?,
                    meeting: PricingMeeting {
                        game_day: row.get(1)?,
                        m_prev: row.get(2)?,
                        m_now: row.get(3)?,
                        growth: row.get(4)?,
                        index_before: row.get(5)?,
                        index_after: row.get(6)?,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let baseline_index_percent = meetings
            .first()
            .map_or(current_index_percent, |first| first.meeting.index_before);
        Ok(PriceIndexHistory {
            from,
            until,
            current_index_percent,
            baseline_index_percent,
            meetings,
        })
    }

    pub fn gold_history(
        &self,
        until: i64,
        hours: u32,
        interval: i64,
    ) -> Result<GoldHistory, AuthError> {
        let from = until - i64::from(hours) * 3600;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let latest = transaction
            .query_row(
                "SELECT ts, total_gold FROM gold_snapshots WHERE ts <= ?1 ORDER BY ts DESC LIMIT 1",
                [until],
                |row| {
                    Ok(GoldSample {
                        timestamp: row.get(0)?,
                        total_gold: row.get(1)?,
                    })
                },
            )
            .optional()?;
        let mut statement = transaction.prepare(
            "SELECT MAX((ts / ?3) * ?3, ?1), AVG(total_gold), MAX(total_gold), COUNT(*)
             FROM gold_snapshots WHERE ts >= ?1 AND ts <= ?2
             GROUP BY ts / ?3 ORDER BY ts / ?3",
        )?;
        let samples = statement
            .query_map(params![from, until, interval], |row| {
                Ok(GoldHistorySample {
                    timestamp: row.get(0)?,
                    total_gold: row.get(1)?,
                    peak_gold: row.get(2)?,
                    sample_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GoldHistory {
            from,
            until,
            sample_interval_seconds: interval,
            latest,
            samples,
        })
    }

    pub(super) fn ensure_account_activity_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS unique_account_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO unique_account_collection VALUES (1, CAST(strftime('%s', 'now') AS INTEGER));
             CREATE TABLE IF NOT EXISTS account_activity_sessions (
                id TEXT PRIMARY KEY,
                account_name TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL CHECK (last_seen_at >= started_at)
             );
             CREATE INDEX IF NOT EXISTS account_activity_last_seen ON account_activity_sessions(last_seen_at);
             CREATE TABLE IF NOT EXISTS unique_account_daily_samples (
                timestamp INTEGER PRIMARY KEY,
                day_accounts INTEGER NOT NULL CHECK (day_accounts >= 0),
                week_accounts INTEGER NOT NULL CHECK (week_accounts >= 0),
                month_accounts INTEGER NOT NULL CHECK (month_accounts >= 0),
                half_year_accounts INTEGER NOT NULL CHECK (half_year_accounts >= 0),
                year_accounts INTEGER NOT NULL CHECK (year_accounts >= 0)
             );",
        )
    }

    pub fn record_account_activities(
        &self,
        activities: &[AccountActivity],
    ) -> Result<(), AuthError> {
        if activities.is_empty() {
            return Ok(());
        }
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO account_activity_sessions (id, account_name, started_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET last_seen_at = MAX(last_seen_at, excluded.last_seen_at)",
            )?;
            for activity in activities {
                statement.execute(params![
                    activity.id,
                    activity.account_name.to_ascii_lowercase(),
                    activity.started_at,
                    activity.last_seen_at
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn backfill_daily_unique_accounts(&self, now: i64) -> Result<usize, AuthError> {
        let missing_dates: Vec<i64> = {
            let conn = self.open_connection()?;
            let collection_started_at = unique_collection_started_at(&conn)?;
            let first = kst_day_start(collection_started_at) + DAY_SECONDS;
            let until = kst_day_start(now);
            let mut statement = conn.prepare(
                "SELECT timestamp FROM unique_account_daily_samples
                 WHERE timestamp >= ?1 AND timestamp <= ?2",
            )?;
            let saved_dates = statement
                .query_map(params![first, until], |row| row.get(0))?
                .collect::<Result<HashSet<i64>, _>>()?;
            (first..=until)
                .step_by(DAY_SECONDS as usize)
                .filter(|timestamp| !saved_dates.contains(timestamp))
                .collect()
        };
        let mut count = 0;
        for timestamp in missing_dates {
            count += usize::from(self.aggregate_daily_unique_accounts(timestamp)?);
        }
        Ok(count)
    }

    pub fn aggregate_daily_unique_accounts(&self, now: i64) -> Result<bool, AuthError> {
        let timestamp = kst_day_start(now);
        let mut conn = self.open_connection()?;
        let transaction =
            conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        if timestamp <= collection_started_at
            || transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM unique_account_daily_samples WHERE timestamp = ?1)",
                [timestamp],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Ok(false);
        }
        transaction.execute(
            "WITH recent_accounts AS (
                SELECT account_name, MAX(last_seen_at) AS last_seen_at
                FROM account_activity_sessions
                WHERE started_at < ?1 AND last_seen_at >= ?1 - 365 * 86400
                GROUP BY account_name
             )
             INSERT INTO unique_account_daily_samples
                (timestamp, day_accounts, week_accounts, month_accounts, half_year_accounts, year_accounts)
             SELECT ?1,
                COUNT(CASE WHEN last_seen_at >= ?1 - 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 7 * 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 30 * 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 180 * 86400 THEN 1 END),
                COUNT(*) FROM recent_accounts", [timestamp],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn unique_account_history(&self, now: i64, days: u32) -> Result<UniqueHistory, AuthError> {
        let until = kst_day_start(now);
        let window = i64::from(days) * DAY_SECONDS;
        let from = until - window;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        let last_aggregated_at = transaction.query_row(
            "SELECT MAX(timestamp) FROM unique_account_daily_samples WHERE timestamp <= ?1",
            [until],
            |row| row.get(0),
        )?;
        let mut statement = transaction.prepare(
            "SELECT timestamp,
                CASE ?3 WHEN 1 THEN day_accounts WHEN 7 THEN week_accounts
                    WHEN 30 THEN month_accounts WHEN 180 THEN half_year_accounts
                    WHEN 365 THEN year_accounts END
             FROM unique_account_daily_samples
             WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp",
        )?;
        let samples = statement
            .query_map(params![from, until, days], |row| {
                Ok(UniqueSample {
                    timestamp: row.get(0)?,
                    accounts: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UniqueHistory {
            from,
            until,
            window_seconds: window,
            sample_interval_seconds: DAY_SECONDS,
            collection_started_at,
            last_aggregated_at,
            samples,
        })
    }

    pub(super) fn ensure_concurrent_samples_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS concurrent_account_samples (
                timestamp INTEGER PRIMARY KEY,
                accounts INTEGER NOT NULL CHECK (accounts >= 0)
            )",
        )?;
        let columns = Self::table_columns(conn, "concurrent_account_samples")?;
        if !columns.contains("web_accounts") {
            conn.execute(
                "ALTER TABLE concurrent_account_samples
                 ADD COLUMN web_accounts INTEGER NOT NULL DEFAULT 0 CHECK (web_accounts >= 0)",
                [],
            )?;
        }
        if !columns.contains("agent_accounts") {
            conn.execute(
                "ALTER TABLE concurrent_account_samples
                 ADD COLUMN agent_accounts INTEGER NOT NULL DEFAULT 0
                 CHECK (agent_accounts >= 0 AND web_accounts + agent_accounts <= accounts)",
                [],
            )?;
        }
        Ok(())
    }

    pub fn record_concurrent_accounts(
        &self,
        now: i64,
        counts: ConcurrentCounts,
    ) -> Result<(), AuthError> {
        let timestamp = now - now.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        self.open_connection()?.execute(
            "INSERT INTO concurrent_account_samples (timestamp, accounts, web_accounts, agent_accounts)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(timestamp) DO UPDATE SET accounts = excluded.accounts,
                 web_accounts = excluded.web_accounts, agent_accounts = excluded.agent_accounts",
            params![timestamp, counts.total(), counts.web_accounts, counts.agent_accounts],
        )?;
        Ok(())
    }

    pub fn concurrent_account_samples(
        &self,
        from: i64,
        until: i64,
        interval: i64,
    ) -> Result<Vec<ConcurrentHistorySample>, AuthError> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "WITH buckets AS (
                SELECT timestamp / ?3 AS bucket, AVG(accounts) AS accounts,
                       MAX(accounts) AS peak_accounts, COUNT(*) AS sample_count,
                       AVG(web_accounts) AS web_accounts, AVG(agent_accounts) AS agent_accounts,
                       AVG(accounts - web_accounts - agent_accounts) AS other_accounts
                FROM concurrent_account_samples
                WHERE timestamp >= ?1 AND timestamp <= ?2
                GROUP BY timestamp / ?3
             )
             SELECT MAX(bucket * ?3, ?1), accounts, peak_accounts,
                    (SELECT MIN(timestamp) FROM concurrent_account_samples
                     WHERE timestamp >= MAX(bucket * ?3, ?1)
                       AND timestamp <= MIN((bucket + 1) * ?3 - 1, ?2)
                       AND accounts = buckets.peak_accounts),
                    sample_count, web_accounts, agent_accounts, other_accounts
             FROM buckets ORDER BY bucket",
        )?;
        let rows = statement.query_map(params![from, until, interval], |row| {
            Ok(ConcurrentHistorySample {
                timestamp: row.get(0)?,
                accounts: row.get(1)?,
                peak_accounts: row.get(2)?,
                peak_timestamp: row.get(3)?,
                sample_count: row.get(4)?,
                web_accounts: row.get(5)?,
                agent_accounts: row.get(6)?,
                other_accounts: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn land_history_starts_from_existing_plots_and_tracks_hourly_ownership_changes() {
        let path = crate::test_util::unique_temp_dir("land_history").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_land_plots_created;
             DROP TABLE character_land_plots_history;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name) VALUES
                (1, 'player', 'Hero'), (2, 'player', 'Alt'), (3, 'npc_test', 'Npc'), (4, 'npcxplayer', 'Other');
             INSERT INTO land_estates (id, owner_id, account_name, grade, created_at) VALUES
                (1, 1, 'player', 1, 100), (2, 1, 'player', 2, 100),
                (3, 3, 'npc_test', 1, 100), (4, 4, 'npcxplayer', 1, 100), (5, 4, 'npcxplayer', 2, 100);
             INSERT INTO land_plots (tile_x, tile_z, quadrant, estate_id) VALUES
                (0, 0, 0, 1), (0, 0, 1, 1), (0, 0, 2, 2), (0, 0, 3, 3), (1, 0, 0, 4);",
        ).unwrap();
        let now = unix_now();
        AuthService::ensure_land_history_schema(&conn).unwrap();
        let initial = auth.land_leaderboard(168, 3600).unwrap();
        assert_eq!(
            initial
                .entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.land_plots))
                .collect::<Vec<_>>(),
            [("Hero", 3), ("Other", 1)]
        );
        assert!((now..=unix_now()).contains(&initial.series[0].started_at));
        assert_eq!(initial.series[0].samples[0].land_plots, 3);
        let recorded = || {
            conn.prepare("SELECT character_id, timestamp, land_plots FROM character_land_plots_history ORDER BY character_id, timestamp")
                .unwrap().query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, u32>(2)?)))
                .unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert_eq!(recorded().len(), 3);
        assert!(!recorded().iter().any(|row| row.0 == 3));
        conn.execute(
            "UPDATE character_land_plots_history SET timestamp = ?1",
            [now - DAY_SECONDS],
        )
        .unwrap();
        let baseline = recorded();
        AuthService::ensure_land_history_schema(&conn).unwrap();
        assert_eq!(recorded(), baseline);
        conn.execute("UPDATE land_estates SET owner_id = 2 WHERE id = 2", [])
            .unwrap();
        let transferred = auth.land_leaderboard(168, 3600).unwrap();
        assert_eq!(
            transferred
                .entries
                .iter()
                .map(|entry| (
                    entry.name.as_str(),
                    entry.land_plots,
                    entry.account_first_rank
                ))
                .collect::<Vec<_>>(),
            [("Hero", 2, 1), ("Alt", 1, 1), ("Other", 1, 3)]
        );
        assert_eq!(recorded(), baseline);
        assert!(auth.record_hourly_character_metrics(now - 7200).unwrap());
        assert_eq!(recorded().len(), 5);
        conn.execute("DELETE FROM land_estates WHERE id = 1", [])
            .unwrap();
        assert!(auth.record_hourly_character_metrics(now - 3600).unwrap());
        assert!(recorded().contains(&(1, now - 3600, 0)));
        let changed = recorded();
        assert!(auth.record_hourly_character_metrics(now).unwrap());
        assert_eq!(recorded(), changed);
        conn.execute(
            "UPDATE characters SET character_name = 'Renamed' WHERE id = 2",
            [],
        )
        .unwrap();
        let reopened = AuthService::new(path).unwrap();
        assert_eq!(recorded(), changed);
        let result = reopened.land_leaderboard(168, 3600).unwrap();
        assert_eq!(result.entries[0].name, "Renamed");
        assert_eq!(
            result.series[0]
                .samples
                .iter()
                .map(|sample| sample.land_plots)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        conn.execute("DELETE FROM characters WHERE id = 2", [])
            .unwrap();
        assert!(!recorded().iter().any(|row| row.0 == 2));
        assert_eq!(
            reopened.land_leaderboard(168, 3600).unwrap().entries.len(),
            1
        );
        conn.execute("INSERT INTO characters (id, account_name, character_name) VALUES (5, 'player', 'NewHero')", []).unwrap();
        assert!(recorded().iter().any(|row| row.0 == 5 && row.2 == 0));
    }

    #[test]
    fn character_metrics_write_only_on_hourly_sampling_and_retry_atomically() {
        let path = crate::test_util::unique_temp_dir("hourly_character_metrics").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test');
             INSERT INTO characters (id, account_name, character_name) VALUES (1, 'player', 'Hero'), (2, 'npc_test', 'Npc');",
        ).unwrap();
        let now = unix_now();
        for metric in ["level", "gold", "weapon_enchant", "armor_enchant"] {
            conn.execute(
                &format!("UPDATE character_{metric}_history SET timestamp = ?1"),
                [now - 7200],
            )
            .unwrap();
        }
        assert!(auth.record_hourly_character_metrics(now - 3600).unwrap());
        for value in 1..=50 {
            conn.execute("UPDATE characters SET gold = ?1, level = ?1, weapon_enchant = ?1, armor_enchant = ?1", [value]).unwrap();
        }
        for metric in ["level", "gold", "weapon_enchant", "armor_enchant"] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM character_{metric}_history"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "{metric} writes must wait for the hourly job");
        }
        conn.execute_batch("CREATE TRIGGER reject_hourly_metrics BEFORE INSERT ON character_gold_history BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(auth.record_hourly_character_metrics(now).is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_level_history", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        conn.execute_batch("DROP TRIGGER reject_hourly_metrics;")
            .unwrap();
        assert!(auth.record_hourly_character_metrics(now).unwrap());
        for metric in ["level", "gold", "weapon_enchant", "armor_enchant"] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM character_{metric}_history"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 2);
            let value: i64 = conn.query_row(&format!("SELECT {metric} FROM character_{metric}_history ORDER BY timestamp DESC LIMIT 1"), [], |r| r.get(0)).unwrap();
            assert_eq!(value, 50);
        }
        conn.execute("UPDATE characters SET gold = 99", []).unwrap();
        assert!(!auth.record_hourly_character_metrics(now).unwrap());
        let reopened = AuthService::new(path).unwrap();
        assert!(!reopened.record_hourly_character_metrics(now).unwrap());
        assert!(reopened
            .record_hourly_character_metrics(now + 3600)
            .unwrap());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_gold_history", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 3);
        assert!(reopened
            .record_hourly_character_metrics(now + 7200)
            .unwrap());
        let unchanged: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_gold_history", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(unchanged, count);
    }

    #[test]
    fn gold_source_migration_preserves_legacy_sales_and_collection_dates() {
        for hourly in [false, true] {
            let path = crate::test_util::unique_temp_dir("gold_source_migration").join("game.db");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE item_sale_collection (id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL);
                 INSERT INTO item_sale_collection VALUES (1, 3000);
                 CREATE TABLE item_sale_samples (
                    timestamp INTEGER NOT NULL, item_def_id TEXT NOT NULL,
                    quantity INTEGER NOT NULL, gold INTEGER NOT NULL,
                    PRIMARY KEY (timestamp, item_def_id)
                 );",
            ).unwrap();
            if hourly {
                conn.execute_batch(
                    "ALTER TABLE item_sale_collection ADD COLUMN hourly INTEGER NOT NULL DEFAULT 1;
                     INSERT INTO item_sale_samples VALUES (3600, 'iron_sword', 3, 13000),
                        (7200, 'iron_sword', 1, 4000);",
                )
                .unwrap();
            } else {
                conn.execute_batch(
                    "INSERT INTO item_sale_samples VALUES (3601, 'iron_sword', 2, 8000),
                        (3650, 'iron_sword', 1, 5000), (7200, 'iron_sword', 1, 4000);",
                )
                .unwrap();
            }
            let auth = AuthService::new(path.clone()).unwrap();
            let sources = auth.item_gold_sources(7200, 1).unwrap();
            assert_eq!(sources.collection_started_at, 3000);
            assert!(sources.rewards_started_at >= unix_now() - 60);
            assert_eq!(
                (sources.entries[0].quantity, sources.total_gold),
                (3, 13000)
            );
            assert_eq!(
                sources.entries[0].source,
                GoldSource::ItemSale {
                    item_def_id: "iron_sword".into()
                }
            );
            conn.execute(
                "UPDATE item_sale_collection SET rewards_started_at = 6000",
                [],
            )
            .unwrap();
            let reopened = AuthService::new(path).unwrap();
            let sources = reopened.item_gold_sources(7200, 1).unwrap();
            assert_eq!(
                (sources.collection_started_at, sources.rewards_started_at),
                (3000, 6000)
            );
            assert_eq!(
                (sources.entries[0].quantity, sources.total_gold),
                (3, 13000)
            );
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM item_sale_samples", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn gold_sources_separate_rewards_from_item_ids_and_respect_hour_boundaries() {
        let path = crate::test_util::unique_temp_dir("gold_source_boundaries").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let sources = [
            GoldSource::DungeonChest,
            GoldSource::CoinPile,
            GoldSource::CoinPouch,
            GoldSource::NpcSalary,
            GoldSource::ItemSale {
                item_def_id: "coin_pile".into(),
            },
        ];
        let mut records = Vec::new();
        for source in sources {
            for (timestamp, gold) in [(3599, 999), (3600, 10), (7199, 20), (7200, 999)] {
                records.push(crate::metrics::GoldSourceRecord {
                    timestamp,
                    source: source.clone(),
                    quantity: 1,
                    gold,
                });
            }
        }
        auth.record_gold_sources(&records).unwrap();
        let result = auth.item_gold_sources(7250, 1).unwrap();
        assert_eq!(
            (result.from, result.until, result.total_gold),
            (3600, 7200, 150)
        );
        assert_eq!(result.entries.len(), 5);
        for entry in result.entries {
            assert_eq!((entry.quantity, entry.gold), (2, 30));
            let value = serde_json::to_value(&entry).unwrap();
            assert_eq!(value["source"], entry.source.storage_key().0);
            assert_eq!(
                value.get("item_def_id").is_some(),
                matches!(entry.source, GoldSource::ItemSale { .. })
            );
        }
    }

    #[test]
    fn item_gold_sources_aggregate_actual_payouts_with_exact_period_boundaries() {
        let path = crate::test_util::unique_temp_dir("item_gold_sources").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let now = unix_now() - unix_now().rem_euclid(3600);
        let sale = |timestamp, id: &str, quantity, gold| crate::metrics::GoldSourceRecord {
            timestamp,
            source: GoldSource::ItemSale {
                item_def_id: id.into(),
            },
            quantity,
            gold,
        };
        let sales = [
            sale(now - 3601, "iron_sword", 1, 99999),
            sale(now - 3600, "iron_sword", 1, 4000),
            sale(now - 1, "iron_sword", 1, 5000),
            sale(now - 1, "healing_potion", 20, 4800),
            sale(now - 1, "healing_potion", 1, 300),
            sale(now - 1, "removed_item", 1, 5100),
            sale(now + 1, "iron_sword", 1, 99999),
        ];
        auth.record_gold_sources(&sales).unwrap();
        let sources = auth.item_gold_sources(now, 1).unwrap();
        assert_eq!(sources.total_gold, 19200);
        assert_eq!(
            sources
                .entries
                .iter()
                .map(|e| (e.source.storage_key().1, e.quantity, e.gold))
                .collect::<Vec<_>>(),
            [
                ("iron_sword", 2, 9000),
                ("healing_potion", 21, 5100),
                ("removed_item", 1, 5100)
            ]
        );
        assert_eq!(
            sources.entries[0].name,
            crate::item_defs::item_defs()
                .get("iron_sword")
                .unwrap()
                .name
        );
        assert_eq!(sources.entries[2].name, "removed_item");
        let reopened = AuthService::new(path).unwrap();
        assert_eq!(
            reopened
                .item_gold_sources(now, 1)
                .unwrap()
                .collection_started_at,
            sources.collection_started_at
        );
        assert_eq!(
            reopened.item_gold_sources(now, 1).unwrap().total_gold,
            19200
        );
        let empty = auth.item_gold_sources(now + 7200, 1).unwrap();
        assert!(empty.entries.is_empty());
        assert_eq!(empty.total_gold, 0);

        let invalid = [
            sale(now - 1, "iron_sword", 1, 4000),
            sale(now - 1, "bad", 0, 1),
        ];
        assert!(auth.record_gold_sources(&invalid).is_err());
        assert_eq!(auth.item_gold_sources(now, 1).unwrap().total_gold, 19200);
    }

    fn inventory_item(
        item_def_id: &str,
        enchant: i32,
        equip_slot: Option<&str>,
    ) -> crate::auth::ItemRow {
        crate::auth::ItemRow {
            item_def_id: item_def_id.into(),
            quantity: 1,
            equip_slot: equip_slot.map(str::to_owned),
            enchant,
            cape_color: None,
            cape_texture: None,
            locked: false,
        }
    }

    #[test]
    fn weapon_enchant_history_tracks_the_strongest_owned_weapon_atomically() {
        let path = crate::test_util::unique_temp_dir("weapon_enchant_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_weapon_enchant_created;
             DROP TRIGGER IF EXISTS character_weapon_enchant_changed;
             DROP TABLE character_weapon_enchant_history;
             DROP INDEX idx_characters_weapon_enchant_ranking;
             ALTER TABLE characters DROP COLUMN weapon_enchant;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, created_at)
             VALUES (1, 'player', 'Hero', 100), (2, 'npc_test', 'Npc', 100), (3, 'npcxplayer', 'NoWeapon', 100);
             INSERT INTO character_items (character_id, item_def_id, quantity, enchant, equip_slot)
             VALUES (1, 'iron_sword', 1, 7, NULL), (1, 'worn_iron_sword', 1, 3, 'main_hand'),
                    (1, 'wooden_shield', 1, 20, 'off_hand'), (1, 'missing_item', 1, 99, NULL),
                    (1, 'iron_sword', 0, 50, NULL), (2, 'iron_sword', 1, 99, NULL),
                    (3, 'wooden_shield', 1, 30, NULL);",
        ).unwrap();
        let before = unix_now();
        AuthService::ensure_weapon_enchant_history_schema(&conn).unwrap();
        let initial = auth.weapon_enchant_leaderboard(168, 3600).unwrap();
        assert_eq!(initial.entries.len(), 2);
        assert_eq!(initial.entries[0].weapon_enchant, 7);
        assert_eq!(initial.entries[1].weapon_enchant, 0);
        assert!((before..=unix_now()).contains(&initial.series[0].started_at));
        conn.execute(
            "UPDATE character_weapon_enchant_history SET timestamp = timestamp - 86400",
            [],
        )
        .unwrap();
        let mut items = vec![
            inventory_item("iron_sword", 7, None),
            inventory_item("worn_iron_sword", 3, Some("main_hand")),
            inventory_item("wooden_shield", 20, Some("off_hand")),
        ];
        let save = |items: &[crate::auth::ItemRow]| {
            auth.save_batch(&[], &[(1, items.to_vec())], &[], &[], None)
                .unwrap();
            AuthService::write_character_metric_samples(&conn, unix_now()).unwrap()
        };
        let count = || {
            conn.query_row(
                "SELECT COUNT(*) FROM character_weapon_enchant_history WHERE character_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        save(&items);
        assert_eq!(count(), 1);
        items[0].equip_slot = Some("main_hand".into());
        items[1].equip_slot = None;
        save(&items);
        assert_eq!(count(), 1);
        items.push(inventory_item("torch", 9, Some("off_hand")));
        save(&items);
        assert_eq!(count(), 2);
        assert_eq!(
            auth.weapon_enchant_leaderboard(168, 3600).unwrap().entries[0].weapon_enchant,
            9
        );
        assert!(auth
            .save_batch(
                &[],
                &[
                    (1, vec![]),
                    (99999, vec![inventory_item("iron_sword", 12, None)])
                ],
                &[],
                &[],
                None
            )
            .is_err());
        assert_eq!(auth.load_inventory(1).unwrap(), items);
        assert_eq!(
            auth.weapon_enchant_leaderboard(168, 3600).unwrap().entries[0].weapon_enchant,
            9
        );
        save(&[inventory_item("worn_iron_sword", 3, None)]);
        save(&[]);
        assert_eq!(
            auth.weapon_enchant_leaderboard(168, 3600).unwrap().series[0]
                .samples
                .last()
                .unwrap()
                .weapon_enchant,
            0
        );
        let recorded_count = count();
        conn.execute(
            "UPDATE characters SET character_name = 'Renamed' WHERE id = 1",
            [],
        )
        .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(count(), recorded_count);
        assert_eq!(
            auth.weapon_enchant_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(count(), 0);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_weapon_enchant_history WHERE character_id = 2",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn armor_enchant_history_sums_slot_maxima_atomically() {
        let path = crate::test_util::unique_temp_dir("armor_enchant_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_armor_enchant_created;
             DROP TRIGGER IF EXISTS character_armor_enchant_changed;
             DROP TABLE character_armor_enchant_history;
             DROP INDEX idx_characters_armor_enchant_ranking;
             ALTER TABLE characters DROP COLUMN armor_enchant;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, created_at)
             VALUES (1, 'player', 'Hero', 100), (2, 'npc_test', 'Npc', 100), (3, 'npcxplayer', 'NoArmor', 100);
             INSERT INTO character_items (character_id, item_def_id, quantity, enchant, equip_slot)
             VALUES (1, 'leather_helmet', 1, 2, NULL), (1, 'iron_helmet', 1, 1, 'head'),
                    (1, 'breastplate', 1, 3, 'chest'), (1, 'wooden_shield', 1, 4, 'off_hand'),
                    (1, 'leather_gloves', 1, 5, NULL), (1, 'leather_pants', 1, 6, NULL),
                    (1, 'iron_boots', 1, 7, 'boots'), (1, 'leather_boots', 0, 99, NULL),
                    (1, 'iron_sword', 1, 99, 'main_hand'), (1, 'ring_of_protection', 1, 99, 'ring'),
                    (1, 'missing_item', 1, 99, NULL), (1, 'raven_shield', 1, -3, NULL),
                    (2, 'breastplate', 1, 99, NULL), (3, 'iron_sword', 1, 99, NULL);",
        ).unwrap();
        let before = unix_now();
        AuthService::ensure_armor_enchant_history_schema(&conn).unwrap();
        let initial = auth.armor_enchant_leaderboard(168, 3600).unwrap();
        assert_eq!(initial.entries.len(), 2);
        assert_eq!(initial.entries[0].armor_enchant, 27);
        assert_eq!(initial.entries[1].armor_enchant, 0);
        assert!((before..=unix_now()).contains(&initial.series[0].started_at));
        conn.execute(
            "UPDATE character_armor_enchant_history SET timestamp = timestamp - 86400",
            [],
        )
        .unwrap();
        let mut items = vec![
            inventory_item("leather_helmet", 2, None),
            inventory_item("iron_helmet", 1, Some("head")),
            inventory_item("breastplate", 3, Some("chest")),
            inventory_item("wooden_shield", 4, Some("off_hand")),
            inventory_item("leather_gloves", 5, None),
            inventory_item("leather_pants", 6, None),
            inventory_item("iron_boots", 7, Some("boots")),
        ];
        let save = |items: &[crate::auth::ItemRow]| {
            auth.save_batch(&[], &[(1, items.to_vec())], &[], &[], None)
                .unwrap();
            AuthService::write_character_metric_samples(&conn, unix_now()).unwrap()
        };
        let count = || {
            conn.query_row(
                "SELECT COUNT(*) FROM character_armor_enchant_history WHERE character_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        save(&items);
        assert_eq!(count(), 1);
        items[0].equip_slot = Some("head".into());
        items[1].equip_slot = None;
        save(&items);
        assert_eq!(count(), 1);
        items[1].enchant = 9;
        save(&items);
        assert_eq!(count(), 2);
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().entries[0].armor_enchant,
            34
        );
        assert!(auth
            .save_batch(
                &[],
                &[
                    (1, vec![]),
                    (99999, vec![inventory_item("breastplate", 12, None)])
                ],
                &[],
                &[],
                None
            )
            .is_err());
        assert_eq!(auth.load_inventory(1).unwrap(), items);
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().entries[0].armor_enchant,
            34
        );
        items.retain(|item| item.item_def_id != "iron_helmet");
        save(&items);
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().entries[0].armor_enchant,
            27
        );
        save(&[]);
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().series[0]
                .samples
                .last()
                .unwrap()
                .armor_enchant,
            0
        );
        save(&[
            inventory_item("iron_helmet", i32::MAX, None),
            inventory_item("breastplate", i32::MAX, None),
            inventory_item("wooden_shield", i32::MAX, None),
            inventory_item("iron_boots", -1, None),
        ]);
        let total = 3 * i32::MAX as u64;
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().entries[0].armor_enchant,
            total
        );
        let recorded_count = count();
        conn.execute(
            "UPDATE characters SET character_name = 'Renamed' WHERE id = 1",
            [],
        )
        .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(count(), recorded_count);
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        assert_eq!(
            auth.armor_enchant_leaderboard(168, 3600).unwrap().entries[0].armor_enchant,
            total
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(count(), 0);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_armor_enchant_history WHERE character_id = 2",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn level_history_seeds_existing_characters_and_tracks_saved_changes() {
        let path = crate::test_util::unique_temp_dir("level_history_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_level_created;
             DROP TRIGGER IF EXISTS character_level_changed;
             DROP TABLE character_level_history;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, level, created_at)
             VALUES (1, 'player', 'Hero', 10, 100), (2, 'npc_test', 'Npc', 99, 100);",
        )
        .unwrap();
        let before = unix_now();
        AuthService::ensure_level_history_schema(&conn).unwrap();
        let baseline: (i64, u32) = conn
            .query_row(
                "SELECT timestamp, level FROM character_level_history WHERE character_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!((before..=unix_now()).contains(&baseline.0));
        assert_eq!(baseline.1, 10);
        conn.execute_batch(
            "UPDATE character_level_history SET timestamp = timestamp - 86400;
             UPDATE characters SET level = 10 WHERE id = 1;
             INSERT INTO characters (id, account_name, character_name) VALUES (3, 'npcxplayer', 'NewHero');"
        ).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        conn.execute_batch(
            "UPDATE characters SET level = 11 WHERE id = 1;
             UPDATE characters SET level = 9 WHERE id = 1;
             UPDATE characters SET character_name = 'Renamed' WHERE id = 1;",
        )
        .unwrap();
        AuthService::write_character_metric_samples(&conn, unix_now()).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(conn.query_row("SELECT level FROM character_level_history WHERE character_id = 1 ORDER BY timestamp DESC LIMIT 1", [], |row| row.get::<_, u32>(0)).unwrap(), 9);
        conn.execute_batch("BEGIN; UPDATE characters SET level = 20 WHERE id = 1; ROLLBACK;")
            .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            count
        );
        assert_eq!(
            auth.level_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_level_history WHERE character_id IN (1, 2)",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn level_history_preserves_baselines_and_last_bucket_values_for_every_period() {
        let path = crate::test_util::unique_temp_dir("level_history_ranges").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player');
             INSERT INTO characters (id, account_name, character_name, level)
             VALUES (1, 'player', 'Hero', 10), (2, 'player', 'NewHero', 1);
             DELETE FROM character_level_history WHERE character_id = 1;",
        )
        .unwrap();
        let now = unix_now();
        let bucket = (now / DAY_SECONDS - 2) * DAY_SECONDS;
        for (timestamp, level) in [
            (now - 400 * DAY_SECONDS, 1),
            (now - 20 * DAY_SECONDS, 5),
            (bucket + 1, 6),
            (bucket + 2, 7),
            (bucket + 3, 6),
            (now - 3600, 10),
        ] {
            conn.execute(
                "INSERT INTO character_level_history VALUES (1, ?1, ?2)",
                params![timestamp, level],
            )
            .unwrap();
        }
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, DAY_SECONDS)] {
            let result = auth.level_leaderboard(hours, interval).unwrap();
            assert_eq!(result.timestamp - result.from, i64::from(hours) * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            let hero = &result.series[0];
            assert_eq!(hero.started_at, now - 400 * DAY_SECONDS);
            assert_eq!(
                hero.samples[0],
                LevelSample {
                    timestamp: result.from,
                    level: if hours == 168 { 5 } else { 1 }
                }
            );
            assert!(hero.samples.contains(&LevelSample {
                timestamp: bucket + 3,
                level: 6
            }));
            assert!(!hero.samples.iter().any(|sample| sample.level == 7));
            assert_eq!(hero.samples.last().unwrap().level, 10);
            assert!(hero
                .samples
                .windows(2)
                .all(|pair| pair[0].timestamp < pair[1].timestamp));
            let new_hero = &result.series[1];
            assert_eq!(new_hero.samples.len(), 1);
            assert_eq!(new_hero.samples[0].timestamp, new_hero.started_at);
            assert!(new_hero.started_at > result.from);
        }
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            7
        );
    }

    #[test]
    fn gold_history_seeds_existing_characters_and_tracks_saved_changes() {
        let path = crate::test_util::unique_temp_dir("gold_history_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_gold_created;
             DROP TRIGGER IF EXISTS character_gold_changed;
             DROP TABLE character_gold_history;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, gold, created_at)
             VALUES (1, 'player', 'Hero', 10, 100), (2, 'npc_test', 'Npc', 99, 100);",
        )
        .unwrap();
        let before = unix_now();
        AuthService::ensure_gold_history_schema(&conn).unwrap();
        let baseline: (i64, i64) = conn
            .query_row(
                "SELECT timestamp, gold FROM character_gold_history WHERE character_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!((before..=unix_now()).contains(&baseline.0));
        assert_eq!(baseline.1, 10);
        conn.execute_batch(
            "UPDATE character_gold_history SET timestamp = timestamp - 86400;
             UPDATE characters SET gold = 10 WHERE id = 1;
             INSERT INTO characters (id, account_name, character_name) VALUES (3, 'npcxplayer', 'NewHero');"
        ).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        conn.execute_batch(
            "UPDATE characters SET gold = 11 WHERE id = 1;
             UPDATE characters SET gold = 9 WHERE id = 1;
             UPDATE characters SET character_name = 'Renamed' WHERE id = 1;",
        )
        .unwrap();
        AuthService::write_character_metric_samples(&conn, unix_now()).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(conn.query_row("SELECT gold FROM character_gold_history WHERE character_id = 1 ORDER BY timestamp DESC LIMIT 1", [], |row| row.get::<_, i64>(0)).unwrap(), 9);
        conn.execute_batch("BEGIN; UPDATE characters SET gold = 20 WHERE id = 1; ROLLBACK;")
            .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            count
        );
        assert_eq!(
            auth.gold_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_gold_history WHERE character_id IN (1, 2)",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn gold_history_preserves_baselines_and_last_bucket_values_for_every_period() {
        let path = crate::test_util::unique_temp_dir("gold_history_ranges").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player');
             INSERT INTO characters (id, account_name, character_name, gold)
             VALUES (1, 'player', 'Hero', 10), (2, 'player', 'NewHero', 0);
             DELETE FROM character_gold_history WHERE character_id = 1;",
        )
        .unwrap();
        let now = unix_now();
        let bucket = (now / DAY_SECONDS - 2) * DAY_SECONDS;
        for (timestamp, gold) in [
            (now - 400 * DAY_SECONDS, 0),
            (now - 20 * DAY_SECONDS, 5),
            (bucket + 1, 6),
            (bucket + 2, 7),
            (bucket + 3, 6),
            (now - 3600, 10),
        ] {
            conn.execute(
                "INSERT INTO character_gold_history VALUES (1, ?1, ?2)",
                params![timestamp, gold],
            )
            .unwrap();
        }
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, DAY_SECONDS)] {
            let result = auth.gold_leaderboard(hours, interval).unwrap();
            assert_eq!(result.timestamp - result.from, i64::from(hours) * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            let hero = &result.series[0];
            assert_eq!(hero.started_at, now - 400 * DAY_SECONDS);
            assert_eq!(
                hero.samples[0],
                CharacterGoldSample {
                    timestamp: result.from,
                    gold: if hours == 168 { 5 } else { 0 }
                }
            );
            assert!(hero.samples.contains(&CharacterGoldSample {
                timestamp: bucket + 3,
                gold: 6
            }));
            assert!(!hero.samples.iter().any(|sample| sample.gold == 7));
            assert_eq!(hero.samples.last().unwrap().gold, 10);
            assert!(hero
                .samples
                .windows(2)
                .all(|pair| pair[0].timestamp < pair[1].timestamp));
            let new_hero = &result.series[1];
            assert_eq!(new_hero.samples.len(), 1);
            assert_eq!(new_hero.samples[0].timestamp, new_hero.started_at);
            assert!(new_hero.started_at > result.from);
        }
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            7
        );
    }

    #[test]
    fn weapon_enchant_history_preserves_baselines_and_last_bucket_values_for_every_period() {
        let path =
            crate::test_util::unique_temp_dir("weapon_enchant_history_ranges").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player');
             INSERT INTO characters (id, account_name, character_name, weapon_enchant)
             VALUES (1, 'player', 'Hero', 10), (2, 'player', 'NewHero', 0);
             DELETE FROM character_weapon_enchant_history WHERE character_id = 1;",
        )
        .unwrap();
        let now = unix_now();
        let bucket = (now / DAY_SECONDS - 2) * DAY_SECONDS;
        for (timestamp, weapon_enchant) in [
            (now - 400 * DAY_SECONDS, 0),
            (now - 20 * DAY_SECONDS, 5),
            (bucket + 1, 6),
            (bucket + 2, 7),
            (bucket + 3, 6),
            (now - 3600, 10),
        ] {
            conn.execute(
                "INSERT INTO character_weapon_enchant_history VALUES (1, ?1, ?2)",
                params![timestamp, weapon_enchant],
            )
            .unwrap();
        }
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, DAY_SECONDS)] {
            let result = auth.weapon_enchant_leaderboard(hours, interval).unwrap();
            assert_eq!(result.timestamp - result.from, i64::from(hours) * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            let hero = &result.series[0];
            assert_eq!(hero.started_at, now - 400 * DAY_SECONDS);
            assert_eq!(
                hero.samples[0],
                WeaponEnchantSample {
                    timestamp: result.from,
                    weapon_enchant: if hours == 168 { 5 } else { 0 }
                }
            );
            assert!(hero.samples.contains(&WeaponEnchantSample {
                timestamp: bucket + 3,
                weapon_enchant: 6
            }));
            assert!(!hero.samples.iter().any(|sample| sample.weapon_enchant == 7));
            assert_eq!(hero.samples.last().unwrap().weapon_enchant, 10);
            assert!(hero
                .samples
                .windows(2)
                .all(|pair| pair[0].timestamp < pair[1].timestamp));
            let new_hero = &result.series[1];
            assert_eq!(new_hero.samples.len(), 1);
            assert_eq!(new_hero.samples[0].timestamp, new_hero.started_at);
            assert!(new_hero.started_at > result.from);
        }
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_weapon_enchant_history",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
            7
        );
    }

    #[test]
    fn gold_history_reuses_hourly_snapshots_and_preserves_gaps_and_peaks() {
        let path = crate::test_util::unique_temp_dir("gold_history").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let empty = auth.gold_history(86400, 24, 3600).unwrap();
        assert_eq!(empty.latest, None);
        assert!(empty.samples.is_empty());
        auth.open_connection()
            .unwrap()
            .execute_batch(
                "INSERT INTO gold_snapshots (ts, total_gold, characters, npc_gold, active_gold, active_characters)
                 VALUES (0, 900, 0, 0, 0, 0), (3600, 0, 0, 0, 0, 0), (7200, 100, 0, 0, 0, 0),
                        (46800, 25, 0, 0, 0, 0), (90000, 999, 0, 0, 0, 0)",
            )
            .unwrap();
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        let history = auth.gold_history(86400, 24, 3600).unwrap();
        assert_eq!(history.samples.len(), 4);
        assert_eq!(history.samples[1].total_gold, 0.0);
        assert_eq!(history.samples[3].timestamp, 46800);
        assert_eq!(
            history.latest,
            Some(GoldSample {
                timestamp: 46800,
                total_gold: 25
            })
        );
        let aggregated = auth.gold_history(86700, 24, 21600).unwrap();
        assert_eq!(
            aggregated.samples,
            vec![
                GoldHistorySample {
                    timestamp: 300,
                    total_gold: 50.0,
                    peak_gold: 100,
                    sample_count: 2
                },
                GoldHistorySample {
                    timestamp: 43200,
                    total_gold: 25.0,
                    peak_gold: 25,
                    sample_count: 1
                },
            ]
        );
        let stale = auth.gold_history(86400, 1, 3600).unwrap();
        assert!(stale.samples.is_empty());
        assert_eq!(stale.latest, history.latest);
    }

    fn activity(id: &str, account: &str, start: i64, end: i64) -> AccountActivity {
        AccountActivity {
            id: id.into(),
            account_name: account.into(),
            started_at: start,
            last_seen_at: end,
        }
    }

    #[test]
    fn per_account_gold_uses_each_kst_days_counts_before_averaging() {
        let midnight = 1000 * DAY_SECONDS - 9 * 3600;
        let (auth, _) = unique_auth("per_account_gold", midnight - 365 * DAY_SECONDS);
        let empty = auth
            .per_account_gold_history(midnight, 24, 3600, 1)
            .unwrap();
        assert!(empty.samples.is_empty());
        assert_eq!(empty.latest, None);
        let conn = auth.open_connection().unwrap();
        for (timestamp, multiplier) in [
            (midnight - DAY_SECONDS, 1),
            (midnight, 2),
            (midnight + DAY_SECONDS, 0),
        ] {
            conn.execute(
                "INSERT INTO unique_account_daily_samples VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    timestamp,
                    2 * multiplier,
                    4 * multiplier,
                    5 * multiplier,
                    10 * multiplier,
                    20 * multiplier
                ],
            )
            .unwrap();
        }
        for (timestamp, gold) in [
            (midnight - 3600, 100),
            (midnight, 100),
            (midnight + 3600, 0),
            (midnight + DAY_SECONDS, 500),
            (midnight + 2 * DAY_SECONDS, 500),
            (midnight + 3 * DAY_SECONDS, 900),
        ] {
            conn.execute(
                "INSERT INTO gold_snapshots (ts, total_gold, characters, npc_gold, active_gold, active_characters)
                 VALUES (?1, ?2, 0, 0, 0, 0)",
                params![timestamp, gold],
            ).unwrap();
        }
        for (days, accounts) in [(1, 4), (7, 8), (30, 10), (180, 20), (365, 40)] {
            let history = auth
                .per_account_gold_history(midnight, 24, 3600, days)
                .unwrap();
            assert_eq!(history.window_seconds, i64::from(days) * DAY_SECONDS);
            assert_eq!(
                history.latest,
                Some(PerAccountGoldSample {
                    timestamp: midnight,
                    total_gold: 100,
                    accounts,
                    gold_per_account: 100.0 / f64::from(accounts),
                })
            );
            assert_eq!(
                history.samples[0].gold_per_account,
                200.0 / f64::from(accounts)
            );
            assert_eq!(
                history.samples[1].gold_per_account,
                100.0 / f64::from(accounts)
            );
        }
        for (hours, interval) in [(4320, 21600), (8760, DAY_SECONDS)] {
            let history = auth
                .per_account_gold_history(midnight + 3600, hours, interval, 1)
                .unwrap();
            assert_eq!(
                history.samples,
                vec![PerAccountGoldHistorySample {
                    timestamp: (midnight - 3600) / interval * interval,
                    gold_per_account: 25.0,
                    peak_gold_per_account: 50.0,
                    sample_count: 3,
                }]
            );
            assert_eq!(history.latest.unwrap().gold_per_account, 0.0);
        }
        for until in [midnight + DAY_SECONDS, midnight + 2 * DAY_SECONDS] {
            let history = auth.per_account_gold_history(until, 168, 3600, 1).unwrap();
            assert_eq!(history.latest, None);
            assert_eq!(history.samples.len(), 3);
        }
        let stale = auth
            .per_account_gold_history(midnight + DAY_SECONDS - 1, 1, 3600, 1)
            .unwrap();
        assert!(stale.samples.is_empty());
        assert_eq!(stale.latest.unwrap().timestamp, midnight + 3600);
        let partial = auth
            .per_account_gold_history(midnight + 300, 1, 3600, 1)
            .unwrap();
        assert_eq!(partial.samples.len(), 1);
        assert_eq!(partial.samples[0].timestamp, midnight);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM unique_account_daily_samples",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
            3
        );
    }

    fn unique_auth(name: &str, started_at: i64) -> (AuthService, std::path::PathBuf) {
        let path = crate::test_util::unique_temp_dir(name).join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        auth.open_connection()
            .unwrap()
            .execute(
                "UPDATE unique_account_collection SET started_at = ?1",
                [started_at],
            )
            .unwrap();
        (auth, path)
    }

    #[test]
    fn daily_unique_counts_deduplicate_each_window_and_include_overlapping_sessions() {
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_windows", end - 400 * DAY_SECONDS);
        auth.record_account_activities(&[
            activity(
                "a1",
                "Alice",
                end - 100 * DAY_SECONDS,
                end - 100 * DAY_SECONDS,
            ),
            activity("a2", "alice", end - 100, end - 1),
            activity("b", "bob", end - 10 * DAY_SECONDS, end + DAY_SECONDS),
            activity("c", "carol", end - 3 * DAY_SECONDS, end - 3 * DAY_SECONDS),
            activity("d", "dan", end - 20 * DAY_SECONDS, end - 20 * DAY_SECONDS),
            activity("e", "eve", end - 100 * DAY_SECONDS, end - 100 * DAY_SECONDS),
            activity(
                "f",
                "frank",
                end - 300 * DAY_SECONDS,
                end - 300 * DAY_SECONDS,
            ),
            activity(
                "old",
                "old",
                end - 366 * DAY_SECONDS,
                end - 366 * DAY_SECONDS,
            ),
            activity("future", "future", end, end),
            activity("short", "short", end - 1, end - 1),
        ])
        .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(end + 30).unwrap());
        for (days, accounts) in [(1, 3), (7, 4), (30, 5), (180, 6), (365, 7)] {
            let history = auth.unique_account_history(end + 123, days).unwrap();
            assert_eq!(
                history.samples,
                vec![UniqueSample {
                    timestamp: end,
                    accounts
                }]
            );
            assert_eq!(history.last_aggregated_at, Some(end));
            assert_eq!(history.sample_interval_seconds, DAY_SECONDS);
            assert_eq!(history.until - history.from, i64::from(days) * DAY_SECONDS);
        }
        assert!(auth
            .aggregate_daily_unique_accounts(end + DAY_SECONDS)
            .unwrap());
        assert_eq!(
            auth.unique_account_history(end + DAY_SECONDS, 1)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            2
        );
    }

    #[test]
    fn daily_unique_results_survive_restart_and_reads_never_recalculate() {
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_daily_cache", end - DAY_SECONDS);
        auth.record_account_activities(&[activity("a", "alice", end - 100, end - 1)])
            .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(end).unwrap());
        auth.record_account_activities(&[activity("late", "bob", end - 50, end - 1)])
            .unwrap();
        assert!(!auth.aggregate_daily_unique_accounts(end + 3600).unwrap());
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        auth.open_connection()
            .unwrap()
            .execute("DROP TABLE account_activity_sessions", [])
            .unwrap();
        assert!(!auth.aggregate_daily_unique_accounts(end + 7200).unwrap());
        let history = auth.unique_account_history(end + 7200, 1).unwrap();
        assert_eq!(
            history.samples,
            vec![UniqueSample {
                timestamp: end,
                accounts: 1
            }]
        );
    }

    #[test]
    fn daily_unique_backfill_respects_collection_start_and_historical_windows() {
        let midnight = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_backfill_windows", midnight + 3600);
        assert_eq!(
            auth.backfill_daily_unique_accounts(midnight + 7200)
                .unwrap(),
            0
        );
        assert!(auth
            .unique_account_history(midnight + 7200, 365)
            .unwrap()
            .samples
            .is_empty());

        let first = midnight + DAY_SECONDS;
        auth.record_account_activities(&[
            activity("a1", "Alice", first - 100, first - 100),
            activity(
                "a2",
                "alice",
                first + DAY_SECONDS + 100,
                first + DAY_SECONDS + 100,
            ),
            activity("b", "bob", first - 50, first + DAY_SECONDS + 50),
            activity(
                "future",
                "future",
                first + 4 * DAY_SECONDS,
                first + 4 * DAY_SECONDS,
            ),
        ])
        .unwrap();
        let now = first + 4 * DAY_SECONDS - 1;
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 4);
        for (offset, accounts) in [2, 1, 2, 0].into_iter().enumerate() {
            let timestamp = first + offset as i64 * DAY_SECONDS;
            assert_eq!(
                auth.unique_account_history(timestamp, 1)
                    .unwrap()
                    .samples
                    .last(),
                Some(&UniqueSample {
                    timestamp,
                    accounts
                })
            );
        }
        for days in [7, 30, 180, 365] {
            let history = auth.unique_account_history(now, days).unwrap();
            assert_eq!(
                history.samples,
                (0..4)
                    .map(|offset| UniqueSample {
                        timestamp: first + offset * DAY_SECONDS,
                        accounts: 2,
                    })
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(auth.backfill_daily_unique_accounts(now + 1).unwrap(), 1);
        assert_eq!(
            auth.unique_account_history(now + 1, 7)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            2
        );
    }

    #[test]
    fn daily_unique_backfill_on_restart_fills_gaps_without_recalculating_saved_days() {
        let first = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_backfill_restart", first - DAY_SECONDS + 3600);
        auth.record_account_activities(&[activity("a", "alice", first - 100, first - 1)])
            .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(first).unwrap());
        assert!(auth
            .aggregate_daily_unique_accounts(first + 2 * DAY_SECONDS)
            .unwrap());
        auth.record_account_activities(&[activity("late", "bob", first - 50, first - 1)])
            .unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let now = first + 5 * DAY_SECONDS + 3600;
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 4);
        let history = auth.unique_account_history(now, 7).unwrap();
        assert_eq!(
            history.samples,
            [1, 2, 1, 2, 2, 2]
                .into_iter()
                .enumerate()
                .map(|(offset, accounts)| UniqueSample {
                    timestamp: first + offset as i64 * DAY_SECONDS,
                    accounts,
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(history.last_aggregated_at, Some(first + 5 * DAY_SECONDS));
        assert!(auth
            .unique_account_history(now, 1)
            .unwrap()
            .samples
            .iter()
            .all(|sample| sample.accounts == 0));
        auth.open_connection()
            .unwrap()
            .execute("DROP TABLE account_activity_sessions", [])
            .unwrap();
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 0);
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples,
            history.samples
        );
    }

    #[test]
    fn daily_unique_backfill_resumes_after_a_failed_day() {
        let first = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_backfill_retry", first - DAY_SECONDS);
        auth.open_connection()
            .unwrap()
            .execute_batch(&format!(
                "CREATE TRIGGER fail_daily_sample BEFORE INSERT ON unique_account_daily_samples
                 WHEN NEW.timestamp = {}
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
                first + DAY_SECONDS
            ))
            .unwrap();
        let now = first + 2 * DAY_SECONDS;
        assert!(auth.backfill_daily_unique_accounts(now).is_err());
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples,
            vec![UniqueSample {
                timestamp: first,
                accounts: 0,
            }]
        );
        auth.open_connection()
            .unwrap()
            .execute("DROP TRIGGER fail_daily_sample", [])
            .unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 2);
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples.len(),
            3
        );
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 0);
    }

    #[test]
    fn daily_unique_history_distinguishes_pending_zero_and_missed_days() {
        let midnight = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_coverage", midnight + 3600);
        assert!(!auth
            .aggregate_daily_unique_accounts(midnight + 7200)
            .unwrap());
        let pending = auth.unique_account_history(midnight + 7200, 1).unwrap();
        assert!(pending.samples.is_empty());
        assert_eq!(pending.last_aggregated_at, None);
        let first = midnight + DAY_SECONDS;
        assert!(auth.aggregate_daily_unique_accounts(first + 30).unwrap());
        assert!(auth
            .aggregate_daily_unique_accounts(first + 3 * DAY_SECONDS)
            .unwrap());
        let history = auth
            .unique_account_history(first + 3 * DAY_SECONDS, 7)
            .unwrap();
        assert_eq!(
            history.samples,
            vec![
                UniqueSample {
                    timestamp: first,
                    accounts: 0
                },
                UniqueSample {
                    timestamp: first + 3 * DAY_SECONDS,
                    accounts: 0
                }
            ]
        );
        assert_eq!(history.last_aggregated_at, Some(first + 3 * DAY_SECONDS));
        assert_eq!(
            auth.unique_account_history(first + 6 * DAY_SECONDS, 1)
                .unwrap()
                .last_aggregated_at,
            history.last_aggregated_at
        );
    }

    #[test]
    fn daily_unique_collection_uses_korean_midnight_and_preserves_latest_activity() {
        assert_eq!(kst_day_start(15 * 3600 - 1), -9 * 3600);
        assert_eq!(kst_day_start(15 * 3600), 15 * 3600);
        assert_eq!(kst_day_start(DAY_SECONDS), 15 * 3600);
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_heartbeat", end - 3 * DAY_SECONDS);
        auth.record_account_activities(&[activity("a", "alice", end - 2 * DAY_SECONDS, end - 1)])
            .unwrap();
        auth.record_account_activities(&[activity(
            "a",
            "alice",
            end - 2 * DAY_SECONDS,
            end - 2 * DAY_SECONDS,
        )])
        .unwrap();
        auth.aggregate_daily_unique_accounts(end).unwrap();
        assert_eq!(
            auth.unique_account_history(end, 1)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            1
        );
    }

    #[test]
    fn concurrent_samples_survive_restart_and_preserve_missing_hours() {
        let path = crate::test_util::unique_temp_dir("concurrent_samples").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        for (now, web_accounts, agent_accounts, other_accounts) in [
            (3900, 1, 2, 0),
            (7140, 2, 1, 1),
            (7260, 0, 0, 0),
            (18300, 3, 4, 0),
        ] {
            auth.record_concurrent_accounts(
                now,
                ConcurrentCounts {
                    web_accounts,
                    agent_accounts,
                    other_accounts,
                },
            )
            .unwrap();
        }
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let samples = auth.concurrent_account_samples(3600, 18000, 3600).unwrap();
        assert_eq!(
            samples,
            vec![
                ConcurrentHistorySample {
                    timestamp: 3600,
                    accounts: 4.0,
                    web_accounts: 2.0,
                    agent_accounts: 1.0,
                    other_accounts: 1.0,
                    peak_accounts: 4,
                    peak_timestamp: 3600,
                    sample_count: 1,
                },
                ConcurrentHistorySample {
                    timestamp: 7200,
                    accounts: 0.0,
                    web_accounts: 0.0,
                    agent_accounts: 0.0,
                    other_accounts: 0.0,
                    peak_accounts: 0,
                    peak_timestamp: 7200,
                    sample_count: 1,
                },
                ConcurrentHistorySample {
                    timestamp: 18000,
                    accounts: 7.0,
                    web_accounts: 3.0,
                    agent_accounts: 4.0,
                    other_accounts: 0.0,
                    peak_accounts: 7,
                    peak_timestamp: 18000,
                    sample_count: 1,
                },
            ]
        );
        assert_eq!(
            auth.concurrent_account_samples(7260, 17940, 3600).unwrap(),
            vec![]
        );
    }

    #[test]
    fn concurrent_history_aggregates_only_observed_legacy_minutes_and_keeps_peaks() {
        let path = crate::test_util::unique_temp_dir("concurrent_aggregates").join("game.db");
        let auth = AuthService::new(path).unwrap();
        for (timestamp, web_accounts, agent_accounts, other_accounts) in [
            (60, 99, 0, 0),
            (120, 0, 0, 0),
            (180, 2, 4, 0),
            (240, 4, 2, 0),
            (1200, 1, 1, 1),
            (1260, 99, 0, 0),
        ] {
            auth.open_connection().unwrap().execute(
                "INSERT INTO concurrent_account_samples (timestamp, accounts, web_accounts, agent_accounts) VALUES (?1, ?2, ?3, ?4)",
                params![timestamp, web_accounts + agent_accounts + other_accounts, web_accounts, agent_accounts],
            ).unwrap();
        }
        assert_eq!(
            auth.concurrent_account_samples(100, 1230, 600).unwrap(),
            vec![
                ConcurrentHistorySample {
                    timestamp: 100,
                    accounts: 4.0,
                    web_accounts: 2.0,
                    agent_accounts: 2.0,
                    other_accounts: 0.0,
                    peak_accounts: 6,
                    peak_timestamp: 180,
                    sample_count: 3,
                },
                ConcurrentHistorySample {
                    timestamp: 1200,
                    accounts: 3.0,
                    web_accounts: 1.0,
                    agent_accounts: 1.0,
                    other_accounts: 1.0,
                    peak_accounts: 3,
                    peak_timestamp: 1200,
                    sample_count: 1,
                },
            ]
        );
    }

    #[test]
    fn concurrent_history_migrates_old_totals_without_inventing_client_types() {
        let path = crate::test_util::unique_temp_dir("concurrent_migration").join("game.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE concurrent_account_samples (timestamp INTEGER PRIMARY KEY, accounts INTEGER NOT NULL);
             INSERT INTO concurrent_account_samples VALUES (60, 8), (120, 4)",
        ).unwrap();
        AuthService::ensure_concurrent_samples_schema(&conn).unwrap();
        AuthService::ensure_concurrent_samples_schema(&conn).unwrap();
        drop(conn);
        let auth = AuthService::new(path).unwrap();
        let old = auth.concurrent_account_samples(60, 120, 60).unwrap();
        assert!(old.iter().all(|sample| sample.web_accounts == 0.0
            && sample.agent_accounts == 0.0
            && sample.other_accounts == sample.accounts));
        auth.record_concurrent_accounts(
            180,
            ConcurrentCounts {
                web_accounts: 2,
                agent_accounts: 4,
                other_accounts: 0,
            },
        )
        .unwrap();
        let samples = auth.concurrent_account_samples(0, 599, 600).unwrap();
        assert_eq!(samples.len(), 1);
        let sample = &samples[0];
        assert_eq!(sample.accounts, 6.0);
        assert_eq!(sample.web_accounts, 2.0 / 3.0);
        assert_eq!(sample.agent_accounts, 4.0 / 3.0);
        assert_eq!(sample.other_accounts, 4.0);
        assert_eq!(sample.peak_accounts, 8);
        assert_eq!(sample.sample_count, 3);
    }

    #[test]
    fn concurrent_history_bounds_a_full_year_of_samples() {
        let path = crate::test_util::unique_temp_dir("concurrent_year").join("game.db");
        let auth = AuthService::new(path).unwrap();
        auth.open_connection()
            .unwrap()
            .execute_batch(
                "WITH RECURSIVE minutes(timestamp) AS (
                SELECT 0 UNION ALL SELECT timestamp + 60 FROM minutes WHERE timestamp < 31535940
             )
             INSERT INTO concurrent_account_samples (timestamp, accounts)
             SELECT timestamp, 4 FROM minutes",
            )
            .unwrap();
        let samples = auth.concurrent_account_samples(0, 31536000, 86400).unwrap();
        assert_eq!(samples.len(), 365);
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.sample_count)
                .sum::<u32>(),
            525600
        );
        assert!(samples.iter().all(|sample| sample.accounts == 4.0
            && sample.web_accounts == 0.0
            && sample.agent_accounts == 0.0
            && sample.other_accounts == 4.0
            && sample.peak_accounts == 4
            && sample.peak_timestamp == sample.timestamp));
    }
}
