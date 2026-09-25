//! LLM Scheduler: centralized priority queue + concurrency limiter for LLM calls.
//!
//! All NPC drivers submit LLM requests through the scheduler instead of calling
//! backends directly. The scheduler dispatches requests respecting `max_concurrent`
//! and preventing urgent traffic from starving routine requests.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

use crate::driver::LlmBackend;
use crate::state::EventUrgency;

const ROUTINE_WAIT_LIMIT: Duration = Duration::from_secs(10);

tokio::task_local! {
    /// How long the request waited for a slot before this task was dispatched.
    /// Published into the dispatched task so an observer running inside it (the
    /// spectator panel's backend wrapper) can attribute the wait without the
    /// scheduler having to know that observer exists.
    static QUEUE_WAIT: Duration;
}

/// Queue wait for the LLM turn running in the current task, if dispatched by
/// the scheduler. `None` when called outside a dispatched turn.
pub fn queue_wait() -> Option<Duration> {
    QUEUE_WAIT.try_with(|d| *d).ok()
}

/// Priority levels for LLM requests (lower number = higher priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LlmPriority {
    /// Urgent: combat damage, direct chat, death (process ASAP)
    Urgent = 0,
    /// Routine: ordinary conversation, action feedback, and active-mode polls
    Routine = 1,
    /// Idle: periodic idle poll (lowest priority)
    Idle = 2,
}

impl From<EventUrgency> for LlmPriority {
    fn from(u: EventUrgency) -> Self {
        match u {
            EventUrgency::Urgent => LlmPriority::Urgent,
            EventUrgency::Routine => LlmPriority::Routine,
            EventUrgency::Noise => LlmPriority::Idle,
        }
    }
}

#[derive(Clone)]
pub struct RequestPriority(Arc<AtomicU8>);

impl RequestPriority {
    pub fn new(priority: LlmPriority) -> Self {
        Self(Arc::new(AtomicU8::new(priority as u8)))
    }

    pub fn promote(&self, priority: LlmPriority) {
        self.0.fetch_min(priority as u8, Ordering::Relaxed);
    }

    fn get(&self) -> LlmPriority {
        match self.0.load(Ordering::Relaxed) {
            0 => LlmPriority::Urgent,
            1 => LlmPriority::Routine,
            _ => LlmPriority::Idle,
        }
    }
}

struct LlmRequest {
    priority: RequestPriority,
    submitted_at: Instant,
    run: Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>,
    response_tx: oneshot::Sender<anyhow::Result<String>>,
    label: String,
}

impl LlmRequest {
    fn dispatch_key(&self, now: Instant) -> (u8, Instant) {
        let priority = self.priority.get();
        let rank = if priority == LlmPriority::Routine
            && now.saturating_duration_since(self.submitted_at) >= ROUTINE_WAIT_LIMIT
        {
            0
        } else {
            priority as u8 + 1
        };
        (rank, self.submitted_at)
    }
}

/// Limits execution time; queue waiting is measured separately.
pub struct TimeoutBackend {
    inner: Arc<dyn LlmBackend>,
    timeout: Duration,
}

impl TimeoutBackend {
    /// `Duration::ZERO` leaves the backend unwrapped — an escape hatch for
    /// stepping through a backend under a debugger.
    pub fn wrap(inner: Arc<dyn LlmBackend>, timeout: Duration) -> Arc<dyn LlmBackend> {
        if timeout.is_zero() {
            return inner;
        }
        Arc::new(Self { inner, timeout })
    }
}

#[async_trait]
impl LlmBackend for TimeoutBackend {
    async fn send_message(&self, content: &str) -> anyhow::Result<String> {
        tokio::time::timeout(self.timeout, self.inner.send_message(content))
            .await
            .map_err(|_| anyhow::anyhow!("LLM call timed out after {}s", self.timeout.as_secs()))?
    }
}

/// Handle for submitting LLM requests to the scheduler.
#[derive(Clone)]
pub struct LlmScheduler {
    request_tx: mpsc::UnboundedSender<LlmRequest>,
    request_timeout: Duration,
}

