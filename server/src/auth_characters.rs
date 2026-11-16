use super::starter_items::{class_starter_items, starter_items};
use super::titles::read_titles;
use super::valid_character_name;
use super::{family, AuthError, AuthService, NPC_ACCOUNT_PREFIX};
use crate::types::CharacterAttributes;
use crate::world_config::world_config;
use onlinerpg_shared::inventory::EquipSlot;
use onlinerpg_shared::{CharacterAppearance, CharacterClass, Gender, VisibleEquipment};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use std::collections::HashMap;

/// One character-select row: the record plus what the preview shows.
pub struct CharacterListing {
    pub record: CharacterRecord,
    pub worn: VisibleEquipment,
    pub titles: Vec<String>,
    pub active_title: Option<String>,
    pub family: Option<String>,
}

impl CharacterListing {
    /// A character with nothing earned yet — the one just created.
    pub fn fresh(record: CharacterRecord, worn: VisibleEquipment) -> Self {
        Self {
            record,
            worn,
            titles: Vec::new(),
            active_title: None,
            family: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharacterRecord {
    pub id: i64,
    pub deletion_due_at: Option<i64>,
    pub name: String,
    pub created_at: i64,
    pub level: u32,
    pub xp: u64,
    pub max_hp: u32,
    pub attributes: CharacterAttributes,
    pub class: CharacterClass,
    pub gender: Gender,
    pub appearance: CharacterAppearance,
    pub last_x: f32,
    pub last_y: f32,
    pub last_z: f32,
    pub last_rotation: f32,
    pub health: Option<u32>,
    pub mana: Option<u32>,
    pub floor_level: i8,
    pub dungeon_epoch: Option<i64>,
    pub gold: i64,
    /// Nonzero unlocks admin for ADMIN_EMAILS-allowlisted accounts (tiers reserved).
    pub admin_role: i64,
    pub satiation: u32,
}

/// Column list shared between queries that return full CharacterRecord rows.
const CHARACTER_COLUMNS: &str = "id, character_name, created_at, level, xp, max_hp, attr_str, attr_dex, attr_con, attr_int, attr_wis, attr_cha, attr_guard, class, last_x, last_y, last_z, last_rotation, health, floor_level, gender, gold, admin_role, satiation, mana, dungeon_epoch, deletion_due_at, appearance";

pub(super) fn class_from_row(
    row: &rusqlite::Row<'_>,
    idx: usize,
) -> rusqlite::Result<CharacterClass> {
    let class_str: String = row.get(idx)?;
    class_str.parse::<CharacterClass>().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            format!("unknown character class: {class_str}").into(),
        )
    })
}

fn character_record_from_row(row: &rusqlite::Row) -> rusqlite::Result<CharacterRecord> {
    let record = CharacterRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        level: row.get(3)?,
        xp: row.get::<_, i64>(4)? as u64,
        max_hp: row.get(5)?,
        attributes: CharacterAttributes {
            r#str: row.get(6)?,
            dex: row.get(7)?,
            con: row.get(8)?,
            int: row.get(9)?,
            wis: row.get(10)?,
            cha: row.get(11)?,
            guard: row.get(12)?,
        },
        class: class_from_row(row, 13)?,
        last_x: row.get::<_, f64>(14).unwrap_or(0.0) as f32,
        last_y: row.get::<_, f64>(15).unwrap_or(0.0) as f32,
        last_z: row.get::<_, f64>(16).unwrap_or(0.0) as f32,
        last_rotation: row.get::<_, f64>(17).unwrap_or(0.0) as f32,
        health: row
            .get::<_, Option<i64>>(18)
            .ok()
            .flatten()
            .map(|v| v as u32),
        floor_level: row.get::<_, i64>(19).unwrap_or(0) as i8,
        gender: match row
            .get::<_, String>(20)
            .unwrap_or_else(|_| "male".to_string())
            .as_str()
        {
            "female" => Gender::Female,
            _ => Gender::Male,
        },
        gold: row.get::<_, i64>(21).unwrap_or(0),
        admin_role: row.get::<_, i64>(22).unwrap_or(0),
        satiation: row
            .get::<_, i64>(23)
            .unwrap_or(i64::from(onlinerpg_shared::hunger::SATIATION_START))
            .clamp(0, i64::from(onlinerpg_shared::hunger::SATIATION_MAX)) as u32,
        mana: row
            .get::<_, Option<i64>>(24)?
            .map(|v| v.clamp(0, i64::from(u32::MAX)) as u32),
        dungeon_epoch: row.get(25)?,
        deletion_due_at: row.get(26)?,
        appearance: {
            let json: String = row.get(27)?;
            serde_json::from_str(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    27,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?
        },
    };
    Ok(CharacterRecord {
        appearance: record
            .appearance
            .for_character(&record.class, record.gender),
        ..record
    })
}

