use super::*;
use onlinerpg_shared::estate_storage::EstateChest;

mod estate_storage;

/// Versioned world bodies and collision shared by independent NPC views.
pub struct WorldCache {
    pub collision_revision: u64,
    passability_cache: PassabilityCache,
    houses: HashMap<String, HouseData>,
    dungeons: Vec<Arc<Dungeon>>,
    /// Runtime collision overlays per dungeon and depth.
    dungeon_doors: HashMap<(String, u8), HashSet<u32>>,
    dungeon_broken_props: HashMap<(String, u8), Vec<u32>>,
    /// Opened chests stay solid but are no longer offered for looting.
    dungeon_opened_props: HashMap<(String, u8), HashSet<u32>>,
    house_views: HashMap<PlayerId, HashSet<String>>,
    house_revisions: HashMap<String, u64>,
    house_deleted: HashSet<String>,
    world_epoch: String,
    retired_epochs: HashSet<String>,
    dungeon_views: HashMap<PlayerId, HashSet<String>>,
    dungeon_revisions: HashMap<String, u64>,
    dungeon_cached: HashSet<String>,
    fetched_furniture_regions: HashSet<(i32, i32)>,
    /// Static placements used to resolve furniture interactions.
    furniture_placements: HashMap<(i32, i32), Vec<FurniturePlacement>>,
    fence_views: HashMap<
        PlayerId,
        HashMap<onlinerpg_shared::fence::FenceEdge, onlinerpg_shared::fence::Fence>,
    >,
    fence_revisions: HashMap<String, u64>,
    fence_deleted: HashSet<String>,
    fence_bodies: HashMap<String, onlinerpg_shared::fence::Fence>,
    estate_chest_views: HashMap<PlayerId, HashSet<i64>>,
    estate_chests: HashMap<i64, EstateChest>,
    estate_chest_revisions: HashMap<i64, u64>,
    estate_chest_deleted: HashSet<i64>,
}

impl WorldCache {
    pub fn world_epoch(&self) -> &str {
        &self.world_epoch
    }

    pub fn is_current_epoch(&self, epoch: &str) -> bool {
        self.world_epoch == epoch
    }
    pub fn ensure_world_epoch(&mut self, epoch: &str) -> bool {
        if self.retired_epochs.contains(epoch) {
            return false;
        }
        if self.world_epoch == epoch {
            return true;
        }
        if !self.world_epoch.is_empty() {
            self.retired_epochs.insert(self.world_epoch.clone());
        }
        let ids: Vec<_> = self.houses.keys().cloned().collect();
        for id in ids {
            self.remove_house(&id);
        }
        self.house_views.clear();
        self.house_revisions.clear();
        self.house_deleted.clear();
        self.dungeon_views.clear();
        self.dungeon_revisions.clear();
        self.dungeon_cached.clear();
        self.dungeon_doors.clear();
        self.dungeon_broken_props.clear();
        self.dungeon_opened_props.clear();
        for dungeon in &self.dungeons {
            self.passability_cache
                .insert(dungeon_cache_key(&dungeon.id), dungeon.passability());
        }
        for region in self.furniture_placements.keys() {
            self.passability_cache
                .remove(&furniture::region_cache_key(region.0, region.1));
        }
        self.furniture_placements.clear();
        self.fetched_furniture_regions.clear();
        self.fence_revisions.clear();
        self.fence_deleted.clear();
        self.fence_bodies.clear();
        let viewers: Vec<_> = self.fence_views.keys().copied().collect();
        for viewer in viewers {
            self.remove_fence_view(viewer);
        }
        self.clear_estate_chests();
        self.world_epoch = epoch.to_owned();
        true
    }

    pub fn houses_for(&self, viewer: PlayerId) -> impl Iterator<Item = &HouseData> {
        self.house_views
            .get(&viewer)
            .into_iter()
            .flatten()
            .filter_map(|id| self.houses.get(id))
    }

