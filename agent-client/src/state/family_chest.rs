use super::*;
use onlinerpg_shared::estate_storage::{EstateChest, EstateChestState};
use onlinerpg_shared::family::FAMILY_CHEST_ITEM;

const CHEST_SIGHT_M: f32 = 30.0;
/// An open chest stays in the prompt only while the agent stands at it.
const CHEST_PANEL_M: f32 = 6.0;
const LISTED_ITEMS: usize = 20;

pub struct FamilyChestPanel {
    pub state: EstateChestState,
}

fn access_text(state: &EstateChestState) -> &'static str {
    match (state.can_deposit, state.can_withdraw) {
        (true, true) => "you may store and take items",
        (true, false) => "you may store items but not take them",
        (false, true) => "you may take items but not store them",
        (false, false) => "your rank may neither store nor take items",
    }
}

impl SharedState {
    /// Family chests on our family's estates within `max_dist`, nearest first.
    pub fn family_chests_near(&self, max_dist: f32) -> Vec<(f32, EstateChest)> {
        let Some(player) = self.self_player.as_ref().filter(|_| self.family.is_some()) else {
            return Vec::new();
        };
        let mut near = self.world_cache.read().unwrap().estate_furniture_near(
            &player.position,
            max_dist,
            FAMILY_CHEST_ITEM,
        );
        near.retain(|(_, c)| self.family_estate(c.estate_id));
        near
    }

    /// The family chest an action means: the named one in sight, else the
    /// open one, else the nearest.
    pub fn family_chest_target(&self, chest_id: Option<i64>, max_dist: f32) -> Option<EstateChest> {
        let player = self.self_player.as_ref()?;
        let named = chest_id.or(self.open_family_chest.as_ref().map(|p| p.state.chest_id));
        if let Some(id) = named {
            let world = self.world_cache.read().unwrap();
            let chest = world
                .estate_furniture(id)
                .filter(|c| c.item_def_id == FAMILY_CHEST_ITEM)
                .filter(|c| c.position.dist_xz_sq(&player.position) <= max_dist * max_dist)
                .cloned();
            if chest.is_some() || chest_id.is_some() {
                return chest;
            }
        }
        self.family_chests_near(max_dist)
            .into_iter()
            .next()
            .map(|(_, c)| c)
    }

    pub(super) fn on_estate_chest_state(
        &mut self,
        state: &Option<EstateChestState>,
        error: &Option<String>,
    ) {
        self.family_chest_replies += 1;
        if let Some(error) = error {
            self.push_agent_event(format!(
                "[FamilyChestFailed] {}.",
                error.trim().trim_end_matches('.')
            ));
        }
        let Some(state) = state.as_ref().filter(|s| s.family) else {
            return;
        };
        self.family_chest_updates += 1;
        self.push_agent_event(format!(
            "[FamilyChest] Family chest [chest_id {}] holds {} stacks; {}.",
            state.chest_id,
            state.items.len(),
            access_text(state)
        ));
        self.open_family_chest = Some(FamilyChestPanel {
            state: state.clone(),
        });
    }

    pub(super) fn format_family_chests(&self, lines: &mut Vec<String>) {
        let chests = self.family_chests_near(CHEST_SIGHT_M);
        for (dist, c) in &chests {
            lines.push(format!(
                "Family chest [chest_id {}] at ({:.1}, {:.1}), {dist:.1}m away — open_family_chest to see inside",
                c.id, c.position.x, c.position.z
            ));
        }
        let Some(panel) = &self.open_family_chest else {
            return;
        };
        let state = &panel.state;
        if !chests
            .iter()
            .any(|(dist, c)| c.id == state.chest_id && *dist <= CHEST_PANEL_M)
        {
            return;
        }
        lines.push(format!(
            "Open family chest [chest_id {}] — {}:",
            state.chest_id,
            access_text(state)
        ));
        if state.items.is_empty() {
            lines.push("  (empty)".to_string());
        }
        for item in state.items.iter().take(LISTED_ITEMS) {
            lines.push(format!(
                "  {}{}{} [id {}]",
                item.item_def_id,
                if item.enchant > 0 {
                    format!(" +{}", item.enchant)
                } else {
                    String::new()
                },
                if item.quantity > 1 {
                    format!(" x{}", item.quantity)
                } else {
                    String::new()
                },
                item.instance_id
            ));
        }
        if state.items.len() > LISTED_ITEMS {
            lines.push(format!(
                "  ({} more stacks not shown)",
                state.items.len() - LISTED_ITEMS
            ));
        }
    }
}
