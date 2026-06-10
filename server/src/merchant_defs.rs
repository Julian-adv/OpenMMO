use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::info;

/// The data file stores the catalog as a semicolon-separated string; parse it
/// once at load so request handlers never re-split it.
fn parse_catalog<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(raw
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MerchantDefinition {
    pub id: String,
    #[serde(rename = "npcName")]
    pub npc_name: String,
    /// Percentage of base price the merchant pays when a player sells.
    #[serde(rename = "sellRatePercent")]
    pub sell_rate_percent: u32,
    /// Item def ids the merchant sells (unlimited stock).
    #[serde(deserialize_with = "parse_catalog")]
    pub catalog: Vec<String>,
}

impl MerchantDefinition {
    pub fn sells(&self, item_def_id: &str) -> bool {
        self.catalog.iter().any(|id| id == item_def_id)
    }
}

/// Merchant definitions keyed by NPC name. NPCs are agent-controlled players,
/// so the stable identity the server sees is the character name.
pub struct MerchantDefs {
    by_npc_name: HashMap<String, MerchantDefinition>,
}

impl MerchantDefs {
    fn load() -> Self {
        let data = include_str!("../../data/merchants.json");
        let by_id: HashMap<String, MerchantDefinition> =
            serde_json::from_str(data).expect("Failed to parse merchants.json");

        info!("Loaded {} merchant definition(s)", by_id.len());
        let by_npc_name = by_id
            .into_values()
            .map(|def| (def.npc_name.clone(), def))
            .collect();

        Self { by_npc_name }
    }

    pub fn get_by_npc_name(&self, npc_name: &str) -> Option<&MerchantDefinition> {
        self.by_npc_name.get(npc_name)
    }
}

pub fn merchant_defs() -> &'static MerchantDefs {
    static DEFS: OnceLock<MerchantDefs> = OnceLock::new();
    DEFS.get_or_init(MerchantDefs::load)
}
