use super::store::RETENTION_SECONDS;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_BATCH_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_LINE_BYTES: usize = 16384;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Deserialize, Serialize)]
struct Cursor {
    device: u64,
    inode: u64,
    offset: u64,
    discarding: bool,
}

#[cfg(unix)]
fn identity(metadata: &Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn identity(_: &Metadata) -> (u64, u64) {
    (0, 0)
}

impl Cursor {
    fn matches(&self, metadata: &Metadata) -> bool {
        (self.device, self.inode) == identity(metadata)
    }
    fn new(metadata: &Metadata, offset: u64) -> Self {
        let (device, inode) = identity(metadata);
        Self {
            device,
            inode,
            offset,
            discarding: false,
        }
    }
}

fn previous_file(path: &Path, cursor: &Cursor) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    std::fs::read_dir(path.parent()?)
        .ok()?
        .take(256)
        .filter_map(|entry| entry.ok())
        .find_map(|entry| {
            let filename = entry.file_name();
            let filename = filename.to_str()?;
            if !filename.starts_with(name) || filename.ends_with(".gz") {
                return None;
            }
            cursor
                .matches(&entry.metadata().ok()?)
                .then(|| entry.path())
        })
}

struct Rotation {
    rank: u32,
    path: PathBuf,
    bytes: u64,
}

fn newer_rotations(path: &Path, previous: &Path) -> Result<(u32, Vec<Rotation>)> {
    let prefix = format!(
        "{}.",
        path.file_name()
            .ok_or("Missing log filename")?
            .to_string_lossy()
    );
    let index = previous
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(&prefix))
        .and_then(|suffix| suffix.parse::<u32>().ok())
        .unwrap_or(0);
    let mut files = vec![Rotation {
        rank: 0,
        path: path.to_owned(),
        bytes: path.metadata()?.len(),
    }];
    for entry in std::fs::read_dir(path.parent().ok_or("Missing log directory")?)?.take(256) {
        let entry = entry?;
        let name = entry.file_name();
        let Some(rank) = name
            .to_str()
            .and_then(|name| name.strip_prefix(&prefix))
            .and_then(|suffix| suffix.parse::<u32>().ok())
        else {
            continue;
        };
        if rank > 0 && rank < index {
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                files.push(Rotation {
                    rank,
                    path: entry.path(),
                    bytes: metadata.len(),
                });
            }
        }
    }
    files.sort_by_key(|file| std::cmp::Reverse(file.rank));
    Ok((index, files))
}

#[derive(Debug)]
struct Record {
    timestamp: i64,
    method: String,
    path: String,
    status: u16,
    bytes: u64,
}

