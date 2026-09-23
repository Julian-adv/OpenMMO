#[path = "pathfinding_load/fixtures.rs"]
mod fixtures;

use fixtures::{Fixtures, Query, GROUPS};
use rand::{
    distributions::{Distribution, WeightedIndex},
    Rng, SeedableRng,
};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::{
    error::Error,
    hint::black_box,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

struct Config {
    root: PathBuf,
    rates: Vec<usize>,
    workers: Vec<usize>,
    seconds: f64,
    queue: usize,
    rounds: usize,
    seed: u64,
    mix: String,
}

impl Config {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut config = Self {
            root: "data".into(),
            rates: vec![1000, 5000],
            workers: vec![1, 4],
            seconds: 5.0,
            queue: 1024,
            rounds: 5,
            seed: 20260923,
            mix: "mixed".into(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--help" {
                println!("pathfinding_load [--data-root data] [--rates 1000,5000] [--workers 1,4] [--seconds 5] [--queue 1024] [--rounds 5] [--seed 20260923] [--mix mixed|dungeon-heavy|open|village|dungeon|stairs|closed_doors|unreachable|long_route]");
                std::process::exit(0);
            }
            let value = args.next().ok_or("missing option value")?;
            match arg.as_str() {
                "--data-root" => config.root = value.into(),
                "--rates" => {
                    config.rates = value.split(',').map(str::parse).collect::<Result<_, _>>()?
                }
                "--workers" => {
                    config.workers = value.split(',').map(str::parse).collect::<Result<_, _>>()?
                }
                "--seconds" => config.seconds = value.parse()?,
                "--queue" => config.queue = value.parse()?,
                "--rounds" => config.rounds = value.parse()?,
                "--seed" => config.seed = value.parse()?,
                "--mix" => config.mix = value,
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        if !config.seconds.is_finite()
            || !(0.1..=60.0).contains(&config.seconds)
            || config.queue == 0
            || config.workers.iter().any(|w| !(1..=64).contains(w))
            || config.rates.iter().any(|r| !(1..=100_000).contains(r))
        {
            return Err("invalid duration, queue, workers or rates".into());
        }
        weights(&config.mix)?;
        Ok(config)
    }
}

fn weights(mix: &str) -> Result<[u32; 7], Box<dyn Error>> {
    match mix {
        "mixed" => Ok([50, 20, 20, 8, 1, 1, 0]),
        "dungeon-heavy" => Ok([0, 0, 84, 14, 1, 1, 0]),
        name => {
            let group = GROUPS
                .iter()
                .position(|g| *g == name)
                .ok_or("unknown mix")?;
            let mut weights = [0; 7];
            weights[group] = 1;
            Ok(weights)
        }
    }
}

fn distribution(mut values: Vec<f64>) -> serde_json::Value {
    if values.is_empty() {
        return json!(null);
    }
    values.sort_by(f64::total_cmp);
    let percentile = |p: f64| values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    json!({"mean": values.iter().sum::<f64>() / values.len() as f64,
        "p50": percentile(0.5), "p95": percentile(0.95), "p99": percentile(0.99),
        "max": values.last().unwrap()})
}

fn reaches_goal(query: &Query, result: &onlinerpg_shared::pathfinding::PathResult) -> bool {
    result.found
        && result.waypoints.last().is_some_and(|p| {
            p.floor == query.to.floor && (p.x - query.to.x).hypot(p.z - query.to.z) < 0.01
        })
}

fn profile(fixtures: &Fixtures, rounds: usize) {
    for (group, queries) in fixtures.queries.iter().enumerate() {
        for query in queries {
            black_box(fixtures.run(query));
        }
        let mut times = Vec::new();
        let mut complete = 0;
        let mut one_waypoint = 0;
        let mut waypoints = 0;
        let mut slowest = (0.0, &queries[0]);
        for _ in 0..rounds {
            for query in queries {
                let start = Instant::now();
                let result = black_box(fixtures.run(black_box(query)));
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                complete += usize::from(reaches_goal(query, &result));
                one_waypoint += usize::from(result.found && result.waypoints.len() == 1);
                waypoints += result.waypoints.len();
                if ms > slowest.0 {
                    slowest = (ms, query);
                }
                times.push(ms);
            }
        }
        println!(
            "{}",
            json!({"kind": "profile", "group": GROUPS[group], "queries": queries.len(),
            "samples": times.len(), "complete": complete, "one_waypoint": one_waypoint,
            "mean_waypoints": waypoints as f64 / times.len() as f64,
            "service_ms": distribution(times), "slowest_query": slowest.1})
        );
    }
}

struct Job {
    group: usize,
    query: usize,
    due: Instant,
    sent: Instant,
}
struct Sample {
    group: usize,
    queue_ms: f64,
    service_ms: f64,
    latency_ms: f64,
    finished: Instant,
    complete: bool,
}

fn process_cpu_seconds(ticks: Option<f64>) -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?) / ticks?)
}

