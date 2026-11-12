//! Nightly maze redraw for `nightlyLayout` dungeons (doc/DUNGEON.md).

use onlinerpg_shared::dungeon::{
    dungeon_cache_key, dungeon_passability, generate_dungeon_for, generate_dungeon_variant,
    layout_is_traversable, FloorLayout,
};
use onlinerpg_shared::fnv::{fnv1a64, FNV_OFFSET};
use onlinerpg_shared::ServerMessage;
use tracing::{info, warn};

use super::GameState;
use crate::dungeon_defs::DungeonEntranceDef;

const VARIANT_ATTEMPTS: u8 = 16;

/// First variant of the night whose maze keeps the dungeon's floor count and
/// is walkable to the chest. A function of the epoch, so a restart mid-night
/// redraws the same maze.
pub(super) fn night_variant(def: &DungeonEntranceDef, epoch: i64) -> (u32, Vec<FloorLayout>) {
    let fixed = generate_dungeon_for(&def.id);
    for attempt in 0..VARIANT_ATTEMPTS {
        let hash = fnv1a64(fnv1a64(FNV_OFFSET, epoch.to_le_bytes()), [attempt]);
        let variant = (hash as u32).max(1);
        let layouts = generate_dungeon_variant(&def.id, variant);
        if layouts.len() == fixed.len() && layout_is_traversable(&def.position(), &layouts) {
            return (variant, layouts);
        }
        warn!(
            "Dungeon '{}' variant {variant} (night {epoch}, attempt {attempt}) rejected: {} of {} floors or no way down",
            def.id,
            layouts.len(),
            fixed.len()
        );
    }
    warn!(
        "Dungeon '{}' found no usable maze for night {epoch}; keeping the fixed layout",
        def.id
    );
    (0, fixed)
}

impl GameState {
    pub(super) fn dungeon_variant(&self, entrance_id: &str) -> u32 {
        self.dungeon_variants
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(entrance_id)
            .copied()
            .unwrap_or(0)
    }

    fn set_dungeon_variant(&self, entrance_id: &str, variant: u32) {
        let mut variants = self
            .dungeon_variants
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if variant == 0 {
            variants.remove(entrance_id);
        } else {
            variants.insert(entrance_id.to_string(), variant);
        }
    }

    pub(crate) fn dungeon_layout_variants_message(&self) -> ServerMessage {
        let mut variants: Vec<(String, u32)> = self
            .dungeon_variants
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, variant)| (id.clone(), *variant))
            .collect();
        variants.sort();
        ServerMessage::DungeonLayoutVariants { variants }
    }

    /// Startup, before `init_passability`: pick tonight's maze.
    pub(crate) fn init_dungeon_variants(&self) {
        let epoch = Self::night_epoch(self.current_total_game_seconds());
        for def in self.dungeon_defs.all().filter(|def| def.nightly_layout) {
            let (variant, _) = night_variant(def, epoch);
            self.set_dungeon_variant(&def.id, variant);
            info!(
                "Dungeon '{}' maze variant {variant} (night {epoch})",
                def.id
            );
        }
    }

    /// Sunset, after the occupants are out: swap in the night's maze along
    /// with everything derived from the old one.
    pub(super) async fn redraw_nightly_dungeons(&self, epoch: i64) {
        let mut redrawn = false;
        for def in self.dungeon_defs.all().filter(|def| def.nightly_layout) {
            let Ok((variant, layouts)) =
                tokio::task::spawn_blocking(move || night_variant(def, epoch)).await
            else {
                warn!("Dungeon '{}' maze generation panicked", def.id);
                continue;
            };
            let old_variant = self.dungeon_variant(&def.id);
            if variant == old_variant {
                continue;
            }
            let old_layouts = {
                let mut dungeons = self.dungeons.write().await;
                let old = match dungeons.get_mut(&def.id) {
                    Some(rt) if rt.occupied() => {
                        warn!(
                            "Dungeon '{}' still occupied after the reset; keeping variant {old_variant}",
                            def.id
                        );
                        continue;
                    }
                    Some(rt) => Some(rt.redraw(layouts.clone())),
                    None => None,
                };
                // Under the runtime lock so `ensure_dungeon_runtime` cannot
                // install the old maze after the swap.
                self.set_dungeon_variant(&def.id, variant);
                old
            };
            let old_layouts =
                old_layouts.unwrap_or_else(|| generate_dungeon_variant(&def.id, old_variant));
            let passability = dungeon_passability(&def.position(), &layouts);
            {
                let mut interest = self.interest_lock();
                interest.remove_dungeon_floors(&def.id, &old_layouts);
                interest.seed_dungeon(&def.id, &def.position(), &layouts);
            }
            self.passability_write()
                .insert(dungeon_cache_key(&def.id), passability);
            let cleared = self.clear_dungeon_ground_items(def).await;
            info!(
                "Dungeon '{}' redrawn for night {epoch}: variant {old_variant} -> {variant}, {cleared} ground item(s) cleared",
                def.id
            );
            redrawn = true;
        }
        if redrawn {
            self.broadcast(self.dungeon_layout_variants_message());
        }
    }

    /// Loot left underground would sit inside the new maze's rock.
    async fn clear_dungeon_ground_items(&self, def: &DungeonEntranceDef) -> usize {
        let mut removed = Vec::new();
        self.ground_items.write().await.retain(|id, sgi| {
            let item = &sgi.item;
            let buried =
                item.floor_level < 0 && def.footprint_contains(item.position.x, item.position.z);
            if buried {
                removed.push((*id, item.position, item.floor_level));
            }
            !buried
        });
        for (id, position, floor_level) in &removed {
            self.publish_nearby(
                position,
                *floor_level,
                ServerMessage::GroundItemRemoved {
                    instance_id: *id,
                    picked_up_by: None,
                },
                None,
            )
            .await;
        }
        removed.len()
    }
}