fn nginx_time(value: &str) -> Option<i64> {
    let (date, offset) = value.split_once(' ')?;
    let parts: Vec<_> = date.split(['/', ':']).collect();
    if parts.len() != 6 || offset.len() != 5 || !offset.is_ascii() {
        return None;
    }
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = (months.iter().position(|month| *month == parts[1])? + 1) as u8;
    let date = time::Date::from_calendar_date(
        parts[2].parse().ok()?,
        month.try_into().ok()?,
        parts[0].parse().ok()?,
    )
    .ok()?;
    let hour: i8 = offset[1..3].parse().ok()?;
    let minute: i8 = offset[3..5].parse().ok()?;
    let sign = match &offset[..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let offset = time::UtcOffset::from_hms(hour * sign, minute * sign, 0).ok()?;
    Some(
        date.with_hms(
            parts[3].parse().ok()?,
            parts[4].parse().ok()?,
            parts[5].parse().ok()?,
        )
        .ok()?
        .assume_offset(offset)
        .unix_timestamp(),
    )
}

fn parse(line: &str) -> Option<Record> {
    let mut record = if line.starts_with('{') {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let timestamp: f64 = value.get("timestamp")?.as_str()?.parse().ok()?;
        if !timestamp.is_finite() || timestamp < 0.0 || timestamp > i64::MAX as f64 {
            return None;
        }
        Record {
            timestamp: timestamp as i64,
            method: value.get("method")?.as_str()?.into(),
            path: value.get("path")?.as_str()?.into(),
            status: value.get("status")?.as_u64()?.try_into().ok()?,
            bytes: value.get("bytes")?.as_u64()?,
        }
    } else {
        let (_, tail) = line.split_once('[')?;
        let (date, tail) = tail.split_once("] \"")?;
        let (request, tail) = tail.split_once('"')?;
        let mut request = request.split_whitespace();
        let method = request.next()?.to_owned();
        let path = request.next()?.to_owned();
        let mut fields = tail.split_whitespace();
        Record {
            timestamp: nginx_time(date)?,
            method,
            path,
            status: fields.next()?.parse().ok()?,
            bytes: fields.next()?.parse().ok()?,
        }
    };
    record
        .path
        .truncate(record.path.find('?').unwrap_or(record.path.len()));
    if record.path.len() > 1024
        || !record.path.starts_with('/')
        || record.path.chars().any(char::is_control)
        || record.bytes > (1u64 << 50)
    {
        return None;
    }
    Some(record)
}

fn category(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("/api/") && !lower.starts_with("/api/cape-texture/") {
        return None;
    }
    let extension = lower.rsplit('.').next()?;
    Some(match extension {
        "mp3" | "m4a" | "ogg" | "wav" | "flac" => {
            if lower.starts_with("/bgm/") {
                "bgm"
            } else {
                "sound"
            }
        }
        "glb" | "gltf" => "model",
        "png" | "jpg" | "jpeg" | "webp" | "avif" | "svg" | "ktx2" | "basis" => "texture",
        "js" | "mjs" | "css" | "wasm" => "code",
        "woff" | "woff2" | "ttf" | "otf" => "font",
        "html" | "json" | "bin" | "ico" | "webmanifest" => "other",
        _ if lower == "/" || lower == "/dashboard/" => "other",
        _ => return None,
    })
}

pub(super) fn collect(conn: &Connection, path: &Path, now: i64) -> Result<()> {
    let key = path.to_string_lossy();
    let current = File::open(path)?;
    let metadata = current.metadata()?;
    if !metadata.is_file() {
        return Err("Access log must be a regular file".into());
    }
    let saved: Option<String> = conn
        .query_row(
            "SELECT cursor FROM access_state WHERE path = ?1",
            [key.as_ref()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(saved) = saved else {
        let cursor = serde_json::to_string(&Cursor::new(&metadata, metadata.len()))?;
        conn.execute(
            "INSERT INTO access_state VALUES (?1, ?2, ?3, ?3, 0, 0, 0)",
            params![key, cursor, now],
        )?;
        return Ok(());
    };
    let mut cursor: Cursor = serde_json::from_str(&saved)?;
    let mut gaps = 0u64;
    let mut pending_rotated_bytes = 0;
    let mut file = if cursor.matches(&metadata) {
        current
    } else {
        match previous_file(path, &cursor) {
            Some(previous) => {
                let old = File::open(&previous)?;
                let (index, newer) = newer_rotations(path, &previous)?;
                if old.metadata()?.len() > cursor.offset {
                    pending_rotated_bytes = newer.iter().map(|file| file.bytes).sum();
                    old
                } else {
                    let next = &newer[0];
                    gaps += u64::from(index == 0 || next.rank + 1 != index);
                    let next = File::open(&next.path)?;
                    cursor = Cursor::new(&next.metadata()?, 0);
                    pending_rotated_bytes = newer.iter().skip(1).map(|file| file.bytes).sum();
                    next
                }
            }
            None => {
                gaps += 1;
                cursor = Cursor::new(&metadata, 0);
                current
            }
        }
    };
    let file_metadata = file.metadata()?;
    if cursor.offset > file_metadata.len() {
        gaps += 1;
        cursor.offset = 0;
        cursor.discarding = false;
    }
    file.seek(SeekFrom::Start(cursor.offset))?;
    let mut buffer = Vec::new();
    file.take(MAX_BATCH_BYTES).read_to_end(&mut buffer)?;
    let started = Instant::now();
    let mut consumed = 0usize;
    let mut skipped = 0u64;
    let mut entries: BTreeMap<(i64, &'static str, String), (u64, u64, u64)> = BTreeMap::new();
    for bytes in buffer.split_inclusive(|byte| *byte == b'\n') {
        if started.elapsed() >= Duration::from_millis(100) || entries.len() >= MAX_ENTRIES {
            break;
        }
        let complete = bytes.last() == Some(&b'\n');
        if !complete {
            if !cursor.matches(&metadata)
                && cursor.offset + buffer.len() as u64 >= file_metadata.len()
            {
                if !cursor.discarding {
                    skipped += 1;
                }
                cursor.discarding = false;
                consumed += bytes.len();
                break;
            }
            if bytes.len() > MAX_LINE_BYTES || cursor.discarding {
                if !cursor.discarding {
                    skipped += 1;
                }
                cursor.discarding = true;
                consumed += bytes.len();
            }
            break;
        }
        consumed += bytes.len();
        if cursor.discarding {
            cursor.discarding = false;
            continue;
        }
        if bytes.len() > MAX_LINE_BYTES {
            skipped += 1;
            continue;
        }
        let Some(record) = std::str::from_utf8(bytes).ok().and_then(parse) else {
            skipped += 1;
            continue;
        };
        if record.timestamp < now - RETENTION_SECONDS || record.timestamp > now + 60 {
            skipped += 1;
            continue;
        }
        if !matches!(record.method.as_str(), "GET" | "HEAD")
            || !(200..300).contains(&record.status) && record.status != 304
        {
            continue;
        }
        let Some(category) = category(&record.path) else {
            continue;
        };
        let timestamp = record.timestamp - record.timestamp.rem_euclid(600);
        let totals = entries
            .entry((timestamp, category, record.path))
            .or_default();
        totals.0 += record.bytes;
        totals.1 += 1;
        totals.2 += u64::from(record.status == 304);
    }
    cursor.offset += consumed as u64;
    let pending = file_metadata.len().saturating_sub(cursor.offset) + pending_rotated_bytes;
    let transaction = conn.unchecked_transaction()?;
    {
        let mut insert = transaction.prepare_cached(
            "INSERT INTO asset_samples VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(timestamp, category, path) DO UPDATE SET bytes = bytes + excluded.bytes,
                requests = requests + excluded.requests, revalidations = revalidations + excluded.revalidations",
        )?;
        for ((timestamp, category, path), (bytes, requests, revalidations)) in entries {
            insert.execute(params![
                timestamp,
                category,
                path,
                bytes,
                requests,
                revalidations
            ])?;
        }
    }
    transaction.execute(
        "UPDATE access_state SET cursor = ?2, updated_at = ?3, skipped_lines = skipped_lines + ?4,
         gaps = gaps + ?5, pending_bytes = ?6 WHERE path = ?1",
        params![
            key,
            serde_json::to_string(&cursor)?,
            now,
            skipped,
            gaps,
            pending
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(name: &str) -> (PathBuf, Connection) {
        let root = crate::test_util::unique_temp_dir(name);
        std::fs::create_dir_all(&root).unwrap();
        let conn = super::super::store::open_writer(&root.join("metrics.db")).unwrap();
        (root.join("access.log"), conn)
    }

    fn line(now: i64, path: &str, bytes: u64, status: u16) -> String {
        format!("{{\"timestamp\":\"{now}.125\",\"method\":\"GET\",\"path\":\"{path}\",\"status\":{status},\"bytes\":{bytes}}}\n")
    }

    fn total(conn: &Connection) -> u64 {
        conn.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM asset_samples",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn parses_combined_timezones_queries_ranges_and_categories() {
        let record = parse("127.0.0.1 - - [15/Sep/2026:20:30:00 +0900] \"GET /bgm/theme.abc12345.ogg?v=1 HTTP/2.0\" 206 1234 \"-\" \"agent\" cc=\"no-cache\"").unwrap();
        assert_eq!(
            record.timestamp,
            nginx_time("15/Sep/2026:11:30:00 +0000").unwrap()
        );
        assert_eq!(record.path, "/bgm/theme.abc12345.ogg");
        assert_eq!(record.bytes, 1234);
        assert_eq!(category(&record.path), Some("bgm"));
        for (path, expected) in [
            ("/sounds/hit.ogg", Some("sound")),
            ("/models/tree.GLB", Some("model")),
            ("/textures/stone.webp", Some("texture")),
            ("/assets/client.wasm", Some("code")),
            ("/api/terrain/tile.bin", None),
            ("/ws", None),
        ] {
            assert_eq!(category(path), expected);
        }
        assert!(parse("malformed").is_none());
        assert!(nginx_time("15/Sep/2026:11:30:00 +ab00").is_none());
    }

    #[test]
    fn checkpoint_counts_new_complete_lines_once_and_resumes_after_restart() {
        let (path, conn) = fixture("traffic_checkpoint");
        let now = crate::auth::unix_now();
        std::fs::write(&path, line(now, "/bgm/old.ogg", 999, 200)).unwrap();
        collect(&conn, &path, now).unwrap();
        assert_eq!(total(&conn), 0);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        write!(
            file,
            "{}{}{}",
            line(now, "/textures/a.webp?v=1", 100, 206),
            line(now, "/textures/a.webp?v=2", 0, 304),
            line(now, "/missing.glb", 99, 404)
        )
        .unwrap();
        let partial = line(now, "/models/tree.glb", 200, 200);
        write!(file, "{}", &partial[..partial.len() - 1]).unwrap();
        collect(&conn, &path, now + 600).unwrap();
        assert_eq!(total(&conn), 100);
        writeln!(file).unwrap();
        collect(&conn, &path, now + 1200).unwrap();
        collect(&conn, &path, now + 1800).unwrap();
        assert_eq!(total(&conn), 300);
        let (requests, revalidations): (u64, u64) = conn
            .query_row(
                "SELECT requests, revalidations FROM asset_samples WHERE path = '/textures/a.webp'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((requests, revalidations), (2, 1));
    }

    #[test]
    #[cfg(unix)]
    fn rotation_drains_old_file_and_reports_missing_or_truncated_logs() {
        let (path, conn) = fixture("traffic_rotation");
        let now = crate::auth::unix_now();
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now).unwrap();
        std::fs::write(&path, line(now, "/models/a.glb", 10, 200)).unwrap();
        let rotated = path.with_extension("log.1");
        std::fs::rename(&path, &rotated).unwrap();
        std::fs::write(&path, line(now, "/models/b.glb", 20, 200)).unwrap();
        collect(&conn, &path, now + 600).unwrap();
        assert_eq!(total(&conn), 10);
        collect(&conn, &path, now + 1200).unwrap();
        assert_eq!(total(&conn), 30);
        collect(&conn, &path, now + 1800).unwrap();
        assert_eq!(total(&conn), 30);
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now + 2400).unwrap();
        let status = super::super::store::access_status(&conn, &path.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(status.gaps, 1);
        std::fs::remove_file(&rotated).unwrap();
        std::fs::rename(&path, &rotated).unwrap();
        std::fs::write(&path, line(now, "/models/c.glb", 30, 200)).unwrap();
        std::fs::remove_file(rotated).unwrap();
        collect(&conn, &path, now + 3000).unwrap();
        assert_eq!(total(&conn), 60);
        let status = super::super::store::access_status(&conn, &path.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(status.gaps, 2);
    }

    #[test]
    #[cfg(unix)]
    fn multiple_rotations_preserve_order_and_skip_an_abandoned_partial_line() {
        let (path, conn) = fixture("traffic_multiple_rotations");
        let now = crate::auth::unix_now();
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now).unwrap();
        std::fs::write(
            &path,
            format!("{}unfinished", line(now, "/models/a.glb", 10, 200)),
        )
        .unwrap();
        std::fs::rename(&path, path.with_extension("log.2")).unwrap();
        std::fs::write(
            path.with_extension("log.1"),
            line(now, "/models/b.glb", 20, 200),
        )
        .unwrap();
        std::fs::write(&path, line(now, "/models/c.glb", 30, 200)).unwrap();
        for expected in [10, 30, 60, 60] {
            collect(&conn, &path, now).unwrap();
            assert_eq!(total(&conn), expected);
        }
        let status = super::super::store::access_status(&conn, &path.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(
            (status.gaps, status.skipped_lines, status.pending_bytes),
            (0, 1, 0)
        );
    }

    #[test]
    fn failed_checkpoint_rolls_back_totals_and_retry_after_reopening_counts_once() {
        let (path, conn) = fixture("traffic_rollback");
        let now = crate::auth::unix_now();
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now).unwrap();
        std::fs::write(&path, line(now, "/bgm/music.ogg", 100, 200)).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_cursor BEFORE UPDATE ON access_state BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(collect(&conn, &path, now).is_err());
        assert_eq!(total(&conn), 0);
        conn.execute_batch("DROP TRIGGER fail_cursor").unwrap();
        drop(conn);
        let conn =
            super::super::store::open_writer(&path.parent().unwrap().join("metrics.db")).unwrap();
        collect(&conn, &path, now).unwrap();
        collect(&conn, &path, now).unwrap();
        assert_eq!(total(&conn), 100);
    }

    #[test]
    fn batch_limit_defers_excess_paths_without_losing_them() {
        let (path, conn) = fixture("traffic_batch_limit");
        let now = crate::auth::unix_now();
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now).unwrap();
        let lines: String = (0..6000)
            .map(|index| line(now, &format!("/models/{index}.glb"), 1, 200))
            .collect();
        std::fs::write(&path, lines).unwrap();
        collect(&conn, &path, now).unwrap();
        assert!(total(&conn) <= MAX_ENTRIES as u64);
        assert!(
            super::super::store::access_status(&conn, &path.to_string_lossy())
                .unwrap()
                .unwrap()
                .pending_bytes
                > 0
        );
        for _ in 0..20 {
            collect(&conn, &path, now).unwrap();
            if total(&conn) == 6000 {
                break;
            }
        }
        assert_eq!(total(&conn), 6000);
    }

    #[test]
    fn oversized_and_incomplete_lines_are_bounded_and_do_not_hide_following_requests() {
        let (path, conn) = fixture("traffic_large_lines");
        let now = crate::auth::unix_now();
        std::fs::write(&path, "").unwrap();
        collect(&conn, &path, now).unwrap();
        std::fs::write(&path, vec![b'x'; MAX_BATCH_BYTES as usize + 10]).unwrap();
        collect(&conn, &path, now).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        write!(file, "\n{}", line(now, "/bgm/music.ogg", 12, 200)).unwrap();
        collect(&conn, &path, now).unwrap();
        assert_eq!(total(&conn), 12);
        let status = super::super::store::access_status(&conn, &path.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(status.skipped_lines, 1);
        assert_eq!(status.pending_bytes, 0);
    }
}
