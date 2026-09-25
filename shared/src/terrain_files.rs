use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainFile {
    pub path: String,
    pub hash: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainFiles {
    pub height: Option<TerrainFile>,
    pub splat: Option<TerrainFile>,
    pub trees: Option<TerrainFile>,
    pub grass: Option<TerrainFile>,
    pub landscape: Option<TerrainFile>,
}
