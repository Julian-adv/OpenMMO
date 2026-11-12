//! One id for every door a player can open, as carried by `ToggleDoor` and
//! `DoorState`: `house:{house}:{room}:{dir}:{segment}`,
//! `dungeon:{entrance}:{depth}:{door}`, `object:{rx}:{rz}:{object}:{door}`.

use crate::housing::WallDirection;
use crate::{ClientMessage, ServerMessage};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DoorId {
    /// A door or window shutter in a house wall.
    House {
        house_id: String,
        room_index: u32,
        wall_dir: WallDirection,
        segment_index: u32,
    },
    /// Depth 0 is the surface entrance door; deeper ones are interior doors.
    Dungeon {
        entrance_id: String,
        depth: u8,
        door_id: u32,
    },
    /// A door of a placed object in region `(rx, rz)`: 0 is its entrance
    /// `swingDoor`, `sideDoors` follow.
    Object {
        rx: i32,
        rz: i32,
        object_id: u32,
        door: u8,
    },
}

impl DoorId {
    pub fn house(house_id: &str, room_index: u32, wall_dir: WallDirection, segment: u32) -> Self {
        Self::House {
            house_id: house_id.to_owned(),
            room_index,
            wall_dir,
            segment_index: segment,
        }
    }

    pub fn dungeon(entrance_id: &str, depth: u8, door_id: u32) -> Self {
        Self::Dungeon {
            entrance_id: entrance_id.to_owned(),
            depth,
            door_id,
        }
    }

    pub fn object(rx: i32, rz: i32, object_id: u32, door: u8) -> Self {
        Self::Object {
            rx,
            rz,
            object_id,
            door,
        }
    }

    /// House and entrance ids are free text, so the numbers are read from the right.
    pub fn parse(door: &str) -> Option<Self> {
        let (kind, rest) = door.split_once(':')?;
        match kind {
            "house" => {
                let mut parts = rest.rsplitn(4, ':');
                let segment_index = parts.next()?.parse().ok()?;
                let wall_dir = WallDirection::from_name(parts.next()?)?;
                let room_index = parts.next()?.parse().ok()?;
                let house_id = parts.next().filter(|id| !id.is_empty())?;
                Some(Self::house(house_id, room_index, wall_dir, segment_index))
            }
            "dungeon" => {
                let mut parts = rest.rsplitn(3, ':');
                let door_id = parts.next()?.parse().ok()?;
                let depth = parts.next()?.parse().ok()?;
                let entrance_id = parts.next().filter(|id| !id.is_empty())?;
                Some(Self::dungeon(entrance_id, depth, door_id))
            }
            "object" => {
                let mut parts = rest.split(':');
                let rx = parts.next()?.parse().ok()?;
                let rz = parts.next()?.parse().ok()?;
                let object_id = parts.next()?.parse().ok()?;
                let door = parts.next()?.parse().ok()?;
                parts
                    .next()
                    .is_none()
                    .then(|| Self::object(rx, rz, object_id, door))
            }
            _ => None,
        }
    }

    pub fn toggle(&self) -> ClientMessage {
        ClientMessage::ToggleDoor {
            door: self.to_string(),
        }
    }

    /// `None` tells a viewer the door left their view or no longer exists.
    pub fn state(&self, is_open: Option<bool>) -> ServerMessage {
        ServerMessage::DoorState {
            door: self.to_string(),
            is_open,
        }
    }
}

impl fmt::Display for DoorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::House {
                house_id,
                room_index,
                wall_dir,
                segment_index,
            } => write!(
                f,
                "house:{house_id}:{room_index}:{}:{segment_index}",
                wall_dir.as_str()
            ),
            Self::Dungeon {
                entrance_id,
                depth,
                door_id,
            } => write!(f, "dungeon:{entrance_id}:{depth}:{door_id}"),
            Self::Object {
                rx,
                rz,
                object_id,
                door,
            } => write!(f, "object:{rx}:{rz}:{object_id}:{door}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_survives_the_round_trip() {
        for door in [
            DoorId::house("r-23_+74_1", 2, WallDirection::West, 3),
            DoorId::house("odd:id", 0, WallDirection::North, 0),
            DoorId::dungeon("old_crypt", 0, 0),
            DoorId::dungeon("odd:id", 3, 4_000_000_000),
            DoorId::object(-23, 74, 9, 1),
        ] {
            assert_eq!(DoorId::parse(&door.to_string()), Some(door));
        }
        assert_eq!(
            DoorId::dungeon("old_crypt", 1, 7).to_string(),
            "dungeon:old_crypt:1:7"
        );
    }

    #[test]
    fn malformed_ids_are_refused() {
        for door in [
            "",
            "object:1:2:3",
            "object:1:2:3:4:5",
            "object:1:2:3:300",
            "object:a:2:3",
            "dungeon::1:2",
            "dungeon:crypt:300:2",
            "house:h:0:up:1",
            "house::0:north:1",
            "gate:1:2:3",
        ] {
            assert_eq!(DoorId::parse(door), None, "{door}");
        }
    }
}
