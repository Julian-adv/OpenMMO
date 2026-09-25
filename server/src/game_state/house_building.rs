use super::{
    auth_db,
    inventory::{consume_one, serialize_inventory, stack_into_bag, BagInsert},
    GameState,
};
use crate::{
    auth::AuthService,
    housing::{next_house_id, validate_house, validate_house_neighbors, world_to_chunk},
    types::{PlayerId, Position, ServerMessage},
};
use onlinerpg_shared::{
    fence::FencePlot,
    housing::{HouseData, RoofRidgeDir, RoomType},
    landscaping::{owns_position, LandscapingTool, TOOLBOX_ITEM},
    shortest_world_delta_x, wrap_world_x,
};
use onlinerpg_terrain::height::{
    flatten_heightmap_rects, restore_heightmap_rects, HeightRect, HeightmapEdit,
};
use std::{
    collections::{BTreeSet, HashSet},
    sync::LazyLock,
};

const PLACEMENT_REACH_M: f32 = 30.0;
const MAX_FOUNDATION_SLOPE_M: f32 = 1.0;
const FOUNDATION_BLEND_RADIUS_M: f32 = 4.0;

const HOUSE_SCROLLS: [(&str, &str); 5] = [
    (
        "scroll_of_medium_two_story_house",
        include_str!("../../../data-src/house-templates/rica-shop.json"),
    ),
    (
        "scroll_of_small_house",
        include_str!("../../../data-src/house-templates/karl-house.json"),
    ),
    (
        "scroll_of_small_two_story_house",
        include_str!("../../../data-src/house-templates/aldwin-house.json"),
    ),
    (
        "scroll_of_large_two_story_house",
        include_str!("../../../data-src/house-templates/aldermark-inn.json"),
    ),
    (
        "scroll_of_medium_house",
        include_str!("../../../data-src/house-templates/rowan-house.json"),
    ),
];

static HOUSE_TEMPLATES: LazyLock<Vec<(&'static str, HouseData)>> = LazyLock::new(|| {
    HOUSE_SCROLLS
        .iter()
        .filter_map(|(item_id, source)| load_template(source).map(|house| (*item_id, house)))
        .collect()
});

