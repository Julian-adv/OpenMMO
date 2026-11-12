//! Field bosses from `data-src/field_bosses.csv`: where each one rises, what
//! keeps its raid going, and how it is woken (doc/TIER5_FIELD_RAID.md). The
//! sites are placed from this table, not from region object files.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::furniture::FurniturePlacement;
use crate::moon::is_full_moon_night;
use crate::world::{wrap_world_x, GameDateTime};

static FIELD_BOSSES_JSON: &str = include_str!("../../data/field_bosses.json");
static CATALOG_JSON: &str = include_str!("../../data/object_catalog.json");

/// Placement ids from here up belong to field-boss sites, never to a region
/// file, so the editor can neither collide with nor save them.
const PLACEMENT_ID_BASE: u32 = 2_000_000_000;

/// What must hold for a raid to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldBossCondition {
    FullMoon,
}

impl FieldBossCondition {
    pub fn holds(self, at: &GameDateTime) -> bool {
        match self {
            Self::FullMoon => is_full_moon_night(at),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    monster: String,
    condition: Option<FieldBossCondition>,
    x: f32,
    z: f32,
    #[serde(default)]
    rotation: f32,
    dormant_object: Option<String>,
    #[serde(default)]
    dormant_sink: f32,
    altar_object: Option<String>,
    altar_x: Option<f32>,
    altar_z: Option<f32>,
    #[serde(default)]
    altar_rotation: f32,
    #[serde(default)]
    altar_sink: f32,
    offering: Option<String>,
    reward: Option<String>,
    token: Option<String>,
    token_tiers: Option<String>,
}

#[derive(Debug)]
pub struct FieldBossDef {
    pub id: String,
    pub monster: String,
    pub condition: Option<FieldBossCondition>,
    pub x: f32,
    pub z: f32,
    /// Degrees, the placement yaw convention.
    pub rotation: f32,
    pub dormant: Option<DormantDef>,
    pub altar: Option<AltarDef>,
    /// Earned by the raiders of a won raid, one per tier their damage reached.
    pub token: Option<RaidToken>,
    index: u32,
}

/// The rock the boss lies as, sunk `sink` m below the ground at its spot.
#[derive(Debug)]
pub struct DormantDef {
    pub object: String,
    pub sink: f32,
}

/// An altar that wakes the boss when `offering` is laid on it, sunk `sink` m
/// so its downhill edge does not float on a slope.
#[derive(Debug)]
pub struct AltarDef {
    pub object: String,
    pub x: f32,
    pub z: f32,
    pub rotation: f32,
    pub sink: f32,
    pub offering: String,
    /// Granted to the summoner when the raid is won with them in it.
    pub reward: Option<String>,
}

#[derive(Debug)]
pub struct RaidToken {
    pub item: String,
    /// Ascending damage each further token needs.
    pub tiers: Vec<u64>,
}

impl RaidToken {
    pub fn count_for(&self, damage: u64) -> u32 {
        self.tiers.iter().filter(|&&t| damage >= t).count() as u32
    }
}

fn parse_token_tiers(id: &str, text: Option<&str>) -> Vec<u64> {
    let text = text.unwrap_or_else(|| panic!("{id}: a token needs tokenTiers"));
    let tiers: Vec<u64> = text
        .split('|')
        .map(|t| {
            t.trim()
                .parse()
                .unwrap_or_else(|_| panic!("{id}: tokenTiers {text:?}"))
        })
        .collect();
    assert!(
        tiers.first() >= Some(&1) && tiers.is_sorted_by(|a, b| a < b),
        "{id}: tokenTiers must rise from at least 1"
    );
    tiers
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogEntry {
    id: String,
    dormant_monster: Option<String>,
    #[serde(default)]
    altar: bool,
}

fn parse(row: Row, index: u32, catalog: &HashMap<String, CatalogEntry>) -> FieldBossDef {
    let id = row.id;
    let dormant = row.dormant_object.map(|object| {
        let entry = catalog.get(&object);
        assert!(
            entry.and_then(|e| e.dormant_monster.as_deref()) == Some(row.monster.as_str()),
            "{id}: {object} must be a catalog object with dormantMonster {}",
            row.monster
        );
        DormantDef {
            object,
            sink: row.dormant_sink,
        }
    });
    let altar = match (row.altar_object, row.altar_x, row.altar_z, row.offering) {
        (None, None, None, None) => {
            assert!(row.reward.is_none(), "{id}: a reward needs an altar");
            None
        }
        (Some(object), Some(x), Some(z), Some(offering)) => {
            assert!(
                catalog.get(&object).is_some_and(|e| e.altar),
                "{id}: {object} must be a catalog object with altar: true"
            );
            Some(AltarDef {
                object,
                x: wrap_world_x(x),
                z,
                rotation: row.altar_rotation,
                sink: row.altar_sink,
                offering,
                reward: row.reward,
            })
        }
        _ => panic!("{id}: altarObject, altarX, altarZ and offering go together"),
    };
    let token = row.token.map(|item| RaidToken {
        tiers: parse_token_tiers(&id, row.token_tiers.as_deref()),
        item,
    });
    FieldBossDef {
        id,
        monster: row.monster,
        condition: row.condition,
        x: wrap_world_x(row.x),
        z: row.z,
        rotation: row.rotation,
        dormant,
        altar,
        token,
        index,
    }
}

/// Every field boss, ordered by id so placement ids stay stable.
pub fn field_boss_defs() -> &'static [FieldBossDef] {
    static DEFS: OnceLock<Vec<FieldBossDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut rows: Vec<Row> = serde_json::from_str::<HashMap<String, Row>>(FIELD_BOSSES_JSON)
            .expect("field_bosses.json is malformed")
            .into_values()
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let catalog: HashMap<String, CatalogEntry> =
            serde_json::from_str::<Vec<CatalogEntry>>(CATALOG_JSON)
                .expect("object_catalog.json is malformed")
                .into_iter()
                .map(|e| (e.id.clone(), e))
                .collect();
        rows.into_iter()
            .zip(0..)
            .map(|(row, index)| parse(row, index, &catalog))
            .collect()
    })
}