    pub fn view_complete(
        &self,
        viewer: PlayerId,
        view: &onlinerpg_shared::interest::WorldView,
    ) -> bool {
        view.subjects.keys().all(|subject| {
            if let Some(id) = subject.strip_prefix("house:") {
                self.houses.contains_key(id)
                    && self
                        .house_views
                        .get(&viewer)
                        .is_some_and(|ids| ids.contains(id))
            } else if let Some(id) = subject.strip_prefix("chest:") {
                id.parse().is_ok_and(|id| {
                    self.estate_chests.contains_key(&id)
                        && self
                            .estate_chest_views
                            .get(&viewer)
                            .is_some_and(|ids| ids.contains(&id))
                })
            } else if subject.starts_with("door:") || subject.starts_with("prop:") {
                self.dungeon_cached.contains(subject)
                    && self
                        .dungeon_views
                        .get(&viewer)
                        .is_some_and(|ids| ids.contains(subject))
            } else {
                true
            }
        })
    }

    pub fn apply_dungeon_event(
        &mut self,
        viewer: PlayerId,
        event: &onlinerpg_shared::interest::WorldEvent,
    ) -> bool {
        use onlinerpg_shared::interest::InterestChange;
        let id = &event.subject;
        let view = self.dungeon_views.entry(viewer).or_default();
        if matches!(event.change, InterestChange::Leave | InterestChange::Delete) {
            view.remove(id);
            if self.dungeon_views.values().any(|view| view.contains(id)) {
                return false;
            }
            self.dungeon_cached.remove(id);
        } else {
            view.insert(id.clone());
            if self
                .dungeon_revisions
                .get(id)
                .is_some_and(|revision| *revision > event.revision)
            {
                return false;
            }
            self.dungeon_revisions.insert(id.clone(), event.revision);
            self.dungeon_cached.insert(id.clone());
        }
        true
    }

    pub fn remove_dungeon_view(&mut self, viewer: PlayerId) {
        let ids = self.dungeon_views.remove(&viewer).unwrap_or_default();
        for id in ids {
            if self.dungeon_views.values().any(|view| view.contains(&id)) {
                continue;
            }
            self.dungeon_cached.remove(&id);
            let parts: Vec<_> = id.rsplitn(4, ':').collect();
            if parts.len() != 4 {
                continue;
            }
            let (Ok(index), Ok(depth)) = (parts[0].parse(), parts[1].parse()) else {
                continue;
            };
            match parts[3] {
                "door" => self.set_dungeon_door(parts[2], depth, index, false),
                "prop" => self.set_dungeon_prop(parts[2], depth, index, false, false),
                _ => {}
            }
        }
    }
    pub fn set_dungeon_prop(
        &mut self,
        id: &str,
        depth: u8,
        prop_id: u32,
        broken: bool,
        opened: bool,
    ) {
        let key = (id.to_owned(), depth);
        let props = self.dungeon_broken_props.entry(key).or_default();
        let was_broken = props.contains(&prop_id);
        if broken && !was_broken {
            props.push(prop_id);
        }
        if !broken {
            props.retain(|id| *id != prop_id);
        }
        if opened {
            self.add_dungeon_opened_prop(id, depth, prop_id);
        } else {
            self.remove_dungeon_opened_prop(id, depth, prop_id);
        }
        if was_broken != broken {
            self.rebuild_dungeon_floor(id, depth);
        }
    }
    pub fn new() -> Self {
        Self {
            collision_revision: 0,
            passability_cache: PassabilityCache::new(),
            houses: HashMap::new(),
            dungeons: Vec::new(),
            dungeon_doors: HashMap::new(),
            dungeon_broken_props: HashMap::new(),
            dungeon_opened_props: HashMap::new(),
            house_views: HashMap::new(),
            house_revisions: HashMap::new(),
            house_deleted: HashSet::new(),
            world_epoch: String::new(),
            retired_epochs: HashSet::new(),
            dungeon_views: HashMap::new(),
            dungeon_revisions: HashMap::new(),
            dungeon_cached: HashSet::new(),
            fetched_furniture_regions: HashSet::new(),
            furniture_placements: HashMap::new(),
            fence_views: HashMap::new(),
            fence_revisions: HashMap::new(),
            fence_deleted: HashSet::new(),
            fence_bodies: HashMap::new(),
            estate_chest_views: HashMap::new(),
            estate_chests: HashMap::new(),
            estate_chest_revisions: HashMap::new(),
            estate_chest_deleted: HashSet::new(),
        }
    }

