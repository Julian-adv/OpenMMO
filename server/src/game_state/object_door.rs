//! Doors on placed region objects (`swingDoor` in the object catalog).
//! State is in memory only: every door is shut after a restart.

use super::passability::regions_around;
use super::*;
use onlinerpg_shared::interest::SubjectArea;
use onlinerpg_shared::{furniture::FurniturePlacement, shelter, shortest_world_delta_x};
use std::borrow::Cow;

const DOOR_REACH: f32 = 2.0;

type DoorKey = (i32, i32, u32);

fn door_id((rx, rz, id): DoorKey) -> String {
    format!("object:{rx}:{rz}:{id}")
}

fn segment_distance([ax, az, bx, bz]: [f32; 4], pos: &Position) -> f32 {
    let dx = shortest_world_delta_x(ax, pos.x);
    let dz = pos.z - az;
    let vx = shortest_world_delta_x(ax, bx);
    let vz = bz - az;
    let t = ((dx * vx + dz * vz) / (vx * vx + vz * vz).max(1e-6)).clamp(0.0, 1.0);
    (dx - vx * t).hypot(dz - vz * t)
}

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

    fn object_door_near(
        &self,
        pos: &Position,
        floor: i8,
        object_id: u32,
    ) -> Option<((i32, i32), [f32; 4], f32)> {
        let index = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner());
        regions_around(pos.x, pos.z)
            .filter_map(|key| index.get(&key).map(|group| (key, group)))
            .flat_map(|(key, group)| group.iter().map(move |p| (key, p)))
            .filter(|(_, p)| p.id == object_id && p.floor_level as i8 == floor)
            .filter_map(|(key, p)| Some((key, shelter::doorway(p)?, p.y)))
            .min_by(|a, b| segment_distance(a.1, pos).total_cmp(&segment_distance(b.1, pos)))
    }

    /// `Some(is_open)` once the request is valid; closing onto someone in the
    /// doorway leaves it open.
    pub async fn toggle_object_door(&self, player_id: &PlayerId, object_id: u32) -> Option<bool> {
        let (pos, _, floor, _) = self.player_pose(player_id).await?;
        let (region, segment, y) = self.object_door_near(&pos, floor, object_id)?;
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
            let (current, segment, _) = self.object_door_near(&pos, floor, object_id)?;
            if current != region {
                return None;
            }
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
            let placements = self
                .interaction_furniture
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&region)
                .cloned()
                .unwrap_or_default();
            self.rebuild_region_furniture_passability(region.0, region.1, &placements);
            let state = |is_open| ServerMessage::DoorState {
                door: door_id(key),
                is_open,
            };
            self.interest_lock().publish(
                door_id(key),
                vec![SubjectArea::point(center, floor)],
                vec![state(!was_open)],
                vec![state(false)],
                vec![state(!was_open)],
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
