pub use onlinerpg_shared::{
    Character, CharacterAttributes, CharacterClass, ClientMessage, GameDateTime, Monster, Player,
    PlayerId, Position, ServerMessage,
};
use uuid::Uuid;

pub fn new_player(
    name: String,
    level: u32,
    max_health: u32,
    class: CharacterClass,
    position: Position,
    rotation: f32,
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
        torch_on: false,
        last_combat_at: 0,
    }
}
