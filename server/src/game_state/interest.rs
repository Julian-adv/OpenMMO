use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use onlinerpg_shared::interest::{Bounds, InterestChange, Space, SubjectArea, WorldEvent};
use onlinerpg_shared::{PlayerId, Position, ServerMessage};
use tokio::sync::mpsc::UnboundedSender;

use super::{encode_server_msg, SpatialCell, EVENT_DELIVERY_RADIUS};

fn encode_world_update(
    epoch: &str,
    view: &View,
    reset: bool,
    events: &[u8],
) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    // MessagePack: one enum tag, then the eight WorldUpdate fields.
    let mut bytes = vec![0x81];
    bytes.extend(rmp_serde::to_vec("WorldUpdate")?);
    bytes.push(0x98);
    bytes.extend(rmp_serde::to_vec(epoch)?);
    bytes.extend(rmp_serde::to_vec(&view.generation)?);
    bytes.extend(rmp_serde::to_vec(&view.sequence)?);
    bytes.extend(rmp_serde::to_vec(&view.position)?);
    bytes.extend(rmp_serde::to_vec(&view.floor)?);
    bytes.extend(rmp_serde::to_vec(&reset)?);
    bytes.extend(rmp_serde::to_vec(&view.ready)?);
    bytes.extend_from_slice(events);
    Ok(bytes)
}

struct Subject {
    areas: Vec<SubjectArea>,
    snapshot: Vec<ServerMessage>,
    leave: Vec<ServerMessage>,
    revision: u64,
    subscribers: HashSet<PlayerId>,
    cells: HashSet<SpatialCell>,
    snapshot_at: std::time::Instant,
}

impl Subject {
    fn current_snapshot(&self) -> Vec<ServerMessage> {
        let mut snapshot = self.snapshot.clone();
        for message in &mut snapshot {
            if let ServerMessage::PlayerMusicStarted { elapsed_secs, .. } = message {
                *elapsed_secs += self.snapshot_at.elapsed().as_secs_f32();
            }
        }
        snapshot
    }
}

struct View {
    position: Position,
    space: Space,
    floor: i8,
    generation: u64,
    sequence: u64,
    subjects: HashSet<String>,
    tx: UnboundedSender<Bytes>,
    ready: bool,
}

pub(super) struct Interest {
    epoch: String,
    generation: u64,
    revision: u64,
    subjects: HashMap<String, Subject>,
    cells: HashMap<SpatialCell, HashSet<String>>,
    views: HashMap<PlayerId, View>,
    viewers: super::SpatialIndex<PlayerId>,
    pub(super) blocked_names: HashMap<PlayerId, HashSet<String>>,
    pub(super) stall_owners: HashMap<u64, String>,
    pending_tiles: HashSet<(i32, i32)>,
}

impl Default for Interest {
    fn default() -> Self {
        Self {
            epoch: uuid::Uuid::new_v4().to_string(),
            generation: 0,
            revision: 0,
            subjects: HashMap::new(),
            cells: HashMap::new(),
            views: HashMap::new(),
            viewers: Default::default(),
            blocked_names: HashMap::new(),
            stall_owners: HashMap::new(),
            pending_tiles: HashSet::new(),
        }
    }
}

fn area_cells(areas: &[SubjectArea], margin: f32) -> HashSet<SpatialCell> {
    let mut rects = Vec::new();
    for area in areas {
        area.bounds.rectangles(&mut rects);
    }
    let mut cells = HashSet::new();
    for (min, max) in rects {
        let center = (min.x + max.x) * 0.5;
        let shift = onlinerpg_shared::wrap_world_x(center) - center;
        let (min_x, max_x) = (min.x + shift - margin, max.x + shift + margin);
        for offset in [
            0.0,
            -onlinerpg_shared::WORLD_WIDTH_X,
            onlinerpg_shared::WORLD_WIDTH_X,
        ] {
            let (left, right) = (min_x + offset, max_x + offset);
            if right < onlinerpg_shared::WORLD_MIN_X || left >= onlinerpg_shared::WORLD_MAX_X {
                continue;
            }
            cells.extend(SpatialCell::covering(
                left.max(onlinerpg_shared::WORLD_MIN_X),
                right.min(onlinerpg_shared::WORLD_MAX_X),
                min.z - margin,
                max.z + margin,
            ));
        }
    }
    cells
}

