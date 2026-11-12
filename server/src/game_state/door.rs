use super::*;

impl GameState {
    /// `Some(is_open)` once the request is valid; a door closing onto someone
    /// in the doorway stays open.
    pub async fn toggle_door(&self, player_id: &PlayerId, door: &str) -> Option<bool> {
        match DoorId::parse(door)? {
            DoorId::House {
                house_id,
                room_index,
                wall_dir,
                segment_index,
            } => {
                self.toggle_house_door(player_id, &house_id, room_index, wall_dir, segment_index)
                    .await
            }
            DoorId::Dungeon {
                entrance_id,
                depth,
                door_id,
            } => {
                self.toggle_dungeon_door(player_id, &entrance_id, depth, door_id)
                    .await
            }
            DoorId::Object { rx, rz, object_id } => {
                self.toggle_object_door(player_id, (rx, rz), object_id)
                    .await
            }
        }
    }
}
