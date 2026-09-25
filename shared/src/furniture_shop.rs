use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FurnitureShop {
    pub name: String,
    pub clerk_npc_name: String,
    pub house_id: String,
    pub region: [i32; 2],
    pub bounds: [f32; 4],
    pub products: Vec<FurnitureProduct>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FurnitureProduct {
    pub object_type: String,
    pub item_def_id: String,
    pub price: i64,
    pub display_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FurnitureOrderLine {
    pub display_id: u32,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FurnitureTip {
    StorageChest,
    Bed,
}

impl FurnitureTip {
    pub fn for_item(item_def_id: &str) -> Option<Self> {
        match item_def_id {
            "storage_chest" => Some(Self::StorageChest),
            "furniture_bed" | "furniture_rustic_bed" => Some(Self::Bed),
            _ => None,
        }
    }
}

pub static SHOP: LazyLock<FurnitureShop> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../data/furniture_shop.json"))
        .expect("Invalid furniture shop")
});

pub fn quote(lines: &[FurnitureOrderLine]) -> Result<i64, &'static str> {
    if lines.is_empty() || lines.len() > 64 {
        return Err("Choose between 1 and 64 furniture pieces.");
    }
    let mut count = 0u32;
    let mut total = 0i64;
    let mut seen = std::collections::HashSet::new();
    for line in lines {
        if line.quantity == 0 || line.quantity > 64 || !seen.insert(line.display_id) {
            return Err("Invalid furniture quantity.");
        }
        let product = SHOP
            .products
            .iter()
            .find(|product| product.display_ids.contains(&line.display_id))
            .ok_or("That display is not for sale.")?;
        count += line.quantity;
        total += product.price * i64::from(line.quantity);
    }
    if count > 64 {
        return Err("Choose at most 64 furniture pieces.");
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_displays_sell_placeable_furniture() {
        let mut displays = std::collections::HashSet::new();
        assert_eq!(SHOP.products.len(), 23);
        for product in &SHOP.products {
            let definition =
                crate::estate_storage::estate_storage_def(&product.item_def_id).unwrap();
            assert_eq!(definition.model_id, product.object_type);
            assert_eq!(
                definition.capacity_kg,
                if product.item_def_id == "storage_chest" {
                    50.0
                } else {
                    0.0
                }
            );
            assert!(product.price > 0);
            for id in &product.display_ids {
                assert!(displays.insert(id));
            }
        }
        assert_eq!(displays.len(), 25);
        assert!(!displays.contains(&84));
    }

    #[test]
    fn checkout_rejects_unknown_displays_and_invalid_quantities() {
        let line = |display_id, quantity| FurnitureOrderLine {
            display_id,
            quantity,
        };
        assert_eq!(quote(&[line(103, 2), line(106, 1)]), Ok(600));
        assert_eq!(quote(&[line(94, 1)]), Ok(1200));
        for lines in [
            vec![],
            vec![line(84, 1)],
            vec![line(103, 0)],
            vec![line(103, 65)],
            vec![line(103, 1), line(103, 1)],
            vec![line(103, 64), line(106, 1)],
        ] {
            assert!(quote(&lines).is_err());
        }
    }
}
