//! Statues of those turned to stone in a dungeon that keeps them
//! (doc/STONE_GARDEN.md 돌이 된 플레이어의 석상).

use std::collections::{HashMap, VecDeque};

use onlinerpg_shared::dungeon::{cell_center, statue_capacity, statue_cells};
use onlinerpg_shared::messages::StatueView;
use tracing::warn;

use super::*;
use crate::auth::{AuthService, StatueRow};

#[derive(Default)]
pub(super) struct StatueBook {
    /// Newest first, per `(entrance, depth)`.
    floors: HashMap<(String, u8), VecDeque<Statue>>,
    next_id: i64,
    unsaved: Vec<StatueRow>,
    removed: Vec<i64>,
}

struct Statue {
    id: i64,
    name: Option<String>,
    look: Player,
    night: i64,
    date: (u32, u8, u8),
}

impl Statue {
    fn from_row(row: &StatueRow) -> Option<Self> {
        let look = serde_json::from_str(&row.look)
            .inspect_err(|e| warn!("Statue {} has an unreadable look: {e}", row.id))
            .ok()?;
        Some(Self {
            id: row.id,
            name: row.name.clone(),
            look,
            night: row.night,
            date: (row.year, row.month, row.day),
        })
    }

    fn view(&self, at: Option<Position>) -> StatueView {
        let mut look = self.look.clone();
        if let Some(at) = at {
            look.position = at;
        }
        StatueView {
            id: self.id,
            name: self.name.clone(),
            look,
            year: self.date.0,
            month: self.date.1,
            day: self.date.2,
        }
    }
}

/// Looks of the excavators nobody remembers, one per class in turn.
static NAMELESS: std::sync::LazyLock<Vec<Player>> = std::sync::LazyLock::new(|| {
    ["knight", "ranger", "barbarian", "samurai"]
        .into_iter()
        .enumerate()
        .filter_map(|(i, class)| {
            serde_json::from_value(serde_json::json!({
                "id": 0,
                "name": "",
                "position": { "x": 0.0, "y": 0.0, "z": 0.0 },
                "rotation": 0.0,
                "level": 1,
                "health": 1,
                "max_health": 0,
                "class": class,
                "gender": if i % 2 == 0 { "male" } else { "female" },
                "petrified": true,
                "armor": {
                    "chest": "leather_armor",
                    "pants": "leather_pants",
                    "boots": "leather_boots",
                },
            }))
            .ok()
        })
        .collect()
});

/// The statue for a cell no player has filled.
fn nameless_statue(slot: usize, at: Position) -> Option<StatueView> {
    let mut look = NAMELESS.get(slot % NAMELESS.len().max(1))?.clone();
    look.position = at;
    look.rotation = slot as f32 * 2.4;
    Some(StatueView {
        id: -(slot as i64) - 1,
        name: None,
        look,
        year: 0,
        month: 0,
        day: 0,
    })
}

impl GameState {
    /// Startup: the statues saved so far.
    pub fn load_statues(&self, rows: &[StatueRow]) {
        let mut book = self.statues.lock().unwrap_or_else(|e| e.into_inner());
        for row in rows {
            book.next_id = book.next_id.max(row.id + 1);
            if let Some(statue) = Statue::from_row(row) {
                book.floors
                    .entry((row.entrance_id.clone(), row.depth))
                    .or_default()
                    .push_front(statue);
            }
        }
    }

    fn tonight(&self) -> i64 {
        Self::night_epoch(self.current_total_game_seconds())
    }

