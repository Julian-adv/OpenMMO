use super::metrics_unavailable;
use crate::auth::unix_now;
use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use onlinerpg_shared::tales::Deed;
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, path::PathBuf};
use tracing::warn;

#[derive(Debug, Serialize, Deserialize)]
struct TaleEntry {
    line: usize,
    #[serde(flatten)]
    deed: Deed,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeroicTales {
    until: i64,
    available: bool,
    skipped_lines: usize,
    entries: Vec<TaleEntry>,
}

pub(super) fn router(ledger_path: PathBuf) -> Router {
    Router::new()
        .route("/api/metrics/heroic-tales", get(heroic_tales))
        .with_state(ledger_path)
}

async fn heroic_tales(State(path): State<PathBuf>) -> Response {
    let mut data = HeroicTales {
        until: unix_now(),
        available: false,
        skipped_lines: 0,
        entries: Vec::new(),
    };
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            data.available = true;
            for (index, line) in content.lines().enumerate() {
                if line.trim().is_empty() || line.trim_start().starts_with('#') {
                    continue;
                }
                if let Some(deed) = Deed::parse(line) {
                    data.entries.push(TaleEntry {
                        line: index + 1,
                        deed,
                    });
                } else {
                    data.skipped_lines += 1;
                }
            }
            data.entries.reverse();
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            warn!("Heroic tales ledger read failed: {error}");
            return metrics_unavailable();
        }
    }
    ([(header::CACHE_CONTROL, "no-store")], Json(data)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn ledger_reads_current_contents_and_distinguishes_missing_empty_and_failed_reads() {
        let dir = crate::test_util::unique_temp_dir("heroic_tales");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.txt");
        let app = router(path.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/api/metrics/heroic-tales",
            listener.local_addr().unwrap()
        );
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::new();

        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let data: HeroicTales = response.json().await.unwrap();
        assert!(!data.available);
        assert!(data.entries.is_empty());
        assert!(!path.exists());

        std::fs::write(
            &path,
            "  # ignored | comment | text\r\n\r\n 2026-09-01 | 용사A | 오거를 쓰러뜨렸다. \r\ninvalid\r\n2026-09-02 | | no hero\r\n2026-09-03 | 용사B | 승리했다. | 영웅적으로 노래한다.\r\n",
        )
        .unwrap();
        let data: HeroicTales = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(data.available);
        assert!(data.until > 0 && data.until <= unix_now());
        assert_eq!(data.skipped_lines, 2);
        assert_eq!(data.entries.len(), 2);
        assert_eq!(data.entries[0].line, 6);
        assert_eq!(data.entries[0].deed.date, "2026-09-03");
        assert_eq!(data.entries[0].deed.hero, "용사B");
        assert_eq!(
            data.entries[0].deed.brief,
            "승리했다. | 영웅적으로 노래한다."
        );
        assert_eq!(data.entries[1].line, 3);
        assert_eq!(data.entries[1].deed.brief, "오거를 쓰러뜨렸다.");

        std::fs::write(&path, "2026-09-01 | 새 이름 | 수정된 기록\n").unwrap();
        let data: HeroicTales = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].deed.hero, "새 이름");
        assert_eq!(data.entries[0].deed.brief, "수정된 기록");
        assert_eq!(data.skipped_lines, 0);

        std::fs::write(&path, "# empty\n").unwrap();
        let data: HeroicTales = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(data.available);
        assert!(data.entries.is_empty());

        std::fs::write(&path, [0xff]).unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.text().await.unwrap(),
            "Metrics are temporarily unavailable"
        );

        std::fs::remove_file(&path).unwrap();
        let data: HeroicTales = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(!data.available);
        assert!(data.entries.is_empty());
        task.abort();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