fn load(fixtures: Arc<Fixtures>, config: &Config, rate: usize, workers: usize, ticks: Option<f64>) {
    let (tx, rx) = mpsc::sync_channel::<Job>(config.queue);
    let rx = Arc::new(Mutex::new(rx));
    let mut handles = Vec::new();
    for _ in 0..workers {
        let rx = rx.clone();
        let fixtures = fixtures.clone();
        handles.push(thread::spawn(move || {
            let mut samples = Vec::new();
            loop {
                let job = rx.lock().unwrap().recv();
                let Ok(job) = job else {
                    break;
                };
                let start = Instant::now();
                let query = &fixtures.queries[job.group][job.query];
                let result = black_box(fixtures.run(black_box(query)));
                let finished = Instant::now();
                samples.push(Sample {
                    group: job.group,
                    queue_ms: (start - job.sent).as_secs_f64() * 1000.0,
                    service_ms: (finished - start).as_secs_f64() * 1000.0,
                    latency_ms: (finished - job.due).as_secs_f64() * 1000.0,
                    finished,
                    complete: reaches_goal(query, &result),
                });
            }
            samples
        }));
    }
    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    let mix = WeightedIndex::new(weights(&config.mix).unwrap()).unwrap();
    let count = (rate as f64 * config.seconds).round() as usize;
    let schedule: Vec<_> = (0..count)
        .map(|_| {
            let group = mix.sample(&mut rng);
            (group, rng.gen_range(0..fixtures.queries[group].len()))
        })
        .collect();
    let start = Instant::now() + Duration::from_millis(50);
    thread::sleep(start.saturating_duration_since(Instant::now()));
    let cpu_start = process_cpu_seconds(ticks);
    let deadline = start + Duration::from_secs_f64(config.seconds);
    let mut dropped = [0usize; 7];
    let mut offered = [0usize; 7];
    let mut generator_lag = Vec::with_capacity(count);
    for (index, (group, query)) in schedule.into_iter().enumerate() {
        let due = start + Duration::from_secs_f64(index as f64 / rate as f64);
        let remaining = due.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            thread::sleep(remaining);
        }
        let sent = Instant::now();
        generator_lag.push(sent.saturating_duration_since(due).as_secs_f64() * 1000.0);
        offered[group] += 1;
        match tx.try_send(Job {
            group,
            query,
            due,
            sent,
        }) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => dropped[group] += 1,
            Err(mpsc::TrySendError::Disconnected(_)) => panic!("workers disconnected"),
        }
    }
    drop(tx);
    let mut samples = Vec::new();
    for handle in handles {
        samples.extend(handle.join().expect("pathfinding worker panicked"));
    }
    thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let elapsed = start.elapsed().as_secs_f64();
    let cpu = process_cpu_seconds(ticks)
        .zip(cpu_start)
        .map(|(end, start)| end - start);
    let finished_in_window = samples.iter().filter(|s| s.finished <= deadline).count();
    let group_stats: Vec<_> = GROUPS.iter().enumerate().map(|(group, name)| {
        let subset: Vec<_> = samples.iter().filter(|s| s.group == group).collect();
        json!({"group": name, "offered": offered[group], "dropped": dropped[group],
            "completed": subset.len(), "complete_paths": subset.iter().filter(|s| s.complete).count(),
            "latency_ms": distribution(subset.iter().map(|s| s.latency_ms).collect())})
    }).collect();
    assert_eq!(count, samples.len() + dropped.iter().sum::<usize>());
    println!(
        "{}",
        json!({"kind": "load", "mix": config.mix, "weights": weights(&config.mix).unwrap(),
        "rate": rate, "workers": workers, "seconds": config.seconds, "queue_capacity": config.queue,
        "offered": count, "completed": samples.len(), "dropped": dropped.iter().sum::<usize>(),
        "finished_in_window": finished_in_window, "in_window_rps": finished_in_window as f64 / config.seconds,
        "drain_ms": (elapsed - config.seconds).max(0.0) * 1000.0,
        "cpu_seconds": cpu, "cpu_cores_over_run": cpu.map(|s| s / elapsed),
        "service_ms": distribution(samples.iter().map(|s| s.service_ms).collect()),
        "queue_ms": distribution(samples.iter().map(|s| s.queue_ms).collect()),
        "latency_ms": distribution(samples.iter().map(|s| s.latency_ms).collect()),
        "generator_lag_ms": distribution(generator_lag), "groups": group_stats})
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::parse()?;
    if cfg!(debug_assertions) {
        return Err("run with --release".into());
    }
    let fixtures = Arc::new(fixtures::load(&config.root, config.seed)?);
    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .map(|s| s.trim_start_matches(['\t', ' ', ':']).to_owned())
            })
        });
    println!(
        "{}",
        json!({"kind": "environment", "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH, "cpu_model": cpu_model,
        "available_threads": thread::available_parallelism().ok().map(|n| n.get()),
        "profile": "release", "data_root": config.root})
    );
    println!("{}", fixtures.manifest);
    let ticks = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok());
    if config.rounds > 0 {
        profile(&fixtures, config.rounds);
    }
    for &workers in &config.workers {
        for &rate in &config.rates {
            load(fixtures.clone(), &config, rate, workers, ticks);
        }
    }
    Ok(())
}