    /// This player died broken as stone: keep them as they stand, crowd out the
    /// floor's oldest statue and show everyone there.
    pub(super) async fn keep_statue_of(&self, player_id: &PlayerId) {
        let Some(mut look) = self.players.read().await.get(player_id).cloned() else {
            return;
        };
        if look.floor_level >= 0 || look.is_official_npc {
            return;
        }
        let depth = look.floor_level.unsigned_abs();
        let Some(entrance) = self
            .dungeon_defs
            .entrance_at(look.position.x, look.position.z)
            .filter(|def| def.keeps_statues)
        else {
            return;
        };
        let Some(capacity) = self
            .dungeons
            .read()
            .await
            .get(&entrance.id)
            .and_then(|rt| rt.layouts.get(usize::from(depth) - 1))
            .map(statue_capacity)
        else {
            return;
        };
        let character_id = self.character_id_of(player_id).await;
        let name = std::mem::take(&mut look.name);
        look.id = PlayerId::from(0);
        look.health = 1;
        look.max_health = 0;
        look.petrified = true;
        look.object_type = None;
        look.object_id = None;
        look.mount = None;
        look.torch_on = false;
        look.radiance_on = false;
        look.raging = false;
        look.wet = false;
        look.title = None;
        let Ok(json) = serde_json::to_string(&look) else {
            return;
        };
        let date = self.current_game_datetime();
        let night = self.tonight();
        {
            let mut book = self.statues.lock().unwrap_or_else(|e| e.into_inner());
            let id = book.next_id.max(1);
            book.next_id = id + 1;
            book.unsaved.push(StatueRow {
                id,
                entrance_id: entrance.id.clone(),
                depth,
                character_id,
                name: Some(name.clone()),
                look: json,
                night,
                year: date.year,
                month: date.month,
                day: date.day,
            });
            let floor = book.floors.entry((entrance.id.clone(), depth)).or_default();
            floor.push_front(Statue {
                id,
                name: Some(name),
                look,
                night,
                date: (date.year, date.month, date.day),
            });
            let crowded: Vec<i64> = floor
                .drain(capacity.min(floor.len())..)
                .map(|s| s.id)
                .collect();
            book.removed.extend(crowded);
        }
        let occupants: Vec<PlayerId> = self
            .dungeons
            .read()
            .await
            .get(&entrance.id)
            .and_then(|rt| rt.floors.get(&depth))
            .map(|floor| floor.players.keys().copied().collect())
            .unwrap_or_default();
        if let Some(message) = self.floor_statues(&entrance.id, depth).await {
            self.send_direct_message_to_players(&occupants, message)
                .await;
        }
    }

    pub(super) async fn send_floor_statues(
        &self,
        player_id: &PlayerId,
        entrance_id: &str,
        depth: u8,
    ) {
        if let Some(message) = self.floor_statues(entrance_id, depth).await {
            self.send_direct_message(player_id, message).await;
        }
    }

    /// The statues standing on a floor. Those turned tonight stand where
    /// they died; older ones and the nameless take the floor's statue cells,
    /// since the maze has changed around them.
    async fn floor_statues(&self, entrance_id: &str, depth: u8) -> Option<ServerMessage> {
        let entrance = self
            .dungeon_defs
            .get(entrance_id)
            .filter(|d| d.keeps_statues)?;
        let cells: Vec<Position> = {
            let dungeons = self.dungeons.read().await;
            let layout = dungeons
                .get(entrance_id)?
                .layouts
                .get(usize::from(depth).checked_sub(1)?)?;
            statue_cells(layout)
                .into_iter()
                .map(|cell| cell_center(&entrance.position(), depth, cell))
                .collect()
        };
        let tonight = self.tonight();
        let statues = {
            let book = self.statues.lock().unwrap_or_else(|e| e.into_inner());
            let kept = book.floors.get(&(entrance_id.to_string(), depth));
            let mut cells = cells.into_iter();
            let mut tonights = 0;
            let mut statues: Vec<StatueView> = kept
                .into_iter()
                .flatten()
                .filter_map(|statue| {
                    if statue.night == tonight {
                        tonights += 1;
                        Some(statue.view(None))
                    } else {
                        cells.next().map(|cell| statue.view(Some(cell)))
                    }
                })
                .collect();
            let free = cells.len().saturating_sub(tonights);
            statues.extend(
                cells
                    .take(free)
                    .enumerate()
                    .filter_map(|(slot, cell)| nameless_statue(slot, cell)),
            );
            statues
        };
        Some(ServerMessage::DungeonStatues {
            entrance_id: entrance_id.to_string(),
            depth,
            statues,
        })
    }

    /// Save statues made since the last flush and drop the crowded-out.
    pub(crate) async fn flush_statues(&self, auth: &AuthService) {
        let (added, removed) = {
            let mut book = self.statues.lock().unwrap_or_else(|e| e.into_inner());
            (
                std::mem::take(&mut book.unsaved),
                std::mem::take(&mut book.removed),
            )
        };
        if added.is_empty() && removed.is_empty() {
            return;
        }
        let db = auth.clone();
        let (rows, ids) = (added.clone(), removed.clone());
        if let Err(err) = super::auth_db(move || db.save_statues(&rows, &ids)).await {
            warn!("Failed to save statues; keeping them for the next flush: {err}");
            let mut book = self.statues.lock().unwrap_or_else(|e| e.into_inner());
            book.unsaved.splice(0..0, added);
            book.removed.splice(0..0, removed);
        }
    }
}
