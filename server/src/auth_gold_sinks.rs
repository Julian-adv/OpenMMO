use super::{AuthError, AuthService};
use crate::metrics::{GoldSink, GoldSinkEntry, GoldSinkRecord, GoldSinks, SAMPLE_INTERVAL_SECONDS};
use rusqlite::{params, Connection};

impl AuthService {
    pub(super) fn ensure_gold_sinks_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS gold_sink_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO gold_sink_collection (id, started_at) VALUES (1, unixepoch());
             CREATE TABLE IF NOT EXISTS gold_sink_samples (
                timestamp INTEGER NOT NULL,
                sink TEXT NOT NULL,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                gold INTEGER NOT NULL CHECK (gold >= 0),
                PRIMARY KEY (timestamp, sink, item_def_id),
                CHECK ((sink IN ('item_purchase', 'item_buyback') AND item_def_id != '') OR
                       (sink IN ('stall_tax', 'land_tax', 'land_recovery') AND item_def_id = ''))
             );",
        )?;
        transaction.commit()
    }

    pub(super) fn write_gold_sinks(
        conn: &Connection,
        records: &[GoldSinkRecord],
    ) -> Result<(), rusqlite::Error> {
        let mut statement = conn.prepare_cached(
            "INSERT INTO gold_sink_samples (timestamp, sink, item_def_id, quantity, gold)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(timestamp, sink, item_def_id) DO UPDATE SET
                quantity = quantity + excluded.quantity, gold = gold + excluded.gold",
        )?;
        for record in records {
            let (sink, item_def_id) = record.sink.storage_key();
            statement.execute(params![
                record.timestamp - record.timestamp.rem_euclid(SAMPLE_INTERVAL_SECONDS),
                sink,
                item_def_id,
                record.quantity,
                record.gold,
            ])?;
        }
        Ok(())
    }

    pub fn record_gold_sinks(&self, records: &[GoldSinkRecord]) -> Result<(), AuthError> {
        if records.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        Self::write_gold_sinks(&transaction, records)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn gold_sinks(&self, until: i64, hours: u32) -> Result<GoldSinks, AuthError> {
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        let until = until - until.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        let from = until - i64::from(hours) * 3600;
        let collection_started_at = transaction.query_row(
            "SELECT started_at FROM gold_sink_collection WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let defs = crate::item_defs::item_defs();
        let mut statement = transaction.prepare(
            "SELECT sink, item_def_id, SUM(quantity), SUM(gold) AS total_gold
             FROM gold_sink_samples WHERE timestamp >= ?1 AND timestamp < ?2
             GROUP BY sink, item_def_id ORDER BY total_gold DESC, sink ASC, item_def_id ASC",
        )?;
        let entries = statement
            .query_map(params![from, until], |row| {
                let kind: String = row.get(0)?;
                let item_def_id: String = row.get(1)?;
                let (sink, name) = match kind.as_str() {
                    "item_purchase" | "item_buyback" => {
                        let name = defs
                            .get(&item_def_id)
                            .map_or_else(|| item_def_id.clone(), |item| item.name.clone());
                        let sink = if kind == "item_purchase" {
                            GoldSink::ItemPurchase { item_def_id }
                        } else {
                            GoldSink::ItemBuyback { item_def_id }
                        };
                        (sink, name)
                    }
                    "stall_tax" => (GoldSink::StallTax, "가판 판매 수수료".into()),
                    "land_tax" => (GoldSink::LandTax, "토지세".into()),
                    "land_recovery" => (GoldSink::LandRecovery, "토지 체납 복구 비용".into()),
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok(GoldSinkEntry {
                    sink,
                    name,
                    quantity: row.get(2)?,
                    gold: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let total_gold = entries
            .iter()
            .try_fold(0i64, |total, entry| total.checked_add(entry.gold))
            .ok_or_else(|| AuthError::Database("Gold sink total overflow".into()))?;
        Ok(GoldSinks {
            from,
            until,
            collection_started_at,
            total_gold,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_sinks_preserve_period_boundaries_categories_and_collection_start() {
        let path = crate::test_util::unique_temp_dir("gold_sink_boundaries").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let purchase = GoldSink::ItemPurchase {
            item_def_id: "retired_item".into(),
        };
        let records: Vec<_> = [
            (3599, purchase.clone(), 1, 90000),
            (3600, purchase.clone(), 2, 3000),
            (7199, purchase.clone(), 3, 2000),
            (7200, purchase.clone(), 1, 90000),
            (
                3600,
                GoldSink::ItemBuyback {
                    item_def_id: "retired_item".into(),
                },
                1,
                5000,
            ),
            (3600, GoldSink::StallTax, 2, 15),
            (3600, GoldSink::LandTax, 1, 2000),
            (3600, GoldSink::LandRecovery, 1, 6000),
        ]
        .into_iter()
        .map(|(timestamp, sink, quantity, gold)| GoldSinkRecord {
            timestamp,
            sink,
            quantity,
            gold,
        })
        .collect();
        auth.record_gold_sinks(&records).unwrap();
        let result = auth.gold_sinks(7250, 1).unwrap();
        assert_eq!(
            (result.from, result.until, result.total_gold),
            (3600, 7200, 18015)
        );
        assert_eq!(result.entries.len(), 5);
        assert_eq!(result.entries[0].sink, GoldSink::LandRecovery);
        assert_eq!(
            result.entries[1].sink,
            GoldSink::ItemBuyback {
                item_def_id: "retired_item".into()
            }
        );
        assert_eq!(result.entries[2].sink, purchase);
        assert_eq!(result.entries[2].name, "retired_item");
        assert_eq!(result.entries[2].quantity, 5);
        let reopened = AuthService::new(path).unwrap();
        let persisted = reopened.gold_sinks(7250, 1).unwrap();
        assert_eq!(
            persisted.collection_started_at,
            result.collection_started_at
        );
        assert_eq!(persisted.total_gold, result.total_gold);
        let invalid = [
            records[1].clone(),
            GoldSinkRecord {
                gold: -1,
                ..records[1].clone()
            },
        ];
        assert!(reopened.record_gold_sinks(&invalid).is_err());
        assert_eq!(
            reopened.gold_sinks(7250, 1).unwrap().total_gold,
            result.total_gold
        );
        let empty = reopened.gold_sinks(14400, 1).unwrap();
        assert_eq!(empty.total_gold, 0);
        assert!(empty.entries.is_empty());
    }
}
