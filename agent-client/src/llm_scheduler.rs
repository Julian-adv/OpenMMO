//! LLM Scheduler: centralized priority queue + concurrency limiter for LLM calls.
//!
//! All NPC drivers submit LLM requests through the scheduler instead of calling
//! backends directly. The scheduler dispatches requests respecting `max_concurrent`
//! and prioritizing urgent events over routine/idle polls.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

use crate::driver::LlmBackend;
use crate::state::EventUrgency;

/// Priority levels for LLM requests (lower number = higher priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LlmPriority {
    /// Urgent: combat damage, direct chat, death (process ASAP)
    Urgent = 0,
    /// Routine: periodic active-mode poll with events
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

/// A request submitted to the scheduler.
struct LlmRequest {
    priority: LlmPriority,
    submitted_at: Instant,
    prompt: String,
    invoker: Arc<dyn LlmBackend>,
    response_tx: oneshot::Sender<anyhow::Result<String>>,
    /// Label for logging (e.g. NPC account name)
    label: String,
}

// BinaryHeap is a max-heap: we want urgent (priority=0) to be "greatest".
impl Eq for LlmRequest {}
impl PartialEq for LlmRequest {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.submitted_at == other.submitted_at
    }
}

impl Ord for LlmRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority value = more urgent = should be popped first (= "greater")
        (other.priority as u8)
            .cmp(&(self.priority as u8))
            // Break ties by submission time: older requests first
            .then_with(|| other.submitted_at.cmp(&self.submitted_at))
    }
}

impl PartialOrd for LlmRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Handle for submitting LLM requests to the scheduler.
#[derive(Clone)]
pub struct LlmScheduler {
    request_tx: mpsc::UnboundedSender<LlmRequest>,
}

impl LlmScheduler {
    /// Create a new scheduler and spawn its background task.
    ///
    /// `max_concurrent`: maximum number of simultaneous LLM calls across all NPCs.
    pub fn new(max_concurrent: usize) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        tokio::spawn(scheduler_loop(request_rx, max_concurrent));
        info!("LLM scheduler started (max_concurrent={})", max_concurrent);
        Self { request_tx }
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
        let (response_tx, response_rx) = oneshot::channel();
        let request = LlmRequest {
            priority,
            submitted_at: Instant::now(),
            prompt,
            invoker,
            response_tx,
            label: label.to_string(),
        };
        self.request_tx
            .send(request)
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
    let mut queue: BinaryHeap<LlmRequest> = BinaryHeap::new();
    let mut in_flight: usize = 0;
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<()>();

    loop {
        // Dispatch as many queued requests as slots allow
        while in_flight < max_concurrent {
            if let Some(req) = queue.pop() {
                // Skip requests whose receiver has been dropped (NPC disconnected)
                if req.response_tx.is_closed() {
                    debug!("[Scheduler] Skipping orphaned request for '{}'", req.label);
                    continue;
                }
                in_flight += 1;
                debug!(
                    "[Scheduler] Dispatching {:?} request for '{}' ({} in flight, {} queued)",
                    req.priority,
                    req.label,
                    in_flight,
                    queue.len()
                );
                let done_tx = done_tx.clone();
                tokio::spawn(async move {
                    let result = req.invoker.send_message(&req.prompt).await;
                    let _ = req.response_tx.send(result);
                    let _ = done_tx.send(());
                });
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
                  req.priority, req.label, in_flight, queue.len() + 1
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
