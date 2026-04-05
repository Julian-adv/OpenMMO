use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    #[serde(rename = "head")]
    Head,
    #[serde(rename = "main_hand")]
    MainHand,
    #[serde(rename = "off_hand")]
    OffHand,
    #[serde(rename = "chest")]
    Chest,
    #[serde(rename = "ear")]
    Ear,
    #[serde(rename = "neck")]
    Neck,
    #[serde(rename = "belt")]
    Belt,
    #[serde(rename = "pants")]
    Pants,
    #[serde(rename = "boots")]
    Boots,
    #[serde(rename = "ring")]
    Ring,
    #[serde(rename = "ring_left")]
    RingLeft,
}

impl EquipSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            EquipSlot::Head => "head",
            EquipSlot::MainHand => "main_hand",
            EquipSlot::OffHand => "off_hand",
            EquipSlot::Chest => "chest",
            EquipSlot::Ear => "ear",
            EquipSlot::Neck => "neck",
            EquipSlot::Belt => "belt",
            EquipSlot::Pants => "pants",
            EquipSlot::Boots => "boots",
            EquipSlot::Ring => "ring",
            EquipSlot::RingLeft => "ring_left",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "head" => Some(EquipSlot::Head),
            "main_hand" => Some(EquipSlot::MainHand),
            "off_hand" => Some(EquipSlot::OffHand),
            "chest" => Some(EquipSlot::Chest),
            "ear" => Some(EquipSlot::Ear),
            "neck" => Some(EquipSlot::Neck),
            "belt" => Some(EquipSlot::Belt),
            "pants" => Some(EquipSlot::Pants),
            "boots" => Some(EquipSlot::Boots),
            "ring" => Some(EquipSlot::Ring),
            "ring_left" => Some(EquipSlot::RingLeft),
            _ => None,
        }
    }

    /// For slots that have an alternate (e.g. ring/ring_left),
    /// returns the alternate slot. Used when the primary is occupied.
    pub fn alternate(&self) -> Option<Self> {
        match self {
            EquipSlot::Ring => Some(EquipSlot::RingLeft),
            EquipSlot::RingLeft => Some(EquipSlot::Ring),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemInstance {
    pub instance_id: u64,
    pub item_def_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub bag: Vec<ItemInstance>,
    pub equipped: HashMap<EquipSlot, ItemInstance>,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            bag: Vec::new(),
            equipped: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundItem {
    pub instance_id: u64,
    pub item_def_id: String,
    pub position: Position,
    pub floor_level: i8,
}
