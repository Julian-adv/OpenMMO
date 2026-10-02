use onlinerpg_shared::pathfinding::{
    find_and_smooth_path, is_cell_sealed, is_circle_blocked_on_floor, PassabilityCache, PathResult,
    PathTermination, PathWaypoint,
};
use onlinerpg_shared::shortest_world_delta_x;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const WORKERS: usize = 4;
const QUEUED_SEARCHES: usize = 128;
const QUEUE_TIMEOUT: Duration = Duration::from_millis(100);

fn open_goal_near(
    cache: &PassabilityCache,
    start: &PathWaypoint,
    goal: &PathWaypoint,
) -> Option<(f32, f32)> {
    let is_open = |x, z| {
        !is_cell_sealed(cache, x, z, goal.floor, None)
            && !is_circle_blocked_on_floor(cache, x, z, 0.31, goal.floor, None)
    };
    if is_open(goal.x, goal.z) {
        return None;
    }
    let (cx, cz) = (goal.x.floor() + 0.5, goal.z.floor() + 0.5);
    let distance = |(x, z): (f32, f32), point: &PathWaypoint| {
        shortest_world_delta_x(x, point.x).powi(2) + (z - point.z).powi(2)
    };
    (-2..=2)
        .flat_map(|dz| (-2..=2).map(move |dx| (cx + dx as f32, cz + dz as f32)))
        .filter(|&(x, z)| is_open(x, z))
        .min_by(|&a, &b| {
            distance(a, goal)
                .total_cmp(&distance(b, goal))
                .then_with(|| distance(a, start).total_cmp(&distance(b, start)))
        })
}

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
            let adjusted = open_goal_near(&cache, &start, &goal);
            let (x, z) = adjusted.unwrap_or((goal.x, goal.z));
            let mut path = find_and_smooth_path(
                start.x,
                start.z,
                start.floor,
                x,
                z,
                goal.floor,
                &cache,
                max_nodes,
            );
            if adjusted.is_some() && path.found {
                path.found = false;
                path.termination = PathTermination::Unreachable;
            }
            path
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
    use onlinerpg_shared::pathfinding::{build_furniture_passability, FurniturePiece};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn a_furniture_goal_finishes_beside_it_without_exhausting_the_search() {
        let pool = PathSearchPool::default();
        let furniture = build_furniture_passability(&[FurniturePiece {
            cells: vec![(5, 5), (5, 6)],
            floor_level: 0,
            y_base: 0.0,
            wall_height: onlinerpg_shared::furniture::FURNITURE_BLOCK_HEIGHT,
        }])
        .unwrap();
        let cache = Arc::new(PassabilityCache::from([("furniture".into(), furniture)]));
        let path = pool
            .search(
                Arc::clone(&cache),
                PathWaypoint {
                    x: 1.5,
                    z: 5.5,
                    floor: 0,
                },
                PathWaypoint {
                    x: 5.5,
                    z: 5.5,
                    floor: 0,
                },
                100,
            )
            .await
            .unwrap();
        assert_eq!(path.termination, PathTermination::Unreachable);
        let destination = path.waypoints.last().unwrap();
        assert_eq!((destination.x, destination.z), (4.5, 5.5));
        assert!(!path.found);
    }

    #[tokio::test]
    async fn a_goal_beside_furniture_leaves_room_for_the_body() {
        let pool = PathSearchPool::default();
        let furniture = build_furniture_passability(&[FurniturePiece {
            cells: vec![(4, 5)],
            floor_level: 0,
            y_base: 0.0,
            wall_height: onlinerpg_shared::furniture::FURNITURE_BLOCK_HEIGHT,
        }])
        .unwrap();
        let cache = Arc::new(PassabilityCache::from([("furniture".into(), furniture)]));
        assert!(!is_cell_sealed(&cache, 5.0, 5.35, 0, None));
        assert!(is_circle_blocked_on_floor(&cache, 5.0, 5.35, 0.3, 0, None));
        for (x, z, termination) in [
            (5.0, 5.35, PathTermination::Unreachable),
            (5.5, 5.5, PathTermination::Reached),
        ] {
            let path = pool
                .search(
                    Arc::clone(&cache),
                    PathWaypoint {
                        x: 1.5,
                        z: 5.5,
                        floor: 0,
                    },
                    PathWaypoint { x, z, floor: 0 },
                    100,
                )
                .await
                .unwrap();
            assert_eq!(path.termination, termination);
            let destination = path.waypoints.last().unwrap();
            assert_eq!((destination.x, destination.z), (5.5, 5.5));
            assert!(!is_circle_blocked_on_floor(
                &cache,
                destination.x,
                destination.z,
                0.3,
                0,
                None
            ));
        }
    }

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
