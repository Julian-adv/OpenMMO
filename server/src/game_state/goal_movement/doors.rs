use super::*;

pub(in crate::game_state) fn segment_distance([ax, az, bx, bz]: [f32; 4], pos: &Position) -> f32 {
    let dx = shortest_world_delta_x(ax, pos.x);
    let dz = pos.z - az;
    let vx = shortest_world_delta_x(ax, bx);
    let vz = bz - az;
    let t = ((dx * vx + dz * vz) / (vx * vx + vz * vz).max(1e-6)).clamp(0.0, 1.0);
    (dx - vx * t).hypot(dz - vz * t)
}

impl GameState {
    pub(in crate::game_state) async fn settle_movement_near(
        &self,
        position: &Position,
        keys: &std::collections::BTreeSet<synchronization::MovementRegion>,
    ) -> (
        synchronization::MovementRegionGuard,
        Vec<direction::DirectionPrediction>,
    ) {
        let ids = self
            .player_spatial_cells
            .read()
            .await
            .keys_near(position, MAX_MOVE_TARGET_DISTANCE * 2.0)
            .copied()
            .collect::<Vec<_>>();
        let predictions = self.settle_goal_players(&ids).await;
        let guard = self.movement_regions.lock(keys, true).await;
        (guard, predictions)
    }

    pub(in crate::game_state) async fn doorway_occupied(
        &self,
        floor: i8,
        segments: &[[f32; 4]],
    ) -> bool {
        self.players.read().await.values().any(|player| {
            player.health > 0
                && player.floor_level == floor
                && segments
                    .iter()
                    .any(|&segment| segment_distance(segment, &player.position) <= 0.31)
        })
    }

    pub(in crate::game_state) async fn dungeon_door_segment(
        &self,
        entrance_id: &str,
        depth: u8,
        door_id: u32,
    ) -> Option<[f32; 4]> {
        let entrance = self.dungeon_defs.get(entrance_id)?.position();
        let dungeons = self.dungeons.read().await;
        let runtime = dungeons.get(entrance_id)?;
        if depth == 0 {
            let center = dungeon::door_position(&entrance, &runtime.layouts, depth, door_id)?;
            let shaft = &runtime.layouts.first()?.up_shaft;
            let half = dungeon::SHAFT_W as f32 * 0.5;
            return Some(if shaft.along_z {
                [center.x - half, center.z, center.x + half, center.z]
            } else {
                [center.x, center.z - half, center.x, center.z + half]
            });
        }
        let door = dungeon::interior_doors(runtime.layouts.get(depth as usize - 1)?)
            .into_iter()
            .find(|d| d.door_id == door_id)?;
        let (ox, oz) = dungeon::dungeon_origin(entrance.x, entrance.z);
        let [ax, az, bx, bz] = door.seg();
        Some([
            ox + ax as f32,
            oz + az as f32,
            ox + bx as f32,
            oz + bz as f32,
        ])
    }
}
