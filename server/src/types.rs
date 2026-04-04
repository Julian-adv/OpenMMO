pub use onlinerpg_shared::{
    Character, CharacterAttributes, CharacterClass, ClientMessage, GameDateTime, Gender, Monster,
    MonsterState, Player, PlayerId, Position, ServerMessage,
};
use uuid::Uuid;

pub fn new_player(
    name: String,
    level: u32,
    max_health: u32,
    class: CharacterClass,
    gender: Gender,
    position: Position,
    rotation: f32,
    is_npc: bool,
) -> Player {
    Player {
        id: Uuid::new_v4().to_string(),
        name,
        position,
        rotation,
        level,
        health: max_health,
        max_health,
        class,
        gender,
        is_npc,
        torch_on: false,
        floor_level: 0,
        furniture_type: None,
        furniture_id: None,
        last_combat_at: 0,
    }
}
