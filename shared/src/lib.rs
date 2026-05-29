//! Shared types and protocol between the Rust server, agent-client, and
//! the WASM web client. Definitions are split by concern across small
//! sibling modules and re-exported here so external callers can keep
//! using flat `onlinerpg_shared::Position` paths regardless of where the
//! type now lives.

pub mod character;
pub mod entity;
pub mod housing;
pub mod inventory;
pub mod messages;
pub mod monster_ai;
pub mod pathfinding;
pub mod world;
pub mod worldgen;
pub mod xp;

#[cfg(target_arch = "wasm32")]
mod wasm_api;

pub use character::{Character, CharacterAttributes, CharacterClass, Gender};
pub use entity::{Monster, MonsterState, Player};
pub use messages::{
    deserialize_client_msg, deserialize_server_msg, serialize_client_msg, serialize_server_msg,
    ClientMessage, PlayerId, ServerMessage,
};
pub use world::{GameDateTime, NoSpawnZone, Position, NPC_SIGHT_RADIUS};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
                gender: Gender::default(),
                is_npc: false,
                torch_on: false,
                floor_level: 0,
                object_type: None,
                object_id: None,
                last_combat_at: 0,
            },
        );
        let msg = ServerMessage::GameState {
            players,
            monsters: HashMap::new(),
            ground_items: Vec::new(),
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

    const ALL_CLASSES: &[CharacterClass] = &[
        CharacterClass::Knight,
        CharacterClass::Barbarian,
        CharacterClass::Caveman,
        CharacterClass::Valkyrie,
        CharacterClass::Ranger,
        CharacterClass::Samurai,
        CharacterClass::Monk,
        CharacterClass::Priest,
        CharacterClass::Archaeologist,
        CharacterClass::Healer,
        CharacterClass::Rogue,
        CharacterClass::Wizard,
        CharacterClass::Tourist,
        CharacterClass::Merchant,
        CharacterClass::Guard,
    ];

    #[test]
    fn character_class_str_roundtrip() {
        for class in ALL_CLASSES {
            let s = class.as_str();
            let back: CharacterClass = s.parse().expect("parse should accept as_str output");
            assert_eq!(&back, class, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn character_class_from_str_rejects_unknown() {
        assert!("".parse::<CharacterClass>().is_err());
        assert!("nonexistent".parse::<CharacterClass>().is_err());
        assert!("Knight".parse::<CharacterClass>().is_err());
    }

    #[test]
    fn character_class_hit_die_is_valid_polyhedron() {
        for class in ALL_CLASSES {
            let d = class.hit_die();
            assert!(
                matches!(d, 4 | 6 | 8 | 10 | 12 | 20),
                "class {:?} has non-standard hit die d{}",
                class,
                d
            );
        }
    }

    #[test]
    fn stat_adjustments_sum_to_zero() {
        // Balanced classes: the six adjustments must net to zero so no class
        // gains or loses total stat points before the 72-rebalance step.
        for class in ALL_CLASSES {
            for gender in [Gender::Male, Gender::Female] {
                let adj = class.stat_adjustments(gender);
                let sum: i32 = adj.iter().map(|v| *v as i32).sum();
                assert_eq!(
                    sum, 0,
                    "class {:?} gender {:?} adjustments sum to {} (expected 0): {:?}",
                    class, gender, sum, adj
                );
            }
        }
    }

    #[test]
    fn stat_adjustments_gender_variants_differ_only_where_expected() {
        // Knight, Barbarian, and Caveman have gendered splits; all others ignore gender.
        let gendered = [
            CharacterClass::Knight,
            CharacterClass::Barbarian,
            CharacterClass::Caveman,
        ];
        for class in ALL_CLASSES {
            let male = class.stat_adjustments(Gender::Male);
            let female = class.stat_adjustments(Gender::Female);
            if gendered.contains(class) {
                assert_ne!(
                    male, female,
                    "class {:?} expected gendered adjustments",
                    class
                );
            } else {
                assert_eq!(
                    male, female,
                    "class {:?} should have identical adjustments for both genders",
                    class
                );
            }
        }
    }

    #[test]
    fn no_spawn_zone_contains_inside_and_boundary() {
        let zone = NoSpawnZone {
            min_x: -10.0,
            min_z: -5.0,
            max_x: 10.0,
            max_z: 5.0,
        };
        assert!(zone.contains(0.0, 0.0));
        assert!(zone.contains(-10.0, -5.0));
        assert!(zone.contains(10.0, 5.0));
        assert!(zone.contains(-10.0, 5.0));
        assert!(zone.contains(10.0, -5.0));
        assert!(!zone.contains(-10.1, 0.0));
        assert!(!zone.contains(10.1, 0.0));
        assert!(!zone.contains(0.0, -5.1));
        assert!(!zone.contains(0.0, 5.1));
    }

    #[test]
    fn no_spawn_zone_degenerate_point_zone() {
        // A zero-area zone still matches its single point.
        let zone = NoSpawnZone {
            min_x: 3.0,
            min_z: 7.0,
            max_x: 3.0,
            max_z: 7.0,
        };
        assert!(zone.contains(3.0, 7.0));
        assert!(!zone.contains(3.0001, 7.0));
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
            ServerMessage::PlayerDisappeared {
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
