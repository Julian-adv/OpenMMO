use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

pub mod housing;
pub mod monster_ai;
pub mod pathfinding;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CharacterClass {
    #[serde(rename = "warrior")]
    Warrior,
    #[serde(rename = "knight")]
    Knight,
    #[serde(rename = "barbarian")]
    Barbarian,
    #[serde(rename = "caveman")]
    Caveman,
    #[serde(rename = "valkyrie")]
    Valkyrie,
    #[serde(rename = "ranger")]
    Ranger,
    #[serde(rename = "samurai")]
    Samurai,
    #[serde(rename = "monk")]
    Monk,
    #[serde(rename = "priest")]
    Priest,
    #[serde(rename = "thief")]
    Thief,
    #[serde(rename = "archaeologist")]
    Archaeologist,
    #[serde(rename = "healer")]
    Healer,
    #[serde(rename = "rogue")]
    Rogue,
    #[serde(rename = "wizard")]
    Wizard,
    #[serde(rename = "tourist")]
    Tourist,
    #[serde(rename = "merchant")]
    Merchant,
    #[serde(rename = "guard")]
    Guard,
}

impl CharacterClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            CharacterClass::Warrior => "warrior",
            CharacterClass::Knight => "knight",
            CharacterClass::Barbarian => "barbarian",
            CharacterClass::Caveman => "caveman",
            CharacterClass::Valkyrie => "valkyrie",
            CharacterClass::Ranger => "ranger",
            CharacterClass::Samurai => "samurai",
            CharacterClass::Monk => "monk",
            CharacterClass::Priest => "priest",
            CharacterClass::Thief => "thief",
            CharacterClass::Archaeologist => "archaeologist",
            CharacterClass::Healer => "healer",
            CharacterClass::Rogue => "rogue",
            CharacterClass::Wizard => "wizard",
            CharacterClass::Tourist => "tourist",
            CharacterClass::Merchant => "merchant",
            CharacterClass::Guard => "guard",
        }
    }

    pub fn hit_die(&self) -> u8 {
        match self {
            CharacterClass::Warrior
            | CharacterClass::Knight
            | CharacterClass::Barbarian
            | CharacterClass::Caveman
            | CharacterClass::Valkyrie => 10,
            CharacterClass::Ranger
            | CharacterClass::Samurai
            | CharacterClass::Monk
            | CharacterClass::Priest
            | CharacterClass::Thief => 8,
            CharacterClass::Archaeologist
            | CharacterClass::Healer
            | CharacterClass::Rogue
            | CharacterClass::Wizard => 6,
            CharacterClass::Tourist | CharacterClass::Merchant => 4,
            CharacterClass::Guard => 10,
        }
    }

    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "warrior" => CharacterClass::Warrior,
            "barbarian" => CharacterClass::Barbarian,
            "caveman" => CharacterClass::Caveman,
            "valkyrie" => CharacterClass::Valkyrie,
            "ranger" => CharacterClass::Ranger,
            "samurai" => CharacterClass::Samurai,
            "monk" => CharacterClass::Monk,
            "priest" => CharacterClass::Priest,
            "thief" => CharacterClass::Thief,
            "archaeologist" => CharacterClass::Archaeologist,
            "healer" => CharacterClass::Healer,
            "rogue" => CharacterClass::Rogue,
            "wizard" => CharacterClass::Wizard,
            "tourist" => CharacterClass::Tourist,
            "merchant" => CharacterClass::Merchant,
            "guard" => CharacterClass::Guard,
            _ => CharacterClass::Knight,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Axis-aligned rectangular zone where monsters must not spawn (e.g. towns).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoSpawnZone {
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
}

