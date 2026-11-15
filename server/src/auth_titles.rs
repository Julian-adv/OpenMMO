use super::{unix_now, AuthError, AuthService};
use rusqlite::{params, params_from_iter, Connection};
use std::collections::HashMap;

/// The preview's gear for a whole account in one query rather than one per
/// character.
/// A character's earned title ids and the shown one (doc/TITLES.md).
pub type TitleSet = (Vec<String>, Option<String>);

/// Earned titles (in definition order) and the shown one, per character.
pub(super) fn read_titles(
    conn: &Connection,
    character_ids: &[i64],
) -> Result<HashMap<i64, TitleSet>, AuthError> {
    if character_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; character_ids.len()].join(",");
    let mut out: HashMap<i64, TitleSet> = HashMap::new();

    let mut stmt = conn.prepare(&format!(
        "SELECT character_id, title_id FROM character_titles \
         WHERE character_id IN ({placeholders})"
    ))?;
    let rows = stmt.query_map(params_from_iter(character_ids), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (character_id, title) = row?;
        out.entry(character_id).or_default().0.push(title);
    }
    for (titles, _) in out.values_mut() {
        crate::title_defs::sort_ids(titles);
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT id, active_title FROM characters \
         WHERE id IN ({placeholders}) AND active_title IS NOT NULL"
    ))?;
    let rows = stmt.query_map(params_from_iter(character_ids), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (character_id, active) = row?;
        let entry = out.entry(character_id).or_default();
        // A stale pick (title row gone) shows nothing rather than a ghost.
        if entry.0.contains(&active) {
            entry.1 = Some(active);
        }
    }
    Ok(out)
}

impl AuthService {
    pub(super) fn ensure_titles_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_titles (
                character_id INTEGER NOT NULL,
                title_id TEXT NOT NULL,
                earned_at INTEGER NOT NULL,
                PRIMARY KEY (character_id, title_id),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Ok(())
    }

    /// Earned title ids (in definition order) and the shown one (doc/TITLES.md).
    pub fn load_titles(
        &self,
        character_id: i64,
    ) -> Result<(Vec<String>, Option<String>), AuthError> {
        let conn = self.open_connection()?;
        Ok(read_titles(&conn, &[character_id])?
            .remove(&character_id)
            .unwrap_or_default())
    }

    /// Record a title; false when the character already had it.
    pub fn grant_title(&self, character_id: i64, title_id: &str) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO character_titles (character_id, title_id, earned_at) \
             VALUES (?1, ?2, ?3)",
            params![character_id, title_id, unix_now()],
        )?;
        Ok(inserted == 1)
    }

    /// Set the shown title. `Some` must be an earned title; otherwise the
    /// row is left alone and false comes back.
    pub fn set_active_title(
        &self,
        character_id: i64,
        title_id: Option<&str>,
    ) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let changed = match title_id {
            None => conn.execute(
                "UPDATE characters SET active_title = NULL WHERE id = ?1",
                params![character_id],
            )?,
            Some(title) => conn.execute(
                "UPDATE characters SET active_title = ?2 WHERE id = ?1 AND EXISTS (\
                     SELECT 1 FROM character_titles \
                     WHERE character_id = ?1 AND title_id = ?2)",
                params![character_id, title],
            )?,
        };
        Ok(changed == 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::tests::create;
    use crate::test_util::temp_auth;

    #[test]
    fn titles_persist_and_only_an_earned_one_can_be_shown() {
        let (auth, _) = temp_auth("auth_titles");
        let player = auth.login_google("sub-titles").unwrap();
        let id = create(&auth, &player, "Slayer").unwrap().id;

        assert_eq!(auth.load_titles(id).unwrap(), (vec![], None));
        assert!(auth.grant_title(id, "orc_slayer").unwrap());
        assert!(!auth.grant_title(id, "orc_slayer").unwrap(), "already held");
        assert!(auth.grant_title(id, "goblin_slayer").unwrap());

        assert!(!auth.set_active_title(id, Some("ogre_slayer")).unwrap());
        assert_eq!(auth.load_titles(id).unwrap().1, None);
        assert!(auth.set_active_title(id, Some("orc_slayer")).unwrap());
        assert_eq!(
            auth.load_titles(id).unwrap(),
            (
                vec!["goblin_slayer".into(), "orc_slayer".into()],
                Some("orc_slayer".into())
            )
        );
        assert!(auth.set_active_title(id, None).unwrap());
        assert_eq!(auth.load_titles(id).unwrap().1, None);

        let listed = auth.list_characters_with_equipment(&player).unwrap();
        assert_eq!(listed[0].titles.len(), 2);
        assert_eq!(listed[0].active_title, None);
    }
}