fn read_characters(
    conn: &Connection,
    account_name: &str,
) -> Result<Vec<CharacterRecord>, AuthError> {
    let account_name = account_name.trim();
    if account_name.is_empty() {
        return Err(AuthError::InvalidInput("Account name is required"));
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT {}
         FROM characters
         WHERE account_name = ?1
         ORDER BY created_at ASC, id ASC",
        CHARACTER_COLUMNS
    ))?;

    let characters = stmt
        .query_map(params![account_name], character_record_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(characters)
}

/// Slots the character-select preview draws.
const PREVIEW_EQUIP_SLOTS: [EquipSlot; 8] = [
    EquipSlot::MainHand,
    EquipSlot::OffHand,
    EquipSlot::Back,
    EquipSlot::Head,
    EquipSlot::Chest,
    EquipSlot::Pants,
    EquipSlot::Boots,
    EquipSlot::Hands,
];

fn read_visible_equipment(
    conn: &Connection,
    character_ids: &[i64],
) -> Result<HashMap<i64, VisibleEquipment>, AuthError> {
    if character_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let slots = PREVIEW_EQUIP_SLOTS.map(|slot| format!("'{}'", slot.as_str()));
    // A family emblem covers the worn cape, as in the world.
    let mut stmt = conn.prepare(&format!(
        "SELECT i.character_id, i.equip_slot, i.item_def_id,
                CASE WHEN f.emblem_texture IS NULL AND f.emblem_color IS NULL THEN i.cape_color
                     ELSE COALESCE(f.emblem_color, i.cape_color) END,
                CASE WHEN f.emblem_texture IS NULL AND f.emblem_color IS NULL THEN i.cape_texture
                     ELSE f.emblem_texture END
         FROM character_items i
         LEFT JOIN family_members m ON m.character_id=i.character_id
         LEFT JOIN families f ON f.id=m.family_id AND m.cape_emblem=1
         WHERE i.character_id IN ({})
           AND i.equip_slot IN ({})",
        vec!["?"; character_ids.len()].join(","),
        slots.join(",")
    ))?;

    let rows = stmt.query_map(params_from_iter(character_ids), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut equipment: HashMap<i64, VisibleEquipment> = HashMap::new();
    for row in rows {
        let (character_id, slot, item_def_id, cape_color, cape_texture) = row?;
        let entry = equipment.entry(character_id).or_default();
        match slot.parse() {
            Ok(EquipSlot::Head) => entry.armor.head = Some(item_def_id),
            Ok(EquipSlot::Chest) => entry.armor.chest = Some(item_def_id),
            Ok(EquipSlot::Pants) => entry.armor.pants = Some(item_def_id),
            Ok(EquipSlot::Boots) => entry.armor.boots = Some(item_def_id),
            Ok(EquipSlot::Hands) => entry.armor.hands = Some(item_def_id),
            Ok(EquipSlot::MainHand) => entry.main_hand = Some(item_def_id),
            Ok(EquipSlot::OffHand) => entry.off_hand = Some(item_def_id),
            Ok(EquipSlot::Back) => {
                entry.back = Some(item_def_id);
                entry.back_color = cape_color;
                entry.back_texture = cape_texture;
            }
            _ => {}
        }
    }
    Ok(equipment)
}

impl AuthService {
    /// The character list plus the gear its preview draws. Both reads share one
    /// connection: this runs on every login, so it costs one checkout, not two.
    pub fn list_characters_with_equipment(
        &self,
        account_name: &str,
    ) -> Result<Vec<CharacterListing>, AuthError> {
        let conn = self.open_connection()?;
        let characters = read_characters(&conn, account_name)?;
        let ids = characters.iter().map(|c| c.id).collect::<Vec<_>>();
        let mut equipment = read_visible_equipment(&conn, &ids)?;
        let mut titles = read_titles(&conn, &ids)?;
        let mut families = family::read_family_names(&conn, account_name)?;
        Ok(characters
            .into_iter()
            .map(|record| {
                let worn = equipment.remove(&record.id).unwrap_or_default();
                let (titles, active_title) = titles.remove(&record.id).unwrap_or_default();
                let family = families.remove(&record.id);
                CharacterListing {
                    record,
                    worn,
                    titles,
                    active_title,
                    family,
                }
            })
            .collect())
    }

    /// The preview's gear for one character — the freshly created one.
    pub fn load_character_equipment(
        &self,
        character_id: i64,
    ) -> Result<VisibleEquipment, AuthError> {
        let conn = self.open_connection()?;
        Ok(read_visible_equipment(&conn, &[character_id])?
            .remove(&character_id)
            .unwrap_or_default())
    }

    pub fn character_names(
        &self,
        character_ids: &[i64],
    ) -> Result<HashMap<i64, String>, AuthError> {
        if character_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.open_connection()?;
        let placeholders = vec!["?"; character_ids.len()].join(",");
        let mut statement = conn.prepare(&format!(
            "SELECT id, character_name FROM characters WHERE id IN ({placeholders})"
        ))?;
        let rows = statement.query_map(params_from_iter(character_ids), |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Match a character name ignoring ASCII case.
    pub fn resolve_character_brief(&self, name: &str) -> Result<Option<(i64, String)>, AuthError> {
        let conn = self.open_connection()?;
        let found = conn
            .query_row(
                "SELECT id, character_name FROM characters
                 WHERE character_name = ?1 COLLATE NOCASE",
                params![name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(found)
    }

    /// Canonical name and owning account for a character, matched ignoring
    /// ASCII case like the other name lookups. `None` when no such character
    /// exists.
    pub fn account_of_character(
        &self,
        character_name: &str,
    ) -> Result<Option<(String, String)>, AuthError> {
        let conn = self.open_connection()?;
        let found = conn
            .query_row(
                "SELECT character_name, account_name FROM characters
                 WHERE character_name = ?1 COLLATE NOCASE",
                params![character_name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(found)
    }

    #[cfg(test)]
    pub fn create_character(
        &self,
        account_name: &str,
        character_name: &str,
        attributes: &CharacterAttributes,
        max_hp: u32,
        class: CharacterClass,
        gender: Gender,
    ) -> Result<CharacterRecord, AuthError> {
        self.create_character_with_appearance(
            account_name,
            character_name,
            attributes,
            max_hp,
            class,
            gender,
            CharacterAppearance::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_character_with_appearance(
        &self,
        account_name: &str,
        character_name: &str,
        attributes: &CharacterAttributes,
        max_hp: u32,
        class: CharacterClass,
        gender: Gender,
        appearance: CharacterAppearance,
    ) -> Result<CharacterRecord, AuthError> {
        let appearance = appearance.for_character(&class, gender);
        let account_name = account_name.trim();
        let character_name = character_name.trim();

        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }

        let conn = self.open_connection()?;

        let account_exists: Option<String> = conn
            .query_row(
                "SELECT player_name FROM accounts WHERE player_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .optional()?;
        if account_exists.is_none() {
            return Err(AuthError::AccountNotFound);
        }

        let character_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM characters WHERE account_name = ?1",
            params![account_name],
            |row| row.get(0),
        )?;
        if character_count >= 3 {
            return Err(AuthError::CharacterLimitReached);
        }

        self.check_new_character_name(&conn, account_name, character_name, None)?;

        let gender_str = gender.as_str();

        conn.execute(
            "INSERT INTO characters (
                account_name,
                character_name,
                level,
                max_hp,
                attr_str,
                attr_dex,
                attr_con,
                attr_int,
                attr_wis,
                attr_cha,
                attr_guard,
                class,
                gender,
                last_x,
                last_y,
                last_z,
                last_rotation,
                gold,
                appearance
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 0, ?18)",
            params![
                account_name,
                character_name,
                1_i64,
                i64::from(max_hp),
                i64::from(attributes.r#str),
                i64::from(attributes.dex),
                i64::from(attributes.con),
                i64::from(attributes.int),
                i64::from(attributes.wis),
                i64::from(attributes.cha),
                i64::from(attributes.guard),
                class.as_str(),
                gender_str,
                f64::from(world_config().spawn_position.x),
                f64::from(world_config().spawn_position.y),
                f64::from(world_config().spawn_position.z),
                f64::from(world_config().spawn_position.rotation),
                serde_json::to_string(&appearance).expect("appearance serializes"),
            ],
        )?;

        let id = conn.last_insert_rowid();

        // Registry NPCs receive their equipment from the loadout.
        let registry_npc = account_name.starts_with(NPC_ACCOUNT_PREFIX)
            && crate::npc_defs::npc_defs()
                .get_by_npc_name(character_name)
                .is_some();
        if !registry_npc {
            let mut stmt = conn.prepare(
                "INSERT INTO character_items (character_id, item_def_id, quantity, equip_slot) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (item_def_id, quantity, equip_slot) in starter_items(&class)
                .iter()
                .chain(class_starter_items(&class, gender))
            {
                stmt.execute(params![id, item_def_id, quantity, equip_slot])?;
            }
        }
        let created_at: i64 = conn.query_row(
            "SELECT created_at FROM characters WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok(CharacterRecord {
            id,
            name: character_name.to_string(),
            deletion_due_at: None,
            created_at,
            level: 1,
            xp: 0,
            max_hp,
            attributes: attributes.clone(),
            class,
            gender,
            appearance,
            last_x: world_config().spawn_position.x,
            last_y: world_config().spawn_position.y,
            last_z: world_config().spawn_position.z,
            last_rotation: world_config().spawn_position.rotation,
            health: None,
            mana: None,
            floor_level: 0,
            dungeon_epoch: None,
            gold: 0,
            admin_role: 0,
            satiation: onlinerpg_shared::hunger::SATIATION_START,
        })
    }

    /// Give one of an account's characters a new name — how a player whose
    /// name later lands on the banned list gets back in.
    pub fn rename_character(
        &self,
        account_name: &str,
        character_id: i64,
        new_name: &str,
    ) -> Result<String, AuthError> {
        let account_name = account_name.trim();
        let new_name = new_name.trim();

        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if character_id <= 0 {
            return Err(AuthError::CharacterNotFound);
        }

        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;

        let old_name: String = tx
            .query_row(
                "SELECT character_name FROM characters WHERE id = ?1 AND account_name = ?2",
                params![character_id, account_name],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(AuthError::CharacterNotFound)?;

        self.check_new_character_name(&tx, account_name, new_name, Some(&old_name))?;

        tx.execute(
            "UPDATE characters SET character_name = ?1 WHERE id = ?2",
            params![new_name, character_id],
        )?;
        // `/block` lists key on the name, so carry them across the rename.
        tx.execute(
            "UPDATE character_blocks SET blocked_name = ?1 WHERE blocked_name = ?2 COLLATE NOCASE",
            params![new_name, old_name],
        )?;
        tx.commit()?;

        Ok(new_name.to_string())
    }

    pub fn delete_character(&self, account_name: &str, character_id: i64) -> Result<(), AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if character_id <= 0 {
            return Err(AuthError::CharacterNotFound);
        }

        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let replacement: Option<i64> = tx
            .query_row(
                "SELECT id FROM characters WHERE account_name=?1 AND id<>?2 ORDER BY id LIMIT 1",
                params![account_name, character_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(replacement) = replacement {
            tx.execute(
                "UPDATE land_estates SET owner_id=?3 WHERE owner_id=?2 AND account_name=?1",
                params![account_name, character_id, replacement],
            )?;
            tx.execute(
                "UPDATE estate_chests SET owner_id=?3 WHERE owner_id=?2 AND
                 EXISTS(SELECT 1 FROM characters WHERE id=?2 AND account_name=?1)",
                params![account_name, character_id, replacement],
            )?;
            tx.execute(
                "UPDATE land_house_claims SET owner_id=?3 WHERE owner_id=?2 AND
                 EXISTS(SELECT 1 FROM characters WHERE id=?2 AND account_name=?1)",
                params![account_name, character_id, replacement],
            )?;
        }
        let rows_affected = tx.execute(
            "DELETE FROM characters WHERE id = ?1 AND account_name = ?2",
            params![character_id, account_name],
        )?;

        if rows_affected == 0 {
            return Err(AuthError::CharacterNotFound);
        }
        tx.commit()?;
        tracing::info!(account = account_name, character_id, "Character deleted");
        Ok(())
    }

    pub fn get_character_for_account(
        &self,
        account_name: &str,
        character_id: i64,
    ) -> Result<CharacterRecord, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if character_id <= 0 {
            return Err(AuthError::CharacterNotFound);
        }

        let conn = self.open_connection()?;
        let character = conn
            .query_row(
                &format!(
                    "SELECT {}
                     FROM characters
                     WHERE id = ?1 AND account_name = ?2",
                    CHARACTER_COLUMNS
                ),
                params![character_id, account_name],
                character_record_from_row,
            )
            .optional()?;

        character.ok_or(AuthError::CharacterNotFound)
    }

    /// The gate every new character name passes. `current_name` is the name
    /// being replaced, so a rename that only changes case is not a conflict
    /// with itself.
    fn check_new_character_name(
        &self,
        conn: &Connection,
        account_name: &str,
        name: &str,
        current_name: Option<&str>,
    ) -> Result<(), AuthError> {
        if !valid_character_name(name) {
            return Err(AuthError::InvalidCharacterName);
        }
        if self.is_name_banned_for(account_name, name) {
            return Err(AuthError::BannedCharacterName);
        }
        if current_name.is_some_and(|current| current.eq_ignore_ascii_case(name)) {
            return Ok(());
        }
        let taken: Option<i64> = conn
            .query_row(
                "SELECT id FROM characters WHERE character_name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() || Self::family_name_in_use(conn, name)? {
            return Err(AuthError::CharacterNameAlreadyExists);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::{create, plain_attributes};
    use crate::test_util::temp_auth;

    fn banned_name_auth(tag: &str) -> AuthService {
        temp_auth(tag).0.with_banned_names(
            ["gm".to_string(), "운영자".to_string()]
                .into_iter()
                .collect(),
        )
    }

    #[test]
    fn female_custom_face_survives_save_reload_and_character_listing() {
        let (auth, db_path) = temp_auth("female_custom_face");
        let account = auth.login_google("sub-female-custom-face").unwrap();
        let appearance = CharacterAppearance {
            face: onlinerpg_shared::FaceStyle::Custom,
            hair: onlinerpg_shared::HairStyle::Long,
            hair_color: "#d2bda2".to_string().try_into().unwrap(),
            eye_color: "#527baa".to_string().try_into().unwrap(),
            hair_color_original: true,
            eye_color_original: true,
        };
        let created = auth
            .create_character_with_appearance(
                &account,
                "NewFace",
                &plain_attributes(),
                10,
                CharacterClass::Knight,
                Gender::Female,
                appearance,
            )
            .unwrap();
        assert_eq!(created.appearance, appearance);
        drop(auth);
        let auth = AuthService::new(db_path).unwrap();
        assert_eq!(
            auth.get_character_for_account(&account, created.id)
                .unwrap()
                .appearance,
            appearance
        );
        let listing = auth.list_characters_with_equipment(&account).unwrap();
        assert_eq!(
            listing
                .iter()
                .find(|c| c.record.id == created.id)
                .unwrap()
                .record
                .appearance,
            appearance
        );
    }

    #[test]
    fn character_appearance_survives_reload_and_legacy_migration() {
        let (auth, db_path) = temp_auth("appearance_migration");
        let account = auth.login_google("sub-appearance").unwrap();
        let legacy = create(&auth, &account, "Original").unwrap();
        drop(auth);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("ALTER TABLE characters DROP COLUMN appearance;")
            .unwrap();
        drop(conn);
        let auth = AuthService::new(db_path.clone()).unwrap();
        let restored = auth.get_character_for_account(&account, legacy.id).unwrap();
        assert_eq!(restored.appearance, CharacterAppearance::default());
        assert_eq!(restored.attributes.r#str, legacy.attributes.r#str);
        assert_eq!(restored.max_hp, legacy.max_hp);
        let appearance = CharacterAppearance {
            face: onlinerpg_shared::FaceStyle::Rugged,
            hair: onlinerpg_shared::HairStyle::WavyBone,
            hair_color: "#dab378".to_string().try_into().unwrap(),
            eye_color: "#527baa".to_string().try_into().unwrap(),
            hair_color_original: false,
            eye_color_original: false,
        };
        let created = auth
            .create_character_with_appearance(
                &account,
                "Waves",
                &plain_attributes(),
                10,
                CharacterClass::Caveman,
                Gender::Male,
                appearance,
            )
            .unwrap();
        let female = auth
            .create_character_with_appearance(
                &account,
                "Female",
                &plain_attributes(),
                10,
                CharacterClass::Rogue,
                Gender::Female,
                appearance,
            )
            .unwrap();
        assert_eq!(
            female.appearance,
            CharacterAppearance {
                face: onlinerpg_shared::FaceStyle::Default,
                hair: onlinerpg_shared::HairStyle::Bob,
                hair_color: "#ceb294".to_string().try_into().unwrap(),
                ..appearance
            }
        );
        drop(auth);
        let auth = AuthService::new(db_path.clone()).unwrap();
        assert_eq!(
            auth.get_character_for_account(&account, created.id)
                .unwrap()
                .appearance,
            appearance
        );
        let listing = auth.list_characters_with_equipment(&account).unwrap();
        assert_eq!(
            listing
                .iter()
                .find(|c| c.record.id == created.id)
                .unwrap()
                .record
                .appearance,
            appearance
        );
    }

    #[test]
    fn banned_names_are_refused_at_creation_but_not_for_npcs() {
        let auth = banned_name_auth("banned_create");
        let account = auth.login_google("sub-banned").unwrap();

        assert!(matches!(
            create(&auth, &account, "GM"),
            Err(AuthError::BannedCharacterName)
        ));
        assert!(matches!(
            create(&auth, &account, " 운영자 "),
            Err(AuthError::BannedCharacterName)
        ));
        // Only exact matches are banned; "gm" inside a name is fine.
        assert!(create(&auth, &account, "Sigmund").is_ok());

        let npc = auth.login_npc("npc_banned_test").unwrap();
        assert!(create(&auth, &npc, "gm").is_ok());
    }

    #[test]
    fn renaming_runs_the_same_gate_as_creation_and_persists() {
        let auth = banned_name_auth("banned_rename");
        let account = auth.login_google("sub-rename").unwrap();
        let character = create(&auth, &account, "Oldname").unwrap();
        create(&auth, &account, "Taken").unwrap();

        assert!(matches!(
            auth.rename_character(&account, character.id, "gm"),
            Err(AuthError::BannedCharacterName)
        ));
        assert!(matches!(
            auth.rename_character(&account, character.id, "Taken"),
            Err(AuthError::CharacterNameAlreadyExists)
        ));
        assert!(matches!(
            auth.rename_character(&account, character.id, "9lives"),
            Err(AuthError::InvalidCharacterName)
        ));
        assert!(matches!(
            auth.rename_character("someone_else", character.id, "Newname"),
            Err(AuthError::CharacterNotFound)
        ));

        auth.rename_character(&account, character.id, " Newname ")
            .unwrap();
        let reloaded = auth
            .get_character_for_account(&account, character.id)
            .unwrap();
        assert_eq!(reloaded.name, "Newname");
        assert!(create(&auth, &account, "Oldname").is_ok());
    }

    #[test]
    fn character_names_unique_ignoring_ascii_case() {
        let (auth, _) = temp_auth("auth_case");
        let account = auth.login_npc("npc_case_test").unwrap();
        assert!(create(&auth, &account, "Valkyrie").is_ok());
        assert!(matches!(
            create(&auth, &account, "valkyrie"),
            Err(AuthError::CharacterNameAlreadyExists)
        ));
        assert!(matches!(
            create(&auth, &account, "VALKYRIE"),
            Err(AuthError::CharacterNameAlreadyExists)
        ));
    }

    #[test]
    fn admin_role_defaults_to_zero_and_loads_after_update() {
        let (auth, db_path) = temp_auth("auth_admin");
        let account = auth.login_npc("npc_admin_role_test").unwrap();

        let record = create(&auth, &account, "AdminRoleTest").unwrap();
        assert_eq!(record.admin_role, 0);

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE characters SET admin_role = 2 WHERE id = ?1",
            params![record.id],
        )
        .unwrap();

        let loaded = auth.get_character_for_account(&account, record.id).unwrap();
        assert_eq!(loaded.admin_role, 2);
    }
}
