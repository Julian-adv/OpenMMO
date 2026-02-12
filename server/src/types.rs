pub use onlinerpg_shared::{
    ClientMessage, Monster, Player, PlayerId, Position, ServerMessage,
};
use uuid::Uuid;

pub fn new_player(name: String) -> Player {
    Player {
        id: Uuid::new_v4().to_string(),
        name,
        position: Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: 0.0,
        level: 1,
        health: 10,
        max_health: 10,
    }
}
