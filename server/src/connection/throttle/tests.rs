use super::*;
use std::net::Ipv4Addr;

#[test]
fn refusal_logging_tracks_each_ip_and_repeat_count_independently() {
    let throttle = LogThrottle::new("Test refusals");
    let start = Instant::now();
    let first = "203.0.113.1".parse().unwrap();
    let second = "2001:db8::2".parse().unwrap();
    assert_eq!(throttle.claim_at(first, start), Some(String::new()));
    for _ in 0..5 {
        assert_eq!(throttle.claim_at(first, start), None);
    }
    assert_eq!(throttle.claim_at(second, start), Some(String::new()));
    assert_eq!(throttle.claim_at(second, start), None);
    assert_eq!(
        throttle.claim_at(first, start + REFUSAL_LOG_WINDOW - Duration::from_nanos(1)),
        None
    );
    let next = start + REFUSAL_LOG_WINDOW;
    assert_eq!(
        throttle.claim_at(first, next),
        Some(" [+6 more in the last 60s]".into())
    );
    assert_eq!(
        throttle.claim_at(second, next),
        Some(" [+1 more in the last 60s]".into())
    );
    assert_eq!(throttle.claim_at(first, next), None);
    assert_eq!(throttle.claim_at(second, next), None);
}

#[test]
fn refusal_logging_sweep_reports_repeats_of_quiet_ips() {
    let throttle = LogThrottle::new("Test refusals");
    let start = Instant::now();
    let noisy = "203.0.113.1".parse().unwrap();
    let quiet = "203.0.113.2".parse().unwrap();
    for _ in 0..3 {
        throttle.claim_at(noisy, start);
    }
    throttle.claim_at(quiet, start);
    assert!(throttle
        .sweep_at(start + REFUSAL_LOG_WINDOW - Duration::from_nanos(1))
        .is_empty());
    assert_eq!(
        throttle.sweep_at(start + REFUSAL_LOG_WINDOW),
        ["Test refusals from 203.0.113.1: +2 more within 60s of the last report"]
    );
    assert!(throttle.windows.lock().unwrap().by_ip.is_empty());
    assert_eq!(
        throttle.claim_at(noisy, start + REFUSAL_LOG_WINDOW),
        Some(String::new())
    );
}

#[test]
fn refusal_logging_counts_ips_past_the_table_until_the_next_sweep() {
    let throttle = LogThrottle::new("Test refusals");
    let start = Instant::now();
    let ip = |n: u32| IpAddr::V4(Ipv4Addr::from(n));
    let max = MAX_REFUSAL_LOG_IPS as u32;
    for n in 0..max {
        assert_eq!(throttle.claim_at(ip(n), start), Some(String::new()));
    }
    assert_eq!(
        throttle.claim_at(ip(max), start + Duration::from_secs(1)),
        None
    );
    assert_eq!(
        throttle.windows.lock().unwrap().by_ip.len(),
        MAX_REFUSAL_LOG_IPS
    );
    let latest = start + REFUSAL_LOG_WINDOW;
    assert_eq!(
        throttle.sweep_at(latest),
        ["Test refusals: +1 more in the last 60s from IPs past the 4096-IP log table"]
    );
    assert!(throttle.windows.lock().unwrap().by_ip.is_empty());
    assert_eq!(throttle.claim_at(ip(max), latest), Some(String::new()));
}
