pub use onlinerpg_shared::{
    Character, CharacterAttributes, CharacterClass, ClientMessage, GameDateTime, Monster, Player,
    PlayerId, Position, ServerMessage,
};
use uuid::Uuid;

pub fn new_player(name: String, level: u32, max_health: u32, class: CharacterClass) -> Player {
    Player {
        id: Uuid::new_v4().to_string(),
        name,
        position: Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: 0.0,
        level,
        health: max_health,
        max_health,
        class,
    }
}
