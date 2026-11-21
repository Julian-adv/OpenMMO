//! Road patrols: guards clear estate fences off roads, and the guard captain
//! sells first-offence seizures back (doc/ROAD_PATROL.md).

use super::{
    auth_db,
    combat::reachable_dist_sq,
    inventory::{rows_with_inserts, BagHold, BagInsert},
    trading::MAX_TRADE_DISTANCE,
    GameState,
};
use crate::auth::{unix_now, AuthService, ImpoundedStack, NewLetter, RoadClearance, RoadSeizure};
use crate::metrics::GoldSink;
use crate::npc_defs::npc_defs;
use crate::road_index::RoadIndex;
use crate::types::{PlayerId, Position, ServerMessage};
use onlinerpg_shared::fence::{self, FenceAxis, FenceEdge};
use onlinerpg_shared::furniture::point_in_house_floor;
use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::messages::{
    ImpoundedItem, PatrolSeizure, SnowClearing, SNOW_CLEAR_LASTS_MIN,
};
use onlinerpg_shared::pathfinding::{
    is_cardinal_move_blocked, is_cell_under_furniture, is_doorway, PassabilityCache, DIRS,
};
use std::collections::{HashSet, VecDeque};

/// How far from the guard one bout of shovelling clears the road.
const SHOVEL_REACH_M: f32 = 6.0;
/// A guard this close to a stretch cleared within `SHOVEL_FRESH_MIN` leaves
/// it: the same stop on the way back, or a partner's spot beside it.
const SHOVEL_SKIP_M: f32 = 3.0;
const SHOVEL_FRESH_MIN: f64 = 200.0;
/// A guard reaching a stretch this soon after another cleared it digs in too.
const SHOVEL_JOIN_MIN: f64 = 2.0;

/// A cleared stretch and the guards who shovelled it.
pub(super) struct ShovelledStretch {
    clearing: SnowClearing,
    by: Vec<PlayerId>,
}

/// The part of `segment` within `reach` of `(x, z)`.
fn clip_to_reach(segment: &[f32; 4], x: f32, z: f32, reach: f32) -> Option<[f32; 4]> {
    let [ax, az, bx, bz] = *segment;
    let (dx, dz) = (bx - ax, bz - az);
    let (fx, fz) = (ax - x, az - z);
    let a = dx * dx + dz * dz;
    if a == 0.0 {
        return None;
    }
    let b = 2.0 * (fx * dx + fz * dz);
    let c = fx * fx + fz * fz - reach * reach;
    let root = (b * b - 4.0 * a * c).sqrt();
    if root.is_nan() {
        return None;
    }
    let from = ((-b - root) / (2.0 * a)).max(0.0);
    let to = ((-b + root) / (2.0 * a)).min(1.0);
    (from < to).then_some([ax + dx * from, az + dz * from, ax + dx * to, az + dz * to])
}

/// What a guard asking to shovel does.
enum Shovelling {
    Clears(SnowClearing),
    Joins,
}

/// How far around a guard one sweep reaches.
const SWEEP_RADIUS_M: f32 = 24.0;
pub(super) const GUARD_CLASS: &str = "guard";
/// A walk that gets this many cells from the guard is not penned in.
const PEN_REACH_CELLS: i32 = 48;
/// Nested pens one break-out works through.
const BREAK_OUT_ROUNDS: usize = 8;
pub(crate) use onlinerpg_shared::schedule::GUARD_CAPTAIN;

fn reclaim_fee(base_price: i64, quantity: u32) -> i64 {
    base_price * i64::from(quantity) / 2
}

