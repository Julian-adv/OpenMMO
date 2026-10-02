use crate::auth::unix_now;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use sysinfo::{DiskRefreshKind, Disks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::watch;
use tracing::warn;

const SAMPLE_SECONDS: u64 = 60;
const RETENTION_SECONDS: i64 = 7 * 86400;

mod store;

#[derive(Clone)]
pub(crate) struct HardwareMetrics {
    latest: Arc<RwLock<Option<Snapshot>>>,
    path: Arc<PathBuf>,
    storage_error: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Snapshot {
    timestamp: i64,
    cpu_count: usize,
    cpu_percent: Option<f32>,
    load_average: Option<[f64; 3]>,
    memory_total_bytes: u64,
    memory_used_bytes: u64,
    disks: Vec<DiskSample>,
    agent: AgentSample,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct DiskSample {
    mount: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct ProcessUsage {
    process_count: usize,
    cpu_percent: Option<f32>,
    memory_bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct AgentSample {
    instances: usize,
    process_count: usize,
    cpu_percent: Option<f32>,
    memory_bytes: u64,
    client: ProcessUsage,
    llm: ProcessUsage,
}

#[derive(Debug, Serialize)]
struct HardwareStatus {
    from: i64,
    until: i64,
    sample_interval_seconds: u64,
    retention_seconds: i64,
    collection_started_at: Option<i64>,
    available: bool,
    latest: Option<Snapshot>,
    samples: Vec<Snapshot>,
}

impl HardwareMetrics {
    pub fn new(path: PathBuf) -> Self {
        Self {
            latest: Arc::default(),
            path: Arc::new(path),
            storage_error: Arc::default(),
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/metrics/hardware", get(status))
            .with_state(self.clone())
    }

    pub async fn run(self, mut shutdown: watch::Receiver<()>) {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            warn!("Hardware metrics are unsupported on this system");
            return;
        }
        let mut collector = Collector::default();
        let mut connection = None;
        let mut interval = tokio::time::interval(Duration::from_secs(SAMPLE_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = interval.tick() => {}
            }
            let path = Arc::clone(&self.path);
            let storage_error = Arc::clone(&self.storage_error);
            match tokio::task::spawn_blocking(move || {
                let sample = collector.sample();
                if let Some(sample) = &sample {
                    let result = (|| {
                        let conn = match &connection {
                            Some(conn) => conn,
                            None => connection.insert(store::open_writer(&path)?),
                        };
                        store::save(conn, sample)
                    })();
                    storage_error.store(result.is_err(), Ordering::Relaxed);
                    if let Err(error) = result {
                        connection = None;
                        warn!(%error, "Hardware metrics could not be saved");
                    }
                }
                (collector, connection, sample)
            })
            .await
            {
                Ok((next, next_connection, sample)) => {
                    collector = next;
                    connection = next_connection;
                    if let Some(sample) = sample {
                        *self.latest.write().unwrap_or_else(|e| e.into_inner()) = Some(sample);
                    } else {
                        warn!("Hardware metrics sample unavailable");
                    }
                }
                Err(error) => {
                    warn!("Hardware metrics worker stopped: {error}");
                    break;
                }
            }
        }
    }

    fn snapshot(&self, now: i64, hours: u32) -> rusqlite::Result<HardwareStatus> {
        let latest = self
            .latest
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|sample| sample.timestamp <= now && sample.timestamp > now - RETENTION_SECONDS);
        let mut data = HardwareStatus {
            from: now - i64::from(hours) * 3600,
            until: now,
            sample_interval_seconds: SAMPLE_SECONDS,
            retention_seconds: RETENTION_SECONDS,
            collection_started_at: None,
            available: false,
            latest,
            samples: vec![],
        };
        if self.path.try_exists().unwrap_or(true) {
            store::load(&store::open_reader(&self.path)?, &mut data)?;
        }
        data.available = !self.storage_error.load(Ordering::Relaxed)
            && data.latest.as_ref().is_some_and(|sample| {
                now.saturating_sub(sample.timestamp) <= (SAMPLE_SECONDS * 3) as i64
            });
        Ok(data)
    }
}

#[derive(Deserialize)]
struct QueryHours {
    hours: Option<u32>,
}

async fn status(
    State(metrics): State<HardwareMetrics>,
    Query(query): Query<QueryHours>,
) -> Result<Json<HardwareStatus>, (StatusCode, &'static str)> {
    let hours = match query.hours.unwrap_or(24) {
        hours @ (1 | 24 | 168) => hours,
        _ => return Err((StatusCode::BAD_REQUEST, "hours must be 1, 24, or 168")),
    };
    match tokio::task::spawn_blocking(move || metrics.snapshot(unix_now(), hours)).await {
        Ok(Ok(data)) => Ok(Json(data)),
        result => {
            warn!(?result, "Hardware history unavailable");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "hardware history unavailable",
            ))
        }
    }
}

#[derive(Default)]
struct Collector {
    system: System,
    disks: Disks,
    primed: bool,
}

impl Collector {
    fn sample(&mut self) -> Option<Snapshot> {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_cpu()
                .with_memory()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        self.disks
            .refresh_specifics(true, DiskRefreshKind::nothing().with_storage());

        let cpu_count = self.system.cpus().len();
        let processes: Vec<_> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessSample {
                pid: pid.as_u32(),
                parent: process.parent().map(|pid| pid.as_u32()),
                agent: is_agent(process.name())
                    || process
                        .exe()
                        .and_then(|path| path.file_name())
                        .is_some_and(is_agent),
                cpu_percent: process.cpu_usage() / cpu_count.max(1) as f32,
                memory_bytes: process.memory(),
            })
            .collect();
        let mut disks: Vec<_> = self
            .disks
            .list()
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .map(|disk| DiskSample {
                mount: disk.mount_point().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space().min(disk.total_space()),
            })
            .collect();
        disks.sort_by(|a, b| a.mount.cmp(&b.mount));
        let cpu_ready = self.primed;
        self.primed = true;
        let total = self.system.total_memory();
        if total == 0 || cpu_count == 0 {
            return None;
        }
        Some(Snapshot {
            timestamp: unix_now(),
            cpu_count,
            cpu_percent: cpu_ready.then(|| self.system.global_cpu_usage()),
            load_average: cfg!(unix).then(|| {
                let load = System::load_average();
                [load.one, load.five, load.fifteen]
            }),
            memory_total_bytes: total,
            memory_used_bytes: total.saturating_sub(self.system.available_memory()),
            disks,
            agent: aggregate_agents(&processes, cpu_ready),
        })
    }
}

fn is_agent(name: &OsStr) -> bool {
    name == "agent-client" || name == "agent-client.exe"
}

struct ProcessSample {
    pid: u32,
    parent: Option<u32>,
    agent: bool,
    cpu_percent: f32,
    memory_bytes: u64,
}

fn aggregate_agents(processes: &[ProcessSample], cpu_ready: bool) -> AgentSample {
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut pending = Vec::new();
    for (index, process) in processes.iter().enumerate() {
        if process.agent {
            pending.push(index);
        }
        if let Some(parent) = process.parent {
            children.entry(parent).or_default().push(index);
        }
    }
    let mut result = AgentSample {
        instances: pending.len(),
        cpu_percent: cpu_ready.then_some(0.0),
        client: ProcessUsage {
            cpu_percent: cpu_ready.then_some(0.0),
            ..ProcessUsage::default()
        },
        llm: ProcessUsage {
            cpu_percent: cpu_ready.then_some(0.0),
            ..ProcessUsage::default()
        },
        ..AgentSample::default()
    };
    let mut visited = HashSet::new();
    while let Some(index) = pending.pop() {
        let process = &processes[index];
        if !visited.insert(process.pid) {
            continue;
        }
        if let Some(cpu) = result.cpu_percent.as_mut() {
            *cpu += process.cpu_percent;
        }
        let usage = if process.agent {
            &mut result.client
        } else {
            &mut result.llm
        };
        usage.process_count += 1;
        usage.memory_bytes = usage.memory_bytes.saturating_add(process.memory_bytes);
        if let Some(cpu) = usage.cpu_percent.as_mut() {
            *cpu += process.cpu_percent;
        }
        if let Some(descendants) = children.get(&process.pid) {
            pending.extend(descendants);
        }
    }
    result.process_count = result.client.process_count + result.llm.process_count;
    result.memory_bytes = result
        .client
        .memory_bytes
        .saturating_add(result.llm.memory_bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage_path(name: &str) -> PathBuf {
        let directory = crate::test_util::unique_temp_dir(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory.join("hardware.db")
    }

    fn process(pid: u32, parent: Option<u32>, agent: bool) -> ProcessSample {
        ProcessSample {
            pid,
            parent,
            agent,
            cpu_percent: 75.0,
            memory_bytes: 1024,
        }
    }

    #[test]
    fn agent_totals_include_descendants_once_and_exclude_unrelated_processes() {
        let processes = [
            process(10, None, true),
            process(11, Some(10), false),
            process(12, Some(11), true),
            process(13, Some(12), false),
            process(20, None, true),
            process(21, Some(20), false),
            process(30, None, false),
        ];
        let totals = aggregate_agents(&processes, true);
        assert_eq!(totals.instances, 3);
        assert_eq!(totals.process_count, 6);
        assert_eq!(totals.cpu_percent, Some(450.0));
        assert_eq!(totals.memory_bytes, 6144);
        assert_eq!(totals.client.process_count, 3);
        assert_eq!(totals.client.memory_bytes, 3072);
        assert_eq!(totals.client.cpu_percent, Some(225.0));
        assert_eq!(totals.llm.process_count, 3);
        assert_eq!(totals.llm.memory_bytes, 3072);
        assert_eq!(totals.llm.cpu_percent, Some(225.0));
        assert_eq!(aggregate_agents(&processes, false).cpu_percent, None);
        assert_eq!(aggregate_agents(&processes, false).llm.cpu_percent, None);
    }

    #[test]
    fn missing_agents_do_not_include_orphaned_children() {
        let totals = aggregate_agents(&[process(11, Some(10), false)], true);
        assert_eq!(totals.instances, 0);
        assert_eq!(totals.process_count, 0);
        assert_eq!(totals.memory_bytes, 0);
        assert!(is_agent(OsStr::new("agent-client.exe")));
        assert!(!is_agent(OsStr::new("agent-client-helper")));
    }

    fn sample(timestamp: i64) -> Snapshot {
        Snapshot {
            timestamp,
            cpu_count: 4,
            cpu_percent: Some(25.0),
            load_average: Some([1.0, 0.5, 0.25]),
            memory_total_bytes: 8 * 1024 * 1024 * 1024,
            memory_used_bytes: 2 * 1024 * 1024 * 1024,
            disks: vec![DiskSample {
                mount: "/".into(),
                total_bytes: 100 * 1024 * 1024 * 1024,
                available_bytes: 60 * 1024 * 1024 * 1024,
            }],
            agent: aggregate_agents(
                &[
                    process(10, None, true),
                    process(11, Some(10), false),
                    process(12, Some(11), false),
                ],
                true,
            ),
        }
    }

    #[test]
    fn history_survives_restart_and_prunes_at_the_seven_day_boundary() {
        let path = storage_path("hardware_history");
        let now = 10 * RETENTION_SECONDS;
        {
            let conn = store::open_writer(&path).unwrap();
            for timestamp in [
                now - RETENTION_SECONDS - 60,
                now - RETENTION_SECONDS,
                now - RETENTION_SECONDS + 1,
                now - 3601,
                now - 3600,
                now - 180,
                now - 60,
                now,
            ] {
                store::save(&conn, &sample(timestamp)).unwrap();
            }
            store::save(&conn, &sample(now)).unwrap();
            let count: usize = conn
                .query_row("SELECT COUNT(*) FROM hardware_samples", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 6);
        }
        let metrics = HardwareMetrics::new(path);
        let week = metrics.snapshot(now, 168).unwrap();
        assert_eq!(week.samples.len(), 6);
        assert_eq!(week.latest, Some(sample(now)));
        assert_eq!(
            week.collection_started_at,
            Some(now - RETENTION_SECONDS + 1)
        );
        assert!(week.available);
        let hour = metrics.snapshot(now, 1).unwrap();
        assert_eq!(
            hour.samples.iter().map(|s| s.timestamp).collect::<Vec<_>>(),
            [now - 180, now - 60, now]
        );
        assert!(!metrics.snapshot(now + 181, 1).unwrap().available);
        let expired = metrics.snapshot(now + RETENTION_SECONDS, 168).unwrap();
        assert!(expired.samples.is_empty());
        assert!(expired.latest.is_none());
        assert!(expired.collection_started_at.is_none());
    }

    #[tokio::test]
    async fn background_collection_persists_split_process_usage() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let path = storage_path("hardware_collector");
        let metrics = HardwareMetrics::new(path.clone());
        let (shutdown_tx, shutdown) = watch::channel(());
        let task = tokio::spawn(metrics.clone().run(shutdown));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if metrics.latest.read().unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        shutdown_tx.send(()).unwrap();
        task.await.unwrap();
        assert!(!metrics.storage_error.load(Ordering::Relaxed));
        let restored = HardwareMetrics::new(path).snapshot(unix_now(), 24).unwrap();
        assert!(restored.available);
        assert_eq!(restored.samples.len(), 1);
        let sample = restored.latest.unwrap();
        assert_eq!(
            sample.agent.memory_bytes,
            sample.agent.client.memory_bytes + sample.agent.llm.memory_bytes
        );
    }

    #[tokio::test]
    async fn history_api_limits_queries_to_supported_periods() {
        let metrics = HardwareMetrics::new(storage_path("hardware_api"));
        let conn = store::open_writer(&metrics.path).unwrap();
        store::save(&conn, &sample(unix_now())).unwrap();
        let base = crate::test_util::serve(metrics.router()).await;
        let client = reqwest::Client::new();
        for hours in [1, 24, 168] {
            let data: serde_json::Value = client
                .get(format!("{base}/api/metrics/hardware?hours={hours}"))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(
                data["until"].as_i64().unwrap() - data["from"].as_i64().unwrap(),
                hours * 3600
            );
            assert_eq!(data["samples"].as_array().unwrap().len(), 1);
        }
        for hours in [0, 2, 169, 720] {
            let response = client
                .get(format!("{base}/api/metrics/hardware?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[test]
    fn collector_warms_up_cpu_and_keeps_stale_samples_identifiable() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let metrics = HardwareMetrics::new(
            crate::test_util::unique_temp_dir("hardware_warmup").join("hardware.db"),
        );
        assert!(!metrics.snapshot(unix_now(), 24).unwrap().available);
        let mut collector = Collector::default();
        let sample = collector.sample().unwrap();
        assert_eq!(sample.cpu_percent, None);
        assert_eq!(sample.agent.cpu_percent, None);
        assert!(sample.memory_used_bytes <= sample.memory_total_bytes);
        let now = sample.timestamp;
        *metrics.latest.write().unwrap() = Some(sample);
        assert!(metrics.snapshot(now, 24).unwrap().available);
        let stale = metrics
            .snapshot(now + SAMPLE_SECONDS as i64 * 3 + 1, 24)
            .unwrap();
        assert!(!stale.available);
        assert_eq!(stale.latest.unwrap().timestamp, now);

        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let sample = collector.sample().unwrap();
        assert!(sample
            .cpu_percent
            .is_some_and(|cpu| (0.0..=100.0).contains(&cpu)));
        assert!(sample.agent.cpu_percent.is_some());
    }
}
