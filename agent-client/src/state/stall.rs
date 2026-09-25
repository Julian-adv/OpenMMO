use super::*;
use onlinerpg_shared::stall::{StallBuyOrder, StallListing};

pub struct StallPanel {
    pub stall_id: u64,
    pub owner_name: String,
    pub owned: bool,
    pub listings: Vec<StallListing>,
    pub buy_orders: Vec<StallBuyOrder>,
}

impl SharedState {
    pub(super) fn format_stalls(&self, lines: &mut Vec<String>) {
        let Some(player) = &self.self_player else {
            return;
        };
        let mut stalls: Vec<_> = self
            .stalls
            .values()
            .filter(|stall| stall.floor_level == self.self_floor_level)
            .collect();
        stalls.sort_by_key(|stall| stall.id);
        for stall in stalls {
            lines.push(format!(
                "Stall [id {}] of {} at ({:.1}, {:.1}), {:.1}m away: {:?} — open_stall to see offers",
                stall.id, stall.owner_name, stall.position.x, stall.position.z,
                stall.position.dist_xz_sq(&player.position).sqrt(), stall.sign,
            ));
        }
        let Some(panel) = &self.open_stall else {
            return;
        };
        lines.push(format!(
            "Open stall [id {}] of {}{}:",
            panel.stall_id,
            panel.owner_name,
            if panel.owned { " (yours)" } else { "" }
        ));
        for listing in &panel.listings {
            lines.push(format!(
                "For sale [instance_id {}]: {} +{} x{}, {} copper each",
                listing.instance_id,
                listing.item_def_id,
                listing.enchant,
                listing.quantity,
                listing.unit_price
            ));
        }
        for order in &panel.buy_orders {
            lines.push(format!(
                "Wanted [order_id {}]: {} +{} x{}, {} copper each before 5% seller tax",
                order.order_id, order.item_def_id, order.enchant, order.quantity, order.unit_price
            ));
        }
        if panel.listings.is_empty() && panel.buy_orders.is_empty() {
            lines.push("No current offers.".to_string());
        }
        for item in &self.self_bag {
            if !item.locked
                && (panel.owned
                    || panel.buy_orders.iter().any(|order| {
                        order.item_def_id == item.item_def_id && order.enchant == item.enchant
                    }))
            {
                lines.push(format!(
                    "Your bag [instance_id {}]: {} +{} x{}",
                    item.instance_id, item.item_def_id, item.enchant, item.quantity
                ));
            }
        }
    }
}
