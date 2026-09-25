use super::{AccessStatus, AssetEntry, AssetTraffic, NetworkHistory, NetworkSample};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use std::time::Duration;

pub(super) const RETENTION_SECONDS: i64 = 30 * 86400;

pub(super) fn open_writer(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(100))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS network_samples (
            timestamp INTEGER PRIMARY KEY, interface TEXT NOT NULL,
            seconds REAL NOT NULL, rx_bytes INTEGER NOT NULL,
            tx_bytes INTEGER NOT NULL, accounts INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS asset_samples (
            timestamp INTEGER NOT NULL, category TEXT NOT NULL, path TEXT NOT NULL,
            bytes INTEGER NOT NULL, requests INTEGER NOT NULL, revalidations INTEGER NOT NULL,
            PRIMARY KEY (timestamp, category, path)
         );
         CREATE TABLE IF NOT EXISTS access_state (
            path TEXT PRIMARY KEY, cursor TEXT NOT NULL, started_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL, skipped_lines INTEGER NOT NULL,
            gaps INTEGER NOT NULL, pending_bytes INTEGER NOT NULL
         );",
    )?;
    Ok(conn)
}

pub(super) fn open_reader(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(100))?;
    Ok(conn)
}

pub(super) fn save_network(conn: &Connection, sample: &NetworkSample) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO network_samples VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            sample.timestamp,
            sample.interface,
            sample.seconds,
            sample.rx_bytes,
            sample.tx_bytes,
            sample.accounts
        ],
    )?;
    Ok(())
}

pub(super) fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    for table in ["network_samples", "asset_samples"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE rowid IN (SELECT rowid FROM {table} WHERE timestamp < ?1 LIMIT 5000)"),
            [now - RETENTION_SECONDS],
        )?;
    }
    Ok(())
}

pub(super) fn network_history(
    conn: &Connection,
    data: &mut NetworkHistory,
) -> rusqlite::Result<()> {
    let conn = conn.unchecked_transaction()?;
    data.collection_started_at = conn.query_row(
        "SELECT MIN(timestamp) FROM network_samples WHERE timestamp <= ?1",
        [data.until],
        |row| row.get(0),
    )?;
    data.latest = conn
        .query_row(
            "SELECT timestamp, interface, seconds, rx_bytes, tx_bytes, accounts
         FROM network_samples WHERE timestamp <= ?1 ORDER BY timestamp DESC LIMIT 1",
            [data.until],
            |row| {
                Ok(NetworkSample {
                    timestamp: row.get(0)?,
                    interface: row.get(1)?,
                    seconds: row.get(2)?,
                    rx_bytes: row.get(3)?,
                    tx_bytes: row.get(4)?,
                    accounts: row.get(5)?,
                })
            },
        )
        .optional()?;
    let mut statement = conn.prepare(
        "SELECT MAX(timestamp), SUM(seconds), SUM(rx_bytes), SUM(tx_bytes)
         FROM network_samples WHERE timestamp > ?1 AND timestamp <= ?2
         GROUP BY timestamp / ?3 ORDER BY MAX(timestamp)",
    )?;
    data.samples = statement
        .query_map(
            params![data.from, data.until, data.sample_interval_seconds],
            |row| {
                Ok(super::NetworkHistorySample {
                    timestamp: row.get(0)?,
                    seconds: row.get(1)?,
                    rx_bytes: row.get(2)?,
                    tx_bytes: row.get(3)?,
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()?;
    data.rx_bytes = data.samples.iter().map(|s| s.rx_bytes).sum();
    data.tx_bytes = data.samples.iter().map(|s| s.tx_bytes).sum();
    Ok(())
}

pub(super) fn access_status(
    conn: &Connection,
    path: &str,
) -> rusqlite::Result<Option<AccessStatus>> {
    conn.query_row(
        "SELECT started_at, updated_at, skipped_lines, gaps, pending_bytes FROM access_state WHERE path = ?1",
        [path],
        |row| Ok(AccessStatus {
            started_at: row.get(0)?, updated_at: row.get(1)?, skipped_lines: row.get(2)?,
            gaps: row.get(3)?, pending_bytes: row.get(4)?,
        }),
    ).optional()
}

pub(super) fn asset_traffic(conn: &Connection, data: &mut AssetTraffic) -> rusqlite::Result<()> {
    let transaction = conn.unchecked_transaction()?;
    for by_file in [false, true] {
        let sql = if by_file {
            "SELECT category, path, SUM(bytes) AS total, SUM(requests), SUM(revalidations)
             FROM asset_samples WHERE timestamp >= ?1 AND timestamp < ?2
             GROUP BY category, path ORDER BY total DESC, path ASC LIMIT 20"
        } else {
            "SELECT category, '', SUM(bytes) AS total, SUM(requests), SUM(revalidations)
             FROM asset_samples WHERE timestamp >= ?1 AND timestamp < ?2
             GROUP BY category ORDER BY total DESC, category ASC"
        };
        let mut statement = transaction.prepare(sql)?;
        let entries: Vec<AssetEntry> = statement
            .query_map(params![data.from, data.until], |row| {
                Ok(AssetEntry {
                    category: row.get(0)?,
                    path: row.get(1)?,
                    bytes: row.get(2)?,
                    requests: row.get(3)?,
                    revalidations: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        if by_file {
            data.files = entries;
        } else {
            data.categories = entries;
        }
    }
    data.total_bytes = data.categories.iter().map(|entry| entry.bytes).sum();
    Ok(())
}