fn seizure_letter(seizure: &RoadSeizure, captain: &str) -> String {
    let (en, ko) = if seizure.obstruction {
        (
            "What you built shut one of my guards in while on duty. We tore down everything \
             around them, and none of it comes back to you. Pen in the watch and you get no \
             first chance."
                .to_string(),
            "네가 세운 것이 근무 중인 내 대원을 가두었다. 둘레에 있던 것은 우리 경비대가 모조리 \
             걷어 냈고, 하나도 돌려주지 않는다. 경비대를 가두는 짓에는 처음이라고 봐주는 일이 없다."
                .to_string(),
        )
    } else if seizure.forfeited() {
        (
            format!(
                "The road belongs to everyone. The watch has torn down what you unlawfully \
                 built on it. This is the {} time, and you were warned. All of it is forfeit. \
                 Do not come asking for it.",
                ordinal(seizure.offence)
            ),
            format!(
                "길은 모두의 것이다. 네가 공용 도로에 세운 불법 건축물은 우리 경비대가 철거했다. \
                 이번이 {}번째다. 경고는 이미 했다. 전부 몰수한다. 찾으러 오지 마라.",
                seizure.offence
            ),
        )
    } else {
        (
            "The road belongs to everyone. The watch has torn down what you unlawfully built \
             on it. As it is your first time, I am holding it at my post. Bring half its price \
             within twenty days and it is yours again; whatever your chests held comes back \
             untouched. After twenty days it is gone, and there is no second time."
                .to_string(),
            "길은 모두의 것이다. 네가 공용 도로에 세운 불법 건축물은 우리 경비대가 철거했다. \
             처음이니 이번만 내가 맡아 둔다. 스무 날 안에 값의 절반을 들고 내 초소로 와라. \
             보관함에 들어 있던 것은 그대로 돌려준다. 스무 날이 지나면 없다. 두 번째는 없다."
                .to_string(),
        )
    };
    format!("{en}\n\n{ko}\n\n— {captain}, Captain of the Guard")
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

const RECORDS_UNAVAILABLE: &str = "Patrol records are unavailable.";

/// What walls in a walk: fence edges and cells under furniture.
struct Pen {
    edges: Vec<FenceEdge>,
    furnished: Vec<(i32, i32)>,
}

/// What walls in the surface cells walkable from `start`. `None` when the walk reaches a road or gets
/// `PEN_REACH_CELLS` away. Doorways count as open.
fn pen_around(
    cache: &PassabilityCache,
    fences: &super::fence::FenceIndex,
    roads: Option<&RoadIndex>,
    start: (i32, i32),
) -> Option<Pen> {
    let floor = onlinerpg_shared::dungeon::passability_floor_for_level(0);
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    let mut edges = HashSet::new();
    let mut furnished = HashSet::new();
    while let Some((x, z)) = queue.pop_front() {
        let away = (x - start.0).abs().max((z - start.1).abs());
        if away > PEN_REACH_CELLS
            || roads.is_some_and(|roads| roads.on_road(x as f32 + 0.5, z as f32 + 0.5))
        {
            return None;
        }
        for (dx, dz) in DIRS {
            let walled = is_cardinal_move_blocked(cache, x, z, dx, dz, floor)
                && !is_doorway(cache, x, z, dx, dz, floor);
            if walled {
                let edge = FenceEdge {
                    x: x + dx.max(0),
                    z: z + dz.max(0),
                    axis: if dx == 0 { FenceAxis::X } else { FenceAxis::Z },
                };
                let (nx, nz) = (x + dx, z + dz);
                if fences.get(&edge).is_some() {
                    edges.insert(edge);
                } else if is_cell_under_furniture(cache, nx as f32, nz as f32, floor) {
                    furnished.insert((nx, nz));
                }
            } else if seen.insert((x + dx, z + dz)) {
                queue.push_back((x + dx, z + dz));
            }
        }
    }
    Some(Pen {
        edges: edges.into_iter().collect(),
        furnished: furnished.into_iter().collect(),
    })
}

/// How far from a cell's centre a piece of furniture can stand and still
/// cover the cell.
fn furniture_reach(item_def_id: &str) -> f32 {
    let half_diagonal = onlinerpg_shared::estate_storage::estate_storage_def(item_def_id)
        .map_or(0.0, |def| {
            def.footprint_width.hypot(def.footprint_depth) / 2.0
        });
    half_diagonal + std::f32::consts::FRAC_1_SQRT_2
}

/// A sweep from where a guard stands.
struct SweepPlan {
    guard: Position,
    /// Estate fences on the road.
    edges: Vec<FenceEdge>,
    /// Furniture on the road or in a house on it, with where it stands.
    furniture: Vec<(i64, Position)>,
    /// Player houses on the road, each with its nearest road cell.
    houses: Vec<(HouseData, Position)>,
}

fn on_road_cell(
    house: &HouseData,
    roads: &RoadIndex,
    near: &Position,
    radius: f32,
) -> Option<Position> {
    super::house_building::foundation_cells(house)
        .into_iter()
        .map(|(x, z)| Position { x, y: near.y, z })
        .filter(|cell| roads.on_road(cell.x, cell.z) && near.dist_xz_sq(cell) <= radius * radius)
        .min_by(|a, b| near.dist_xz_sq(a).total_cmp(&near.dist_xz_sq(b)))
}

fn is_guard_captain(npc: &crate::types::Player) -> bool {
    is_registry_npc(npc, GUARD_CAPTAIN)
}

/// Whether `npc` is the official NPC registered as `id`.
/// Why a player cannot deal with a registry NPC.
pub(super) enum NpcReach {
    TooFar,
    Asleep,
}

pub(super) fn is_registry_npc(npc: &crate::types::Player, id: &str) -> bool {
    npc.is_official_npc
        && npc_defs()
            .get_by_npc_name(&npc.name)
            .is_some_and(|def| def.id == id)
}

impl GameState {
    pub fn set_roads(&self, roads: RoadIndex) {
        let _ = self.roads.set(roads);
    }

    pub(super) async fn surface_guard(
        &self,
        player_id: &PlayerId,
    ) -> Result<Position, &'static str> {
        let players = self.players.read().await;
        let guard = players.get(player_id).ok_or("Character not found.")?;
        let is_guard = guard.is_official_npc
            && npc_defs()
                .get_by_npc_name(&guard.name)
                .is_some_and(|def| def.class == GUARD_CLASS);
        if !is_guard {
            return Err("Only the guard does this.");
        }
        if guard.health == 0 || guard.floor_level != 0 {
            return Err("Stand on the surface, alive.");
        }
        Ok(guard.position)
    }

    /// Player houses near `guard` standing on the road, each with its nearest
    /// road cell.
    async fn road_houses(&self, guard: &Position, roads: &RoadIndex) -> Vec<(HouseData, Position)> {
        let (cx, cz) = crate::housing::world_to_chunk(guard.x, guard.z);
        let mut found = Vec::new();
        for (dx, dz) in (-2..=2).flat_map(|dx| (-2..=2).map(move |dz| (dx, dz))) {
            let Ok(houses) = self.housing_io.read_chunk(cx + dx, cz + dz).await else {
                continue;
            };
            for house in houses {
                let owned = house.owner_id.parse::<i64>().is_ok_and(|id| id > 0);
                let cell = on_road_cell(&house, roads, guard, SWEEP_RADIUS_M);
                if let (true, Some(cell)) = (owned, cell) {
                    found.push((house, cell));
                }
            }
        }
        found
    }

    async fn sweep_plan(&self, guard: Position) -> SweepPlan {
        let mut plan = SweepPlan {
            guard,
            edges: Vec::new(),
            furniture: Vec::new(),
            houses: Vec::new(),
        };
        let Some(roads) = self.roads.get() else {
            return plan;
        };
        plan.edges = self
            .fences
            .read()
            .await
            .within(&guard, SWEEP_RADIUS_M)
            .into_iter()
            .filter(|fence| {
                let centre = fence.edge.center(0.0);
                roads.on_road(centre.x, centre.z)
            })
            .map(|fence| fence.edge)
            .collect();
        plan.houses = self.road_houses(&guard, roads).await;
        let index = self.estate_chests.read().await;
        let reach = SWEEP_RADIUS_M + 2.0 * crate::housing::CHUNK_SIZE;
        for chest in index.around(&guard, reach) {
            let at = chest.position;
            let house_cell = u8::try_from(chest.floor_level).ok().and_then(|floor| {
                plan.houses
                    .iter()
                    .find(|(house, _)| point_in_house_floor(house, at.x, at.z, floor))
                    .map(|(_, cell)| *cell)
            });
            let on_road = chest.floor_level == 0
                && guard.dist_xz_sq(&at) <= SWEEP_RADIUS_M * SWEEP_RADIUS_M
                && roads.on_road(at.x, at.z);
            if let Some(target) = house_cell.or(on_road.then_some(at)) {
                plan.furniture.push((chest.id, target));
            }
        }
        drop(index);
        plan
    }

    /// The nearest thing a sweep from here would pull down.
    pub async fn patrol_scout(&self, player_id: &PlayerId) {
        let guard = self.surface_guard(player_id).await.ok();
        let plan = match guard {
            Some(guard) => Some(self.sweep_plan(guard).await),
            None => None,
        };
        let target = plan.and_then(|plan| {
            let guard = plan.guard;
            plan.edges
                .iter()
                .map(|edge| edge.center(guard.y))
                .chain(plan.furniture.iter().map(|(_, at)| *at))
                .chain(plan.houses.iter().map(|(_, cell)| *cell))
                .min_by(|a, b| guard.dist_xz_sq(a).total_cmp(&guard.dist_xz_sq(b)))
        });
        self.send_direct_message(player_id, ServerMessage::PatrolScouted { target })
            .await;
    }

    pub async fn patrol_strike(&self, player_id: &PlayerId) {
        let Ok(position) = self.surface_guard(player_id).await else {
            return;
        };
        let swung = ServerMessage::PlayerSwung {
            player_id: *player_id,
        };
        self.publish_nearby(&position, 0, swung, None).await;
    }

    /// Road stretches still clear of snow.
    pub fn road_snow_message(&self) -> ServerMessage {
        let now = self.game_minutes_now();
        let mut cleared = self.road_snow.lock().expect("road snow lock poisoned");
        cleared.retain(|stretch| now - stretch.clearing.at_min < SNOW_CLEAR_LASTS_MIN);
        ServerMessage::RoadSnowCleared {
            reset: true,
            stretches: cleared.iter().map(|s| s.clearing.clone()).collect(),
        }
    }

    /// Clear the road around the guard, or join a partner who just did.
    /// `None` when no snow lies here or the stretch was cleared a while ago.
    fn shovel_here(&self, player_id: &PlayerId, guard: &Position) -> Option<Shovelling> {
        let roads = self.roads.get()?;
        if !roads.near_road(guard.x, guard.z) || !self.snow_lies_at(guard.x, guard.z) {
            return None;
        }
        let now = self.game_minutes_now();
        let mut cleared = self.road_snow.lock().expect("road snow lock poisoned");
        cleared.retain(|stretch| now - stretch.clearing.at_min < SNOW_CLEAR_LASTS_MIN);
        let fresh = cleared.iter_mut().find(|stretch| {
            now - stretch.clearing.at_min < SHOVEL_FRESH_MIN
                && stretch.clearing.segments.iter().any(|segment| {
                    onlinerpg_shared::worldgen::vector_features::point_segment_distance(
                        guard.x, guard.z, segment,
                    ) <= SHOVEL_SKIP_M
                })
        });
        if let Some(stretch) = fresh {
            let joins =
                now - stretch.clearing.at_min < SHOVEL_JOIN_MIN && !stretch.by.contains(player_id);
            if joins {
                stretch.by.push(*player_id);
            }
            return joins.then_some(Shovelling::Joins);
        }
        let clearing = SnowClearing {
            segments: roads
                .segments_near(guard.x, guard.z, SHOVEL_REACH_M)
                .iter()
                .filter_map(|segment| clip_to_reach(segment, guard.x, guard.z, SHOVEL_REACH_M))
                .collect(),
            at_min: now,
        };
        cleared.push(ShovelledStretch {
            clearing: clearing.clone(),
            by: vec![*player_id],
        });
        Some(Shovelling::Clears(clearing))
    }

    /// Shovel the lying snow off the road around `at`; false when there is none to clear.
    pub(super) async fn shovel_at(&self, player_id: &PlayerId, at: &Position) -> bool {
        let Some(shovelling) = self.shovel_here(player_id, at) else {
            return false;
        };
        let shovelled = ServerMessage::PlayerShovelled {
            player_id: *player_id,
        };
        self.publish_nearby(at, 0, shovelled, None).await;
        if let Shovelling::Clears(stretch) = shovelling {
            self.broadcast(ServerMessage::RoadSnowCleared {
                reset: false,
                stretches: vec![stretch],
            });
        }
        true
    }

    pub async fn patrol_shovel(&self, player_id: &PlayerId) {
        let cleared = match self.surface_guard(player_id).await {
            Ok(guard) => self.shovel_at(player_id, &guard).await,
            Err(_) => false,
        };
        self.send_direct_message(player_id, ServerMessage::PatrolShovelled { cleared })
            .await;
    }

    pub async fn patrol_sweep(&self, player_id: &PlayerId, patrol_id: i64, auth: &AuthService) {
        let seized = match self.try_patrol_sweep(player_id, patrol_id, auth).await {
            Ok(result) => result,
            Err(reason) => {
                tracing::warn!(?player_id, reason, "patrol sweep refused");
                Default::default()
            }
        };
        self.send_direct_message(player_id, ServerMessage::PatrolSweepResult { seized })
            .await;
    }

    async fn try_patrol_sweep(
        &self,
        player_id: &PlayerId,
        patrol_id: i64,
        auth: &AuthService,
    ) -> Result<Vec<PatrolSeizure>, &'static str> {
        let guard = self.surface_guard(player_id).await?;
        self.sweep_road_at(guard, patrol_id, auth).await
    }

    /// Pull down everything a patrol standing at `from` clears off the road.
    pub(super) async fn sweep_road_at(
        &self,
        from: Position,
        patrol_id: i64,
        auth: &AuthService,
    ) -> Result<Vec<PatrolSeizure>, &'static str> {
        let plan = self.sweep_plan(from).await;
        let clearance = RoadClearance {
            edges: plan.edges,
            furniture: plan.furniture.into_iter().map(|(id, _)| id).collect(),
            houses: Vec::new(),
        };
        let houses = plan.houses.into_iter().map(|(house, _)| house).collect();
        let seizures = self
            .clear_road(clearance, houses, patrol_id, false, auth)
            .await?;
        Ok(seizures.iter().map(RoadSeizure::report).collect())
    }

    /// Pull down `houses`, then take the fences and furniture in `clearance`
    /// away, log each owner's loss and write to owners newly in offence.
    async fn clear_road(
        &self,
        mut clearance: RoadClearance,
        houses: Vec<HouseData>,
        patrol_id: i64,
        obstruction: bool,
        auth: &AuthService,
    ) -> Result<Vec<RoadSeizure>, &'static str> {
        if clearance.edges.is_empty() && clearance.furniture.is_empty() && houses.is_empty() {
            return Ok(Vec::new());
        }
        let _persistence = self.persistence_lock.lock().await;
        let mut evicted = Vec::new();
        for house in &houses {
            match self.raze_house(house).await {
                Ok(occupants) => {
                    evicted.extend(occupants);
                    if let Ok(owner) = house.owner_id.parse::<i64>() {
                        clearance
                            .houses
                            .push((owner, house.source_scroll_id.clone()));
                    }
                    tracing::info!(house = %house.id, owner = %house.owner_id, "road patrol razed house");
                }
                Err(error) => {
                    tracing::warn!(%error, house = %house.id, "Failed to raze road house")
                }
            }
        }
        let db = auth.clone();
        let seizures = auth_db(move || db.seize_road(&clearance, patrol_id, obstruction))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Failed to clear the road");
                RECORDS_UNAVAILABLE
            })?;
        let removed: Vec<FenceEdge> = seizures
            .iter()
            .flat_map(|seizure| seizure.edges.iter().copied())
            .collect();
        if !removed.is_empty() {
            let mut fences = self.fences.write().await;
            let groups = fences.drop_edges(&removed);
            let mut cache = self.passability_write();
            for (key, group) in groups {
                fence::sync_passability(&mut cache, &key, &group);
            }
            drop(cache);
            self.publish_subject_change(ServerMessage::FenceVisibility {
                added: vec![],
                removed,
            });
        }
        let furniture: Vec<i64> = seizures
            .iter()
            .flat_map(|seizure| seizure.furniture.iter().copied())
            .collect();
        self.remove_estate_furniture(&furniture).await;
        drop(_persistence);
        for player_id in evicted {
            self.teleport_to_town(&player_id).await;
            self.send_system_message(
                &player_id,
                "The road patrol pulled down the house you were in. You have been moved to town.",
            )
            .await;
        }
        for seizure in &seizures {
            tracing::info!(
                owner = %seizure.owner_name,
                fences = seizure.edges.len(),
                furniture = seizure.furniture.len(),
                houses = seizure.houses,
                offence = seizure.offence,
                forfeited = seizure.forfeited(),
                obstruction,
                "road patrol cleared road"
            );
            if seizure.new_offence {
                self.send_seizure_letter(seizure, auth).await;
            }
        }
        Ok(seizures)
    }

    pub async fn patrol_break_out(&self, player_id: &PlayerId, recall: bool, auth: &AuthService) {
        let (seized, recalled) = match self.try_patrol_break_out(player_id, recall, auth).await {
            Ok(result) => result,
            Err(reason) => {
                tracing::warn!(?player_id, reason, "patrol break-out refused");
                Default::default()
            }
        };
        self.send_direct_message(
            player_id,
            ServerMessage::PatrolBrokeOut { seized, recalled },
        )
        .await;
    }

    /// The estate fences and furniture penning in whoever stands at `start`,
    /// or `None` when nothing pens them in.
    async fn pen_at(&self, start: (i32, i32)) -> Option<RoadClearance> {
        let cache = self.passability.snapshot();
        let Pen { edges, furnished } = {
            let fences = self.fences.read().await;
            pen_around(&cache, &fences, self.roads.get(), start)?
        };
        let index = self.estate_chests.read().await;
        let mut furniture = HashSet::new();
        for (x, z) in furnished {
            let centre = Position {
                x: x as f32 + 0.5,
                y: 0.0,
                z: z as f32 + 0.5,
            };
            furniture.extend(
                index
                    .around(&centre, 4.0)
                    .into_iter()
                    .filter(|piece| {
                        let reach = furniture_reach(&piece.item_def_id);
                        piece.floor_level == 0
                            && piece.position.dist_xz_sq(&centre) <= reach * reach
                    })
                    .map(|piece| piece.id),
            );
        }
        Some(RoadClearance {
            edges,
            furniture: furniture.into_iter().collect(),
            houses: Vec::new(),
        })
    }

    /// Forfeit the estate fences and furniture penning in whoever stands at
    /// `start`, ring by ring. Returns each owner's loss and whether the pen
    /// still holds.
    async fn clear_pen(
        &self,
        start: (i32, i32),
        auth: &AuthService,
    ) -> Result<(Vec<PatrolSeizure>, bool), &'static str> {
        let patrol_id = unix_now();
        let mut seized: Vec<PatrolSeizure> = Vec::new();
        for _ in 0..BREAK_OUT_ROUNDS {
            let Some(pen) = self.pen_at(start).await else {
                return Ok((seized, false));
            };
            let taken = self
                .clear_road(pen, Vec::new(), patrol_id, true, auth)
                .await?;
            if taken.is_empty() {
                return Ok((seized, true));
            }
            for seizure in taken.iter().map(RoadSeizure::report) {
                match seized.iter_mut().find(|s| s.owner == seizure.owner) {
                    Some(entry) => {
                        entry.fences += seizure.fences;
                        entry.furniture += seizure.furniture;
                    }
                    None => seized.push(seizure),
                }
            }
        }
        let penned = self.pen_at(start).await.is_some();
        Ok((seized, penned))
    }

    /// A guard on furniture stands in a walled cell without being penned, so
    /// it is left alone. Any other guard still penned once its pen is
    /// cleared, or one asking for `recall`, goes back to town.
    async fn try_patrol_break_out(
        &self,
        player_id: &PlayerId,
        recall: bool,
        auth: &AuthService,
    ) -> Result<(Vec<PatrolSeizure>, bool), &'static str> {
        let guard = self.surface_guard(player_id).await?;
        let floor = onlinerpg_shared::dungeon::passability_floor_for_level(0);
        let seated = is_cell_under_furniture(&self.passability.snapshot(), guard.x, guard.z, floor);
        let (seized, penned) = if seated {
            Default::default()
        } else {
            let start = (guard.x.floor() as i32, guard.z.floor() as i32);
            self.clear_pen(start, auth).await?
        };
        let recalled = recall || penned;
        if recalled {
            let guard = self.player_name_of(player_id).await;
            tracing::info!(%guard, recall, "penned guard recalled to town");
            self.teleport_to_town_with_effects(player_id).await;
        }
        Ok((seized, recalled))
    }

    async fn send_seizure_letter(&self, seizure: &RoadSeizure, auth: &AuthService) {
        let Some(captain) = npc_defs().npc_name_by_id(GUARD_CAPTAIN) else {
            return;
        };
        let captain = captain.to_string();
        let body = seizure_letter(seizure, &captain);
        let recipient = seizure.owner_name.clone();
        let game_seconds = self.current_total_game_seconds();
        let db = auth.clone();
        let sent = auth_db(move || {
            let sender_id = db
                .resolve_character_brief(&captain)?
                .map_or(0, |(id, _)| id);
            db.send_mail(&NewLetter {
                sender_id,
                sender: &captain,
                recipient: &recipient,
                body: &body,
                game_seconds,
            })
        })
        .await;
        match sent {
            Ok(Ok(delivered)) => self.notify_unread_mail(delivered.character_id, auth).await,
            Ok(Err(_)) => {}
            Err(error) => tracing::warn!(%error, "Failed to send seizure letter"),
        }
    }

    async fn at_guard_captain(&self, player_id: &PlayerId, npc_player_id: &PlayerId) -> bool {
        self.at_registry_npc(player_id, npc_player_id, GUARD_CAPTAIN)
            .await
            .is_ok()
    }

    /// Whether `npc_player_id` is the registry NPC `id`, within trading reach of `player_id` and awake.
    /// Sleep is checked after range so it can't be probed from afar.
    pub(super) async fn at_registry_npc(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        id: &str,
    ) -> Result<(), NpcReach> {
        let players = self.players.read().await;
        let (Some(player), Some(npc)) = (players.get(player_id), players.get(npc_player_id)) else {
            return Err(NpcReach::TooFar);
        };
        let in_reach = is_registry_npc(npc, id)
            && reachable_dist_sq(
                player.position,
                player.floor_level,
                npc.position,
                npc.floor_level,
            )
            .is_some_and(|dist_sq| dist_sq <= MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE);
        if !in_reach {
            Err(NpcReach::TooFar)
        } else if self.is_npc_asleep(&npc.name) {
            Err(NpcReach::Asleep)
        } else {
            Ok(())
        }
    }

    fn priced(&self, stacks: &[ImpoundedStack]) -> Vec<ImpoundedItem> {
        stacks
            .iter()
            .map(|stack| ImpoundedItem {
                item_def_id: stack.item_def_id.clone(),
                quantity: stack.quantity,
                fee: self.stack_fee(stack, stack.quantity),
                enchant: stack.enchant,
            })
            .collect()
    }

    fn stack_fee(&self, stack: &ImpoundedStack, units: u32) -> i64 {
        if !stack.charged {
            return 0;
        }
        let price = self
            .item_defs
            .get(&stack.item_def_id)
            .and_then(|def| def.base_price)
            .unwrap_or(0);
        reclaim_fee(price, units)
    }

    /// How many units of each stack fit the bag and the purse, in order.
    fn affordable(
        &self,
        stacks: &[ImpoundedStack],
        mut room_kg: f32,
        mut gold: i64,
    ) -> Vec<(ImpoundedStack, u32)> {
        let mut taken = Vec::new();
        for stack in stacks {
            let weight = self.item_defs.weight(&stack.item_def_id);
            let by_weight = if weight > 0.0 {
                (room_kg / weight).floor().max(0.0) as u32
            } else {
                u32::MAX
            };
            let mut units = stack.quantity.min(by_weight);
            while units > 0 && self.stack_fee(stack, units) > gold {
                units -= 1;
            }
            if units > 0 {
                room_kg -= weight * units as f32;
                gold -= self.stack_fee(stack, units);
                taken.push((stack.clone(), units));
            }
        }
        taken
    }

    /// The captain opens the reclaim window for a player standing with him.
    pub async fn offer_impound(&self, npc_player_id: &PlayerId, target_id: &PlayerId) {
        let target_name = {
            let players = self.players.read().await;
            if !players.get(npc_player_id).is_some_and(is_guard_captain) {
                return;
            }
            players.get(target_id).map(|target| target.name.clone())
        };
        let (to, message) = if self.at_guard_captain(target_id, npc_player_id).await {
            let npc_player_id = *npc_player_id;
            (target_id, ServerMessage::ImpoundOffered { npc_player_id })
        } else {
            let message = match target_name {
                Some(name) => format!(
                    "[Impound] {name} is not standing with you; ask them to come closer first."
                ),
                None => "[Impound] That visitor is no longer here.".to_string(),
            };
            let localization = None;
            (
                npc_player_id,
                ServerMessage::SystemMessage {
                    message,
                    localization,
                },
            )
        };
        self.send_direct_message(to, message).await;
    }

    pub async fn impound_action(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        reclaim: bool,
        auth: &AuthService,
    ) {
        let result = self
            .try_impound_action(player_id, npc_player_id, reclaim, auth)
            .await;
        let (items, error) = match result {
            Ok(items) => (items, None),
            Err(reason) => (Vec::new(), Some(reason.to_string())),
        };
        let reclaimed = reclaim && error.is_none() && items.is_empty();
        self.send_direct_message(
            player_id,
            ServerMessage::ImpoundState {
                npc_player_id: *npc_player_id,
                reclaimed,
                items,
                error,
            },
        )
        .await;
    }

    async fn try_impound_action(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        reclaim: bool,
        auth: &AuthService,
    ) -> Result<Vec<ImpoundedItem>, &'static str> {
        if !self.at_guard_captain(player_id, npc_player_id).await {
            return Err("Visit the Captain of the Guard to reclaim seized goods.");
        }
        let character_id = self
            .character_id_of(player_id)
            .await
            .ok_or("Character not found.")?;
        let read = |db: AuthService| async move {
            auth_db(move || db.impounded_items(character_id))
                .await
                .map_err(|error| {
                    tracing::warn!(%error, "Failed to read impound");
                    "Impound records are unavailable."
                })
        };
        if !reclaim {
            return Ok(self.priced(&read(auth.clone()).await?));
        }
        let _persistence = self.persistence_lock.lock().await;
        let stacks = read(auth.clone()).await?;
        if stacks.is_empty() {
            return Err("Nothing of yours is held here.");
        }
        if self.reject_if_trading(player_id, "reclaim goods").await {
            return Err("Finish your player trade first.");
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or("Character not found.")?;
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let carried = {
            let inventories = self.inventories.read().await;
            let inventory = inventories.get(player_id).ok_or("Inventory not found.")?;
            self.calc_total_weight(inventory, armor_mult)
        };
        let purse = *self
            .player_gold
            .read()
            .await
            .get(player_id)
            .ok_or("Gold balance not found.")?;
        let taken = self.affordable(&stacks, max_weight - carried, purse);
        if taken.is_empty() {
            return Err("You cannot carry or pay for any of it yet.");
        }
        let fee: i64 = taken
            .iter()
            .map(|(stack, units)| self.stack_fee(stack, *units))
            .sum();
        let mut inserts = Vec::with_capacity(taken.len());
        for (stack, units) in &taken {
            let stackable = self.item_defs.stackable(&stack.item_def_id);
            let mut insert = BagInsert::one(stackable, &stack.item_def_id, stack.enchant, 0);
            insert.locked = stack.locked;
            insert.cape_color = stack.cape_color.clone();
            insert.cape_texture = stack.cape_texture.clone();
            insert.quantity = *units;
            insert.first_instance_id = self.reserve_instance_ids(insert.ids_reserved()).await;
            inserts.push(insert);
        }
        let rows = {
            let inventories = self.inventories.read().await;
            let inventory = inventories.get(player_id).ok_or("Inventory not found.")?;
            rows_with_inserts(inventory, &inserts)
        };
        {
            let mut gold = self.player_gold.write().await;
            let balance = gold.get_mut(player_id).ok_or("Gold balance not found.")?;
            if *balance < fee {
                return Err("You do not have enough gold.");
            }
            *balance -= fee;
            character.gold = *balance;
        }
        let db = auth.clone();
        let handed = taken.clone();
        let outcome = auth_db(move || db.reclaim_impounds(&character, &rows, &handed))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Failed to reclaim impound");
                "The reclaim could not be saved. Nothing was charged."
            })
            .flatten();
        if outcome.is_err() {
            self.adjust_gold(player_id, fee).await;
        }
        self.finish_bag_write(player_id, BagHold::default(), &inserts, outcome)
            .await?;
        for (stack, units) in &taken {
            let paid = self.stack_fee(stack, *units);
            if paid > 0 {
                let item_def_id = stack.item_def_id.clone();
                self.record_gold_sink(GoldSink::ImpoundFee { item_def_id }, *units, paid)
                    .await;
            }
        }
        self.mark_dirty(player_id).await;
        self.send_gold_update(player_id).await;
        tracing::info!(?player_id, fee, "impound reclaimed");
        drop(_persistence);
        Ok(self.priced(&read(auth.clone()).await?))
    }
}
