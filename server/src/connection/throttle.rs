use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tracing::warn;

/// Log each IP's first event and summarize its repeats every minute.
pub const REFUSAL_LOG_WINDOW: Duration = Duration::from_secs(60);
const MAX_REFUSAL_LOG_IPS: usize = 4096;

struct LogWindow {
    started: Instant,
    suppressed: u32,
}

/// Separate refusal reasons keep unrelated first events visible.
pub(super) struct LogThrottle {
    label: &'static str,
    windows: Mutex<LogWindows>,
}

#[derive(Default)]
struct LogWindows {
    by_ip: HashMap<IpAddr, LogWindow>,
    untracked: u32,
}

impl LogThrottle {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            windows: Mutex::default(),
        }
    }

    /// Return the repeat summary, or None inside an open window.
    pub(super) fn claim(&self, ip: IpAddr) -> Option<String> {
        self.claim_at(ip, Instant::now())
    }

    fn claim_at(&self, ip: IpAddr, now: Instant) -> Option<String> {
        let mut guard = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let windows = &mut *guard;
        if let Some(window) = windows.by_ip.get_mut(&ip) {
            if now.duration_since(window.started) < REFUSAL_LOG_WINDOW {
                window.suppressed += 1;
                return None;
            }
        } else if windows.by_ip.len() >= MAX_REFUSAL_LOG_IPS {
            // Fail closed: under an address-rotating flood, logging every new IP is the worse failure.
            windows.untracked += 1;
            return None;
        }
        let suppressed = windows
            .by_ip
            .insert(
                ip,
                LogWindow {
                    started: now,
                    suppressed: 0,
                },
            )
            .map_or(0, |window| window.suppressed);
        Some(match suppressed {
            0 => String::new(),
            n => format!(" [+{n} more in the last {}s]", REFUSAL_LOG_WINDOW.as_secs()),
        })
    }

    /// Drop expired windows, returning the repeats no later claim reported.
    fn sweep_at(&self, now: Instant) -> Vec<String> {
        let mut guard = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let windows = &mut *guard;
        let secs = REFUSAL_LOG_WINDOW.as_secs();
        let mut lines = Vec::new();
        windows.by_ip.retain(|ip, window| {
            let open = now.duration_since(window.started) < REFUSAL_LOG_WINDOW;
            if !open && window.suppressed > 0 {
                lines.push(format!(
                    "{} from {ip}: +{} more within {secs}s of the last report",
                    self.label, window.suppressed
                ));
            }
            open
        });
        if windows.untracked > 0 {
            lines.push(format!(
                "{}: +{} more in the last {secs}s from IPs past the {MAX_REFUSAL_LOG_IPS}-IP log table",
                self.label,
                std::mem::take(&mut windows.untracked)
            ));
        }
        lines
    }
}

pub(super) static PROTOCOL_REFUSAL_LOG: LazyLock<LogThrottle> =
    LazyLock::new(|| LogThrottle::new("Protocol refusals"));
pub(super) static EARLY_MESSAGE_LOG: LazyLock<LogThrottle> =
    LazyLock::new(|| LogThrottle::new("Pre-ClientInfo refusals"));
pub(super) static RATE_LIMIT_LOG: LazyLock<LogThrottle> =
    LazyLock::new(|| LogThrottle::new("Rate-limited connections"));

/// Runs every `REFUSAL_LOG_WINDOW` so quiet IPs' repeat counts still reach the log.
pub fn flush_refusal_logs() {
    let now = Instant::now();
    for throttle in [&PROTOCOL_REFUSAL_LOG, &EARLY_MESSAGE_LOG, &RATE_LIMIT_LOG] {
        for line in throttle.sweep_at(now) {
            warn!("{line}");
        }
    }
}

#[cfg(test)]
mod tests;
