use std::time::Duration;

use rand::Rng;
use tokio::time::Instant;
use tracing::{info, warn};

const INITIAL_DELAY: Duration = Duration::from_secs(15);
const MAX_DELAY: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct PromptBackoff {
    delay: Duration,
    retry_at: Option<Instant>,
}

impl PromptBackoff {
    pub(super) fn ready(&self) -> bool {
        self.retry_at.is_none_or(|at| Instant::now() >= at)
    }

    pub(super) fn record_result(&mut self, result: &anyhow::Result<String>, label: &str) {
        if result.is_ok() {
            if self.retry_at.take().is_some() {
                info!("[{label}] LLM recovered; restoring normal prompt interval");
            }
            self.delay = Duration::ZERO;
            return;
        }

        self.delay = (self.delay * 2).clamp(INITIAL_DELAY, MAX_DELAY);
        let jitter = rand::thread_rng().gen_range(0..=self.delay.as_millis() as u64 / 5);
        let wait = self.delay - Duration::from_millis(jitter);
        self.retry_at = Some(Instant::now() + wait);
        warn!(
            "[{label}] LLM error backoff: next prompt allowed in {:.1}s",
            wait.as_secs_f64()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn consecutive_failures_back_off_until_the_cap_and_success_resets() {
        let mut backoff = PromptBackoff::default();
        let failure = Err(anyhow::anyhow!("Selected model is at capacity"));
        assert!(backoff.ready());

        for seconds in [15, 30, 60, 120, 240, 300, 300, 300] {
            let now = Instant::now();
            backoff.record_result(&failure, "test");
            let wait = backoff.retry_at.unwrap() - now;
            let ceiling = Duration::from_secs(seconds);
            assert!((ceiling * 4 / 5..=ceiling).contains(&wait));
            assert!(!backoff.ready());
            tokio::time::advance(wait - Duration::from_millis(1)).await;
            assert!(!backoff.ready());
            tokio::time::advance(Duration::from_millis(1)).await;
            assert!(backoff.ready());
        }

        backoff.record_result(&Ok("{}".into()), "test");
        assert!(backoff.ready());
        assert!(backoff.retry_at.is_none());
        backoff.record_result(&failure, "test");
        let wait = backoff.retry_at.unwrap() - Instant::now();
        assert!((Duration::from_secs(12)..=INITIAL_DELAY).contains(&wait));
    }
}
