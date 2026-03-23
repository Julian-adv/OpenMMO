pub mod routes;

use onlinerpg_shared::housing::{HouseData, RoomData, RoomType};
use std::path::PathBuf;
use tokio::fs;
use tracing::{error, info};

const MIN_ROOM_SIZE: u8 = 3;
const MAX_ROOM_SIZE: u8 = 6;
const MAX_FLOOR_LEVEL: u8 = 3;

/// Validate a house before saving. Returns Ok(()) or an error message.
pub fn validate_house(house: &HouseData, neighbors: &[HouseData]) -> Result<(), String> {
    if house.rooms.is_empty() {
        return Err("House must have at least one room".into());
    }

    for (i, room) in house.rooms.iter().enumerate() {
        // Room size constraints (stairwells allow smaller sizes)
        let min_size = if room.room_type == RoomType::Stairwell {
            1
        } else {
            MIN_ROOM_SIZE
        };
        if room.size_x < min_size || room.size_x > MAX_ROOM_SIZE {
            return Err(format!(
                "Room {} size_x ({}) must be {}-{}",
                i, room.size_x, min_size, MAX_ROOM_SIZE
            ));
        }
        if room.size_z < min_size || room.size_z > MAX_ROOM_SIZE {
            return Err(format!(
                "Room {} size_z ({}) must be {}-{}",
                i, room.size_z, min_size, MAX_ROOM_SIZE
            ));
        }
        if room.wall_height <= 0.0 || room.wall_height > 10.0 {
            return Err(format!(
                "Room {} wall_height ({}) must be 0-10",
                i, room.wall_height
            ));
        }
        if room.floor_level > MAX_FLOOR_LEVEL {
            return Err(format!(
                "Room {} floor_level ({}) must be 0-{}",
                i, room.floor_level, MAX_FLOOR_LEVEL
            ));
        }
        // Wall segment counts must match room dimensions
        let sx = room.size_x as usize;
        let sz = room.size_z as usize;
        if room.wall_north.len() != sx
            || room.wall_south.len() != sx
            || room.wall_east.len() != sz
            || room.wall_west.len() != sz
        {
            return Err(format!(
                "Room {} wall segment counts don't match room size",
                i
            ));
        }
    }

    // Check internal room-room overlap (stairwells may overlap normal rooms)
    for i in 0..house.rooms.len() {
        for j in (i + 1)..house.rooms.len() {
            let a = &house.rooms[i];
            let b = &house.rooms[j];
            if a.room_type == RoomType::Stairwell || b.room_type == RoomType::Stairwell {
                continue;
            }
            if rooms_overlap_world(a, 0.0, 0.0, b, 0.0, 0.0) {
                return Err(format!("Rooms {} and {} overlap", i, j));
            }
        }
    }

    // Check overlap with neighboring houses
    for neighbor in neighbors {
        if neighbor.id == house.id {
            continue;
        }
        for (i, room) in house.rooms.iter().enumerate() {
            for (_j, other) in neighbor.rooms.iter().enumerate() {
                if rooms_overlap_world(
                    room,
                    house.origin.x,
                    house.origin.z,
                    other,
                    neighbor.origin.x,
                    neighbor.origin.z,
                ) {
                    return Err(format!("Room {} overlaps with a neighboring house", i));
                }
            }
        }
    }

    // Validate upper-floor rooms have full floor support from the floor below
    for (i, room) in house.rooms.iter().enumerate() {
        if room.floor_level == 0 {
            continue;
        }
        let support_level = room.floor_level - 1;
        let rx = room.local_x;
        let rz = room.local_z;
        // Check each 1m² cell is supported by a room on the floor below
        for x in rx..(rx + room.size_x as i32) {
            for z in rz..(rz + room.size_z as i32) {
                let supported = house.rooms.iter().any(|other| {
                    other.floor_level == support_level
                        && x >= other.local_x
                        && x < other.local_x + other.size_x as i32
                        && z >= other.local_z
                        && z < other.local_z + other.size_z as i32
                });
                if !supported {
                    return Err(format!(
                        "Room {} ({}F) at ({},{}) has no floor support from {}F",
                        i,
                        room.floor_level + 1,
                        x,
                        z,
                        support_level + 1
                    ));
                }
            }
        }
    }

    Ok(())
}

