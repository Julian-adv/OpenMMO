//! Per-source-IP admission control for the WebSocket accept loop.
//!
//! Guards against a client fleet that reconnects in a tight loop — the failure
//! mode after a protocol bump, when builds we cannot redeploy are refused at
//! the handshake and retry forever. One runaway host is capped; a NAT'd site
//! full of legitimate players is not.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bucket depth: how many sessions one IP may open back to back.
const BURST: f32 = 20.0;
/// Sustained rate once the burst is spent. Generous enough for carrier-grade
/// NAT, where thousands of players can share a single address.
const REFILL_PER_SEC: f32 = 2.0;
/// Buckets untouched this long are dropped, so scanners can't grow the map.
const IDLE_EVICT: Duration = Duration::from_secs(300);
const PRUNE_INTERVAL: Duration = Duration::from_secs(60);
/// Hard ceiling on tracked addresses. Past it we stop admitting new buckets
/// rather than let an address-rotating flood grow the map (and the prune scan)
/// without bound.
const MAX_TRACKED_IPS: usize = 100_000;

/// The address to hold a client accountable for.
///
/// Behind nginx the TCP peer is always 127.0.0.1 and the real address arrives
/// in `X-Real-IP`, which nginx overwrites with `$remote_addr` — so it is
/// trustworthy, but only from nginx. Bound to 0.0.0.0 the listener is also
/// reachable directly, where the header would be attacker-supplied; there the
/// peer address is already the truth. Trusting it only from loopback covers
/// both, and pairs with `allow`'s loopback exemption so each connection is
/// charged exactly once.
pub fn resolve_client_ip(peer: SocketAddr, forwarded: Option<IpAddr>) -> IpAddr {
    match forwarded {
        Some(ip) if peer.ip().is_loopback() => ip,
        _ => peer.ip(),
    }
}

struct Bucket {
    tokens: f32,
    last: Instant,
}

struct Inner {
    buckets: HashMap<IpAddr, Bucket>,
    last_prune: Instant,
}

pub struct ConnectLimiter {
    inner: Mutex<Inner>,
}

impl Default for ConnectLimiter {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                buckets: HashMap::new(),
                last_prune: Instant::now(),
            }),
        }
    }
}

impl ConnectLimiter {
    /// False when `ip` is over budget and the connection should be dropped.
    ///
    /// Loopback is always allowed: behind nginx every socket arrives from
    /// 127.0.0.1, so charging it would put the whole playerbase in one bucket.
    /// Callers charge the address `resolve_client_ip` picked instead.
    pub fn allow(&self, ip: IpAddr) -> bool {
        if ip.is_loopback() {
            return true;
        }

        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if now.duration_since(inner.last_prune) >= PRUNE_INTERVAL {
            inner.last_prune = now;
            inner
                .buckets
                .retain(|_, b| now.duration_since(b.last) < IDLE_EVICT);
        }

        let at_capacity = inner.buckets.len() >= MAX_TRACKED_IPS;
        let bucket = match inner.buckets.entry(ip) {
            Entry::Occupied(e) => e.into_mut(),
            // Fail open: an unbounded map is the worse failure.
            Entry::Vacant(_) if at_capacity => return true,
            Entry::Vacant(e) => e.insert(Bucket {
                tokens: BURST,
                last: now,
            }),
        };

        let refilled = now.duration_since(bucket.last).as_secs_f32() * REFILL_PER_SEC;
        bucket.tokens = (bucket.tokens + refilled).min(BURST);
        bucket.last = now;

        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, last))
    }

    #[test]
    fn burst_is_allowed_then_exhausted() {
        let limiter = ConnectLimiter::default();
        for _ in 0..BURST as usize {
            assert!(limiter.allow(ip(1)));
        }
        assert!(!limiter.allow(ip(1)));
    }

    #[test]
    fn buckets_are_per_ip() {
        let limiter = ConnectLimiter::default();
        for _ in 0..BURST as usize {
            assert!(limiter.allow(ip(1)));
        }
        assert!(!limiter.allow(ip(1)));
        assert!(limiter.allow(ip(2)));
    }

    #[test]
    fn loopback_is_never_limited() {
        let limiter = ConnectLimiter::default();
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..(BURST as usize * 3) {
            assert!(limiter.allow(local));
        }
    }

    #[test]
    fn forwarded_ip_is_trusted_only_from_the_local_proxy() {
        let real: IpAddr = "203.0.113.7".parse().unwrap();
        let via_proxy: SocketAddr = "127.0.0.1:44100".parse().unwrap();
        let direct: SocketAddr = "198.51.100.9:44100".parse().unwrap();

        // Behind nginx the header is the only real address we get.
        assert_eq!(resolve_client_ip(via_proxy, Some(real)), real);
        // Straight to the listener it is attacker-supplied, so the peer wins.
        assert_eq!(resolve_client_ip(direct, Some(real)), direct.ip());
        // Proxy without the header falls back to the exempt loopback address.
        assert_eq!(resolve_client_ip(via_proxy, None), via_proxy.ip());
    }

    /// The two call sites split on `is_loopback`, so each connection must be
    /// charged exactly once however it arrived.
    #[test]
    fn each_connection_is_charged_once() {
        let limiter = ConnectLimiter::default();
        let real: IpAddr = "203.0.113.7".parse().unwrap();
        let via_proxy: SocketAddr = "127.0.0.1:44100".parse().unwrap();

        // Accept loop sees loopback and skips; the post-upgrade check charges.
        for _ in 0..BURST as usize {
            assert!(limiter.allow(via_proxy.ip()));
            assert!(limiter.allow(resolve_client_ip(via_proxy, Some(real))));
        }
        assert!(!limiter.allow(real));
    }
}