impl Interest {
    pub(super) fn refresh_player(&mut self, player: &onlinerpg_shared::Player) {
        if let Some(subject) = self.subjects.get_mut(&format!("player:{}", player.id)) {
            for message in &mut subject.snapshot {
                if let ServerMessage::PlayerAppeared { player: cached } = message {
                    *cached = player.clone();
                }
            }
            self.revision += 1;
            subject.revision = self.revision;
        }
    }
    pub(super) fn refresh_monster(&mut self, monster: &onlinerpg_shared::Monster) {
        let id = format!("monster:{}", monster.id);
        if let Some(subject) = self.subjects.get_mut(&id) {
            for message in &mut subject.snapshot {
                if let ServerMessage::MonsterSpawned { monster: cached } = message {
                    *cached = monster.clone();
                }
            }
            self.revision += 1;
            subject.revision = self.revision;
        }
    }

    pub(super) fn has_subject(&self, id: &str) -> bool {
        self.subjects.contains_key(id)
    }

    #[cfg(test)]
    pub(super) fn watches_subject(&self, viewer: PlayerId, id: &str) -> bool {
        self.views
            .get(&viewer)
            .is_some_and(|view| view.subjects.contains(id))
    }
    pub(super) fn publish_fishing(&mut self, message: ServerMessage) {
        let player_id = match &message {
            ServerMessage::FishingCasted { player_id, .. }
            | ServerMessage::FishingBite { player_id }
            | ServerMessage::FishingFight { player_id, .. }
            | ServerMessage::FishingEnded { player_id, .. } => *player_id,
            _ => return,
        };
        let id = format!("fishing:{player_id}");
        if matches!(message, ServerMessage::FishingEnded { .. }) {
            self.remove(&id, vec![message]);
            return;
        }
        if matches!(message, ServerMessage::FishingCasted { .. }) {
            let Some(player) = self.subjects.get(&format!("player:{player_id}")) else {
                return;
            };
            self.publish(
                id,
                player.areas.clone(),
                vec![message.clone()],
                vec![ServerMessage::FishingEnded {
                    player_id,
                    outcome: onlinerpg_shared::fishing::FishingOutcome::Aborted,
                }],
                vec![message],
            );
        } else {
            self.update_snapshot(&id, message.clone(), |snapshot| {
                snapshot.truncate(1);
                snapshot.push(message);
            });
        }
    }
    pub(super) fn publish_player_movement(
        &mut self,
        player: &super::Player,
        from: Position,
        floor: i8,
        message: ServerMessage,
    ) {
        let id = format!("player:{}", player.id);
        let extras = self
            .subjects
            .get(&id)
            .map(|subject| {
                subject
                    .current_snapshot()
                    .into_iter()
                    .skip(1)
                    .filter(|message| {
                        player.object_type.as_deref()
                            == Some(onlinerpg_shared::messages::MUSIC_EMOTE)
                            || !matches!(
                                message,
                                ServerMessage::PlayerMusicStarted { .. }
                                    | ServerMessage::PlayerInstrumentStarted { .. }
                            )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut snapshot = vec![ServerMessage::PlayerAppeared {
            player: player.clone(),
        }];
        snapshot.extend(extras);
        let mut movement_snapshot = snapshot.clone();
        movement_snapshot.push(message.clone());
        self.publish(
            id.clone(),
            vec![
                SubjectArea::point(from, floor),
                SubjectArea::point(player.position, player.floor_level),
            ],
            movement_snapshot,
            vec![ServerMessage::PlayerDisappeared {
                player_id: player.id,
            }],
            vec![message.clone()],
        );
        self.publish(
            id,
            vec![SubjectArea::point(player.position, player.floor_level)],
            snapshot,
            vec![ServerMessage::PlayerDisappeared {
                player_id: player.id,
            }],
            vec![],
        );
    }

    pub(super) fn publish_monster_movement(
        &mut self,
        monster: &onlinerpg_shared::Monster,
        message: ServerMessage,
    ) {
        let target = match &message {
            ServerMessage::MonsterMoved {
                target_position,
                state: onlinerpg_shared::MonsterState::Walk | onlinerpg_shared::MonsterState::Run,
                ..
            } => *target_position,
            _ => monster.position,
        };
        self.publish(
            format!("monster:{}", monster.id),
            vec![
                SubjectArea::point(monster.position, monster.floor_level),
                SubjectArea::point(target, monster.floor_level),
            ],
            vec![
                ServerMessage::MonsterSpawned {
                    monster: monster.clone(),
                },
                message.clone(),
            ],
            vec![ServerMessage::MonsterRemoved {
                monster_id: monster.id.clone(),
            }],
            vec![message],
        );
    }
    pub(super) fn update_snapshot(
        &mut self,
        id: &str,
        message: ServerMessage,
        update: impl FnOnce(&mut Vec<ServerMessage>),
    ) {
        let Some(subject) = self.subjects.get_mut(id) else {
            return;
        };
        subject.snapshot = subject.current_snapshot();
        update(&mut subject.snapshot);
        let (areas, snapshot, leave) = (
            subject.areas.clone(),
            subject.snapshot.clone(),
            subject.leave.clone(),
        );
        self.publish(id.to_owned(), areas, snapshot, leave, vec![message]);
    }

    pub(super) fn stop_movement(&mut self, id: &str) {
        if let Some(subject) = self.subjects.get_mut(id) {
            subject.areas.truncate(1);
            subject
                .snapshot
                .retain(|msg| !matches!(msg, ServerMessage::MonsterMoved { .. }));
        }
    }

    pub(super) fn publish_effect(
        &self,
        area: SubjectArea,
        message: &ServerMessage,
        skip: Option<&PlayerId>,
    ) {
        let speaker = match message {
            ServerMessage::ChatMessage { player_id, .. }
            | ServerMessage::Recital { player_id, .. }
            | ServerMessage::PlayerInstrumentNotes { player_id, .. } => self
                .subjects
                .get(&format!("player:{player_id}"))
                .and_then(|s| {
                    s.snapshot.iter().find_map(|msg| match msg {
                        ServerMessage::PlayerAppeared { player } => Some(&player.name),
                        _ => None,
                    })
                }),
            _ => None,
        };
        let Some(bytes) = encode_server_msg(message) else {
            return;
        };
        let mut sent = HashSet::new();
        for cell in area_cells(std::slice::from_ref(&area), EVENT_DELIVERY_RADIUS) {
            if let Some(ids) = self.viewers.cells.get(&cell) {
                for id in ids {
                    if skip == Some(id)
                        || !sent.insert(*id)
                        || speaker.is_some_and(|name| {
                            self.blocked_names
                                .get(id)
                                .is_some_and(|names| names.contains(name))
                        })
                    {
                        continue;
                    }
                    if let Some(view) = self.views.get(id) {
                        if area.contains(&view.position, &view.space) {
                            let _ = view.tx.send(bytes.clone());
                        }
                    }
                }
            }
        }
    }

    fn send(&mut self, viewer: PlayerId, reset: bool, mut events: Vec<WorldEvent>) {
        if let Some(blocked) = self.blocked_names.get(&viewer) {
            for event in &mut events {
                for message in &mut event.messages {
                    match message {
                        ServerMessage::StallAppeared { stall }
                        | ServerMessage::StallPlaced { stall }
                            if blocked.contains(&stall.owner_name) =>
                        {
                            stall.sign.clear()
                        }
                        ServerMessage::StallSignChanged { stall_id, sign }
                            if self
                                .stall_owners
                                .get(stall_id)
                                .is_some_and(|name| blocked.contains(name)) =>
                        {
                            sign.clear()
                        }
                        _ => {}
                    }
                }
            }
        }
        let Ok(body) = rmp_serde::to_vec(&events) else {
            return;
        };
        self.send_body(viewer, reset, &body);
    }

    fn send_body(&mut self, viewer: PlayerId, reset: bool, events: &[u8]) {
        let Some(view) = self.views.get_mut(&viewer) else {
            return;
        };
        view.sequence += 1;
        if let Ok(bytes) = encode_world_update(&self.epoch, view, reset, events) {
            tracing::trace!(viewer = %viewer, bytes = bytes.len(), subjects = view.subjects.len(), reset, "world_delivery");
            let _ = view.tx.send(bytes.into());
        }
    }

    pub fn remove_view(&mut self, viewer: PlayerId) {
        self.detach_view(viewer, true);
    }

    fn detach_view(&mut self, viewer: PlayerId, prune: bool) {
        if let Some(view) = self.views.remove(&viewer) {
            self.viewers.remove(&viewer, &view.position);
            for id in view.subjects {
                if let Some(subject) = self.subjects.get_mut(&id) {
                    subject.subscribers.remove(&viewer);
                }
                if prune {
                    self.prune_tile(&id);
                }
            }
        }
    }

    pub fn open_view(&mut self, player: &super::Player, tx: UnboundedSender<Bytes>) {
        let previous = self
            .views
            .get(&player.id)
            .map(|view| view.subjects.clone())
            .unwrap_or_default();
        self.detach_view(player.id, false);
        self.generation += 1;
        self.views.insert(
            player.id,
            View {
                position: player.position,
                space: Space::at(&player.position, player.floor_level),
                floor: player.floor_level,
                generation: self.generation,
                sequence: 0,
                subjects: HashSet::new(),
                tx,
                ready: true,
            },
        );
        self.viewers.insert(player.id, &player.position);
        self.refresh_player(player);
        self.reconcile(player.id, player.position, player.floor_level, true);
        for id in previous {
            self.prune_tile(&id);
        }
    }

    pub fn reconcile(&mut self, viewer: PlayerId, position: Position, floor: i8, reset: bool) {
        let Some(view) = self.views.get_mut(&viewer) else {
            return;
        };
        self.viewers.moved(&viewer, &view.position, &position);
        view.position = position;
        view.space = Space::at(&position, floor);
        view.floor = floor;
        let mut candidates = view.subjects.clone();
        candidates.insert(format!("player:{viewer}"));
        for cell in SpatialCell::within_radius(&position, EVENT_DELIVERY_RADIUS) {
            if let Some(ids) = self.cells.get(&cell) {
                candidates.extend(ids.iter().cloned());
            }
        }
        let mut events = Vec::new();
        for id in candidates {
            let Some(subject) = self.subjects.get_mut(&id) else {
                continue;
            };
            let visible = id == format!("player:{viewer}")
                || subject
                    .areas
                    .iter()
                    .any(|area| area.contains(&position, &view.space));
            let was = view.subjects.contains(&id);
            if visible && !was {
                subject.subscribers.insert(viewer);
                view.subjects.insert(id.clone());
                events.push(WorldEvent {
                    subject: id,
                    revision: subject.revision,
                    change: InterestChange::Enter,
                    messages: subject.current_snapshot(),
                });
            } else if !visible && was {
                subject.subscribers.remove(&viewer);
                view.subjects.remove(&id);
                events.push(WorldEvent {
                    subject: id,
                    revision: subject.revision,
                    change: InterestChange::Leave,
                    messages: subject.leave.clone(),
                });
            }
        }
        let departed: Vec<_> = events
            .iter()
            .filter(|event| event.change == InterestChange::Leave)
            .map(|event| event.subject.clone())
            .collect();
        self.send(viewer, reset, events);
        for id in departed {
            self.prune_tile(&id);
        }
    }

    fn prune_tile(&mut self, id: &str) {
        if id.starts_with("terrain:")
            && self
                .subjects
                .get(id)
                .is_some_and(|subject| subject.subscribers.is_empty())
        {
            self.remove(id, vec![]);
        }
    }

    pub fn publish(
        &mut self,
        id: String,
        areas: Vec<SubjectArea>,
        snapshot: Vec<ServerMessage>,
        leave: Vec<ServerMessage>,
        changes: Vec<ServerMessage>,
    ) {
        self.revision += 1;
        let cells = area_cells(&areas, 0.0);
        let previous = self.subjects.remove(&id);
        let created = previous.is_none();
        let mut candidates = previous
            .as_ref()
            .map(|old| old.subscribers.clone())
            .unwrap_or_default();
        if let Some(old) = &previous {
            for cell in &old.cells {
                if let Some(ids) = self.cells.get_mut(cell) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.cells.remove(cell);
                    }
                }
            }
        }
        for cell in area_cells(&areas, EVENT_DELIVERY_RADIUS) {
            if let Some(ids) = self.viewers.cells.get(&cell) {
                candidates.extend(ids.iter().copied());
            }
        }
        for cell in &cells {
            self.cells.entry(*cell).or_default().insert(id.clone());
        }
        let mut subject = Subject {
            areas,
            snapshot,
            leave,
            revision: self.revision,
            subscribers: HashSet::new(),
            cells,
            snapshot_at: std::time::Instant::now(),
        };
        let enter_messages = if created
            && changes.first().is_some_and(|message| {
                matches!(
                    message,
                    ServerMessage::PlayerJoined { .. }
                        | ServerMessage::MonsterSpawned { .. }
                        | ServerMessage::GroundItemSpawned { .. }
                        | ServerMessage::CampfireSpawned { .. }
                        | ServerMessage::StallPlaced { .. }
                        | ServerMessage::MealPlaced { .. }
                        | ServerMessage::TipHatPlaced { .. }
                )
            }) {
            &changes
        } else {
            &subject.snapshot
        };
        let mut bodies = HashMap::new();
        for viewer in candidates {
            let Some(view) = self.views.get_mut(&viewer) else {
                continue;
            };
            let visible = id == format!("player:{viewer}")
                || subject
                    .areas
                    .iter()
                    .any(|area| area.contains(&view.position, &view.space));
            let was = view.subjects.contains(&id);
            let change = match (was, visible) {
                (false, true) => InterestChange::Enter,
                (true, true) => InterestChange::Update,
                (true, false) => InterestChange::Leave,
                (false, false) => continue,
            };
            if visible {
                subject.subscribers.insert(viewer);
                view.subjects.insert(id.clone());
            } else {
                view.subjects.remove(&id);
            }
            if change == InterestChange::Update && changes.is_empty() {
                continue;
            }
            let event = || WorldEvent {
                subject: id.clone(),
                revision: subject.revision,
                change,
                messages: match change {
                    InterestChange::Enter => enter_messages.clone(),
                    InterestChange::Update => changes.clone(),
                    InterestChange::Leave => {
                        changes.iter().chain(&subject.leave).cloned().collect()
                    }
                    InterestChange::Delete => unreachable!(),
                },
            };
            if id.starts_with("stall:") && self.blocked_names.contains_key(&viewer) {
                self.send(viewer, false, vec![event()]);
            } else {
                let body = bodies.entry(change).or_insert_with(|| {
                    rmp_serde::to_vec(&vec![event()]).expect("world event serializes")
                });
                self.send_body(viewer, false, body);
            }
        }
        self.subjects.insert(id, subject);
    }

    pub fn remove(&mut self, id: &str, messages: Vec<ServerMessage>) {
        let Some(subject) = self.subjects.remove(id) else {
            return;
        };
        self.revision += 1;
        for cell in subject.cells {
            if let Some(ids) = self.cells.get_mut(&cell) {
                ids.remove(id);
                if ids.is_empty() {
                    self.cells.remove(&cell);
                }
            }
        }
        for viewer in subject.subscribers {
            if let Some(view) = self.views.get_mut(&viewer) {
                view.subjects.remove(id);
            }
            self.send(
                viewer,
                false,
                vec![WorldEvent {
                    subject: id.to_owned(),
                    revision: self.revision,
                    change: InterestChange::Delete,
                    messages: messages.clone(),
                }],
            );
        }
    }

    fn pending_tile(&mut self, x: i32, z: i32) {
        self.pending_tiles.insert((x, z));
        let area = SubjectArea {
            bounds: Bounds::tile(x, z, 64.0),
            space: Space::Surface,
        };
        let affected: Vec<_> = self
            .views
            .iter()
            .filter_map(|(id, view)| area.contains(&view.position, &view.space).then_some(*id))
            .collect();
        for id in affected {
            if let Some(view) = self.views.get_mut(&id) {
                view.ready = false;
            }
            self.send(id, false, vec![]);
        }
    }
}

impl super::GameState {
    pub(super) fn interest_lock(&self) -> std::sync::MutexGuard<'_, Interest> {
        self.interest.lock().expect("world interest lock poisoned")
    }

    pub(crate) fn publish_subject_change(&self, message: ServerMessage) {
        assert!(self.interest_lock().publish_state(&message));
    }

    pub(crate) async fn sync_interest_blocks(&self) {
        let blocked = self.blocked_names.read().await;
        self.interest_lock().blocked_names = blocked.clone();
    }
    pub(crate) async fn world_edit_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.persistence_lock.lock().await
    }

