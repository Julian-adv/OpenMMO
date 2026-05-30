//! `wasm-bindgen` exports consumed by the web client. Wraps three
//! crate-internal subsystems behind a JS-friendly surface:
//! - the passability cache (per-house geometry + door state),
//! - A* pathfinding queries against that cache, and
//! - the monster-AI brain registry that drives in-browser NPCs.
//!
//! State lives in `thread_local!` cells so each WASM worker has its own
//! cache; JS-facing functions are named `passability_*` / `ai_*` so the
//! TypeScript wrappers can group them by subsystem.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::housing;
use crate::messages::{deserialize_server_msg, serialize_client_msg, ClientMessage};
use crate::monster_ai::{self, AiTemplate, MonsterBrain, NearbyPlayer};
use crate::pathfinding::{self, PassabilityCache};
use crate::world::Position;

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
    serialize_client_msg(&msg).map_err(|e| JsError::new(&format!("Serialization failed: {e}")))
}

#[wasm_bindgen]
pub fn deserialize_server_message(bytes: &[u8]) -> Result<JsValue, JsError> {
    let msg = deserialize_server_msg(bytes)
        .map_err(|e| JsError::new(&format!("Deserialization failed: {e}")))?;
    to_js(&msg)
}

/// XP threshold for a given level, as an f64 for JS interop.
/// Saturates at Number.MAX_SAFE_INTEGER for levels beyond safe integer range.
#[wasm_bindgen]
pub fn xp_for_level(level: u32) -> f64 {
    const MAX_SAFE: u64 = (1u64 << 53) - 1;
    let xp = crate::xp::xp_for_level(level);
    xp.min(MAX_SAFE) as f64
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
pub fn passability_is_circle_blocked(x: f32, z: f32, r: f32, y: f32) -> bool {
    with_cache(|c| pathfinding::is_circle_blocked(c, x, z, r, y))
}

#[wasm_bindgen]
pub fn passability_is_cardinal_move_blocked(
    cell_x: i32,
    cell_z: i32,
    dx: i32,
    dz: i32,
    floor_level: u8,
) -> bool {
    with_cache(|c| pathfinding::is_cardinal_move_blocked(c, cell_x, cell_z, dx, dz, floor_level))
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
        walk_speed: f32,
        run_speed: f32,
        template_name: String,
    }

    let args: CreateBrainArgs = serde_wasm_bindgen::from_value(val)
        .map_err(|e| JsError::new(&format!("Invalid brain args: {e}")))?;

    let template = get_template(&args.template_name);

    let brain = MonsterBrain::new(
        args.monster_id.clone(),
        args.monster_type,
        args.template_name,
        args.position,
        args.health,
        args.max_health,
        args.walk_speed,
        args.run_speed,
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

        let template = get_template(&brain.template_name);
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

        let template = get_template(&brain.template_name);
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