fn load_template(source: &str) -> Option<HouseData> {
    let mut house: HouseData = serde_json::from_str(source).ok()?;
    house.id = "house-placement-preview".to_string();
    house.owner_id.clear();
    house.origin = Position {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    for room in &mut house.rooms {
        for wall in [
            &mut room.wall_north,
            &mut room.wall_south,
            &mut room.wall_east,
            &mut room.wall_west,
        ] {
            for segment in wall {
                segment.is_open = false;
            }
        }
    }
    Some(house)
}

fn template_for_item(item_id: &str) -> Option<HouseData> {
    HOUSE_TEMPLATES
        .iter()
        .find_map(|(id, house)| (*id == item_id).then(|| house.clone()))
}

fn rotate_edge_mask(mask: u8) -> u8 {
    let mut rotated = mask & !0x0f;
    if mask & 0x01 != 0 {
        rotated |= 0x08;
    }
    if mask & 0x02 != 0 {
        rotated |= 0x01;
    }
    if mask & 0x04 != 0 {
        rotated |= 0x02;
    }
    if mask & 0x08 != 0 {
        rotated |= 0x04;
    }
    rotated
}

fn rotate_house_quarter_turn(house: &mut HouseData) {
    for room in &mut house.rooms {
        let old_x = room.local_x;
        let old_z = room.local_z;
        let old_size_x = room.size_x;
        let old_size_z = room.size_z;
        let stair_ran_along_x = room.room_type == RoomType::Stairwell && room.size_x > room.size_z;

        room.local_x = old_z;
        room.local_z = -old_x - i32::from(old_size_x);
        room.size_x = old_size_z;
        room.size_z = old_size_x;
        room.roof_ridge_dir = match room.roof_ridge_dir {
            RoofRidgeDir::X => RoofRidgeDir::Z,
            RoofRidgeDir::Z => RoofRidgeDir::X,
            RoofRidgeDir::Auto => RoofRidgeDir::Auto,
        };
        if stair_ran_along_x {
            room.stair_reversed = !room.stair_reversed;
        }

        let north = std::mem::take(&mut room.wall_north);
        let south = std::mem::take(&mut room.wall_south);
        let east = std::mem::take(&mut room.wall_east);
        let west = std::mem::take(&mut room.wall_west);
        room.wall_north = east;
        room.wall_south = west;
        room.wall_east = south.into_iter().rev().collect();
        room.wall_west = north.into_iter().rev().collect();
    }

    for grid in &mut house.passability {
        let old_origin_x = grid.origin_x;
        let old_origin_z = grid.origin_z;
        let old_width = usize::from(grid.width);
        let old_depth = usize::from(grid.depth);
        let mut cells = vec![0; grid.cells.len()];
        for z in 0..old_depth {
            for x in 0..old_width {
                let old_index = z * old_width + x;
                let new_x = z;
                let new_z = old_width - 1 - x;
                cells[new_z * old_depth + new_x] = rotate_edge_mask(grid.cells[old_index]);
            }
        }
        grid.origin_x = old_origin_z;
        grid.origin_z = -old_origin_x - old_width as i32;
        grid.width = old_depth as u8;
        grid.depth = old_width as u8;
        grid.cells = cells;
    }
}

fn foundation_cells(house: &HouseData) -> Vec<(f32, f32)> {
    let mut cells = HashSet::new();
    for room in house
        .rooms
        .iter()
        .filter(|room| room.floor_level == 0 && room.room_type != RoomType::Stairwell)
    {
        for z in room.local_z..room.local_z + i32::from(room.size_z) {
            for x in room.local_x..room.local_x + i32::from(room.size_x) {
                cells.insert((x, z));
            }
        }
    }
    cells
        .into_iter()
        .map(|(x, z)| {
            (
                wrap_world_x(house.origin.x + x as f32 + 0.5),
                house.origin.z + z as f32 + 0.5,
            )
        })
        .collect()
}

fn foundation_inside_estate(house: &HouseData, plots: &[FencePlot]) -> bool {
    let cells = foundation_cells(house);
    !cells.is_empty() && cells.into_iter().all(|(x, z)| owns_position(plots, x, z))
}

fn distance_to_house(house: &HouseData, position: &Position) -> f32 {
    house
        .rooms
        .iter()
        .filter(|room| room.floor_level == 0 && room.room_type != RoomType::Stairwell)
        .map(|room| {
            let origin_x = shortest_world_delta_x(position.x, house.origin.x);
            let min_x = origin_x + room.local_x as f32;
            let max_x = min_x + room.size_x as f32;
            let min_z = house.origin.z + room.local_z as f32;
            let max_z = min_z + room.size_z as f32;
            let dx = if 0.0 < min_x {
                min_x
            } else if 0.0 > max_x {
                -max_x
            } else {
                0.0
            };
            let dz = if position.z < min_z {
                min_z - position.z
            } else if position.z > max_z {
                position.z - max_z
            } else {
                0.0
            };
            dx.hypot(dz)
        })
        .fold(f32::INFINITY, f32::min)
}

fn expand_rect([min_x, min_z, max_x, max_z]: HeightRect, margin: f32) -> HeightRect {
    [
        min_x - margin,
        min_z - margin,
        max_x + margin,
        max_z + margin,
    ]
}

fn rects_overlap(a: HeightRect, b: HeightRect) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

impl GameState {
    async fn apply_heightmap_edits(
        &self,
        house_id: &str,
        edits: Vec<HeightmapEdit>,
        preserve_original: bool,
    ) -> Vec<(i32, i32)> {
        let mut changed = Vec::new();
        for edit in edits {
            let result = async {
                if preserve_original {
                    self.terrain_io
                        .ensure_original_heightmap(edit.tile_x, edit.tile_z)
                        .await?;
                }
                self.save_terrain_heightmap_locked(edit.tile_x, edit.tile_z, &edit.data)
                    .await
            }
            .await;
            match result {
                Ok(()) => changed.push((edit.tile_x, edit.tile_z)),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        %house_id,
                        tile_x = edit.tile_x,
                        tile_z = edit.tile_z,
                        "Failed to update terrain for house"
                    );
                }
            }
        }
        changed
    }

    async fn restore_demolished_house_terrain(&self, house: &HouseData) -> Vec<(i32, i32)> {
        let restored_areas =
            crate::housing::routes::house_foundation_rects(house, FOUNDATION_BLEND_RADIUS_M);
        let restore_edits = match restore_heightmap_rects(&self.terrain_io, &restored_areas).await {
            Ok(edits) => edits,
            Err(error) => {
                tracing::warn!(%error, house_id = %house.id, "Failed to restore demolished house terrain");
                return Vec::new();
            }
        };
        let mut changed: BTreeSet<_> = self
            .apply_heightmap_edits(&house.id, restore_edits, false)
            .await
            .into_iter()
            .collect();
        if changed.is_empty() {
            return Vec::new();
        }

        let remaining = match self.housing_io.read_all_houses().await {
            Ok(houses) => houses,
            Err(error) => {
                tracing::warn!(%error, house_id = %house.id, "Failed to load houses after terrain restoration");
                return changed.into_iter().collect();
            }
        };
        let foundations = remaining
            .iter()
            .map(|other| {
                (
                    other,
                    crate::housing::routes::house_foundation_rects(other, 0.0),
                )
            })
            .collect::<Vec<_>>();

        for (remaining_house, rects) in &foundations {
            let affected = rects
                .iter()
                .copied()
                .filter(|rect| {
                    let influence = expand_rect(*rect, FOUNDATION_BLEND_RADIUS_M);
                    restored_areas
                        .iter()
                        .any(|restored| rects_overlap(influence, *restored))
                })
                .collect::<Vec<_>>();
            if affected.is_empty() {
                continue;
            }
            let protected = foundations
                .iter()
                .filter(|(other, _)| other.id != remaining_house.id)
                .flat_map(|(_, rects)| rects.iter().copied())
                .filter(|rect| {
                    affected.iter().any(|target| {
                        rects_overlap(*rect, expand_rect(*target, FOUNDATION_BLEND_RADIUS_M))
                    })
                })
                .collect::<Vec<_>>();
            let edits = match flatten_heightmap_rects(
                &self.terrain_io,
                &affected,
                remaining_house.origin.y,
                FOUNDATION_BLEND_RADIUS_M,
                &protected,
            )
            .await
            {
                Ok(edits) => edits,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        house_id = %remaining_house.id,
                        "Failed to reapply neighboring house terrain"
                    );
                    continue;
                }
            };
            changed.extend(
                self.apply_heightmap_edits(&remaining_house.id, edits, false)
                    .await,
            );
        }
        changed.into_iter().collect()
    }

    pub async fn try_start_house_placement(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        auth: &AuthService,
    ) -> bool {
        let item_id = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inventory| {
                inventory
                    .bag
                    .iter()
                    .find(|item| item.instance_id == instance_id && item.quantity > 0)
            })
            .map(|item| item.item_def_id.clone());
        let Some(item_id) = item_id else {
            return false;
        };
        let Some(house) = template_for_item(&item_id) else {
            return false;
        };
        let plots = match self
            .try_start_landscaping_mode(player_id, auth, LandscapingTool::House, false)
            .await
        {
            Ok(plots) => plots,
            Err(error) => {
                self.send_system_message(player_id, error).await;
                return true;
            }
        };
        self.send_direct_message(
            player_id,
            ServerMessage::HousePlacementStarted {
                instance_id,
                item_name: self.item_name(&item_id),
                house,
                plots,
            },
        )
        .await;
        true
    }

    pub async fn place_house(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        origin: Position,
        quarter_turns: u8,
        auth: &AuthService,
    ) {
        let result = self
            .try_place_house(player_id, instance_id, origin, quarter_turns, auth)
            .await;
        self.send_direct_message(
            player_id,
            ServerMessage::HousePlacementResult {
                error: result.err(),
            },
        )
        .await;
    }

    pub async fn demolish_house(&self, player_id: &PlayerId, house_id: String, auth: &AuthService) {
        let result = self.try_demolish_house(player_id, &house_id, auth).await;
        self.send_direct_message(
            player_id,
            ServerMessage::HouseDemolitionResult {
                house_id,
                error: result.err(),
            },
        )
        .await;
    }

    async fn try_demolish_house(
        &self,
        player_id: &PlayerId,
        house_id: &str,
        auth: &AuthService,
    ) -> Result<(), String> {
        if self.reject_if_trading(player_id, "demolish a house").await {
            return Err("Finish your player trade first.".to_string());
        }
        self.tick_land_taxes(auth).await;
        let _persistence = self.persistence_lock.lock().await;
        let character_id = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .map(|(id, _, _)| *id)
            .ok_or("Character not found.")?;
        if !self.holds_item(player_id, TOOLBOX_ITEM).await {
            return Err("Carry a Landscaper's Toolbox to demolish a house.".to_string());
        }
        let player_position = {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("Character not found.")?;
            if player.health == 0 || player.floor_level != 0 {
                return Err("Stand outdoors while alive to demolish a house.".to_string());
            }
            player.position
        };
        let house = self
            .housing_io
            .find_house(house_id)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %house_id, "Failed to load house for demolition");
                "House demolition is temporarily unavailable.".to_string()
            })?
            .ok_or("That house no longer exists.")?;
        if house.owner_id != character_id.to_string() {
            return Err("You can only demolish your own house.".to_string());
        }
        if distance_to_house(&house, &player_position) > PLACEMENT_REACH_M {
            return Err("Move within 30 metres of the house.".to_string());
        }
        if self.house_contains_estate_chest(&house).await {
            return Err("Remove every storage chest before demolishing this house.".to_string());
        }
        let refund = match house.source_scroll_id.as_deref() {
            Some(item_id)
                if HOUSE_SCROLLS.iter().any(|(known, _)| *known == item_id)
                    && self.item_defs.get(item_id).is_some() =>
            {
                let max_weight = self.max_carry_weight(player_id).await;
                let armor_mult = self.armor_weight_mult(player_id).await;
                let next_id = self.next_instance_id().await;
                let character = self
                    .get_player_save_data(player_id)
                    .await
                    .ok_or("Character not found.")?;
                let inventories = self.inventories.write().await;
                let inventory = inventories.get(player_id).ok_or("Inventory not found.")?;
                if self.calc_total_weight(inventory, armor_mult) + self.item_defs.weight(item_id)
                    > max_weight
                {
                    return Err("Your bag is too heavy to recover this house scroll.".to_string());
                }
                let mut updated = inventory.clone();
                stack_into_bag(
                    &mut updated.bag,
                    BagInsert::one(self.item_defs.stackable(item_id), item_id, 0, next_id),
                );
                Some((inventories, updated, character))
            }
            Some(_) => {
                return Err("This house's construction scroll is unavailable.".to_string());
            }
            None => None,
        };
        let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
        let deleted = self
            .housing_io
            .delete_house(house_id, cx, cz)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %house_id, "Failed to demolish house");
                "The house could not be demolished.".to_string()
            })?;
        if !deleted {
            return Err("That house no longer exists.".to_string());
        }
        if let Some((mut inventories, updated, character)) = refund {
            let rows = serialize_inventory(&updated);
            let auth_copy = auth.clone();
            if let Err(error) = auth_db(move || {
                auth_copy.save_batch(
                    std::slice::from_ref(&character),
                    &[(character.character_id, rows)],
                    &[],
                    &[],
                    None,
                )
            })
            .await
            {
                tracing::warn!(%error, %house_id, "Failed to persist recovered house scroll");
                if let Err(restore_error) = self.housing_io.write_house(&house).await {
                    tracing::error!(%restore_error, %house_id, "Failed to restore house after refund failure");
                }
                return Err(
                    "The house could not be demolished. Your scroll was not changed.".to_string(),
                );
            }
            *inventories
                .get_mut(player_id)
                .expect("inventory existed while demolition lock was held") = updated.clone();
            drop(inventories);
            self.send_inventory_snapshot(player_id, updated).await;
        }
        let changed_height_tiles = self.restore_demolished_house_terrain(&house).await;
        self.passability_remove_house(house_id).await;
        crate::housing::routes::publish_house_terrain_changes(
            self,
            &changed_height_tiles,
            &[],
            &[],
        )
        .await;
        Ok(())
    }

    async fn try_place_house(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        origin: Position,
        quarter_turns: u8,
        auth: &AuthService,
    ) -> Result<(), String> {
        if !origin.is_finite() {
            return Err("Choose a valid building position.".to_string());
        }
        if quarter_turns > 3 {
            return Err("Choose a valid house rotation.".to_string());
        }
        self.tick_land_taxes(auth).await;
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "place a house").await {
            return Err("Finish your player trade first.".to_string());
        }
        let character_id = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .map(|(id, _, _)| *id)
            .ok_or("Character not found.")?;
        let player_position = {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("Character not found.")?;
            if player.health == 0 || player.floor_level != 0 {
                return Err("Stand outdoors while alive to build a house.".to_string());
            }
            player.position
        };
        let item_id = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inventory| {
                inventory
                    .bag
                    .iter()
                    .find(|item| item.instance_id == instance_id && item.quantity > 0)
            })
            .map(|item| item.item_def_id.clone())
            .ok_or("That house scroll is no longer in your bag.")?;
        if !self.holds_item(player_id, TOOLBOX_ITEM).await {
            return Err("Carry a Landscaper's Toolbox to place a house.".to_string());
        }
        let mut house = template_for_item(&item_id)
            .ok_or_else(|| "That item is not a house scroll.".to_string())?;
        for _ in 0..quarter_turns {
            rotate_house_quarter_turn(&mut house);
        }
        let x = wrap_world_x(origin.x.round());
        let z = origin.z.round();
        if shortest_world_delta_x(player_position.x, x).hypot(z - player_position.z)
            > PLACEMENT_REACH_M
        {
            return Err("Choose a position within 30 metres.".to_string());
        }
        house.origin.x = x;
        house.origin.z = z;
        let auth_copy = auth.clone();
        let plots = auth_db(move || auth_copy.fence_plots(character_id))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Failed to load house placement permissions");
                "House placement is temporarily unavailable.".to_string()
            })?;
        if !foundation_inside_estate(&house, &plots) {
            return Err("The whole house must fit inside your estate.".to_string());
        }
        let cells = foundation_cells(&house);
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        let mut height_sum = 0.0;
        let cell_count = cells.len();
        for (cell_x, cell_z) in cells {
            let Some((height, depth)) = self.ground_and_depth_at(cell_x, cell_z).await else {
                return Err("Terrain is unavailable at that position.".to_string());
            };
            if depth > 0.1 {
                return Err("Houses need dry ground.".to_string());
            }
            min_height = min_height.min(height);
            max_height = max_height.max(height);
            height_sum += height;
        }
        if max_height - min_height > MAX_FOUNDATION_SLOPE_M {
            return Err("The ground is too uneven for this house.".to_string());
        }
        house.origin.y = height_sum / cell_count as f32;
        house.owner_id = character_id.to_string();
        house.source_scroll_id = Some(item_id.clone());
        validate_house(&house)?;
        let neighbors = self.housing_io.read_all_houses().await.map_err(|error| {
            tracing::warn!(%error, "Failed to load houses for placement");
            "House placement is temporarily unavailable.".to_string()
        })?;
        let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
        house.id = next_house_id(cx, cz, &neighbors);
        validate_house_neighbors(&house, &neighbors)
            .map_err(|_| "That position overlaps another house.".to_string())?;
        let foundation_rects = crate::housing::routes::house_foundation_rects(&house, 0.0);
        let foundation_influence = foundation_rects
            .iter()
            .map(|rect| expand_rect(*rect, FOUNDATION_BLEND_RADIUS_M))
            .collect::<Vec<_>>();
        let protected_rects = neighbors
            .iter()
            .flat_map(|neighbor| crate::housing::routes::house_foundation_rects(neighbor, 0.0))
            .filter(|rect| {
                foundation_influence
                    .iter()
                    .any(|influence| rects_overlap(*influence, *rect))
            })
            .collect::<Vec<_>>();
        let height_edits = flatten_heightmap_rects(
            &self.terrain_io,
            &foundation_rects,
            house.origin.y,
            FOUNDATION_BLEND_RADIUS_M,
            &protected_rects,
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Failed to prepare terrain for house placement");
            "The terrain could not be prepared for this house.".to_string()
        })?;

        let character = self
            .get_player_save_data(player_id)
            .await
            .ok_or("Character not found.")?;
        let mut inventories = self.inventories.write().await;
        let inventory = inventories
            .get_mut(player_id)
            .ok_or("Inventory not found.")?;
        if !inventory.bag.iter().any(|item| {
            item.instance_id == instance_id && item.item_def_id == item_id && item.quantity > 0
        }) {
            return Err("That house scroll is no longer in your bag.".to_string());
        }
        let mut updated = inventory.clone();
        consume_one(&mut updated, instance_id);
        self.housing_io.write_house(&house).await.map_err(|error| {
            tracing::warn!(%error, "Failed to save placed house");
            "The house could not be saved. Your scroll was not consumed.".to_string()
        })?;
        let rows = serialize_inventory(&updated);
        let auth_copy = auth.clone();
        if let Err(error) = auth_db(move || {
            auth_copy.save_batch(
                std::slice::from_ref(&character),
                &[(character.character_id, rows)],
                &[],
                &[],
                None,
            )
        })
        .await
        {
            tracing::warn!(%error, "Failed to persist house scroll consumption");
            let _ = self.housing_io.delete_house(&house.id, cx, cz).await;
            return Err("The house could not be saved. Your scroll was not consumed.".to_string());
        }
        *inventory = updated.clone();
        drop(inventories);
        self.send_inventory_snapshot(player_id, updated).await;
        let changed_height_tiles = self
            .apply_heightmap_edits(&house.id, height_edits, true)
            .await;
        self.passability_add_house(&house).await;
        let (tree_result, grass_result) = tokio::join!(
            crate::housing::routes::remove_house_trees(&self.terrain_io, &house),
            crate::housing::routes::remove_house_grass(&self.terrain_io, &house),
        );
        let changed_tree_tiles = match tree_result {
            Ok(stats) => stats.changed_tiles,
            Err((_, error)) => {
                tracing::warn!(house_id = %house.id, %error, "House saved but tree clearing failed");
                Vec::new()
            }
        };
        let changed_grass_tiles = match grass_result {
            Ok(stats) => stats.changed_tiles,
            Err((_, error)) => {
                tracing::warn!(house_id = %house.id, %error, "House saved but grass clearing failed");
                Vec::new()
            }
        };
        crate::housing::routes::publish_house_terrain_changes(
            self,
            &changed_height_tiles,
            &changed_tree_tiles,
            &changed_grass_tiles,
        )
        .await;
        let name = self.item_name(&item_id);
        self.send_system_message(
            player_id,
            format!(
                "Construction complete: {}.",
                name.trim_end_matches(" Scroll")
            ),
        )
        .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_house_scroll_has_a_valid_closed_template() {
        for (item_id, _) in HOUSE_SCROLLS {
            let house = template_for_item(item_id).expect("template");
            validate_house(&house).expect("valid source house");
            assert!(house
                .rooms
                .iter()
                .flat_map(|room| [
                    &room.wall_north,
                    &room.wall_south,
                    &room.wall_east,
                    &room.wall_west,
                ])
                .flatten()
                .all(|wall| !wall.is_open));
        }
    }

    #[test]
    fn house_templates_rotate_in_quarter_turns() {
        for (item_id, _) in HOUSE_SCROLLS {
            let original = template_for_item(item_id).expect("template");
            for quarter_turns in 0..4 {
                let mut rotated = original.clone();
                for _ in 0..quarter_turns {
                    rotate_house_quarter_turn(&mut rotated);
                }
                validate_house(&rotated).expect("valid rotated template");
                assert!(rotated.passability.iter().all(|grid| {
                    grid.cells.len() == usize::from(grid.width) * usize::from(grid.depth)
                }));
            }

            let mut full_turn = original.clone();
            for _ in 0..4 {
                rotate_house_quarter_turn(&mut full_turn);
            }
            assert_eq!(
                serde_json::to_value(full_turn).unwrap(),
                serde_json::to_value(original).unwrap()
            );
        }
    }

    #[test]
    fn quarter_turn_rotates_room_walls_and_passability_edges() {
        use onlinerpg_shared::housing::WallVariant;

        let mut house = template_for_item("scroll_of_small_house").expect("template");
        assert!(house.rooms[0]
            .wall_south
            .iter()
            .any(|wall| wall.variant == WallVariant::WithDoor));
        rotate_house_quarter_turn(&mut house);
        assert_eq!((house.rooms[0].size_x, house.rooms[0].size_z), (4, 6));
        assert!(house.rooms[0]
            .wall_east
            .iter()
            .any(|wall| wall.variant == WallVariant::WithDoor));
        assert_eq!(rotate_edge_mask(0x01), 0x08);
        assert_eq!(rotate_edge_mask(0x02), 0x01);
        assert_eq!(rotate_edge_mask(0x04), 0x02);
        assert_eq!(rotate_edge_mask(0x08), 0x04);
    }

    #[test]
    fn foundation_requires_every_cell_to_be_owned() {
        let mut house = template_for_item("scroll_of_small_house").expect("template");
        house.origin.x = 0.0;
        house.origin.z = 0.0;
        assert!(foundation_inside_estate(
            &house,
            &[FencePlot { x: 0, z: 0 }]
        ));
        house.origin.x = 30.0;
        assert!(!foundation_inside_estate(
            &house,
            &[FencePlot { x: 0, z: 0 }]
        ));
    }
}
