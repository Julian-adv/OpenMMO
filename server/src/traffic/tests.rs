use super::*;
use rusqlite::params;

#[test]
fn counters_use_elapsed_time_and_reset_after_interface_changes_or_counter_resets() {
    let before = Counters {
        interface: "eth0".into(),
        index: "2".into(),
        rx: 100,
        tx: 200,
        at: Instant::now(),
    };
    let mut after = Counters {
        rx: 700,
        tx: 1400,
        at: before.at + Duration::from_secs(120),
        ..before.clone()
    };
    let sample = after.sample(&before, 0, 1000).unwrap();
    assert_eq!(
        (sample.rx_bytes, sample.tx_bytes, sample.accounts),
        (600, 1200, 0)
    );
    assert_eq!(sample.seconds, 120.0);
    after.rx = 1;
    assert!(after.sample(&before, 1, 1000).is_none());
    after.rx = 700;
    after.index = "3".into();
    assert!(after.sample(&before, 1, 1000).is_none());
    after.index = "2".into();
    after.interface = "eth1".into();
    assert!(after.sample(&before, 1, 1000).is_none());
}

#[test]
fn default_route_ignores_loopback_down_routes_and_more_expensive_routes() {
    let routes = "Iface Destination Gateway Flags RefCnt Use Metric Mask\nlo 00000000 00000000 0001 0 0 0 00000000\neth0 00000000 01000000 0003 0 0 100 00000000\neth1 00000000 01000000 0003 0 0 10 00000000\neth2 00000000 01000000 0002 0 0 0 00000000\neth3 000010AC 00000000 0001 0 0 0 0000FFFF";
    assert_eq!(default_interface(routes).as_deref(), Some("eth1"));
    assert!(default_interface("Iface\ninvalid").is_none());
}

#[tokio::test]
async fn endpoints_report_weighted_network_rates_totals_rankings_and_bounded_ranges() {
    let root = crate::test_util::unique_temp_dir("traffic_api");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("metrics.db");
    let conn = store::open_writer(&path).unwrap();
    let now = unix_now();
    let boundary = now - now.rem_euclid(600);
    for (age, seconds, rx, tx, accounts) in [(20, 60.0, 60, 120, 2), (10, 120.0, 360, 720, 0)] {
        store::save_network(
            &conn,
            &NetworkSample {
                timestamp: now - age,
                interface: "eth0".into(),
                seconds,
                rx_bytes: rx,
                tx_bytes: tx,
                accounts,
            },
        )
        .unwrap();
    }
    for (category, path, bytes, timestamp) in [
        ("model", "/models/tree.glb", 300, boundary - 600),
        ("bgm", "/bgm/a.ogg", 200, boundary - 600),
        ("texture", "/textures/old.webp", 9999, boundary - 7200),
        ("texture", "/textures/current.webp", 999, boundary),
    ] {
        conn.execute(
            "INSERT INTO asset_samples VALUES (?1, ?2, ?3, ?4, 2, 1)",
            params![timestamp, category, path, bytes],
        )
        .unwrap();
    }
    let metrics = TrafficMetrics::new(Config {
        path,
        interface: None,
        access_log: None,
        interval_seconds: 60,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api/metrics", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, metrics.router()).await.unwrap() });
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{url}/network?hours=1"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let data: serde_json::Value = response.json().await.unwrap();
    assert_eq!(data["rx_bytes"], 420);
    assert_eq!(data["tx_bytes"], 840);
    assert_eq!(data["latest"]["accounts"], 0);
    assert_eq!(data["latest"]["seconds"], 120.0);
    let seconds: f64 = data["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["seconds"].as_f64().unwrap())
        .sum();
    assert_eq!(seconds, 180.0);
    let data: serde_json::Value = client
        .get(format!("{url}/asset-traffic?hours=1"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["total_bytes"], 500);
    assert_eq!(data["files"][0]["path"], "/models/tree.glb");
    assert_eq!(data["categories"].as_array().unwrap().len(), 2);
    assert_eq!(data["until"], boundary);
    for endpoint in ["network", "asset-traffic"] {
        for hours in ["0", "6", "8760", "invalid"] {
            assert_eq!(
                client
                    .get(format!("{url}/{endpoint}?hours={hours}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
    }
    task.abort();
}
