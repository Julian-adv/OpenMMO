use super::{PathDiagnostics, PathSample, BRAIN_PATH_NODE_BUDGET};
use onlinerpg_shared::monster_ai::{CachePathProvider, PathProvider};
use onlinerpg_shared::pathfinding::{
    find_and_smooth_path_avoiding_with_budget, PassabilityCache, PathResult, PathTermination,
    SearchExtent,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Weak;
use std::time::{Duration, Instant};

/// A failure is invalidated by geometry edits it read, so this only backstops
/// an input the fingerprint misses.
const FAILURE_TTL: Duration = Duration::from_secs(60);
const MAX_FAILURES: usize = 32;
/// Margin past the searched cells that edge and mover-radius checks read.
const REGION_PAD: i32 = 2;

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
    extent: SearchExtent,
    fingerprint: u64,
    expires: Instant,
}

/// Hash of the geometry a search over `extent` could have read, so an edit
/// elsewhere (a dungeon door, another estate) leaves the failure valid. Only
/// the overlapping cells are hashed: region and shell grids are large.
fn fingerprint(cache: &PassabilityCache, extent: &SearchExtent) -> u64 {
    let (x0, z0) = (extent.min.0 - REGION_PAD, extent.min.1 - REGION_PAD);
    let (x1, z1) = (extent.max.0 + REGION_PAD, extent.max.1 + REGION_PAD);
    cache
        .iter()
        .filter(|(_, rp)| {
            rp.max_x >= x0 as f32
                && rp.min_x <= (x1 + 1) as f32
                && rp.max_z >= z0 as f32
                && rp.min_z <= (z1 + 1) as f32
                && (rp.floors.iter().any(|f| extent.has_floor(f.floor_level))
                    || rp.stairwells.iter().any(|s| {
                        extent.has_floor(s.lower_floor) || extent.has_floor(s.upper_floor)
                    }))
        })
        .map(|(key, rp)| {
            let mut h = DefaultHasher::new();
            key.hash(&mut h);
            (
                rp.yields_to_trapped_mover,
                rp.allows_projectiles,
                rp.is_ground,
            )
                .hash(&mut h);
            for s in &rp.stairwells {
                (s.local_min_x, s.local_min_z, s.local_max_x, s.local_max_z).hash(&mut h);
                (s.lower_floor, s.upper_floor, s.along_z, s.reversed).hash(&mut h);
            }
            let (ox, oz) = (
                rp.house_origin_x.floor() as i32,
                rp.house_origin_z.floor() as i32,
            );
            for f in rp.floors.iter().filter(|f| extent.has_floor(f.floor_level)) {
                let (fx, fz) = (ox + f.origin_x, oz + f.origin_z);
                let (w, d) = (i32::from(f.width), i32::from(f.depth));
                let (ax, az) = (x0.max(fx), z0.max(fz));
                let (bx, bz) = (x1.min(fx + w - 1), z1.min(fz + d - 1));
                if ax > bx || az > bz {
                    continue;
                }
                (f.floor_level, ax, az, bx, bz).hash(&mut h);
                (f.y_base.to_bits(), f.wall_height.to_bits()).hash(&mut h);
                for z in az..=bz {
                    let row = ((z - fz) * w) as usize;
                    f.cells[row + (ax - fx) as usize..=row + (bx - fx) as usize].hash(&mut h);
                }
            }
            h.finish()
        })
        .fold(0, u64::wrapping_add)
}

#[cfg(test)]
pub(super) fn furniture_piece(
    x: i32,
    z: i32,
    floor_level: u8,
) -> onlinerpg_shared::pathfinding::RuntimePassability {
    onlinerpg_shared::pathfinding::build_furniture_passability(&[
        onlinerpg_shared::pathfinding::FurniturePiece {
            cells: vec![(x, z)],
            floor_level,
            y_base: 0.0,
            wall_height: 3.0,
        },
    ])
    .unwrap()
}

#[derive(Default)]
pub(super) struct FailedPaths {
    geometry: Weak<PassabilityCache>,
    entries: VecDeque<FailedPath>,
}

impl FailedPaths {
    /// Drops expired failures and those an edit could have changed; true when
    /// an edit dropped any.
    pub(super) fn synchronize(
        &mut self,
        geometry: &Weak<PassabilityCache>,
        cache: &PassabilityCache,
        now: Instant,
    ) -> bool {
        if Weak::ptr_eq(&self.geometry, geometry) {
            return false;
        }
        self.geometry = geometry.clone();
        let mut changed = false;
        self.entries.retain(|entry| {
            let live = entry.expires > now;
            let keep = live && fingerprint(cache, &entry.extent) == entry.fingerprint;
            changed |= live && !keep;
            keep
        });
        changed
    }

    fn get(&self, query: &Query, now: Instant) -> Option<PathResult> {
        self.entries
            .iter()
            .find(|entry| entry.expires > now && entry.query == *query)
            .map(|entry| entry.result.clone())
    }