fn rooms_overlap_world(
    a: &RoomData,
    a_ox: f32,
    a_oz: f32,
    b: &RoomData,
    b_ox: f32,
    b_oz: f32,
) -> bool {
    let ax = a_ox + a.local_x as f32;
    let az = a_oz + a.local_z as f32;
    let bx = b_ox + b.local_x as f32;
    let bz = b_oz + b.local_z as f32;
    ax < bx + b.size_x as f32
        && ax + a.size_x as f32 > bx
        && az < bz + b.size_z as f32
        && az + a.size_z as f32 > bz
        && a.floor_level == b.floor_level
}

/// File-based housing storage, organized by terrain chunk.
///
/// Layout: `{base_dir}/{chunk_x}_{chunk_z}/{house_id}.json`
///
/// Chunk coordinates are derived from house origin using CHUNK_SIZE.
#[derive(Clone)]
pub struct HousingIO {
    base_dir: PathBuf,
}

/// Chunk size in world units — matches terrain tile size.
pub(crate) const CHUNK_SIZE: f32 = 64.0;

fn chunk_prefix(cx: i32, cz: i32) -> String {
    format!("r{:+03}_{:+03}", cx, cz)
}

/// Generate the next house ID from already-loaded houses: `r{cx}_{cz}_{n}`
pub fn next_house_id(cx: i32, cz: i32, existing: &[HouseData]) -> String {
    let prefix = format!("{}_", chunk_prefix(cx, cz));
    let max_n = existing
        .iter()
        .filter_map(|h| h.id.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{}{}", prefix, max_n + 1)
}

/// Parse chunk coordinates from a house ID (e.g. `r-24_+07_1` → Some((-24, 7)))
pub fn parse_chunk_from_id(id: &str) -> Option<(i32, i32)> {
    let s = id.strip_prefix('r')?;
    let mut parts = s.splitn(3, '_');
    let cx = parts.next()?.parse::<i32>().ok()?;
    let cz = parts.next()?.parse::<i32>().ok()?;
    Some((cx, cz))
}

pub fn world_to_chunk(x: f32, z: f32) -> (i32, i32) {
    (
        (x / CHUNK_SIZE).floor() as i32,
        (z / CHUNK_SIZE).floor() as i32,
    )
}

impl HousingIO {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn chunk_dir(&self, cx: i32, cz: i32) -> PathBuf {
        self.base_dir.join(chunk_prefix(cx, cz))
    }

    fn house_path(&self, cx: i32, cz: i32, house_id: &str) -> PathBuf {
        self.chunk_dir(cx, cz).join(format!("{}.json", house_id))
    }

    /// Read all houses in a chunk.
    pub async fn read_chunk(&self, cx: i32, cz: i32) -> std::io::Result<Vec<HouseData>> {
        let dir = self.chunk_dir(cx, cz);
        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };

        let mut houses = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                match fs::read_to_string(&path).await {
                    Ok(content) => match serde_json::from_str::<HouseData>(&content) {
                        Ok(house) => houses.push(house),
                        Err(e) => error!("Failed to parse house file {:?}: {}", path, e),
                    },
                    Err(e) => error!("Failed to read house file {:?}: {}", path, e),
                }
            }
        }

        Ok(houses)
    }

    /// Save a house to disk. Creates chunk directory if needed.
    pub async fn write_house(&self, house: &HouseData) -> std::io::Result<()> {
        let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
        let dir = self.chunk_dir(cx, cz);
        fs::create_dir_all(&dir).await?;

        let path = self.house_path(cx, cz, &house.id);
        let json = serde_json::to_string_pretty(house)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json).await?;
        info!("Saved house {} to {:?}", house.id, path);
        Ok(())
    }

    /// Delete a house from disk. Returns true if the file existed.
    pub async fn delete_house(&self, house_id: &str, cx: i32, cz: i32) -> std::io::Result<bool> {
        let path = self.house_path(cx, cz, house_id);
        match fs::remove_file(&path).await {
            Ok(()) => {
                info!("Deleted house {} from {:?}", house_id, path);
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Find and read a house by ID. Parses chunk coords from the ID for O(1) lookup.
    pub async fn find_house(&self, house_id: &str) -> std::io::Result<Option<HouseData>> {
        if let Some((cx, cz)) = parse_chunk_from_id(house_id) {
            let path = self.house_path(cx, cz, house_id);
            match fs::read_to_string(&path).await {
                Ok(content) => match serde_json::from_str::<HouseData>(&content) {
                    Ok(house) => return Ok(Some(house)),
                    Err(e) => error!("Failed to parse house {:?}: {}", path, e),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}
