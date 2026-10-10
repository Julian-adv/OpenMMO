use super::{PathDiagnostics, PathSample, BRAIN_PATH_NODE_BUDGET};
use onlinerpg_shared::monster_ai::{CachePathProvider, PathProvider};
use onlinerpg_shared::pathfinding::{
    find_and_smooth_path_avoiding_with_budget, PassabilityCache, PathResult, PathTermination,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::Weak;
use std::time::{Duration, Instant};

const FAILURE_TTL: Duration = Duration::from_secs(10);
const MAX_FAILURES: usize = 32;

#[derive(PartialEq)]
struct Query {
    start: (f32, f32, u8),
    goal: (f32, f32, u8),
    max_nodes: usize,
    blocked: Vec<(i32, i32)>,
}

struct FailedPath {
    query: Query,
    result: PathResult,
    expires: Instant,
}

#[derive(Default)]
pub(super) struct FailedPaths {
    geometry: Weak<PassabilityCache>,
    entries: VecDeque<FailedPath>,
}

impl FailedPaths {
    pub(super) fn synchronize(&mut self, geometry: &Weak<PassabilityCache>) -> bool {
        if Weak::ptr_eq(&self.geometry, geometry) {
            return false;
        }
        let invalidated = !self.entries.is_empty();
        self.entries.clear();
        self.geometry = geometry.clone();
        invalidated
    }

    fn get(&self, query: &Query, now: Instant) -> Option<PathResult> {
        self.entries
            .iter()
            .find(|entry| entry.expires > now && entry.query == *query)
            .map(|entry| entry.result.clone())
    }

    fn insert(&mut self, query: Query, result: PathResult, now: Instant) {
        while self
            .entries
            .front()
            .is_some_and(|entry| entry.expires <= now)
        {
            self.entries.pop_front();
        }
        if self.entries.len() == MAX_FAILURES {
            self.entries.pop_front();
        }
        self.entries.push_back(FailedPath {
            query,
            result,
            expires: now + FAILURE_TTL,
        });
    }
}

pub(super) struct CountingPath<'a> {
    inner: CachePathProvider<'a>,
    tick_remaining: &'a Cell<usize>,
    failures: &'a RefCell<FailedPaths>,
    now: Instant,
    pub(super) remaining: Cell<usize>,
    pub(super) diagnostics: Cell<PathDiagnostics>,
}

impl<'a> CountingPath<'a> {
    pub(super) fn new(
        cache: &'a PassabilityCache,
        tick_remaining: &'a Cell<usize>,
        failures: &'a RefCell<FailedPaths>,
        now: Instant,
    ) -> Self {
        Self {
            inner: CachePathProvider { cache },
            tick_remaining,
            failures,
            now,
            remaining: Cell::new(BRAIN_PATH_NODE_BUDGET),
            diagnostics: Cell::default(),
        }
    }

    fn defer(&self) -> PathResult {
        self.diagnostics.update(|mut d| {
            d.deferred = true;
            d
        });
        PathResult {
            waypoints: vec![],
            found: false,
            termination: PathTermination::NodeLimit,
        }
    }
}

