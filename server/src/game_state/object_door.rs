//! Doors on placed region objects (`swingDoor` in the object catalog).
//! State is in memory only: every door is shut after a restart.

use super::goal_movement::doors::segment_distance;
use super::passability::regions_around;
use super::*;
use onlinerpg_shared::interest::SubjectArea;
use onlinerpg_shared::{furniture::FurniturePlacement, shelter, shortest_world_delta_x};
use std::borrow::Cow;

const DOOR_REACH: f32 = 2.0;

type DoorKey = (i32, i32, u32);

impl GameState {
    fn object_door_is_open(&self, key: &DoorKey) -> bool {
        self.open_object_doors
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(key)
    }

    pub(super) fn with_object_door_state<'a>(
        &self,
        rx: i32,
        rz: i32,
        placements: Cow<'a, [FurniturePlacement]>,
    ) -> Cow<'a, [FurniturePlacement]> {
        let open = self
            .open_object_doors
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if !placements.iter().any(|p| open.contains(&(rx, rz, p.id))) {
            return placements;
        }
        placements
            .iter()
            .cloned()
            .map(|mut p| {
                p.door_open = open.contains(&(rx, rz, p.id));
                p
            })
            .collect()
    }

    /// Only the walls change when a door moves; furniture and floors stay.
    fn rebuild_region_shelter_walls(&self, rx: i32, rz: i32) {
        let walls = {
            let index = self
                .interaction_furniture
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let placements = index.get(&(rx, rz)).map_or(&[][..], Vec::as_slice);
            let placements =
                self.with_object_door_state(rx, rz, self.without_woken_stand_in(placements));
            shelter::build_passability(&placements)
        };
        let key = onlinerpg_shared::furniture::region_cache_key(rx, rz);
        onlinerpg_shared::furniture::sync_shelter_walls(&mut self.passability_write(), &key, walls);
    }

    /// Forget doors whose object the editor removed from the region.
    pub(super) fn prune_object_doors(&self, rx: i32, rz: i32, placements: &[FurniturePlacement]) {
        self.open_object_doors
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|&(x, z, id)| {
                (x, z) != (rx, rz)
                    || placements
                        .iter()
                        .any(|p| p.id == id && shelter::doorway(p).is_some())
            });
    }

    /// The doorway segment and floor height, if the region is near `pos`.
    fn object_doorway(
        &self,
        pos: &Position,
        floor: i8,
        region: (i32, i32),
        object_id: u32,
    ) -> Option<([f32; 4], f32)> {
        if !regions_around(pos.x, pos.z).any(|key| key == region) {
            return None;
        }
        let index = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner());
        index
            .get(&region)?
            .iter()
            .filter(|p| p.id == object_id && p.floor_level as i8 == floor)
            .find_map(|p| Some((shelter::doorway(p)?, p.y)))
    }

    /// `Some(is_open)` once the request is valid; closing onto someone in the
    /// doorway leaves it open.
    pub async fn toggle_object_door(
        &self,
        player_id: &PlayerId,
        region: (i32, i32),
        object_id: u32,
    ) -> Option<bool> {
        let (pos, _, floor, _) = self.player_pose(player_id).await?;
        let (segment, y) = self.object_doorway(&pos, floor, region, object_id)?;
        let end = |x: f32, z: f32| Position { x, y, z };
        let center = end(
            segment[0] + shortest_world_delta_x(segment[0], segment[2]) * 0.5,
            (segment[1] + segment[3]) * 0.5,
        );
        let keys = self.movement_region_keys(
            end(segment[0], segment[1]),
            end(segment[2], segment[3]),
            0.31,
        );
        let (movement, predictions) = self.settle_movement_near(&center, &keys).await;
        let _edit = self.world_edit_guard().await;
        let result = async {
            let (pos, _, floor, name) = self.player_pose(player_id).await?;
            let (segment, _) = self.object_doorway(&pos, floor, region, object_id)?;
            let distance = segment_distance(segment, &pos);
            if distance > DOOR_REACH {
                warn!(
                    "Object door toggle refused: {name} is {distance:.1}m from door {object_id} in region {region:?}"
                );
                return None;
            }
            let key = (region.0, region.1, object_id);
            let was_open = self.object_door_is_open(&key);
            if was_open && self.doorway_occupied(floor, &[segment]).await {
                return Some(true);
            }
            {
                let mut open = self
                    .open_object_doors
                    .write()
                    .unwrap_or_else(|e| e.into_inner());
                if was_open {
                    open.remove(&key);
                } else {
                    open.insert(key);
                }
            }
            self.rebuild_region_shelter_walls(region.0, region.1);
            let door = DoorId::object(region.0, region.1, object_id);
            self.interest_lock().publish(
                door.to_string(),
                vec![SubjectArea::point(center, floor)],
                vec![door.state(Some(!was_open))],
                vec![door.state(None)],
                vec![door.state(Some(!was_open))],
            );
            Some(!was_open)
        }
        .await;
        drop(movement);
        drop(_edit);
        self.send_direction_predictions(predictions).await;
        result
    }
}
