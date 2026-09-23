use onlinerpg_shared::pathfinding::{
    find_and_smooth_path, PassabilityCache, PathResult, PathWaypoint,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const WORKERS: usize = 4;
const QUEUED_SEARCHES: usize = 128;
const QUEUE_TIMEOUT: Duration = Duration::from_millis(100);

pub(super) struct PathSearchPool {
    workers: Arc<Semaphore>,
    admission: Arc<Semaphore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SearchError {
    Busy,
    QueueTimeout,
    WorkerFailed,
}

impl Default for PathSearchPool {
    fn default() -> Self {
        Self::new(WORKERS, QUEUED_SEARCHES)
    }
}

impl PathSearchPool {
    #[cfg(test)]
    pub(super) async fn reserve_workers(&self) -> tokio::sync::OwnedSemaphorePermit {
        Arc::clone(&self.workers)
            .acquire_many_owned(WORKERS as u32)
            .await
            .expect("reserve workers")
    }

    fn new(workers: usize, queued: usize) -> Self {
        Self {
            workers: Arc::new(Semaphore::new(workers)),
            admission: Arc::new(Semaphore::new(workers + queued)),
        }
    }

    pub(super) async fn search(
        &self,
        cache: Arc<PassabilityCache>,
        start: PathWaypoint,
        goal: PathWaypoint,
        max_nodes: usize,
    ) -> Result<PathResult, SearchError> {
        self.run(move || {
            find_and_smooth_path(
                start.x,
                start.z,
                start.floor,
                goal.x,
                goal.z,
                goal.floor,
                &cache,
                max_nodes,
            )
        })
        .await
    }

    async fn run<T: Send + 'static>(
        &self,
        search: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, SearchError> {
        let admission = Arc::clone(&self.admission)
            .try_acquire_owned()
            .map_err(|_| SearchError::Busy)?;
        let worker = tokio::time::timeout(QUEUE_TIMEOUT, Arc::clone(&self.workers).acquire_owned())
            .await
            .map_err(|_| SearchError::QueueTimeout)?
            .map_err(|_| SearchError::WorkerFailed)?;
        tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let _worker = worker;
            search()
        })
        .await
        .map_err(|_| SearchError::WorkerFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn saturation_rejects_without_running_the_job() {
        let pool = PathSearchPool::new(1, 0);
        let admission = pool.admission.acquire().await.expect("admission");
        let result = pool.run(|| panic!("overload must not execute")).await;
        assert_eq!(result, Err(SearchError::Busy));
        drop(admission);
        assert_eq!(pool.run(|| 7).await, Ok(7));
    }

    #[tokio::test(start_paused = true)]
    async fn queue_deadline_releases_admission_without_running() {
        let pool = PathSearchPool::new(1, 1);
        let worker = pool.workers.acquire().await.expect("worker");
        let result = pool.run(|| panic!("expired work must not execute")).await;
        assert_eq!(result, Err(SearchError::QueueTimeout));
        assert_eq!(pool.admission.available_permits(), 2);
        drop(worker);
    }

    #[tokio::test]
    async fn cancelling_the_caller_keeps_a_running_worker_counted() {
        let pool = Arc::new(PathSearchPool::new(1, 0));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let worker_pool = Arc::clone(&pool);
        let task = tokio::spawn(async move {
            worker_pool
                .run(move || {
                    let _ = started_tx.send(());
                    finish_rx.recv().expect("release worker");
                })
                .await
        });
        started_rx.await.expect("started");
        task.abort();
        assert!(task.await.expect_err("cancelled").is_cancelled());
        assert_eq!(pool.run(|| ()).await, Err(SearchError::Busy));
        finish_tx.send(()).expect("finish");
        let _finished = tokio::time::timeout(Duration::from_secs(2), pool.admission.acquire())
            .await
            .expect("worker released its permit")
            .expect("admission");
    }

    #[tokio::test]
    async fn cancelled_queued_work_does_not_start_later() {
        let pool = Arc::new(PathSearchPool::new(1, 1));
        let worker = pool.workers.acquire().await.expect("worker");
        let ran = Arc::new(AtomicBool::new(false));
        let job_pool = Arc::clone(&pool);
        let job_ran = Arc::clone(&ran);
        let task = tokio::spawn(async move {
            job_pool
                .run(move || job_ran.store(true, Ordering::SeqCst))
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(pool.admission.available_permits(), 1);
        task.abort();
        assert!(task.await.expect_err("cancelled").is_cancelled());
        drop(worker);
        assert!(!ran.load(Ordering::SeqCst));
        assert_eq!(pool.admission.available_permits(), 2);
    }
}