    fn insert(
        &mut self,
        query: Query,
        result: PathResult,
        extent: SearchExtent,
        cache: &PassabilityCache,
        now: Instant,
    ) {
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
            extent,
            fingerprint: fingerprint(cache, &extent),
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

    fn ranged_line_blocked(&self, fx: f32, fz: f32, tx: f32, tz: f32, floor: u8) -> bool {
        self.inner.ranged_line_blocked(fx, fz, tx, tz, floor)
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
        let tick = self.tick_remaining.get();
        let started = Instant::now();
        let mut extent = SearchExtent::default();
        let result = find_and_smooth_path_avoiding_with_budget(
            sx,
            sz,
            sf,
            gx,
            gz,
            gf,
            self.inner.cache,
            max_nodes.min(tick),
            blocked,
            &self.remaining,
            &mut extent,
        );
        let expanded_nodes = remaining - self.remaining.get();
        self.tick_remaining.set(tick.saturating_sub(expanded_nodes));
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
        // Cut short by the tick rather than its own limit: rerun it next tick.
        if tick < max_nodes.min(remaining) && result.termination == PathTermination::NodeLimit {
            return self.defer();
        }
        if !result.found && remaining >= max_nodes && max_nodes > 0 {
            self.failures.borrow_mut().insert(
                query,
                result.clone(),
                extent,
                self.inner.cache,
                self.now,
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::passability_snapshot::PassabilityStore;
    use onlinerpg_shared::dungeon::DUNGEON_FLOOR_INDEX_BASE;

    fn obstacle() -> PassabilityCache {
        PassabilityCache::from([("obstacle".into(), furniture_piece(0, 0, 0))])
    }

    /// The obstacle behind a long fence of furniture, so a query for it spends
    /// every node it is allowed before the two sides of the search can meet.
    fn walled_off() -> PassabilityCache {
        let mut cache = obstacle();
        let wall = onlinerpg_shared::pathfinding::FurniturePiece {
            cells: (-40..=40).map(|z| (5, z)).collect(),
            floor_level: 0,
            y_base: 0.0,
            wall_height: 3.0,
        };
        cache.extend(
            onlinerpg_shared::pathfinding::build_furniture_passability(&[wall])
                .map(|wall| ("wall".into(), wall)),
        );
        cache
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
        let cache = walled_off();
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
            assert!(!failures
                .borrow_mut()
                .synchronize(&cache.token(), &cache, Instant::now()));
            let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
            assert!(!query(&path, 20).found);
        }
        store.write().clear();
        let cache = store.read();
        assert!(failures
            .borrow_mut()
            .synchronize(&cache.token(), &cache, Instant::now()));
        let path = CountingPath::new(&cache, &budget, &failures, Instant::now());
        assert!(query(&path, 20).found);
        assert_eq!(path.diagnostics.get().cache_hits, 0);
    }

    #[test]
    fn edits_outside_a_failures_floor_and_area_keep_it() {
        let store = PassabilityStore::default();
        *store.write() = obstacle();
        let failures = RefCell::new(FailedPaths::default());
        let budget = Cell::new(1000);
        let now = Instant::now();
        {
            let cache = store.read();
            failures
                .borrow_mut()
                .synchronize(&cache.token(), &cache, Instant::now());
            let path = CountingPath::new(&cache, &budget, &failures, now);
            assert!(!query(&path, 20).found);
        }
        for (key, entry) in [
            ("far", furniture_piece(200, 0, 0)),
            (
                "dungeon below",
                furniture_piece(5, 0, DUNGEON_FLOOR_INDEX_BASE),
            ),
        ] {
            store.write().insert(key.into(), entry);
            let cache = store.read();
            assert!(!failures
                .borrow_mut()
                .synchronize(&cache.token(), &cache, Instant::now()));
            let path = CountingPath::new(&cache, &budget, &failures, now);
            query(&path, 20);
            assert_eq!(path.diagnostics.get().cache_hits, 1);
        }
        store
            .write()
            .insert("near".into(), furniture_piece(5, 0, 0));
        let cache = store.read();
        assert!(failures
            .borrow_mut()
            .synchronize(&cache.token(), &cache, Instant::now()));
        let path = CountingPath::new(&cache, &budget, &failures, now);
        query(&path, 20);
        assert_eq!(path.diagnostics.get().cache_hits, 0);
    }

    #[test]
    fn brains_share_a_tick_budget_without_caching_deferred_queries() {
        let cache = walled_off();
        let first_failures = RefCell::default();
        let second_failures = RefCell::default();
        let budget = Cell::new(30);
        let now = Instant::now();
        let first = CountingPath::new(&cache, &budget, &first_failures, now);
        query(&first, 20);
        let second = CountingPath::new(&cache, &budget, &second_failures, now);
        let postponed = query(&second, 20);
        assert!(postponed.waypoints.is_empty());
        assert_eq!(budget.get(), 0);
        assert!(second.diagnostics.get().deferred);
        assert!(second_failures.borrow().entries.is_empty());
        budget.set(30);
        let next_tick = CountingPath::new(&cache, &budget, &second_failures, now);
        assert!(!query(&next_tick, 20).waypoints.is_empty());
        assert_eq!(budget.get(), 10);
        assert!(!next_tick.diagnostics.get().deferred);
        assert_eq!(next_tick.diagnostics.get().count(), 1);
    }

    #[test]
    fn a_short_search_runs_on_what_the_tick_has_left() {
        let cache = walled_off();
        let failures = RefCell::default();
        let budget = Cell::new(35);
        let now = Instant::now();
        let first = CountingPath::new(&cache, &budget, &failures, now);
        query(&first, 20);
        let path = CountingPath::new(&cache, &budget, &failures, now);
        assert!(path.find_path(-1.5, 0.5, 0, 2.5, 0.5, 0).found);
        assert!(!path.diagnostics.get().deferred);
        assert!(budget.get() < 15);
    }

    #[test]
    fn a_brain_budget_truncated_failure_does_not_poison_a_full_search() {
        let cache = walled_off();
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