    pub(crate) async fn publish_terrain_tiles(&self, tiles: &[(i32, i32)]) -> std::io::Result<()> {
        self.deliver_terrain_tiles(tiles, true).await
    }

    async fn deliver_terrain_tiles(
        &self,
        tiles: &[(i32, i32)],
        rebuild: bool,
    ) -> std::io::Result<()> {
        let mut first_error = None;
        for &(x, z) in tiles.iter().collect::<std::collections::BTreeSet<_>>() {
            let result = if rebuild {
                self.terrain_io.rebuild_manifest(x, z).await
            } else {
                self.terrain_io.tile_manifest(x, z).await
            };
            let message = match result {
                Ok(message) => message,
                Err(error) => {
                    self.interest_lock().pending_tile(x, z);
                    first_error.get_or_insert(error);
                    continue;
                }
            };
            let mut interest = self.interest_lock();
            interest.pending_tiles.remove(&(x, z));
            interest.publish(
                format!("terrain:{x},{z}"),
                vec![SubjectArea {
                    bounds: Bounds::tile(x, z, 64.0),
                    space: Space::Surface,
                }],
                vec![message.clone()],
                vec![],
                vec![message],
            );
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(crate) async fn retry_terrain_delivery(&self) {
        let pending: Vec<_> = self.interest_lock().pending_tiles.iter().copied().collect();
        if pending.is_empty() {
            return;
        }
        {
            let _guard = self.world_edit_guard().await;
            for tile in pending {
                let _ = self.publish_terrain_tiles(&[tile]).await;
            }
        }
        let views: Vec<_> = self
            .interest_lock()
            .views
            .iter()
            .filter_map(|(id, view)| (!view.ready).then_some(*id))
            .collect();
        for viewer in views {
            self.reset_world_view(&viewer).await;
        }
    }
    pub(crate) async fn reconcile_view(&self, player_id: &PlayerId) {
        if !self.interest_lock().views.contains_key(player_id) {
            return;
        }
        let player = self.players.read().await.get(player_id).cloned();
        if let Some(player) = &player {
            if let Err(error) = self.prepare_terrain_view(player).await {
                tracing::warn!(%error, "Terrain view remains pending");
                return;
            }
        }
        let players = self.players.read().await;
        if let Some(player) = players.get(player_id) {
            self.interest_lock()
                .reconcile(*player_id, player.position, player.floor_level, false);
        }
    }

    pub(crate) async fn reset_world_view(&self, player_id: &PlayerId) {
        if !self.direct_channels.read().await.contains_key(player_id) {
            return;
        }
        let player = self.players.read().await.get(player_id).cloned();
        if let Some(player) = &player {
            if let Err(error) = self.prepare_terrain_view(player).await {
                tracing::warn!(%error, "Terrain snapshot remains pending");
                return;
            }
        }
        let players = self.players.read().await;
        let channels = self.direct_channels.read().await;
        if let (Some(player), Some(tx)) = (players.get(player_id), channels.get(player_id)) {
            self.interest_lock().open_view(player, tx.clone());
        }
    }

    async fn prepare_terrain_view(&self, player: &super::Player) -> std::io::Result<()> {
        if player.floor_level < 0 {
            return Ok(());
        }
        let center = player.position;
        let mut missing = Vec::new();
        {
            let interest = self.interest_lock();
            for z in onlinerpg_terrain::coords::world_to_tile(center.z - EVENT_DELIVERY_RADIUS)
                ..=onlinerpg_terrain::coords::world_to_tile(center.z + EVENT_DELIVERY_RADIUS)
            {
                for x in onlinerpg_terrain::coords::world_to_tile(center.x - EVENT_DELIVERY_RADIUS)
                    ..=onlinerpg_terrain::coords::world_to_tile(center.x + EVENT_DELIVERY_RADIUS)
                {
                    let x = onlinerpg_terrain::coords::wrap_tile_x(x);
                    if (!interest.has_subject(&format!("terrain:{x},{z}"))
                        || interest.pending_tiles.contains(&(x, z)))
                        && Bounds::tile(x, z, 64.0).distance_sq(&center)
                            <= EVENT_DELIVERY_RADIUS.powi(2)
                    {
                        missing.push((x, z));
                    }
                }
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let _guard = self.world_edit_guard().await;
        missing.retain(|(x, z)| {
            let interest = self.interest_lock();
            !interest.has_subject(&format!("terrain:{x},{z}"))
                || interest.pending_tiles.contains(&(*x, *z))
        });
        self.deliver_terrain_tiles(&missing, false).await
    }

    pub(crate) fn publish_house(&self, house: &onlinerpg_shared::housing::HouseData) {
        let message = ServerMessage::HouseUpdated {
            house: house.clone(),
        };
        self.interest_lock().publish(
            format!("house:{}", house.id),
            vec![SubjectArea {
                bounds: Bounds::house(house),
                space: Space::Surface,
            }],
            vec![message.clone()],
            vec![ServerMessage::HouseRemoved {
                house_id: house.id.clone(),
            }],
            vec![message],
        );
    }

    pub(crate) fn remove_house_subject(&self, house_id: &str) {
        self.interest_lock().remove(
            &format!("house:{house_id}"),
            vec![ServerMessage::HouseRemoved {
                house_id: house_id.to_owned(),
            }],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::tests::make_player;
    use onlinerpg_shared::interest::WorldView;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn receive(rx: &mut UnboundedReceiver<Bytes>) -> ServerMessage {
        onlinerpg_shared::deserialize_server_msg(&rx.try_recv().unwrap()).unwrap()
    }

    fn events(rx: &mut UnboundedReceiver<Bytes>) -> Vec<WorldEvent> {
        match receive(rx) {
            ServerMessage::WorldUpdate { events, .. } => events,
            other => panic!("Expected world update, got {other:?}"),
        }
    }

    fn watch(interest: &mut Interest, player: &super::super::Player) -> UnboundedReceiver<Bytes> {
        let (tx, rx) = unbounded_channel();
        interest.open_view(player, tx);
        rx
    }

    fn state(text: &str) -> Vec<ServerMessage> {
        vec![ServerMessage::SystemMessage {
            localization: None,
            message: text.into(),
        }]
    }

    #[test]
    fn cached_wire_body_matches_shared_protocol() {
        let position = Position {
            x: -3.0,
            y: 2.0,
            z: 5.0,
        };
        let events = vec![WorldEvent {
            subject: "house:a".into(),
            revision: 8,
            change: InterestChange::Enter,
            messages: state("snapshot"),
        }];
        let (tx, _rx) = unbounded_channel();
        let view = View {
            position,
            space: Space::at(&position, 1),
            floor: 1,
            generation: 3,
            sequence: 6,
            subjects: HashSet::new(),
            tx,
            ready: true,
        };
        let bytes = encode_world_update("epoch", &view, true, &rmp_serde::to_vec(&events).unwrap())
            .unwrap();
        let expected = ServerMessage::WorldUpdate {
            world_epoch: "epoch".into(),
            generation: 3,
            sequence: 6,
            position,
            floor_level: 1,
            reset: true,
            ready: true,
            events,
        };
        assert_eq!(
            bytes,
            onlinerpg_shared::serialize_server_msg(&expected).unwrap()
        );
        assert!(matches!(
            onlinerpg_shared::deserialize_server_msg(&bytes),
            Ok(ServerMessage::WorldUpdate { sequence: 6, .. })
        ));
    }

    #[test]
    fn actual_regions_cross_house_floors_and_delete_previous_subscribers() {
        let mut interest = Interest::default();
        let mut upstairs = make_player("upstairs", 60.0, 0.0);
        upstairs.floor_level = 2;
        let mut rx = watch(&mut interest, &upstairs);
        events(&mut rx);
        let area = SubjectArea {
            bounds: Bounds::tile(0, 0, 64.0),
            space: Space::Surface,
        };
        interest.publish(
            "house:a".into(),
            vec![area.clone()],
            state("open"),
            state("leave"),
            state("open"),
        );
        assert_eq!(events(&mut rx)[0].change, InterestChange::Enter);
        interest.publish(
            "house:a".into(),
            vec![area],
            state("closed"),
            state("leave"),
            state("closed"),
        );
        assert_eq!(events(&mut rx)[0].change, InterestChange::Update);
        interest.reconcile(
            upstairs.id,
            Position {
                x: 100.0,
                ..upstairs.position
            },
            2,
            false,
        );
        assert_eq!(events(&mut rx)[0].change, InterestChange::Leave);
        interest.reconcile(upstairs.id, upstairs.position, 2, false);
        assert!(
            matches!(&events(&mut rx)[0].messages[0], ServerMessage::SystemMessage { message, .. } if message == "closed")
        );
        interest.remove("house:a", state("deleted"));
        assert_eq!(events(&mut rx)[0].change, InterestChange::Delete);
        assert!(!interest.watches_subject(upstairs.id, "house:a"));
    }

    #[test]
    fn endpoint_union_does_not_include_the_middle_and_sends_last_update_before_leave() {
        let mut interest = Interest::default();
        let middle = make_player("middle", 0.0, 0.0);
        let edge = make_player("edge", 40.0, 0.0);
        let mut middle_rx = watch(&mut interest, &middle);
        let mut edge_rx = watch(&mut interest, &edge);
        events(&mut middle_rx);
        events(&mut edge_rx);
        interest.publish(
            "monster:a".into(),
            vec![
                SubjectArea::point(
                    Position {
                        x: -40.0,
                        ..middle.position
                    },
                    0,
                ),
                SubjectArea::point(edge.position, 0),
            ],
            state("spawn+move"),
            state("leave"),
            state("move"),
        );
        assert!(middle_rx.try_recv().is_err());
        assert_eq!(events(&mut edge_rx)[0].change, InterestChange::Enter);
        interest.publish(
            "monster:a".into(),
            vec![SubjectArea::point(
                Position {
                    x: 80.0,
                    ..middle.position
                },
                0,
            )],
            state("new"),
            state("leave"),
            state("stop"),
        );
        let departed = events(&mut edge_rx);
        assert_eq!(departed[0].change, InterestChange::Leave);
        assert!(
            matches!(&departed[0].messages[..], [ServerMessage::SystemMessage { message: first, .. }, ServerMessage::SystemMessage { message: last, .. }] if first == "stop" && last == "leave")
        );
    }

    #[test]
    fn payloadless_tile_leave_and_empty_reset_clear_active_membership() {
        let mut interest = Interest::default();
        let player = make_player("viewer", 60.0, 0.0);
        let mut rx = watch(&mut interest, &player);
        events(&mut rx);
        interest.publish(
            "terrain:0,0".into(),
            vec![SubjectArea {
                bounds: Bounds::tile(0, 0, 64.0),
                space: Space::Surface,
            }],
            state("tile"),
            vec![],
            state("tile"),
        );
        assert_eq!(events(&mut rx)[0].change, InterestChange::Enter);
        interest.reconcile(
            player.id,
            Position {
                x: 65.0,
                ..player.position
            },
            0,
            false,
        );
        let left = events(&mut rx);
        assert_eq!(left[0].change, InterestChange::Leave);
        assert!(left[0].messages.is_empty());
        let mut view = WorldView::default();
        let mut moved = player.clone();
        moved.position.x = 100.0;
        let mut reset_rx = watch(&mut interest, &moved);
        match receive(&mut reset_rx) {
            ServerMessage::WorldUpdate {
                world_epoch,
                generation,
                sequence,
                reset,
                events,
                ..
            } => {
                assert!(view.accept(&world_epoch, generation, sequence, reset, &events));
                assert!(view.subjects.is_empty());
            }
            _ => panic!("missing snapshot"),
        }
    }

    #[test]
    fn music_enter_restores_elapsed_time_and_fishing_uses_the_angler() {
        let mut interest = Interest::default();
        let angler = make_player("angler", 0.0, 0.0);
        interest.publish_state(&ServerMessage::PlayerAppeared {
            player: angler.clone(),
        });
        interest.publish_state(&ServerMessage::PlayerMusicStarted {
            player_id: angler.id,
            track: "tune".into(),
            elapsed_secs: 3.0,
        });
        interest
            .subjects
            .get_mut(&format!("player:{}", angler.id))
            .unwrap()
            .snapshot_at -= std::time::Duration::from_secs(2);
        let nearby = make_player("near", 31.0, 0.0);
        let mut rx = watch(&mut interest, &nearby);
        assert!(events(&mut rx).iter().flat_map(|event| &event.messages).any(|message| matches!(message, ServerMessage::PlayerMusicStarted { elapsed_secs, .. } if *elapsed_secs >= 5.0)));
        let distant = make_player("far", 50.0, 0.0);
        let mut far_rx = watch(&mut interest, &distant);
        events(&mut far_rx);
        let cast = ServerMessage::FishingCasted {
            player_id: angler.id,
            position: distant.position,
            rotation: 0.0,
        };
        interest.publish_fishing(cast);
        assert_eq!(events(&mut rx)[0].change, InterestChange::Enter);
        assert!(far_rx.try_recv().is_err());
        let mut late_rx = watch(&mut interest, &make_player("late_angler_viewer", 2.0, 0.0));
        assert!(events(&mut late_rx)
            .iter()
            .flat_map(|event| &event.messages)
            .any(|message| matches!(message,
                ServerMessage::FishingCasted { player_id, position, .. }
                    if *player_id == angler.id && *position == distant.position
            )));
        interest.publish_fishing(ServerMessage::FishingEnded {
            player_id: angler.id,
            outcome: onlinerpg_shared::fishing::FishingOutcome::Aborted,
        });
        assert_eq!(events(&mut rx)[0].change, InterestChange::Delete);
    }
}