    /// Generate every registry dungeon and register its passability — stair
    /// shafts included, so the shared A* walks from the surface down to the
    /// deepest floor with no extra machinery. Run once at startup, mirroring
    /// the server's own `init_passability`; the entries also give surface
    /// paths the entrance walls the server already collides against.
    pub fn register_dungeons(&mut self) {
        for dungeon in crate::dungeon::build_all() {
            self.passability_cache
                .insert(dungeon_cache_key(&dungeon.id), dungeon.passability());
            self.dungeons.push(Arc::new(dungeon));
        }
    }

    /// Dungeon whose footprint covers (x, z), by the shared registry's
    /// footprint test — the same one the server admits us underground by.
    pub fn dungeon_at(&self, x: f32, z: f32) -> Option<Arc<Dungeon>> {
        let def = onlinerpg_shared::dungeon::entrance_at(x, z)?;
        self.dungeon_by_id(&def.id)
    }

    /// Dungeon with the closest entrance.
    pub fn nearest_dungeon(&self, x: f32, z: f32) -> Option<Arc<Dungeon>> {
        self.dungeons
            .iter()
            .min_by(|a, b| {
                let da = crate::geom::PlanarDelta::xz(x, z, a.entrance.x, a.entrance.z).dist;
                let db = crate::geom::PlanarDelta::xz(x, z, b.entrance.x, b.entrance.z).dist;
                da.total_cmp(&db)
            })
            .map(Arc::clone)
    }

    pub fn dungeon_by_id(&self, id: &str) -> Option<Arc<Dungeon>> {
        self.dungeons.iter().find(|d| d.id == id).map(Arc::clone)
    }

    /// Every registered dungeon: the watch panel draws their entrances, and
    /// name resolution and the world-state listing read it.
    pub fn all_dungeons(&self) -> &[Arc<Dungeon>] {
        &self.dungeons
    }

    pub fn open_dungeon_doors(&self, id: &str, depth: u8) -> HashSet<u32> {
        self.dungeon_doors
            .get(&(id.to_string(), depth))
            .cloned()
            .unwrap_or_default()
    }

    /// Replace the open-door set for a dungeon (the `DungeonDoorsState`
    /// snapshot covers every depth at once, so unlisted floors are all shut).
    pub fn set_dungeon_doors(&mut self, id: &str, doors: &[(u8, u32)]) {
        let touched: HashSet<u8> = self
            .dungeon_doors
            .keys()
            .filter(|(k, _)| k == id)
            .map(|(_, depth)| *depth)
            .chain(doors.iter().map(|(depth, _)| *depth))
            .collect();
        for depth in &touched {
            self.dungeon_doors.remove(&(id.to_string(), *depth));
        }
        for (depth, door_id) in doors {
            self.dungeon_doors
                .entry((id.to_string(), *depth))
                .or_default()
                .insert(*door_id);
        }
        for depth in touched {
            self.rebuild_dungeon_floor(id, depth);
        }
    }

    pub fn set_dungeon_door(&mut self, id: &str, depth: u8, door_id: u32, is_open: bool) {
        let set = self
            .dungeon_doors
            .entry((id.to_string(), depth))
            .or_default();
        // Re-broadcasts are common; rebuilding a floor's 6400 cells under the
        // shared write lock for a state we already hold is not worth it.
        let changed = if is_open {
            set.insert(door_id)
        } else {
            set.remove(&door_id)
        };
        if changed {
            self.rebuild_dungeon_floor(id, depth);
        }
    }