impl NoSpawnZone {
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub position: Position,
    pub rotation: f32,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub class: CharacterClass,
    #[serde(default)]
    pub is_npc: bool,
    #[serde(default)]
    pub torch_on: bool,
    #[serde(default)]
    pub floor_level: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furniture_type: Option<String>,
    #[serde(skip)]
    pub furniture_id: Option<u32>,
    #[serde(skip)]
    pub last_combat_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonsterState {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "walk")]
    Walk,
    #[serde(rename = "run")]
    Run,
    #[serde(rename = "attack")]
    Attack,
    #[serde(rename = "hit")]
    Hit,
    #[serde(rename = "dead")]
    Dead,
}

impl fmt::Display for MonsterState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Walk => write!(f, "walk"),
            Self::Run => write!(f, "run"),
            Self::Attack => write!(f, "attack"),
            Self::Hit => write!(f, "hit"),
            Self::Dead => write!(f, "dead"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    pub id: String,
    pub monster_type: String,
    pub position: Position,
    pub rotation: f32,
    pub state: MonsterState,
    pub owner_id: Option<String>,
    pub health: u32,
    pub max_health: u32,
    #[serde(skip)]
    pub last_attack_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub level: u32,
    pub xp: u64,
    pub max_hp: u32,
    pub attributes: CharacterAttributes,
    pub class: CharacterClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAttributes {
    pub r#str: u8,
    pub dex: u8,
    pub con: u8,
    pub int: u8,
    pub wis: u8,
    pub cha: u8,
    pub guard: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDateTime {
    pub year: u32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Authenticate {
        account_name: String,
        password_hash: String,
        create_account: bool,
        #[serde(default)]
        is_npc: bool,
    },
    CreateCharacter {
        character_name: String,
        character_class: CharacterClass,
    },
    RollCharacterStats,
    DeleteCharacter {
        character_id: i64,
    },
    EnterGame {
        character_id: i64,
    },
    PlayerMove {
        position: Position,
        rotation: f32,
        #[serde(default)]
        floor_level: i8,
    },
    ChatMessage {
        message: String,
    },
    RequestSpawnMonster {
        monster_type: String,
        position: Position,
        rotation: f32,
    },
    MonsterMove {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
    },
    PlayerAttack {
        monster_id: String,
    },
    MonsterAttack {
        monster_id: String,
        target_player_id: String,
    },
    RequestRespawn,
    DebugTeleport {
        position: Position,
    },
    TorchToggle {
        enabled: bool,
    },
    InteractFurniture {
        furniture_type: String,
        furniture_id: u32,
    },
    StopInteraction,
    Heartbeat,
    PlaceHouse {
        house: housing::HouseData,
    },
    ModifyRoom {
        house_id: String,
        room_index: u32,
        room: housing::RoomData,
    },
    RemoveHouse {
        house_id: String,
    },
    ToggleDoor {
        house_id: String,
        room_index: u32,
        wall_dir: housing::WallDirection,
        segment_index: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    AuthSuccess {
        account_name: String,
        characters: Vec<Character>,
    },
    JoinSuccess {
        player: Player,
    },
    AuthError {
        message: String,
    },
    CharacterCreated {
        character: Character,
    },
    CharacterStatsRolled {
        attributes: CharacterAttributes,
        max_hp: u32,
    },
    CharacterDeleted {
        character_id: i64,
    },
    CharacterError {
        message: String,
    },
    PlayerJoined {
        player: Player,
    },
    PlayerLeft {
        player_id: String,
    },
    PlayerMoved {
        player_id: String,
        position: Position,
        rotation: f32,
    },
    PlayerTeleported {
        player_id: String,
        position: Position,
        rotation: f32,
    },
    ChatMessage {
        player_id: String,
        message: String,
    },
    GameState {
        players: HashMap<String, Player>,
        monsters: HashMap<String, Monster>,
    },
    GameTimeSync {
        datetime: GameDateTime,
        is_night: bool,
    },
    MonsterSpawned {
        monster: Monster,
    },
    /// Server assigns a monster to this client for AI control.
    MonsterAssigned {
        monster: Monster,
    },
    /// Server requests this client to spawn a monster within the given area.
    /// Client should find a valid position (avoiding water, interiors, cliffs)
    /// and reply with RequestSpawnMonster.
    SpawnMonsterRequest {
        monster_type: String,
        min_x: f32,
        min_z: f32,
        max_x: f32,
        max_z: f32,
    },
    MonsterMoved {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
        owner_id: Option<String>,
    },
    MonsterRemoved {
        monster_id: String,
    },
    MonsterDead {
        monster_id: String,
    },
    PlayerAttacked {
        player_id: String,
        monster_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
    },
    MonsterAttackedPlayer {
        monster_id: String,
        player_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
        current_health: u32,
    },
    PlayerDead {
        player_id: String,
    },
    PlayerRespawned {
        player: Player,
    },
    PlayerHealthUpdate {
        player_id: String,
        health: u32,
        max_health: u32,
    },
    XpGained {
        player_id: String,
        xp_amount: u32,
        xp_lost: u64,
        total_xp: u64,
        new_level: u32,
        leveled_up: bool,
        max_hp: u32,
        current_hp: u32,
    },
    Kicked {
        player_id: String,
        reason: String,
    },
    PlayerTorchToggled {
        player_id: String,
        enabled: bool,
    },
    PlayerInteractionChanged {
        player_id: String,
        furniture_type: Option<String>,
    },
    InteractionRejected {
        reason: String,
    },
    HouseSpawned {
        house: housing::HouseData,
    },
    HouseUpdated {
        house: housing::HouseData,
    },
    HouseRemoved {
        house_id: String,
    },
    HousesInArea {
        houses: Vec<housing::HouseData>,
    },
    DoorToggled {
        house_id: String,
        room_index: u32,
        wall_dir: housing::WallDirection,
        segment_index: u32,
        is_open: bool,
    },
    /// Sent once on join: all no-spawn zones so the client can validate spawn positions.
    NoSpawnZones {
        zones: Vec<NoSpawnZone>,
    },
}

pub type PlayerId = String;

// Serialization helpers (used by both server and wasm)
pub fn serialize_client_msg(msg: &ClientMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

pub fn deserialize_client_msg(bytes: &[u8]) -> Result<ClientMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

pub fn serialize_server_msg(msg: &ServerMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

pub fn deserialize_server_msg(bytes: &[u8]) -> Result<ServerMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

#[cfg(target_arch = "wasm32")]
mod wasm_api {
    use super::*;
    use serde::Serialize;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use wasm_bindgen::prelude::*;

    use crate::pathfinding::{self, PassabilityCache};

    fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        value
            .serialize(&serializer)
            .map_err(|e| JsError::new(&format!("JS conversion failed: {e}")))
    }

    #[wasm_bindgen]
    pub fn serialize_client_message(val: JsValue) -> Result<Vec<u8>, JsError> {
        let msg: ClientMessage = serde_wasm_bindgen::from_value(val)
            .map_err(|e| JsError::new(&format!("Invalid client message: {e}")))?;
        rmp_serde::to_vec(&msg).map_err(|e| JsError::new(&format!("Serialization failed: {e}")))
    }

    #[wasm_bindgen]
    pub fn deserialize_server_message(bytes: &[u8]) -> Result<JsValue, JsError> {
        let msg: ServerMessage = rmp_serde::from_slice(bytes)
            .map_err(|e| JsError::new(&format!("Deserialization failed: {e}")))?;
        to_js(&msg)
    }

    // --- Passability cache (WASM global state) ---

    thread_local! {
        static PASSABILITY_CACHE: RefCell<PassabilityCache> = RefCell::new(HashMap::new());
    }

    fn with_cache<R>(f: impl FnOnce(&PassabilityCache) -> R) -> R {
        PASSABILITY_CACHE.with(|c| f(&c.borrow()))
    }

    fn with_cache_mut<R>(f: impl FnOnce(&mut PassabilityCache) -> R) -> R {
        PASSABILITY_CACHE.with(|c| f(&mut c.borrow_mut()))
    }

    #[wasm_bindgen]
    pub fn passability_add_house(val: JsValue) -> Result<(), JsError> {
        let house: housing::HouseData = serde_wasm_bindgen::from_value(val)
            .map_err(|e| JsError::new(&format!("Invalid HouseData: {e}")))?;
        let rp = pathfinding::build_runtime_passability(&house);
        with_cache_mut(|c| {
            c.insert(house.id.clone(), rp);
            pathfinding::apply_door_overlays(c, &house);
        });
        Ok(())
    }

    #[wasm_bindgen]
    pub fn passability_remove_house(house_id: &str) {
        with_cache_mut(|c| c.remove(house_id));
    }

    #[wasm_bindgen]
    pub fn passability_update_door(
        house_id: &str,
        room_val: JsValue,
        wall_dir_val: JsValue,
        segment_index: u32,
        is_open: bool,
    ) -> Result<(), JsError> {
        let room: housing::RoomData = serde_wasm_bindgen::from_value(room_val)
            .map_err(|e| JsError::new(&format!("Invalid RoomData: {e}")))?;
        let wall_dir: housing::WallDirection = serde_wasm_bindgen::from_value(wall_dir_val)
            .map_err(|e| JsError::new(&format!("Invalid WallDirection: {e}")))?;
        with_cache_mut(|c| {
            pathfinding::update_door_edge(
                c,
                house_id,
                &room,
                wall_dir,
                segment_index as usize,
                is_open,
            );
        });
        Ok(())
    }

    #[wasm_bindgen]
    pub fn passability_find_path(
        start_x: f32,
        start_z: f32,
        start_floor: u8,
        goal_x: f32,
        goal_z: f32,
        goal_floor: u8,
    ) -> Result<JsValue, JsError> {
        let result = with_cache(|c| {
            pathfinding::find_and_smooth_path(
                start_x,
                start_z,
                start_floor,
                goal_x,
                goal_z,
                goal_floor,
                c,
                pathfinding::DEFAULT_MAX_NODES,
            )
        });
        to_js(&PathResultJs {
            waypoints: result
                .waypoints
                .iter()
                .map(|w| WaypointJs {
                    x: w.x,
                    z: w.z,
                    floor: w.floor,
                })
                .collect(),
            found: result.found,
        })
    }

    #[wasm_bindgen]
    pub fn passability_is_movement_blocked(
        from_x: f32,
        from_z: f32,
        to_x: f32,
        to_z: f32,
        y: f32,
    ) -> bool {
        with_cache(|c| pathfinding::is_movement_blocked(c, from_x, from_z, to_x, to_z, y))
    }

    #[wasm_bindgen]
    pub fn passability_is_cardinal_move_blocked(
        cell_x: i32,
        cell_z: i32,
        dx: i32,
        dz: i32,
        floor_level: u8,
    ) -> bool {
        with_cache(|c| {
            pathfinding::is_cardinal_move_blocked(c, cell_x, cell_z, dx, dz, floor_level)
        })
    }

    #[wasm_bindgen]
    pub fn passability_get_floor_at(x: f32, z: f32, y: f32) -> u8 {
        with_cache(|c| pathfinding::get_floor_at_position(c, x, z, y))
    }

    #[wasm_bindgen]
    pub fn passability_get_floor_y_base(x: f32, z: f32, floor_level: u8) -> f32 {
        with_cache(|c| pathfinding::get_floor_y_base(c, x, z, floor_level).unwrap_or(f32::NAN))
    }

    #[wasm_bindgen]
    pub fn passability_debug_info() -> Result<JsValue, JsError> {
        with_cache(|c| {
            let entries: Vec<String> = c.iter().map(|(id, rp)| {
                let total_cells: usize = rp.floors.iter().map(|f| f.cells.len()).sum();
                let non_zero: usize = rp.floors.iter()
                    .flat_map(|f| f.cells.iter())
                    .filter(|&&b| b != 0)
                    .count();
                format!(
                    "{}: origin=({:.1},{:.1}) aabb=({:.1},{:.1})→({:.1},{:.1}) floors={} stairwells={} cells={} non_zero={}",
                    id, rp.house_origin_x, rp.house_origin_z,
                    rp.min_x, rp.min_z, rp.max_x, rp.max_z,
                    rp.floors.len(), rp.stairwells.len(),
                    total_cells, non_zero
                )
            }).collect();
            to_js(&entries)
        })
    }

    // Serializable types for WASM return values
    #[derive(Serialize)]
    struct WaypointJs {
        x: f32,
        z: f32,
        floor: u8,
    }

    #[derive(Serialize)]
    struct PathResultJs {
        waypoints: Vec<WaypointJs>,
        found: bool,
    }

    // --- Monster AI WASM bindings ---

    use crate::monster_ai::{self, AiTemplate, MonsterBrain, NearbyPlayer};

    thread_local! {
        static MONSTER_BRAINS: RefCell<HashMap<String, MonsterBrain>> = RefCell::new(HashMap::new());
        static AI_TEMPLATES: RefCell<HashMap<String, AiTemplate>> = RefCell::new(HashMap::new());
    }

    fn get_template(monster_type: &str) -> AiTemplate {
        AI_TEMPLATES.with(|t| t.borrow().get(monster_type).cloned().unwrap_or_default())
    }

    struct WasmPathProvider;
    impl monster_ai::PathProvider for WasmPathProvider {
        fn find_path(
            &self,
            start_x: f32,
            start_z: f32,
            start_floor: u8,
            goal_x: f32,
            goal_z: f32,
            goal_floor: u8,
        ) -> pathfinding::PathResult {
            with_cache(|c| {
                pathfinding::find_and_smooth_path(
                    start_x,
                    start_z,
                    start_floor,
                    goal_x,
                    goal_z,
                    goal_floor,
                    c,
                    pathfinding::DEFAULT_MAX_NODES,
                )
            })
        }
    }

    #[wasm_bindgen]
    pub fn ai_load_templates(json: &str) -> Result<(), JsError> {
        let templates = monster_ai::load_templates(json)
            .map_err(|e| JsError::new(&format!("Failed to parse AI templates: {e}")))?;
        AI_TEMPLATES.with(|t| *t.borrow_mut() = templates);
        Ok(())
    }

    #[wasm_bindgen]
    pub fn ai_create_brain(val: JsValue) -> Result<(), JsError> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreateBrainArgs {
            monster_id: String,
            monster_type: String,
            position: Position,
            health: u32,
            max_health: u32,
            template_name: String,
        }

        let args: CreateBrainArgs = serde_wasm_bindgen::from_value(val)
            .map_err(|e| JsError::new(&format!("Invalid brain args: {e}")))?;

        let template = get_template(&args.template_name);

        let brain = MonsterBrain::new(
            args.monster_id.clone(),
            args.monster_type,
            args.position,
            args.health,
            args.max_health,
            &template,
        );

        MONSTER_BRAINS.with(|b| b.borrow_mut().insert(args.monster_id, brain));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn ai_remove_brain(monster_id: &str) {
        MONSTER_BRAINS.with(|b| b.borrow_mut().remove(monster_id));
    }

    #[wasm_bindgen]
    pub fn ai_tick_brain(
        monster_id: &str,
        delta_ms: f32,
        nearby_players: JsValue,
    ) -> Result<JsValue, JsError> {
        let players: Vec<NearbyPlayer> = serde_wasm_bindgen::from_value(nearby_players)
            .map_err(|e| JsError::new(&format!("Invalid nearby_players: {e}")))?;

        let result = MONSTER_BRAINS.with(|brains| {
            let mut brains = brains.borrow_mut();
            let brain = match brains.get_mut(monster_id) {
                Some(b) => b,
                None => return None,
            };

            let template = get_template(&brain.monster_type);
            let mut rng = rand::thread_rng();
            Some(brain.tick(delta_ms, &players, &template, &WasmPathProvider, &mut rng))
        });

        match result {
            Some(r) => to_js(&r),
            None => to_js(&serde_json::Value::Null),
        }
    }

    #[wasm_bindgen]
    pub fn ai_handle_hit(
        monster_id: &str,
        attacker_id: &str,
        hit: bool,
        damage: u32,
    ) -> Result<JsValue, JsError> {
        let commands = MONSTER_BRAINS.with(|brains| {
            let mut brains = brains.borrow_mut();
            let brain = match brains.get_mut(monster_id) {
                Some(b) => b,
                None => return vec![],
            };

            let template = get_template(&brain.monster_type);
            let mut rng = rand::thread_rng();
            brain.handle_hit(
                attacker_id,
                hit,
                damage,
                &template,
                &WasmPathProvider,
                &mut rng,
            )
        });

        to_js(&commands)
    }

    #[wasm_bindgen]
    pub fn ai_handle_death(monster_id: &str) {
        MONSTER_BRAINS.with(|brains| {
            if let Some(brain) = brains.borrow_mut().get_mut(monster_id) {
                brain.handle_death();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_client_message() {
        let msg = ClientMessage::PlayerMove {
            position: Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            rotation: 1.5,
            floor_level: 1,
        };
        let bytes = serialize_client_msg(&msg).unwrap();
        let decoded = deserialize_client_msg(&bytes).unwrap();
        match decoded {
            ClientMessage::PlayerMove {
                position,
                rotation,
                floor_level,
            } => {
                assert_eq!(position.x, 1.0);
                assert_eq!(position.y, 2.0);
                assert_eq!(position.z, 3.0);
                assert_eq!(rotation, 1.5);
                assert_eq!(floor_level, 1);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn roundtrip_unit_variant() {
        let msg = ClientMessage::RequestRespawn;
        let bytes = serialize_client_msg(&msg).unwrap();
        let decoded = deserialize_client_msg(&bytes).unwrap();
        assert!(matches!(decoded, ClientMessage::RequestRespawn));
    }

    #[test]
    fn roundtrip_server_message_with_hashmap() {
        let mut players = HashMap::new();
        players.insert(
            "p1".to_string(),
            Player {
                id: "p1".to_string(),
                name: "Test".to_string(),
                position: Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation: 0.0,
                level: 1,
                health: 10,
                max_health: 10,
                class: CharacterClass::Knight,
                torch_on: false,
                floor_level: 0,
                last_combat_at: 0,
            },
        );
        let msg = ServerMessage::GameState {
            players,
            monsters: HashMap::new(),
        };
        let bytes = serialize_server_msg(&msg).unwrap();
        let decoded = deserialize_server_msg(&bytes).unwrap();
        match decoded {
            ServerMessage::GameState {
                players, monsters, ..
            } => {
                assert!(players.contains_key("p1"));
                assert!(monsters.is_empty());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn roundtrip_all_server_messages() {
        let messages = vec![
            ServerMessage::AuthError {
                message: "bad".to_string(),
            },
            ServerMessage::PlayerLeft {
                player_id: "p1".to_string(),
            },
            ServerMessage::MonsterDead {
                monster_id: "m1".to_string(),
            },
            ServerMessage::PlayerAttacked {
                player_id: "p1".to_string(),
                monster_id: "m1".to_string(),
                hit: true,
                roll: 18,
                damage: 5,
            },
            ServerMessage::Kicked {
                player_id: "p1".to_string(),
                reason: "test".to_string(),
            },
        ];
        for msg in messages {
            let bytes = serialize_server_msg(&msg).unwrap();
            let decoded = deserialize_server_msg(&bytes).unwrap();
            // Just verify it roundtrips without error
            assert!(format!("{:?}", decoded).len() > 0);
        }
    }
}
