use super::{HardwareStatus, Snapshot, RETENTION_SECONDS};
use rusqlite::{params, types::Type, Connection, OpenFlags, OptionalExtension};
use std::{path::Path, time::Duration};

pub(super) fn open_writer(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(100))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS hardware_samples (
            timestamp INTEGER PRIMARY KEY, sample TEXT NOT NULL
         );",
    )?;
    Ok(conn)
}

pub(super) fn open_reader(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(100))?;
    Ok(conn)
}

pub(super) fn save(conn: &Connection, sample: &Snapshot) -> rusqlite::Result<()> {
    let json = serde_json::to_string(sample)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO hardware_samples VALUES (?1, ?2)",
        params![sample.timestamp, json],
    )?;
    tx.execute(
        "DELETE FROM hardware_samples WHERE timestamp <= ?1",
        [sample.timestamp - RETENTION_SECONDS],
    )?;
    tx.commit()
}

fn snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
    let json: String = row.get(0)?;
    serde_json::from_str(&json)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error)))
}

pub(super) fn load(conn: &Connection, data: &mut HardwareStatus) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let cutoff = data.until - RETENTION_SECONDS;
    data.collection_started_at = tx.query_row(
        "SELECT MIN(timestamp) FROM hardware_samples WHERE timestamp > ?1 AND timestamp <= ?2",
        params![cutoff, data.until],
        |row| row.get(0),
    )?;
    if data.latest.is_none() {
        data.latest = tx
            .query_row(
                "SELECT sample FROM hardware_samples WHERE timestamp > ?1 AND timestamp <= ?2
                 ORDER BY timestamp DESC LIMIT 1",
                params![cutoff, data.until],
                snapshot,
            )
            .optional()?;
    }
    data.samples = tx
        .prepare(
            "SELECT sample FROM hardware_samples WHERE timestamp > ?1 AND timestamp <= ?2
             ORDER BY timestamp",
        )?
        .query_map(params![data.from.max(cutoff), data.until], snapshot)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(())
}
