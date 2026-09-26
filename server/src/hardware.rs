use crate::auth::unix_now;
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    sync::{Arc, RwLock},
    time::Duration,
};
use sysinfo::{DiskRefreshKind, Disks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::sync::watch;
use tracing::warn;

const SAMPLE_SECONDS: u64 = 60;

#[derive(Clone, Default)]
pub(crate) struct HardwareMetrics {
    latest: Arc<RwLock<Option<Snapshot>>>,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
struct DiskSample {
    mount: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
struct AgentSample {
    instances: usize,
    process_count: usize,
    cpu_percent: Option<f32>,
    memory_bytes: u64,
}

#[derive(Serialize)]
struct HardwareStatus {
    until: i64,
    sample_interval_seconds: u64,
    available: bool,
    latest: Option<Snapshot>,
}

impl HardwareMetrics {
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
        let mut interval = tokio::time::interval(Duration::from_secs(SAMPLE_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = interval.tick() => {}
            }
            match tokio::task::spawn_blocking(move || {
                let sample = collector.sample();
                (collector, sample)
            })
            .await
            {
                Ok((next, sample)) => {
                    collector = next;
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

    fn snapshot(&self, now: i64) -> HardwareStatus {
        let latest = self
            .latest
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        HardwareStatus {
            until: now,
            sample_interval_seconds: SAMPLE_SECONDS,
            available: latest.as_ref().is_some_and(|sample| {
                now.saturating_sub(sample.timestamp) <= (SAMPLE_SECONDS * 3) as i64
            }),
            latest,
        }
    }
}

async fn status(State(metrics): State<HardwareMetrics>) -> Json<HardwareStatus> {
    Json(metrics.snapshot(unix_now()))
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
        ..AgentSample::default()
    };
    let mut visited = HashSet::new();
    while let Some(index) = pending.pop() {
        let process = &processes[index];
        if !visited.insert(process.pid) {
            continue;
        }
        result.process_count += 1;
        result.memory_bytes = result.memory_bytes.saturating_add(process.memory_bytes);
        if let Some(cpu) = result.cpu_percent.as_mut() {
            *cpu += process.cpu_percent;
        }
        if let Some(descendants) = children.get(&process.pid) {
            pending.extend(descendants);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(aggregate_agents(&processes, false).cpu_percent, None);
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

    #[test]
    fn collector_warms_up_cpu_and_keeps_stale_samples_identifiable() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let metrics = HardwareMetrics::default();
        assert!(!metrics.snapshot(unix_now()).available);
        let mut collector = Collector::default();
        let sample = collector.sample().unwrap();
        assert_eq!(sample.cpu_percent, None);
        assert_eq!(sample.agent.cpu_percent, None);
        assert!(sample.memory_used_bytes <= sample.memory_total_bytes);
        let now = sample.timestamp;
        *metrics.latest.write().unwrap() = Some(sample);
        assert!(metrics.snapshot(now).available);
        let stale = metrics.snapshot(now + SAMPLE_SECONDS as i64 * 3 + 1);
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