impl LlmScheduler {
    /// Create a new scheduler and spawn its background task.
    ///
    /// `max_concurrent`: maximum number of simultaneous LLM calls across all NPCs.
    /// `request_timeout`: what every backend is wrapped in before it can be
    /// submitted — it guards the slot, not any one provider.
    pub fn new(max_concurrent: usize, request_timeout: Duration) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        tokio::spawn(scheduler_loop(request_rx, max_concurrent));
        info!(
            "LLM scheduler started (max_concurrent={}, request_timeout={}s)",
            max_concurrent,
            request_timeout.as_secs()
        );
        Self {
            request_tx,
            request_timeout,
        }
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Submit an LLM request and wait for the response.
    ///
    /// The request is queued by priority. When a slot is available, the scheduler
    /// dispatches the call and returns the result.
    pub async fn submit(
        &self,
        label: &str,
        priority: LlmPriority,
        prompt: String,
        invoker: Arc<dyn LlmBackend>,
    ) -> anyhow::Result<String> {
        self.submit_deferred(label, RequestPriority::new(priority), async move {
            invoker.send_message(&prompt).await
        })
        .await
    }

    pub async fn submit_deferred<F>(
        &self,
        label: &str,
        priority: RequestPriority,
        run: F,
    ) -> anyhow::Result<String>
    where
        F: Future<Output = anyhow::Result<String>> + Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();
        info!(
            "LLM request queued npc={label} priority={:?}",
            priority.get()
        );
        self.request_tx
            .send(LlmRequest {
                priority,
                submitted_at: Instant::now(),
                run: Box::pin(run),
                response_tx,
                label: label.to_string(),
            })
            .map_err(|_| anyhow::anyhow!("LLM scheduler shut down"))?;
        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("LLM scheduler dropped request"))?
    }
}