/// The boss whose altar is the placement `id`, with that altar.
pub fn field_boss_by_altar(id: u32) -> Option<(&'static FieldBossDef, &'static AltarDef)> {
    field_boss_defs()
        .iter()
        .filter(|d| d.altar_placement_id() == id)
        .find_map(|d| Some((d, d.altar.as_ref()?)))
}

impl FieldBossDef {
    pub fn altar_placement_id(&self) -> u32 {
        PLACEMENT_ID_BASE + self.index * 2
    }

    pub fn dormant_placement_id(&self) -> u32 {
        PLACEMENT_ID_BASE + self.index * 2 + 1
    }

    pub fn condition_holds(&self, at: &GameDateTime) -> bool {
        self.condition.is_none_or(|c| c.holds(at))
    }

    /// The site's objects, standing on `altar_ground` and `spawn_ground`.
    pub fn placements(&self, altar_ground: f32, spawn_ground: f32) -> Vec<FurniturePlacement> {
        let altar = self.altar.as_ref().map(|a| FurniturePlacement {
            id: self.altar_placement_id(),
            type_id: a.object.clone(),
            x: a.x,
            y: altar_ground - a.sink,
            z: a.z,
            rotation_deg: a.rotation,
            floor_level: 0,
            open_doors: 0,
        });
        let dormant = self.dormant.as_ref().map(|d| FurniturePlacement {
            id: self.dormant_placement_id(),
            type_id: d.object.clone(),
            x: self.x,
            y: spawn_ground - d.sink,
            z: self.z,
            rotation_deg: self.rotation,
            floor_level: 0,
            open_doors: 0,
        });
        altar.into_iter().chain(dormant).collect()
    }
}

pub fn is_field_boss_placement(id: u32) -> bool {
    id >= PLACEMENT_ID_BASE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terraclash_rises_west_of_its_altar_under_a_full_moon() {
        let def = field_boss_defs()
            .iter()
            .find(|d| d.id == "terraclash")
            .expect("terraclash row");
        assert_eq!(def.monster, "terraclash");
        assert_eq!(def.condition, Some(FieldBossCondition::FullMoon));
        let altar = def.altar.as_ref().expect("terraclash altar");
        assert_eq!(altar.offering, "earth_amulet");
        assert_eq!(altar.reward.as_deref(), Some("helm_of_brilliance"));
        let (boss, _) = field_boss_by_altar(def.altar_placement_id()).unwrap();
        assert!(std::ptr::eq(boss, def));
        let placements = def.placements(16.5, 23.4);
        assert_eq!(placements.len(), 2);
        assert!(placements.iter().all(|p| is_field_boss_placement(p.id)));
        assert!((placements[0].y - 16.3).abs() < 1e-4, "altar sunk a little");
        assert!((placements[1].y - 20.5).abs() < 1e-4, "sunk into the slope");
    }
}