impl PathProvider for CountingPath<'_> {
    fn find_path(&self, sx: f32, sz: f32, sf: u8, gx: f32, gz: f32, gf: u8) -> PathResult {
        self.find_path_avoiding(
            sx,
            sz,
            sf,
            gx,
            gz,
            gf,
            &[],
            onlinerpg_shared::dungeon::path_max_nodes(sf, gf),
        )
    }

    fn attack_line_blocked(&self, fx: f32, fz: f32, tx: f32, tz: f32, floor: u8) -> bool {
        self.inner.attack_line_blocked(fx, fz, tx, tz, floor)
    }

    fn cell_passable(&self, x: f32, z: f32, floor: u8) -> bool {
        self.inner.cell_passable(x, z, floor)
    }

    fn find_path_avoiding(
        &self,
        sx: f32,
        sz: f32,
        sf: u8,
        gx: f32,
        gz: f32,
        gf: u8,
        blocked: &[(i32, i32)],
        max_nodes: usize,
    ) -> PathResult {
        if self.diagnostics.get().deferred {
            return self.defer();
        }
        let mut blocked_key = blocked.to_vec();
        blocked_key.sort_unstable();
        blocked_key.dedup();
        let query = Query {
            start: (sx, sz, sf),
            goal: (gx, gz, gf),
            max_nodes,
            blocked: blocked_key,
        };
        if let Some(result) = self.failures.borrow().get(&query, self.now) {
            self.diagnostics.update(|mut d| {
                d.cache_hits += 1;
                d
            });
            return result;
        }
        let remaining = self.remaining.get();
        if self.tick_remaining.get() < max_nodes.min(remaining) {
            return self.defer();
        }
        let started = Instant::now();
        let result = find_and_smooth_path_avoiding_with_budget(
            sx,
            sz,
            sf,
            gx,
            gz,
            gf,
            self.inner.cache,
            max_nodes,
            blocked,
            &self.remaining,
        );
        let expanded_nodes = remaining - self.remaining.get();
        self.tick_remaining
            .set(self.tick_remaining.get() - expanded_nodes);
        self.diagnostics.update(|mut d| {
            d.record(PathSample {
                start: (sx, sz, sf),
                goal: (gx, gz, gf),
                elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
                termination: result.termination,
                waypoints: result.waypoints.len(),
                expanded_nodes,
            });
            d
        });
        if !result.found && remaining >= max_nodes && max_nodes > 0 {
            self.failures
                .borrow_mut()
                .insert(query, result.clone(), self.now);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::passability_snapshot::PassabilityStore;
    use onlinerpg_shared::pathfinding::{build_furniture_passability, FurniturePiece};

    fn obstacle() -> PassabilityCache {
        PassabilityCache::from([(
            "obstacle".into(),
            build_furniture_passability(&[FurniturePiece {
                cells: vec![(0, 0)],
                floor_level: 0,
                y_base: 0.0,
                wall_height: 3.0,
            }])
            .unwrap(),
        )])
    }

    fn query(path: &CountingPath<'_>, limit: usize) -> PathResult {
        path.find_path_avoiding(10.5, 0.5, 0, 0.5, 0.5, 0, &[], limit)
    }

    #[test]
    fn all_path_queries_share_the_brains_node_budget() {
        let cache = obstacle();
        let failures = RefCell::default();
        let budget = Cell::new(BRAIN_PATH_NODE_BUDGET);
        let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
        path.remaining.set(20);
        let first = query(&path, 6);
        assert_eq!(first.termination, PathTermination::NodeLimit);
        assert_eq!(path.remaining.get(), 14);
        let second = query(&path, 7);
        assert_eq!(second.termination, PathTermination::NodeLimit);
        assert_eq!(path.remaining.get(), 7);
        let third = path.find_path(10.5, 0.5, 0, 0.5, 0.5, 0);
        assert_eq!(third.termination, PathTermination::NodeLimit);
        assert_eq!(path.remaining.get(), 0);
        let fourth = path.find_path(10.5, 0.5, 0, 0.5, 0.5, 0);
        assert_eq!(fourth.termination, PathTermination::NodeLimit);
        assert!(fourth.waypoints.is_empty());
        assert_eq!(path.remaining.get(), 0);
        assert_eq!(path.diagnostics.get().expanded_nodes, 20);

        path.remaining.set(20);
        path.diagnostics.set(PathDiagnostics::default());
        let reached = path.find_path(-1.5, 0.5, 0, 2.5, 0.5, 0);
        assert!(reached.found);
        assert!(path.remaining.get() > 0 && path.remaining.get() < 20);
        assert_eq!(
            path.diagnostics.get().expanded_nodes,
            20 - path.remaining.get()
        );
    }

    #[test]
    fn unchanged_failures_reuse_partial_paths_until_the_original_expiry() {
        let cache = obstacle();
        let failures = RefCell::default();
        let budget = Cell::new(100);
        let now = Instant::now();
        let path = CountingPath::new(&cache, &budget, &failures, now);
        let first = query(&path, 20);
        assert_eq!(first.termination, PathTermination::NodeLimit);
        assert!(!first.waypoints.is_empty());
        assert_eq!(budget.get(), 80);
        for elapsed in [4, 8] {
            let repeated = CountingPath::new(
                &cache,
                &budget,
                &failures,
                now + Duration::from_secs(elapsed),
            );
            let result = query(&repeated, 20);
            assert_eq!(result, first);
            assert_eq!(repeated.diagnostics.get().cache_hits, 1);
            assert_eq!(repeated.diagnostics.get().count(), 0);
            assert_eq!(budget.get(), 80);
        }
        let expired = CountingPath::new(&cache, &budget, &failures, now + FAILURE_TTL);
        query(&expired, 20);
        assert_eq!(expired.diagnostics.get().count(), 1);
        assert_eq!(budget.get(), 60);
    }

    #[test]
    fn changed_query_inputs_are_not_reused() {
        let cache = obstacle();
        let failures = RefCell::default();
        let budget = Cell::new(1000);
        let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
        query(&path, 20);
        for (sx, sz, sf, gx, gz, gf, blocked, limit) in [
            (11.5, 0.5, 0, 0.5, 0.5, 0, vec![], 20),
            (10.5, 1.5, 0, 0.5, 0.5, 0, vec![], 20),
            (10.5, 0.5, 1, 0.5, 0.5, 0, vec![], 20),
            (10.5, 0.5, 0, 0.6, 0.5, 0, vec![], 20),
            (10.5, 0.5, 0, 0.5, 0.6, 0, vec![], 20),
            (10.5, 0.5, 0, 0.5, 0.5, 1, vec![], 20),
            (10.5, 0.5, 0, 0.5, 0.5, 0, vec![(5, 0)], 20),
            (10.5, 0.5, 0, 0.5, 0.5, 0, vec![], 21),
        ] {
            let before = path.diagnostics.get().count();
            path.find_path_avoiding(sx, sz, sf, gx, gz, gf, &blocked, limit);
            assert_eq!(path.diagnostics.get().count(), before + 1);
        }
        assert_eq!(path.diagnostics.get().cache_hits, 0);
        for blocked in [vec![(5, 0), (6, 0)], vec![(6, 0), (5, 0)]] {
            path.find_path_avoiding(10.5, 0.5, 0, 0.5, 0.5, 0, &blocked, 20);
        }
        assert_eq!(path.diagnostics.get().cache_hits, 1);
    }

    #[test]
    fn changed_geometry_invalidates_failures_and_opens_the_route() {
        let store = PassabilityStore::default();
        *store.write() = obstacle();
        let failures = RefCell::new(FailedPaths::default());
        let budget = Cell::new(100);
        {
            let cache = store.read();
            assert!(!failures.borrow_mut().synchronize(&cache.token()));
            let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
            assert!(!query(&path, 20).found);
        }
        store.write().clear();
        let cache = store.read();
        assert!(failures.borrow_mut().synchronize(&cache.token()));
        let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
        assert!(query(&path, 20).found);
        assert_eq!(path.diagnostics.get().cache_hits, 0);
    }

    #[test]
    fn brains_share_a_tick_budget_without_caching_deferred_queries() {
        let cache = obstacle();
        let first_failures = RefCell::default();
        let second_failures = RefCell::default();
        let budget = Cell::new(30);
        let now = Instant::now();
        let first = CountingPath::new(&cache, &budget, &first_failures, now);
        query(&first, 20);
        let second = CountingPath::new(&cache, &budget, &second_failures, now);
        let postponed = query(&second, 20);
        assert!(postponed.waypoints.is_empty());
        assert_eq!(budget.get(), 10);
        assert!(second.diagnostics.get().deferred);
        assert_eq!(second.diagnostics.get().count(), 0);
        assert!(second_failures.borrow().entries.is_empty());
        budget.set(30);
        let next_tick = CountingPath::new(&cache, &budget, &second_failures, now);
        assert!(!query(&next_tick, 20).waypoints.is_empty());
        assert_eq!(budget.get(), 10);
        assert!(!next_tick.diagnostics.get().deferred);
        assert_eq!(next_tick.diagnostics.get().count(), 1);
    }

    #[test]
    fn a_brain_budget_truncated_failure_does_not_poison_a_full_search() {
        let cache = obstacle();
        let failures = RefCell::default();
        let budget = Cell::new(100);
        let now = Instant::now();
        let path = CountingPath::new(&cache, &budget, &failures, now);
        path.remaining.set(5);
        query(&path, 20);
        assert_eq!(budget.get(), 95);
        assert!(failures.borrow().entries.is_empty());
        let next_tick = CountingPath::new(&cache, &budget, &failures, now);
        query(&next_tick, 20);
        assert_eq!(budget.get(), 75);
        assert_eq!(next_tick.diagnostics.get().cache_hits, 0);
    }

    #[test]
    fn moving_targets_cannot_grow_the_failure_cache_without_bound() {
        let cache = obstacle();
        let failures = RefCell::default();
        let budget = Cell::new(1000);
        let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
        for i in 0..MAX_FAILURES + 10 {
            path.find_path_avoiding(10.5 + i as f32, 0.5, 0, 0.5, 0.5, 0, &[], 1);
        }
        assert_eq!(failures.borrow().entries.len(), MAX_FAILURES);
    }
}