    pub fn set_dungeon_broken_props(&mut self, id: &str, depth: u8, broken: Vec<u32>) {
        let key = (id.to_string(), depth);
        if self.dungeon_broken_props.get(&key) == Some(&broken) {
            return;
        }
        self.dungeon_broken_props.insert(key, broken);
        self.rebuild_dungeon_floor(id, depth);
    }

    pub fn add_dungeon_broken_prop(&mut self, id: &str, depth: u8, prop_id: u32) {
        let broken = self
            .dungeon_broken_props
            .entry((id.to_string(), depth))
            .or_default();
        if broken.contains(&prop_id) {
            return;
        }
        broken.push(prop_id);
        self.rebuild_dungeon_floor(id, depth);
    }

    pub fn set_dungeon_opened_props(&mut self, id: &str, depth: u8, opened: Vec<u32>) {
        self.dungeon_opened_props
            .insert((id.to_string(), depth), opened.into_iter().collect());
    }

    pub fn add_dungeon_opened_prop(&mut self, id: &str, depth: u8, prop_id: u32) {
        self.dungeon_opened_props
            .entry((id.to_string(), depth))
            .or_default()
            .insert(prop_id);
    }

    pub fn remove_dungeon_opened_prop(&mut self, id: &str, depth: u8, prop_id: u32) {
        if let Some(opened) = self.dungeon_opened_props.get_mut(&(id.to_string(), depth)) {
            opened.remove(&prop_id);
        }
    }

    pub fn opened_dungeon_props(&self, id: &str, depth: u8) -> Option<&HashSet<u32>> {
        self.dungeon_opened_props.get(&(id.to_string(), depth))
    }

