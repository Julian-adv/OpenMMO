use super::{AuthService, NPC_ACCOUNT_PREFIX};
use onlinerpg_shared::{CharacterClass, Gender};
use rusqlite::{params, Connection};

/// Starter gear omits basePrice. (item_def_id, quantity, equip_slot)
const STARTER_ITEMS: &[(&str, u32, Option<&str>)] = &[
    ("worn_iron_sword", 1, Some("main_hand")),
    ("worn_torch", 1, None),
];

pub(super) fn starter_items(
    class: &CharacterClass,
) -> &'static [(&'static str, u32, Option<&'static str>)] {
    match class {
        CharacterClass::Rogue => &[
            ("worn_dagger", 1, Some("main_hand")),
            ("worn_torch", 1, None),
        ],
        CharacterClass::Priest => &[("worn_mace", 1, Some("main_hand")), ("worn_torch", 1, None)],
        _ => STARTER_ITEMS,
    }
}

pub(super) fn class_starter_items(
    class: &CharacterClass,
    gender: Gender,
) -> &'static [(&'static str, u32, Option<&'static str>)] {
    match class {
        CharacterClass::Knight => &[
            ("worn_plate_helmet", 1, Some("head")),
            ("worn_breastplate", 1, Some("chest")),
            ("worn_plate_greaves", 1, Some("pants")),
            ("worn_plate_boots", 1, Some("boots")),
            ("worn_plate_gauntlets", 1, Some("hands")),
        ],
        CharacterClass::Valkyrie => &[
            ("worn_valkyrie_helmet", 1, Some("head")),
            ("worn_valkyrie_top", 1, Some("chest")),
            ("worn_valkyrie_pants", 1, Some("pants")),
            ("worn_valkyrie_gloves", 1, Some("hands")),
            ("worn_valkyrie_boots", 1, Some("boots")),
        ],
        CharacterClass::Barbarian => &[
            ("worn_barbarian_helmet", 1, Some("head")),
            ("worn_barbarian_armor", 1, Some("chest")),
            ("worn_barbarian_pants", 1, Some("pants")),
            ("worn_barbarian_boots", 1, Some("boots")),
            ("worn_barbarian_bracers", 1, Some("hands")),
        ],
        CharacterClass::Caveman => &[
            ("worn_caveman_top", 1, Some("chest")),
            ("worn_caveman_pants", 1, Some("pants")),
            ("worn_caveman_boots", 1, Some("boots")),
            ("worn_caveman_bracers", 1, Some("hands")),
        ],
        CharacterClass::Rogue => &[
            ("worn_rogue_top", 1, Some("chest")),
            ("worn_rogue_pants", 1, Some("pants")),
            ("worn_rogue_gloves", 1, Some("hands")),
            ("worn_rogue_boots", 1, Some("boots")),
        ],
        CharacterClass::Ranger => &[
            ("worn_ranger_top", 1, Some("chest")),
            ("worn_ranger_pants", 1, Some("pants")),
            ("worn_ranger_gloves", 1, Some("hands")),
            ("worn_ranger_boots", 1, Some("boots")),
        ],
        CharacterClass::Priest if gender == Gender::Male => &[
            ("worn_priest_helmet", 1, Some("head")),
            ("worn_priest_top", 1, Some("chest")),
            ("worn_priest_pants", 1, Some("pants")),
            ("worn_priest_boots", 1, Some("boots")),
        ],
        CharacterClass::Priest => &[
            ("worn_priest_helmet", 1, Some("head")),
            ("worn_priest_top", 1, Some("chest")),
            ("worn_priest_pants", 1, Some("pants")),
            ("worn_priest_gloves", 1, Some("hands")),
            ("worn_priest_boots", 1, Some("boots")),
        ],
        CharacterClass::Bard if gender == Gender::Male => &[
            ("worn_bard_helmet", 1, Some("head")),
            ("worn_bard_top", 1, Some("chest")),
            ("worn_bard_pants", 1, Some("pants")),
            ("worn_bard_boots", 1, Some("boots")),
            ("worn_mandolin", 1, None),
        ],
        CharacterClass::Bard => &[
            ("worn_bard_helmet", 1, Some("head")),
            ("worn_bard_top", 1, Some("chest")),
            ("worn_bard_pants", 1, Some("pants")),
            ("worn_bard_gloves", 1, Some("hands")),
            ("worn_bard_boots", 1, Some("boots")),
            ("worn_mandolin", 1, None),
        ],
        _ => &[],
    }
}