/// The scheduler's main loop. Receives requests, queues by priority, dispatches
/// up to `max_concurrent` at a time.
async fn scheduler_loop(
    mut request_rx: mpsc::UnboundedReceiver<LlmRequest>,
    max_concurrent: usize,
) {
    let mut queue: Vec<LlmRequest> = Vec::new();
    let mut in_flight: usize = 0;
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<()>();

    loop {
        // Dispatch as many queued requests as slots allow
        while in_flight < max_concurrent {
            let now = Instant::now();
            let next = queue
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| r.dispatch_key(now))
                .map(|(i, _)| i);
            if let Some(index) = next {
                let req = queue.remove(index);
                // Skip requests whose receiver has been dropped (NPC disconnected)
                if req.response_tx.is_closed() {
                    debug!("[Scheduler] Skipping orphaned request for '{}'", req.label);
                    continue;
                }
                in_flight += 1;
                debug!(
                    "[Scheduler] Dispatching {:?} request for '{}' ({} in flight, {} queued)",
                    req.priority.get(),
                    req.label,
                    in_flight,
                    queue.len()
                );
                let done_tx = done_tx.clone();
                let waited = req.submitted_at.elapsed();
                info!(
                    "LLM request dispatched npc={} priority={:?} wait={:.1}s routine_overdue={}",
                    req.label,
                    req.priority.get(),
                    waited.as_secs_f64(),
                    req.dispatch_key(now).0 == 0
                );
                tokio::spawn(QUEUE_WAIT.scope(waited, async move {
                    let result = req.run.await;
                    let _ = req.response_tx.send(result);
                    let _ = done_tx.send(());
                }));
            } else {
                break;
            }
        }

        // Wait for new request or completion notification
        tokio::select! {
          recv = request_rx.recv() => {
            match recv {
              Some(req) => {
                debug!(
                  "[Scheduler] Queued {:?} request for '{}' ({} in flight, {} queued)",
                  req.priority.get(), req.label, in_flight, queue.len() + 1
                );
                queue.push(req);
              }
              None => {
                // All senders dropped — scheduler shutting down
                debug!("[Scheduler] All senders dropped, shutting down");
                return;
              }
            }
          }
          Some(()) = done_rx.recv() => {
            in_flight = in_flight.saturating_sub(1);
            debug!("[Scheduler] Request completed ({} in flight, {} queued)", in_flight, queue.len());
          }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn queued_request(
        priority: LlmPriority,
        submitted_at: Instant,
    ) -> (LlmRequest, oneshot::Receiver<anyhow::Result<String>>) {
        let (response_tx, response_rx) = oneshot::channel();
        (
            LlmRequest {
                priority: RequestPriority::new(priority),
                submitted_at,
                run: Box::pin(async { Ok(String::new()) }),
                response_tx,
                label: "test".into(),
            },
            response_rx,
        )
    }

    #[test]
    fn routine_gets_the_next_slot_only_after_ten_seconds() {
        let submitted_at = Instant::now();
        let (routine, _routine_rx) = queued_request(LlmPriority::Routine, submitted_at);
        let (urgent, _urgent_rx) = queued_request(LlmPriority::Urgent, submitted_at);
        let before = submitted_at + ROUTINE_WAIT_LIMIT - Duration::from_nanos(1);
        let due = submitted_at + ROUTINE_WAIT_LIMIT;
        assert!(urgent.dispatch_key(before) < routine.dispatch_key(before));
        assert!(routine.dispatch_key(due) < urgent.dispatch_key(due));
        assert_eq!(routine.priority.get(), LlmPriority::Routine);
    }

    #[test]
    fn idle_promotion_preserves_wait_time_for_routine_scheduling() {
        let submitted_at = Instant::now();
        let now = submitted_at + Duration::from_secs(3600);
        let (request, _rx) = queued_request(LlmPriority::Idle, submitted_at);
        let (routine, _routine_rx) = queued_request(LlmPriority::Routine, now);
        assert!(routine.dispatch_key(now) < request.dispatch_key(now));
        request.priority.promote(LlmPriority::Routine);
        assert!(request.dispatch_key(now) < routine.dispatch_key(now));
    }

    async fn block_slot(
        scheduler: &LlmScheduler,
    ) -> (
        oneshot::Sender<()>,
        tokio::task::JoinHandle<anyhow::Result<String>>,
    ) {
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let blocker = {
            let scheduler = scheduler.clone();
            tokio::spawn(async move {
                scheduler
                    .submit_deferred(
                        "blocker",
                        RequestPriority::new(LlmPriority::Urgent),
                        async {
                            started_tx.send(()).unwrap();
                            release_rx.await.unwrap();
                            Ok(String::new())
                        },
                    )
                    .await
            })
        };
        started_rx.await.unwrap();
        (release_tx, blocker)
    }

    #[tokio::test]
    async fn overdue_routines_run_oldest_first_before_urgent_and_idle_stays_last() {
        let scheduler = LlmScheduler::new(1, Duration::from_secs(120));
        let (release_tx, blocker) = block_slot(&scheduler).await;

        let now = Instant::now();
        let (order_tx, mut order_rx) = mpsc::unbounded_channel();
        let mut responses = Vec::new();
        for (label, priority, age) in [
            ("idle", LlmPriority::Idle, 3600),
            ("urgent", LlmPriority::Urgent, 60),
            ("routine_newer", LlmPriority::Routine, 11),
            ("routine_oldest", LlmPriority::Routine, 12),
            ("routine_fresh", LlmPriority::Routine, 0),
        ] {
            let (mut request, response) = queued_request(priority, now - Duration::from_secs(age));
            let order_tx = order_tx.clone();
            request.run = Box::pin(async move {
                order_tx.send(label).unwrap();
                Ok(String::new())
            });
            scheduler.request_tx.send(request).unwrap();
            responses.push(response);
        }
        tokio::task::yield_now().await;
        assert!(order_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        for response in responses {
            response.await.unwrap().unwrap();
        }
        blocker.await.unwrap().unwrap();
        let mut order = Vec::new();
        while let Ok(label) = order_rx.try_recv() {
            order.push(label);
        }
        assert_eq!(
            order,
            [
                "routine_oldest",
                "routine_newer",
                "urgent",
                "routine_fresh",
                "idle"
            ]
        );
    }

    #[tokio::test]
    async fn queued_work_can_be_promoted_without_losing_its_place() {
        let scheduler = LlmScheduler::new(1, Duration::from_secs(120));
        let (release_tx, blocker) = block_slot(&scheduler).await;

        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let priority = RequestPriority::new(LlmPriority::Routine);
        let routine = scheduler.submit_deferred("routine", priority.clone(), {
            let order = Arc::clone(&order);
            async move {
                order.lock().unwrap().push("routine");
                Ok(String::new())
            }
        });
        tokio::pin!(routine);
        assert!(futures_util::poll!(&mut routine).is_pending());
        let urgent =
            scheduler.submit_deferred("urgent", RequestPriority::new(LlmPriority::Urgent), {
                let order = Arc::clone(&order);
                async move {
                    order.lock().unwrap().push("urgent");
                    Ok(String::new())
                }
            });
        tokio::pin!(urgent);
        assert!(futures_util::poll!(&mut urgent).is_pending());
        tokio::task::yield_now().await;
        assert!(order.lock().unwrap().is_empty());
        priority.promote(LlmPriority::Urgent);
        priority.promote(LlmPriority::Idle);
        assert_eq!(priority.get(), LlmPriority::Urgent);
        release_tx.send(()).unwrap();
        let (routine, urgent) = tokio::join!(routine, urgent);
        routine.unwrap();
        urgent.unwrap();
        blocker.await.unwrap().unwrap();
        assert_eq!(*order.lock().unwrap(), ["routine", "urgent"]);
    }

    #[tokio::test]
    async fn cancelled_queued_work_is_not_executed() {
        let scheduler = LlmScheduler::new(1, Duration::from_secs(120));
        {
            let cancelled = scheduler.submit_deferred(
                "cancelled",
                RequestPriority::new(LlmPriority::Urgent),
                async {
                    panic!("cancelled work must not execute");
                },
            );
            tokio::pin!(cancelled);
            assert!(futures_util::poll!(&mut cancelled).is_pending());
        }
        scheduler
            .submit("next", LlmPriority::Routine, String::new(), probe())
            .await
            .unwrap();
    }

    /// A backend that sleeps, then reports the queue wait the scheduler scoped
    /// into its task — proving the wait is readable exactly where the panel
    /// wrapper reads it.
    struct Probe {
        hold: Duration,
        seen: Arc<std::sync::Mutex<Vec<Option<Duration>>>>,
    }

    #[async_trait]
    impl LlmBackend for Probe {
        async fn send_message(&self, _prompt: &str) -> anyhow::Result<String> {
            self.seen.lock().unwrap().push(queue_wait());
            tokio::time::sleep(self.hold).await;
            Ok(String::new())
        }
    }

    /// An idle probe, for tests that need a backend but assert nothing on it.
    fn probe() -> Arc<dyn LlmBackend> {
        Arc::new(Probe {
            hold: Duration::ZERO,
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        })
    }

    /// Reports whether its call was cancelled: a hung stdio backend only stops
    /// spawning-and-leaking children if the future is actually dropped.
    #[derive(Default)]
    struct Hang {
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    }

    struct SetOnDrop(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl LlmBackend for Hang {
        async fn send_message(&self, _prompt: &str) -> anyhow::Result<String> {
            let _guard = SetOnDrop(Arc::clone(&self.cancelled));
            std::future::pending::<()>().await;
            unreachable!()
        }
    }

    #[tokio::test]
    async fn a_hung_backend_gives_up_and_is_cancelled() {
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let backend = TimeoutBackend::wrap(
            Arc::new(Hang {
                cancelled: Arc::clone(&cancelled),
            }),
            Duration::from_millis(50),
        );

        let err = backend.send_message("p").await.unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        // Dropping the inner future is what kills a `kill_on_drop` child.
        assert!(cancelled.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_hung_backend_only_holds_its_slot_until_the_timeout() {
        let sched = LlmScheduler::new(1, Duration::from_millis(50));
        let hung = TimeoutBackend::wrap(Arc::new(Hang::default()), sched.request_timeout());

        let stuck = {
            let s = sched.clone();
            tokio::spawn(async move {
                s.submit("hung", LlmPriority::Routine, "p".into(), hung)
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(10)).await; // let it claim the only slot

        // Would never return if the hung call kept the slot.
        let next = tokio::time::timeout(
            Duration::from_secs(5),
            sched.submit("next", LlmPriority::Routine, "p".into(), probe()),
        )
        .await;
        assert!(next.expect("slot never freed").is_ok());
        assert!(stuck.await.unwrap().is_err());
    }

    #[test]
    fn a_zero_timeout_leaves_the_backend_alone() {
        let inner = probe();
        let wrapped = TimeoutBackend::wrap(Arc::clone(&inner), Duration::ZERO);
        assert!(Arc::ptr_eq(&inner, &wrapped));
    }

    #[tokio::test]
    async fn queue_wait_reflects_time_spent_behind_a_full_scheduler() {
        let sched = LlmScheduler::new(1, Duration::from_secs(120)); // one slot, so the second turn waits
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let backend = |hold| {
            Arc::new(Probe {
                hold,
                seen: Arc::clone(&seen),
            }) as Arc<dyn LlmBackend>
        };

        let hold = Duration::from_millis(300);
        let a = {
            let s = sched.clone();
            let b = backend(hold);
            tokio::spawn(async move { s.submit("a", LlmPriority::Routine, "p".into(), b).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await; // let `a` claim the slot
        let b = {
            let s = sched.clone();
            let b = backend(Duration::ZERO);
            tokio::spawn(async move { s.submit("b", LlmPriority::Routine, "p".into(), b).await })
        };

        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();

        let waits = seen.lock().unwrap().clone();
        assert_eq!(waits.len(), 2);
        // `a` ran first with no meaningful wait; `b` sat behind `a`'s hold.
        assert!(waits[0].unwrap() < Duration::from_millis(100));
        assert!(
            waits[1].unwrap() >= Duration::from_millis(250),
            "second turn should report the queue wait, got {:?}",
            waits[1]
        );
    }

    #[tokio::test]
    async fn queue_wait_is_none_outside_a_dispatched_turn() {
        assert!(queue_wait().is_none());
    }
}