    /// Broken prop ids for one dungeon floor — `break_prop` checks this before
    /// walking out to a barrel someone already smashed.
    pub fn dungeon_broken_props(&self, id: &str, depth: u8) -> &[u32] {
        self.dungeon_broken_props
            .get(&(id.to_string(), depth))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether a mover can stand in the cell holding `(x, z)` on `floor`. What
    /// the in-room sighting queries use to decide where a prop can be opened
    /// from.
    pub fn is_walkable(&self, x: f32, z: f32, floor: u8) -> bool {
        !pathfinding::is_cell_sealed(self.passability_cache(), x, z, floor, None)
    }

    /// Recompute one dungeon floor's cells from the live door/prop state
    /// (shared `dungeon::floor_cells`).
    fn rebuild_dungeon_floor(&mut self, id: &str, depth: u8) {
        self.collision_revision = self.collision_revision.wrapping_add(1);
        let Some(dungeon) = self.dungeon_by_id(id) else {
            return;
        };
        let open = self.open_dungeon_doors(id, depth);
        let broken = self
            .dungeon_broken_props
            .get(&(id.to_string(), depth))
            .cloned()
            .unwrap_or_default();
        let Some(cells) = floor_cells(dungeon.layouts(), depth, &broken, Some(&open)) else {
            return;
        };
        set_floor_cells(&mut self.passability_cache, id, depth, cells);
    }

    pub fn passability_cache(&self) -> &PassabilityCache {
        &self.passability_cache
    }

    pub fn update_fences(
        &mut self,
        viewer: PlayerId,
        added: &[onlinerpg_shared::fence::Fence],
        removed: &[onlinerpg_shared::fence::FenceEdge],
    ) {
        let view = self.fence_views.entry(viewer).or_default();
        let mut changed = false;
        for edge in removed {
            changed |= view.remove(edge).is_some();
        }
        for fence in added {
            if view.get(&fence.edge) != Some(fence) {
                view.insert(fence.edge, fence.clone());
                changed = true;
            }
        }
        if !changed {
            return;
        }
        self.collision_revision = self.collision_revision.wrapping_add(1);
        let fences: Vec<_> = view.values().cloned().collect();
        onlinerpg_shared::fence::sync_passability(
            &mut self.passability_cache,
            &format!("fence-view:{viewer}"),
            &fences,
        );
    }

    pub fn remove_fence_view(&mut self, viewer: PlayerId) {
        self.fence_views.remove(&viewer);
        self.passability_cache
            .remove(&format!("fence-view:{viewer}"));
    }

    pub fn apply_fence_event(
        &mut self,
        viewer: PlayerId,
        event: &onlinerpg_shared::interest::WorldEvent,
    ) {
        use onlinerpg_shared::interest::InterestChange;
        let known = self
            .fence_revisions
            .get(&event.subject)
            .copied()
            .unwrap_or(0);
        for message in &event.messages {
            let ServerMessage::FenceVisibility { added, removed } = message else {
                continue;
            };
            if event.change == InterestChange::Leave {
                self.update_fences(viewer, &[], removed);
                continue;
            }
            if known > event.revision
                || (known == event.revision && self.fence_deleted.contains(&event.subject))
            {
                if matches!(event.change, InterestChange::Enter | InterestChange::Update) {
                    if let Some(fence) = self.fence_bodies.get(&event.subject).cloned() {
                        self.update_fences(viewer, &[fence], &[]);
                    }
                }
                continue;
            }
            self.fence_revisions
                .insert(event.subject.clone(), event.revision);
            if event.change == InterestChange::Delete {
                self.fence_bodies.remove(&event.subject);
                self.fence_deleted.insert(event.subject.clone());
                let viewers: Vec<_> = self.fence_views.keys().copied().collect();
                for id in viewers {
                    self.update_fences(id, &[], removed);
                }
            } else {
                self.fence_deleted.remove(&event.subject);
                for fence in added {
                    self.fence_bodies
                        .insert(event.subject.clone(), fence.clone());
                    let viewers: Vec<_> = self
                        .fence_views
                        .iter()
                        .filter_map(|(id, view)| view.contains_key(&fence.edge).then_some(*id))
                        .collect();
                    for id in viewers {
                        self.update_fences(id, std::slice::from_ref(fence), &[]);
                    }
                }
                self.update_fences(viewer, added, &[]);
            }
        }
    }

    #[cfg(test)]
    pub fn houses(&self) -> &HashMap<String, HouseData> {
        &self.houses
    }

    pub fn is_indoors(&self, x: f32, z: f32, floor: u8) -> bool {
        self.houses
            .values()
            .any(|h| h.room_at(x, z, floor).is_some())
    }

    pub fn add_house(&mut self, house: HouseData) {
        self.collision_revision = self.collision_revision.wrapping_add(1);
        let rp = pathfinding::build_runtime_passability(&house);
        self.passability_cache.insert(house.id.clone(), rp);
        pathfinding::apply_door_overlays(&mut self.passability_cache, &house);
        self.houses.insert(house.id.clone(), house);
    }

    pub fn remove_house(&mut self, house_id: &str) {
        self.collision_revision = self.collision_revision.wrapping_add(1);
        self.houses.remove(house_id);
        self.passability_cache.remove(house_id);
    }

    pub fn remove_house_view(&mut self, viewer: PlayerId) {
        let ids = self.house_views.remove(&viewer).unwrap_or_default();
        for id in ids {
            if !self.house_views.values().any(|view| view.contains(&id)) {
                self.remove_house(&id);
            }
        }
    }

    pub fn apply_house_event(
        &mut self,
        viewer: PlayerId,
        epoch: &str,
        event: &onlinerpg_shared::interest::WorldEvent,
    ) {
        use onlinerpg_shared::interest::InterestChange;
        let Some(id) = event.subject.strip_prefix("house:") else {
            return;
        };
        if !self.ensure_world_epoch(epoch) {
            return;
        }
        match event.change {
            InterestChange::Leave => {
                if let Some(view) = self.house_views.get_mut(&viewer) {
                    view.remove(id);
                }
                if !self.house_views.values().any(|view| view.contains(id)) {
                    self.remove_house(id);
                }
            }
            InterestChange::Delete => {
                if self
                    .house_revisions
                    .get(id)
                    .is_some_and(|rev| *rev > event.revision)
                {
                    return;
                }
                self.house_revisions.insert(id.to_owned(), event.revision);
                self.house_deleted.insert(id.to_owned());
                for view in self.house_views.values_mut() {
                    view.remove(id);
                }
                self.remove_house(id);
            }
            InterestChange::Enter | InterestChange::Update => {
                if self.house_deleted.contains(id)
                    && self
                        .house_revisions
                        .get(id)
                        .is_some_and(|rev| *rev >= event.revision)
                {
                    return;
                }
                self.house_views
                    .entry(viewer)
                    .or_default()
                    .insert(id.to_owned());
                if self
                    .house_revisions
                    .get(id)
                    .is_some_and(|rev| *rev >= event.revision)
                    && self.houses.contains_key(id)
                {
                    return;
                }
                if self
                    .house_revisions
                    .get(id)
                    .is_some_and(|rev| *rev > event.revision)
                {
                    return;
                }
                self.house_revisions.insert(id.to_owned(), event.revision);
                self.house_deleted.remove(id);
                for message in &event.messages {
                    if let ServerMessage::HouseUpdated { house }
                    | ServerMessage::HouseSpawned { house } = message
                    {
                        self.add_house(house.clone());
                    }
                }
            }
        }
    }

    pub fn unfetched_furniture_regions(&self, wanted: &mut HashSet<(i32, i32)>) {
        wanted.retain(|r| !self.fetched_furniture_regions.contains(r));
    }

    pub fn mark_furniture_fetched(&mut self, region: (i32, i32)) {
        self.fetched_furniture_regions.insert(region);
    }

    /// Register (or replace) a region's solid furniture in the passability cache
    /// so the bot paths around it, mirroring the browser's
    /// `passability_set_furniture` (same `furniture:rx,rz` key + shared
    /// `furniture` resolution). Empty/non-solid regions clear the entry.
    pub fn sync_furniture(&mut self, rx: i32, rz: i32, placements: Vec<FurniturePlacement>) {
        let key = furniture::region_cache_key(rx, rz);
        match furniture::build_furniture_passability_for_placements(&placements) {
            Some(rp) => {
                self.passability_cache.insert(key, rp);
            }
            None => {
                self.passability_cache.remove(&key);
            }
        }
        self.furniture_placements.insert((rx, rz), placements);
    }

    /// The `type_id` placement with editor id `object_id` nearest `(x, z)`,
    /// within `max_dist`. Ids are unique only within their region file, so
    /// the type and distance bound are what disambiguate.
    pub fn furniture_placement_near(
        &self,
        type_id: &str,
        object_id: u32,
        x: f32,
        z: f32,
        max_dist: f32,
    ) -> Option<&FurniturePlacement> {
        let max_d2 = max_dist * max_dist;
        let d2 = |p: &FurniturePlacement| (p.x - x).powi(2) + (p.z - z).powi(2);
        self.furniture_placements
            .values()
            .flatten()
            .filter(|p| p.id == object_id && p.type_id == type_id && d2(p) <= max_d2)
            .min_by(|a, b| d2(a).total_cmp(&d2(b)))
    }

    pub fn update_door(
        &mut self,
        house_id: &str,
        room_index: u32,
        wall_dir: WallDirection,
        segment_index: usize,
        is_open: bool,
    ) {
        if let Some(house) = self.houses.get_mut(house_id) {
            if let Some(room) = house.rooms.get_mut(room_index as usize) {
                // The wall is the source of truth (door hunting reads
                // `is_open` off it); the edge is derived from it.
                if let Some(wall) = room.wall_mut(wall_dir).get_mut(segment_index) {
                    wall.is_open = is_open;
                    pathfinding::update_door_edge(
                        &mut self.passability_cache,
                        house_id,
                        room,
                        wall_dir,
                        segment_index,
                        is_open,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod interest_tests {
    use super::*;
    use onlinerpg_shared::housing::PassabilityGrid;
    use onlinerpg_shared::interest::{InterestChange, WorldEvent};

    fn house(x: f32) -> HouseData {
        HouseData {
            id: "a".into(),
            owner_id: "owner".into(),
            source_scroll_id: None,
            origin: Position { x, y: 0.0, z: 0.0 },
            rooms: vec![],
            passability: vec![PassabilityGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 1,
                depth: 1,
                cells: vec![15],
            }],
        }
    }

    fn event(revision: u64, change: InterestChange, x: f32) -> WorldEvent {
        WorldEvent {
            subject: "house:a".into(),
            revision,
            change,
            messages: vec![ServerMessage::HouseUpdated { house: house(x) }],
        }
    }

    #[test]
    fn duplicate_fence_events_preserve_routes_and_shared_collision() {
        use onlinerpg_shared::fence::{Fence, FenceAxis, FenceEdge};
        let mut cache = WorldCache::new();
        let (a, b) = (PlayerId::from(1), PlayerId::from(2));
        let mut fence = Fence {
            edge: FenceEdge {
                x: 2,
                z: 1,
                axis: FenceAxis::Z,
            },
            y: 0.0,
            owner_id: 1,
        };
        let mut event = WorldEvent {
            subject: "fence:2,1:Z".into(),
            revision: 1,
            change: InterestChange::Enter,
            messages: vec![ServerMessage::FenceVisibility {
                added: vec![fence.clone()],
                removed: vec![],
            }],
        };
        for viewer in [a, b] {
            cache.apply_fence_event(viewer, &event);
        }
        let revision = cache.collision_revision;
        for viewer in [a, b] {
            cache.apply_fence_event(viewer, &event);
        }
        assert_eq!(cache.collision_revision, revision);

        fence.y = 1.0;
        event.revision = 2;
        event.change = InterestChange::Update;
        event.messages = vec![ServerMessage::FenceVisibility {
            added: vec![fence.clone()],
            removed: vec![],
        }];
        cache.apply_fence_event(a, &event);
        assert!(cache.collision_revision > revision);
        let revision = cache.collision_revision;
        cache.apply_fence_event(b, &event);
        assert_eq!(cache.collision_revision, revision);
        for viewer in [a, b] {
            assert_eq!(cache.fence_views[&viewer][&fence.edge], fence);
        }

        event.change = InterestChange::Leave;
        event.messages = vec![ServerMessage::FenceVisibility {
            added: vec![],
            removed: vec![fence.edge],
        }];
        cache.apply_fence_event(a, &event);
        let revision = cache.collision_revision;
        cache.apply_fence_event(a, &event);
        assert_eq!(cache.collision_revision, revision);
        assert!(!cache
            .passability_cache
            .contains_key(&format!("fence-view:{a}")));
        assert!(cache
            .passability_cache
            .contains_key(&format!("fence-view:{b}")));

        event.revision = 3;
        event.change = InterestChange::Delete;
        cache.apply_fence_event(b, &event);
        assert!(cache.collision_revision > revision);
        assert!(!cache
            .passability_cache
            .contains_key(&format!("fence-view:{b}")));
        let revision = cache.collision_revision;
        cache.apply_fence_event(b, &event);
        assert_eq!(cache.collision_revision, revision);
    }

    #[test]
    fn independent_views_merge_revisions_and_keep_other_view_collision() {
        let mut cache = WorldCache::new();
        let (a, b) = (PlayerId::from(1), PlayerId::from(2));
        cache.apply_house_event(a, "epoch", &event(1, InterestChange::Enter, 0.0));
        cache.apply_house_event(b, "epoch", &event(2, InterestChange::Enter, 10.0));
        cache.apply_house_event(a, "epoch", &event(1, InterestChange::Update, 0.0));
        assert_eq!(cache.houses_for(a).next().unwrap().origin.x, 10.0);
        cache.apply_house_event(a, "epoch", &event(1, InterestChange::Leave, 0.0));
        assert_eq!(cache.houses_for(a).count(), 0);
        assert_eq!(cache.houses_for(b).count(), 1);
        assert!(cache.passability_cache.contains_key("a"));
        cache.remove_house_view(b);
        assert!(cache.houses.is_empty());
        assert!(!cache.passability_cache.contains_key("a"));
    }

    #[test]
    fn stale_enter_after_last_view_leaves_requires_a_fresh_snapshot() {
        let mut cache = WorldCache::new();
        let viewer = PlayerId::from(1);
        cache.apply_house_event(viewer, "epoch", &event(5, InterestChange::Enter, 10.0));
        cache.remove_house_view(viewer);
        cache.apply_house_event(viewer, "epoch", &event(4, InterestChange::Enter, 0.0));
        let mut view = onlinerpg_shared::interest::WorldView::default();
        view.subjects.insert("house:a".into(), 4);
        assert!(!cache.view_complete(viewer, &view));
        cache.apply_house_event(viewer, "epoch", &event(5, InterestChange::Enter, 10.0));
        assert!(cache.view_complete(viewer, &view));

        let mut door = WorldEvent {
            subject: "door:old_crypt:1:7".into(),
            revision: 8,
            change: InterestChange::Enter,
            messages: vec![],
        };
        assert!(cache.apply_dungeon_event(viewer, &door));
        cache.remove_dungeon_view(viewer);
        door.revision = 7;
        assert!(!cache.apply_dungeon_event(viewer, &door));
        view.subjects.insert(door.subject.clone(), 7);
        assert!(!cache.view_complete(viewer, &view));
        door.revision = 8;
        assert!(cache.apply_dungeon_event(viewer, &door));
        assert!(cache.view_complete(viewer, &view));
    }

    #[test]
    fn tombstones_and_retired_epochs_prevent_late_resurrection() {
        let mut cache = WorldCache::new();
        let viewer = PlayerId::from(1);
        cache.apply_house_event(viewer, "old", &event(3, InterestChange::Enter, 0.0));
        cache.apply_house_event(viewer, "old", &event(4, InterestChange::Delete, 0.0));
        for revision in [3, 4] {
            cache.apply_house_event(viewer, "old", &event(revision, InterestChange::Enter, 0.0));
        }
        assert!(cache.houses.is_empty());
        cache.apply_house_event(viewer, "new", &event(1, InterestChange::Enter, 10.0));
        cache.apply_house_event(viewer, "old", &event(100, InterestChange::Delete, 0.0));
        assert_eq!(cache.houses_for(viewer).next().unwrap().origin.x, 10.0);
    }

    #[test]
    fn dungeon_leave_does_not_clear_another_connections_door() {
        let mut cache = WorldCache::new();
        let (a, b) = (PlayerId::from(1), PlayerId::from(2));
        let mut event = WorldEvent {
            subject: "door:old_crypt:1:7".into(),
            revision: 5,
            change: InterestChange::Enter,
            messages: vec![],
        };
        assert!(cache.apply_dungeon_event(a, &event));
        assert!(cache.apply_dungeon_event(b, &event));
        cache.set_dungeon_door("old_crypt", 1, 7, true);
        event.change = InterestChange::Leave;
        assert!(!cache.apply_dungeon_event(a, &event));
        assert!(cache.open_dungeon_doors("old_crypt", 1).contains(&7));
        cache.remove_dungeon_view(b);
        assert!(!cache.open_dungeon_doors("old_crypt", 1).contains(&7));
    }
}