impl AuthService {
    /// Gives existing characters the outfit new ones start with; worn slots stay as is.
    pub(super) fn migrate_starter_outfit(
        conn: &Connection,
        name: &str,
        class: CharacterClass,
        gender: Gender,
    ) -> Result<(), rusqlite::Error> {
        let tx = conn.unchecked_transaction()?;
        let done: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = ?1)",
            [name],
            |row| row.get(0),
        )?;
        if done {
            return Ok(());
        }
        let gender_str = match gender {
            Gender::Male => "male",
            Gender::Female => "female",
        };
        let mut characters = tx.prepare(
            "SELECT id, account_name, character_name FROM characters \
             WHERE gender = ?1 AND class = ?2",
        )?;
        for character in characters.query_map(params![gender_str, class.as_str()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (id, account, name) = character?;
            if account.starts_with(NPC_ACCOUNT_PREFIX)
                && crate::npc_defs::npc_defs().get_by_npc_name(&name).is_some()
            {
                continue;
            }
            for (item, quantity, slot) in class_starter_items(&class, gender) {
                tx.execute(
                    "INSERT INTO character_items (character_id, item_def_id, quantity) \
                     SELECT ?1, ?2, ?3 WHERE NOT EXISTS \
                     (SELECT 1 FROM character_items WHERE character_id = ?1 AND item_def_id = ?2)",
                    params![id, item, quantity],
                )?;
                tx.execute(
                    "UPDATE character_items SET equip_slot = ?3 WHERE id = \
                     (SELECT id FROM character_items WHERE character_id = ?1 AND item_def_id = ?2 \
                      AND equip_slot IS NULL ORDER BY id LIMIT 1) \
                     AND NOT EXISTS (SELECT 1 FROM character_items WHERE character_id = ?1 AND equip_slot = ?3)",
                    params![id, item, slot],
                )?;
            }
        }
        drop(characters);
        tx.execute("INSERT INTO migrations (name) VALUES (?1)", [name])?;
        tx.commit()
    }

    /// Without `replaces` the weapon only lands in the bag and worn gear stays as is.
    pub(super) fn migrate_starter_weapon(
        conn: &Connection,
        name: &str,
        class: CharacterClass,
        weapon: &str,
        replaces: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let tx = conn.unchecked_transaction()?;
        let done: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = ?1)",
            [name],
            |row| row.get(0),
        )?;
        if done {
            return Ok(());
        }
        let mut characters =
            tx.prepare("SELECT id, account_name, character_name FROM characters WHERE class = ?1")?;
        for row in characters.query_map([class.as_str()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (id, account, name) = row?;
            if account.starts_with(NPC_ACCOUNT_PREFIX)
                && crate::npc_defs::npc_defs().get_by_npc_name(&name).is_some()
            {
                continue;
            }
            tx.execute(
                "INSERT INTO character_items (character_id, item_def_id, quantity) \
                 SELECT ?1, ?2, 1 WHERE NOT EXISTS \
                 (SELECT 1 FROM character_items WHERE character_id = ?1 AND item_def_id = ?2)",
                params![id, weapon],
            )?;
            let Some(old) = replaces else { continue };
            tx.execute(
                "UPDATE character_items SET equip_slot = NULL \
                 WHERE character_id = ?1 AND item_def_id = ?2 AND equip_slot = 'main_hand'",
                params![id, old],
            )?;
            tx.execute(
                "UPDATE character_items SET equip_slot = 'main_hand' WHERE id = \
                 (SELECT id FROM character_items WHERE character_id = ?1 AND item_def_id = ?2 ORDER BY id LIMIT 1) \
                 AND NOT EXISTS (SELECT 1 FROM character_items WHERE character_id = ?1 AND equip_slot = 'main_hand')",
                params![id, weapon],
            )?;
        }
        drop(characters);
        tx.execute("INSERT INTO migrations (name) VALUES (?1)", [name])?;
        tx.commit()
    }

    /// Registry NPCs created before they skipped the starter kit still carry
    /// its worn sword (some equipped, in plain sight). Drop it; the torch
    /// stays — it is harmless in the bag and useful at night. Scoped to the
    /// registry, the same predicate as the skip: a free `npc_` agent account
    /// keeps its starter weapon.
    pub(super) fn strip_npc_starter_weapons(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "DELETE FROM character_items WHERE item_def_id = 'worn_iron_sword' \
             AND character_id IN \
             (SELECT id FROM characters WHERE character_name = ?1 \
              AND account_name GLOB ?2)",
        )?;
        let pattern = format!("{NPC_ACCOUNT_PREFIX}*");
        let mut stripped = 0;
        for name in crate::npc_defs::npc_defs().npc_names() {
            stripped += stmt.execute(params![name, pattern])?;
        }
        if stripped > 0 {
            tracing::info!(stripped, "Removed starter swords from NPC characters");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::plain_attributes;
    use crate::test_util::temp_auth;
    use crate::types::CharacterAttributes;
    use onlinerpg_shared::inventory::EquipSlot;

    #[test]
    fn knights_start_with_equipped_unpriced_plate_armor() {
        let (auth, _) = temp_auth("auth_knight_armor");
        let defs = crate::item_defs::item_defs();
        for (name, gender) in [("Armored", Gender::Male), ("LadyKnight", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let knight = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Knight,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(knight.id).unwrap();
            let starter = class_starter_items(&CharacterClass::Knight, gender);
            for (id, quantity, slot) in starter {
                let item = items.iter().find(|item| item.item_def_id == *id).unwrap();
                assert_eq!(item.quantity, *quantity);
                assert_eq!(item.equip_slot.as_deref(), *slot);
                let def = defs.get(id).unwrap();
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + starter.len());
            let worn = auth.load_character_equipment(knight.id).unwrap();
            assert_eq!(worn.armor.head.as_deref(), Some("worn_plate_helmet"));
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_breastplate"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_plate_greaves"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_plate_boots"));
            assert_eq!(worn.armor.hands.as_deref(), Some("worn_plate_gauntlets"));
            assert_eq!(worn.main_hand.as_deref(), Some("worn_iron_sword"));
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
            assert_eq!(knight.gold, 0);
        }

        let account = auth.login_google("sub-unarmored").unwrap();
        let wizard = auth
            .create_character(
                &account,
                "Unarmored",
                &plain_attributes(),
                10,
                CharacterClass::Wizard,
                Gender::Male,
            )
            .unwrap();
        assert_eq!(
            auth.load_inventory(wizard.id).unwrap().len(),
            STARTER_ITEMS.len()
        );
        assert_eq!(
            auth.load_character_equipment(wizard.id).unwrap().armor,
            Default::default()
        );
    }

    #[test]
    fn barbarians_start_with_equipped_unpriced_armor() {
        let (auth, _) = temp_auth("auth_barbarian_armor");
        for (name, gender) in [("Barbarian", Gender::Male), ("Amazon", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let character = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Barbarian,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(character.id).unwrap();
            let worn = auth.load_character_equipment(character.id).unwrap();
            for (id, slot) in [
                ("worn_barbarian_helmet", "head"),
                ("worn_barbarian_armor", "chest"),
                ("worn_barbarian_pants", "pants"),
                ("worn_barbarian_boots", "boots"),
                ("worn_barbarian_bracers", "hands"),
            ] {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + 5);
            assert_eq!(worn.main_hand.as_deref(), Some("worn_iron_sword"));
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            assert_eq!(worn.armor.head.as_deref(), Some("worn_barbarian_helmet"));
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_barbarian_armor"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_barbarian_pants"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_barbarian_boots"));
            assert_eq!(worn.armor.hands.as_deref(), Some("worn_barbarian_bracers"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
            assert_eq!(character.gold, 0);
        }
    }

    #[test]
    fn female_rogue_outfit_migration_preserves_equipment_and_runs_once() {
        let (auth, path) = temp_auth("female_rogue_outfit_migration");
        let account = auth.login_google("sub-legacy-rogue").unwrap();
        let female = auth
            .create_character(
                &account,
                "LegacyRogue",
                &plain_attributes(),
                10,
                CharacterClass::Rogue,
                Gender::Female,
            )
            .unwrap();
        let male = auth
            .create_character(
                &account,
                "MaleRogue",
                &plain_attributes(),
                10,
                CharacterClass::Rogue,
                Gender::Male,
            )
            .unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute(
            "DELETE FROM migrations WHERE name = 'female_rogue_starter_outfit_20261027'",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM character_items WHERE character_id = ?1 AND item_def_id IN ('worn_rogue_pants', 'worn_rogue_boots')", [female.id]).unwrap();
        conn.execute("UPDATE character_items SET equip_slot = NULL WHERE character_id = ?1 AND item_def_id = 'worn_rogue_gloves'", [female.id]).unwrap();
        conn.execute("UPDATE character_items SET equip_slot = 'chest', enchant = 3 WHERE character_id = ?1 AND item_def_id = 'worn_rogue_top'", [female.id]).unwrap();
        conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) VALUES (?1, 'leather_pants', 1, 'pants')", [female.id]).unwrap();
        let male_before = auth.load_inventory(male.id).unwrap().len();
        drop(conn);
        drop(auth);
        let auth = AuthService::new(path.clone()).unwrap();
        let items = auth.load_inventory(female.id).unwrap();
        assert_eq!(
            items
                .iter()
                .filter(|item| item.item_def_id == "worn_rogue_top")
                .count(),
            1
        );
        assert_eq!(
            items
                .iter()
                .find(|item| item.item_def_id == "worn_rogue_top")
                .unwrap()
                .enchant,
            3
        );
        let armor = auth.load_character_equipment(female.id).unwrap().armor;
        assert_eq!(armor.chest.as_deref(), Some("worn_rogue_top"));
        assert_eq!(armor.pants.as_deref(), Some("leather_pants"));
        assert_eq!(armor.hands.as_deref(), Some("worn_rogue_gloves"));
        assert_eq!(armor.boots.as_deref(), Some("worn_rogue_boots"));
        assert_eq!(auth.load_inventory(male.id).unwrap().len(), male_before);
        let conn = auth.open_connection().unwrap();
        conn.execute("DELETE FROM character_items WHERE character_id = ?1 AND item_def_id = 'worn_rogue_boots'", [female.id]).unwrap();
        drop(conn);
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        assert!(!auth
            .load_inventory(female.id)
            .unwrap()
            .iter()
            .any(|item| item.item_def_id == "worn_rogue_boots"));
    }

    #[test]
    fn valkyrie_outfit_migration_fills_empty_slots_and_runs_once() {
        let (auth, path) = temp_auth("valkyrie_outfit_migration");
        let account = auth.login_google("sub-legacy-valkyrie").unwrap();
        let valkyrie = auth
            .create_character(
                &account,
                "LegacyValkyrie",
                &plain_attributes(),
                10,
                CharacterClass::Valkyrie,
                Gender::Female,
            )
            .unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute(
            "DELETE FROM migrations WHERE name = 'valkyrie_starter_outfit_20261103'",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM character_items WHERE character_id = ?1 AND item_def_id LIKE 'worn_valkyrie_%'", [valkyrie.id]).unwrap();
        conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) VALUES (?1, 'chain_mail', 1, 'chest')", [valkyrie.id]).unwrap();
        drop(conn);
        drop(auth);
        let auth = AuthService::new(path.clone()).unwrap();
        let items = auth.load_inventory(valkyrie.id).unwrap();
        for (id, _, _) in class_starter_items(&CharacterClass::Valkyrie, Gender::Female) {
            assert_eq!(
                items.iter().filter(|item| item.item_def_id == *id).count(),
                1,
                "{id}"
            );
        }
        let armor = auth.load_character_equipment(valkyrie.id).unwrap().armor;
        assert_eq!(armor.head.as_deref(), Some("worn_valkyrie_helmet"));
        assert_eq!(armor.chest.as_deref(), Some("chain_mail"));
        assert_eq!(armor.pants.as_deref(), Some("worn_valkyrie_pants"));
        assert_eq!(armor.hands.as_deref(), Some("worn_valkyrie_gloves"));
        assert_eq!(armor.boots.as_deref(), Some("worn_valkyrie_boots"));
        let conn = auth.open_connection().unwrap();
        conn.execute("DELETE FROM character_items WHERE character_id = ?1 AND item_def_id = 'worn_valkyrie_boots'", [valkyrie.id]).unwrap();
        drop(conn);
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        assert!(!auth
            .load_inventory(valkyrie.id)
            .unwrap()
            .iter()
            .any(|item| item.item_def_id == "worn_valkyrie_boots"));
    }

    #[test]
    fn starter_weapon_migrations_keep_other_weapons_and_run_once() {
        for (class, migration, weapon, main_hands) in [
            (
                CharacterClass::Rogue,
                "rogue_starter_dagger_20261027",
                "worn_dagger",
                [
                    Some("worn_dagger"),
                    Some("steel_longsword"),
                    Some("worn_dagger"),
                    Some("worn_dagger"),
                ],
            ),
            (
                CharacterClass::Priest,
                "priest_starter_mace_20261031",
                "worn_mace",
                [Some("worn_iron_sword"), Some("steel_longsword"), None, None],
            ),
        ] {
            let (auth, path) = temp_auth(migration);
            let mut ids = Vec::new();
            for (name, gender) in [
                ("OldOne", Gender::Male),
                ("OldTwo", Gender::Female),
                ("OldThree", Gender::Male),
                ("OldFour", Gender::Female),
            ] {
                let account = auth
                    .login_google(&format!("sub-{migration}-{name}"))
                    .unwrap();
                let character = auth
                    .create_character(
                        &account,
                        name,
                        &plain_attributes(),
                        10,
                        class.clone(),
                        gender,
                    )
                    .unwrap();
                ids.push(character.id);
            }
            let conn = auth.open_connection().unwrap();
            conn.execute("DELETE FROM migrations WHERE name = ?1", [migration])
                .unwrap();
            for id in &ids {
                conn.execute(
                    "DELETE FROM character_items WHERE character_id = ?1 AND item_def_id = ?2",
                    params![id, weapon],
                )
                .unwrap();
            }
            conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot, enchant) VALUES (?1, 'worn_iron_sword', 1, 'main_hand', 2)", [ids[0]]).unwrap();
            conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, enchant) VALUES (?1, 'worn_iron_sword', 1, 1)", [ids[1]]).unwrap();
            conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) VALUES (?1, 'steel_longsword', 1, 'main_hand')", [ids[1]]).unwrap();
            conn.execute("INSERT INTO character_items (character_id, item_def_id, quantity, enchant) VALUES (?1, 'worn_iron_sword', 1, 3)", [ids[2]]).unwrap();
            drop(conn);
            drop(auth);
            let auth = AuthService::new(path.clone()).unwrap();
            for (index, (sword_enchant, held)) in [Some(2), Some(1), Some(3), None]
                .into_iter()
                .zip(main_hands)
                .enumerate()
            {
                let case = format!("{migration}:{index}");
                let items = auth.load_inventory(ids[index]).unwrap();
                let main_hand = auth.load_character_equipment(ids[index]).unwrap().main_hand;
                assert_eq!(main_hand.as_deref(), held, "{case}");
                let granted: Vec<_> = items
                    .iter()
                    .filter(|item| item.item_def_id == weapon)
                    .collect();
                assert_eq!(granted.len(), 1, "{case}");
                assert_eq!(granted[0].enchant, 0, "{case}");
                assert_eq!(
                    granted[0].equip_slot.is_some(),
                    held == Some(weapon),
                    "{case}"
                );
                let sword_slot = (held == Some("worn_iron_sword")).then_some("main_hand");
                assert_eq!(
                    items
                        .iter()
                        .find(|item| item.item_def_id == "worn_iron_sword")
                        .map(|sword| (sword.enchant, sword.equip_slot.as_deref())),
                    sword_enchant.map(|enchant| (enchant, sword_slot)),
                    "{case}"
                );
            }
            let conn = auth.open_connection().unwrap();
            conn.execute(
                "DELETE FROM character_items WHERE character_id = ?1 AND item_def_id = ?2",
                params![ids[0], weapon],
            )
            .unwrap();
            drop(conn);
            drop(auth);
            let auth = AuthService::new(path).unwrap();
            assert!(!auth
                .load_inventory(ids[0])
                .unwrap()
                .iter()
                .any(|item| item.item_def_id == weapon));
        }
    }

    #[test]
    fn valkyries_start_with_five_equipped_zero_price_untradeable_pieces() {
        let (auth, _) = temp_auth("auth_valkyrie_armor");
        let account = auth.login_google("sub-valkyrie").unwrap();
        let character = auth
            .create_character(
                &account,
                "Valkyrie",
                &plain_attributes(),
                10,
                CharacterClass::Valkyrie,
                Gender::Female,
            )
            .unwrap();
        let items = auth.load_inventory(character.id).unwrap();
        for (id, _, slot) in class_starter_items(&CharacterClass::Valkyrie, Gender::Female) {
            let item = items.iter().find(|item| item.item_def_id == *id).unwrap();
            assert_eq!(item.quantity, 1);
            assert_eq!(item.equip_slot.as_deref(), *slot);
            let def = crate::item_defs::item_defs().get(id).unwrap();
            assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), *slot);
            assert_eq!(def.base_price, Some(0));
            assert!(def.untradeable);
            assert!(def.chest_tier.is_none());
        }
        assert_eq!(items.len(), STARTER_ITEMS.len() + 5);
        assert!(!items
            .iter()
            .any(|item| item.item_def_id.contains("hair") || item.item_def_id.contains("wing")));
        let worn = auth.load_character_equipment(character.id).unwrap();
        assert_eq!(worn.armor.head.as_deref(), Some("worn_valkyrie_helmet"));
        assert_eq!(worn.armor.chest.as_deref(), Some("worn_valkyrie_top"));
        assert_eq!(worn.armor.pants.as_deref(), Some("worn_valkyrie_pants"));
        assert_eq!(worn.armor.hands.as_deref(), Some("worn_valkyrie_gloves"));
        assert_eq!(worn.armor.boots.as_deref(), Some("worn_valkyrie_boots"));
        assert_eq!(
            auth.list_characters_with_equipment(&account).unwrap()[0]
                .worn
                .armor,
            worn.armor
        );
    }

    #[test]
    fn rogues_start_with_four_equipped_unpriced_pieces_and_no_helmet() {
        let (auth, _) = temp_auth("auth_rogue_armor");
        for (name, gender) in [("Rogue", Gender::Male), ("Shadow", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let character = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Rogue,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(character.id).unwrap();
            for (id, slot) in [
                ("worn_rogue_top", "chest"),
                ("worn_rogue_pants", "pants"),
                ("worn_rogue_gloves", "hands"),
                ("worn_rogue_boots", "boots"),
            ] {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + 4);
            let worn = auth.load_character_equipment(character.id).unwrap();
            assert_eq!(worn.main_hand.as_deref(), Some("worn_dagger"));
            let dagger = crate::item_defs::item_defs().get("worn_dagger").unwrap();
            assert_eq!(
                dagger.weapon_type,
                Some(crate::item_defs::WeaponType::Dagger)
            );
            assert!(dagger.untradeable);
            assert_eq!(dagger.base_price, None);
            assert!(!items
                .iter()
                .any(|item| item.item_def_id == "worn_iron_sword"));
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            assert_eq!(worn.armor.head, None);
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_rogue_top"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_rogue_pants"));
            assert_eq!(worn.armor.hands.as_deref(), Some("worn_rogue_gloves"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_rogue_boots"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
        }
    }

    #[test]
    fn rangers_of_both_genders_start_with_four_equipped_unpriced_pieces_and_no_helmet() {
        let (auth, _) = temp_auth("auth_ranger_armor");
        for (name, gender) in [("Ranger", Gender::Male), ("Shadow", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let character = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Ranger,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(character.id).unwrap();
            for (id, slot) in [
                ("worn_ranger_top", "chest"),
                ("worn_ranger_pants", "pants"),
                ("worn_ranger_gloves", "hands"),
                ("worn_ranger_boots", "boots"),
            ] {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + 4);
            let worn = auth.load_character_equipment(character.id).unwrap();
            assert_eq!(worn.main_hand.as_deref(), Some("worn_iron_sword"));
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            assert_eq!(worn.armor.head, None);
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_ranger_top"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_ranger_pants"));
            assert_eq!(worn.armor.hands.as_deref(), Some("worn_ranger_gloves"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_ranger_boots"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
        }
    }

    #[test]
    fn priests_start_with_equipped_unpriced_outfit_and_gloves_for_women() {
        let (auth, _) = temp_auth("auth_priest_armor");
        for (name, gender) in [("Priest", Gender::Male), ("Shadow", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let character = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Priest,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(character.id).unwrap();
            let worn = auth.load_character_equipment(character.id).unwrap();
            assert_eq!(worn.main_hand.as_deref(), Some("worn_mace"));
            let starter = starter_items(&CharacterClass::Priest).len();
            let female = gender == Gender::Female;
            let mut outfit = vec![
                ("worn_priest_helmet", "head"),
                ("worn_priest_top", "chest"),
                ("worn_priest_pants", "pants"),
                ("worn_priest_boots", "boots"),
            ];
            if female {
                outfit.push(("worn_priest_gloves", "hands"));
            }
            for &(id, slot) in &outfit {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), starter + outfit.len());
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            assert_eq!(worn.armor.head.as_deref(), Some("worn_priest_helmet"));
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_priest_top"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_priest_pants"));
            assert_eq!(
                worn.armor.hands.as_deref(),
                female.then_some("worn_priest_gloves")
            );
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_priest_boots"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
        }
    }

    #[test]
    fn cavemen_start_with_four_equipped_unpriced_pieces_and_no_helmet() {
        let (auth, _) = temp_auth("auth_caveman_armor");
        for (name, gender) in [("Caveman", Gender::Male), ("Cavewoman", Gender::Female)] {
            let account = auth.login_google(&format!("sub-{name}")).unwrap();
            let character = auth
                .create_character(
                    &account,
                    name,
                    &plain_attributes(),
                    10,
                    CharacterClass::Caveman,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(character.id).unwrap();
            for (id, slot) in [
                ("worn_caveman_top", "chest"),
                ("worn_caveman_pants", "pants"),
                ("worn_caveman_bracers", "hands"),
                ("worn_caveman_boots", "boots"),
            ] {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
                assert!(def.chest_tier.is_none());
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + 4);
            let worn = auth.load_character_equipment(character.id).unwrap();
            assert_eq!(worn.main_hand.as_deref(), Some("worn_iron_sword"));
            assert!(items.iter().any(|item| item.item_def_id == "worn_torch"));
            assert_eq!(worn.armor.head, None);
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_caveman_top"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_caveman_pants"));
            assert_eq!(worn.armor.hands.as_deref(), Some("worn_caveman_bracers"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_caveman_boots"));
            let listed = auth.list_characters_with_equipment(&account).unwrap();
            assert_eq!(listed[0].worn.armor, worn.armor);
        }
    }

    #[test]
    fn a_bard_starts_with_a_worn_mandolin_on_top_of_the_common_kit() {
        let (auth, _) = temp_auth("auth_bard");
        let account = auth.login_google("sub-bard").unwrap();
        let attributes = CharacterAttributes {
            r#str: 10,
            dex: 12,
            con: 10,
            int: 10,
            wis: 10,
            cha: 14,
            guard: 0,
        };

        for (name, gender) in [("Lark", Gender::Female), ("Robin", Gender::Male)] {
            let bard = auth
                .create_character(
                    &account,
                    name,
                    &attributes,
                    12,
                    CharacterClass::Bard,
                    gender,
                )
                .unwrap();
            let items = auth.load_inventory(bard.id).unwrap();
            assert!(items.iter().any(|r| r.item_def_id == "worn_mandolin"));
            assert!(items.iter().any(|r| r.item_def_id == "worn_iron_sword"));
            let worn = auth.load_character_equipment(bard.id).unwrap();
            let female = gender == Gender::Female;
            let gloves = female.then_some(("worn_bard_gloves", "hands"));
            for (id, slot) in [
                ("worn_bard_helmet", "head"),
                ("worn_bard_top", "chest"),
                ("worn_bard_pants", "pants"),
                ("worn_bard_boots", "boots"),
            ]
            .into_iter()
            .chain(gloves)
            {
                let item = items.iter().find(|item| item.item_def_id == id).unwrap();
                assert_eq!(item.quantity, 1);
                assert_eq!(item.equip_slot.as_deref(), Some(slot));
                let def = crate::item_defs::item_defs().get(id).unwrap();
                assert_eq!(def.equip_slot.as_ref().map(EquipSlot::as_str), Some(slot));
                assert_eq!(def.base_price, None);
                assert!(def.untradeable);
                assert_eq!(def.guard, Some(1));
            }
            assert_eq!(items.len(), STARTER_ITEMS.len() + 5 + usize::from(female));
            assert_eq!(worn.armor.head.as_deref(), Some("worn_bard_helmet"));
            assert_eq!(worn.armor.chest.as_deref(), Some("worn_bard_top"));
            assert_eq!(worn.armor.pants.as_deref(), Some("worn_bard_pants"));
            assert_eq!(worn.armor.boots.as_deref(), Some("worn_bard_boots"));
            assert_eq!(
                worn.armor.hands.as_deref(),
                female.then_some("worn_bard_gloves")
            );
        }

        let knight = auth
            .create_character(
                &account,
                "Tass",
                &attributes,
                16,
                CharacterClass::Knight,
                Gender::Male,
            )
            .unwrap();
        let items = auth.load_inventory(knight.id).unwrap();
        assert!(items.iter().all(|r| r.item_def_id != "worn_mandolin"));
    }

    #[test]
    fn a_registry_npc_starts_bare_and_loses_a_legacy_starter_sword() {
        let (auth, db_path) = temp_auth("auth_npc");
        let account = auth.login_npc("npc_merchant").unwrap();
        let rica = auth
            .create_character(
                &account,
                "Rica",
                &plain_attributes(),
                10,
                CharacterClass::Merchant,
                Gender::Female,
            )
            .unwrap();
        assert!(
            auth.load_inventory(rica.id).unwrap().is_empty(),
            "registry NPCs must not receive the starter kit"
        );

        // A sword from before the skip is stripped on the next boot.
        auth.open_connection()
            .unwrap()
            .execute(
                "INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) \
                 VALUES (?1, 'worn_iron_sword', 1, 'main_hand')",
                params![rica.id],
            )
            .unwrap();
        drop(auth);
        let auth = AuthService::new(db_path).unwrap();
        assert!(auth.load_inventory(rica.id).unwrap().is_empty());
    }

    #[test]
    fn a_registry_npc_with_a_loadout_skips_the_starter_kit() {
        let (auth, _) = temp_auth("auth_loadout");
        let account = auth.login_npc("npc_loadout_test").unwrap();
        let def = crate::npc_defs::npc_defs().get_by_npc_name("Karl").unwrap();
        assert!(
            !def.loadout.is_empty(),
            "Karl's registry row carries a loadout"
        );
        let attributes = CharacterAttributes {
            r#str: 15,
            dex: 13,
            con: 14,
            int: 9,
            wis: 11,
            cha: 10,
            guard: 11,
        };

        let karl = auth
            .create_character(
                &account,
                "Karl",
                &attributes,
                20,
                CharacterClass::Guard,
                Gender::Male,
            )
            .unwrap();
        let items = auth.load_inventory(karl.id).unwrap();
        assert!(
            items.is_empty(),
            "issued gear comes from join-time seeding, not creation: {items:?}"
        );
    }
}
