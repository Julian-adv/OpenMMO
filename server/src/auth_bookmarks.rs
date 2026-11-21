use super::{AuthError, AuthService};
use onlinerpg_shared::bookmark::{Bookmark, BookmarkError, MAX_BOOKMARKS};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

fn is_duplicate(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(failure, _)
        if failure.code == rusqlite::ErrorCode::ConstraintViolation)
}

fn read_bookmark(row: &rusqlite::Row) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: row.get(0)?,
        name: row.get(1)?,
        x: row.get(2)?,
        z: row.get(3)?,
    })
}

impl AuthService {
    pub(super) fn ensure_bookmark_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS character_bookmarks (
                id INTEGER PRIMARY KEY,
                character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                name TEXT NOT NULL COLLATE NOCASE,
                x REAL NOT NULL,
                z REAL NOT NULL,
                UNIQUE (character_id, name)
            );",
        )
    }

    pub fn bookmarks(&self, character_id: i64) -> Result<Vec<Bookmark>, AuthError> {
        let conn = self.open_connection()?;
        let mut query = conn.prepare(
            "SELECT id, name, x, z FROM character_bookmarks WHERE character_id=?1 ORDER BY id",
        )?;
        let bookmarks = query
            .query_map([character_id], read_bookmark)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(bookmarks)
    }

    pub fn bookmark(&self, character_id: i64, id: i64) -> Result<Option<Bookmark>, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT id, name, x, z FROM character_bookmarks WHERE id=?2 AND character_id=?1",
                params![character_id, id],
                read_bookmark,
            )
            .optional()?)
    }

    pub fn save_bookmark(
        &self,
        character_id: i64,
        name: &str,
        x: f32,
        z: f32,
    ) -> Result<Result<(), BookmarkError>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: usize = tx.query_row(
            "SELECT COUNT(*) FROM character_bookmarks WHERE character_id=?1",
            [character_id],
            |row| row.get(0),
        )?;
        if count >= MAX_BOOKMARKS {
            return Ok(Err(BookmarkError::Full));
        }
        match tx.execute(
            "INSERT INTO character_bookmarks (character_id, name, x, z) VALUES (?1,?2,?3,?4)",
            params![character_id, name, x, z],
        ) {
            Ok(_) => {}
            Err(error) if is_duplicate(&error) => return Ok(Err(BookmarkError::Duplicate)),
            Err(error) => return Err(error.into()),
        }
        tx.commit()?;
        Ok(Ok(()))
    }

    pub fn rename_bookmark(
        &self,
        character_id: i64,
        id: i64,
        name: &str,
    ) -> Result<Result<(), BookmarkError>, AuthError> {
        let conn = self.open_connection()?;
        match conn.execute(
            "UPDATE character_bookmarks SET name=?3 WHERE character_id=?1 AND id=?2",
            params![character_id, id, name],
        ) {
            Ok(0) => Ok(Err(BookmarkError::Unknown)),
            Ok(_) => Ok(Ok(())),
            Err(error) if is_duplicate(&error) => Ok(Err(BookmarkError::Duplicate)),
            Err(error) => Err(error.into()),
        }
    }

    pub fn delete_bookmark(
        &self,
        character_id: i64,
        id: i64,
    ) -> Result<Result<(), BookmarkError>, AuthError> {
        let conn = self.open_connection()?;
        let deleted = conn.execute(
            "DELETE FROM character_bookmarks WHERE character_id=?1 AND id=?2",
            params![character_id, id],
        )?;
        Ok(if deleted > 0 {
            Ok(())
        } else {
            Err(BookmarkError::Unknown)
        })
    }

    /// Whether the plot belongs to an estate of another account that the
    /// character's family does not share.
    pub fn plot_is_foreign(
        &self,
        character_id: i64,
        (tile_x, tile_z, quadrant): (i32, i32, u8),
    ) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT 1 FROM land_plots p JOIN land_estates e ON e.id=p.estate_id
                 WHERE p.tile_x=?2 AND p.tile_z=?3 AND p.quadrant=?4
                   AND e.account_name<>(SELECT account_name FROM characters WHERE id=?1)
                   AND (e.family_id IS NULL OR e.family_id NOT IN
                        (SELECT family_id FROM family_members WHERE character_id=?1))",
                params![character_id, tile_x, tile_z, quadrant],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::create;
    use crate::test_util::temp_auth;

    #[test]
    fn bookmarks_are_capped_unique_by_name_and_private_to_their_character() {
        let (auth, _) = temp_auth("auth_bookmarks");
        let player = auth.login_google("sub-bookmarks").unwrap();
        let id = create(&auth, &player, "Marker").unwrap().id;
        let other = create(&auth, &player, "Other").unwrap().id;

        assert_eq!(auth.save_bookmark(id, "Gate", 1.0, 2.0).unwrap(), Ok(()));
        assert_eq!(
            auth.save_bookmark(id, "gate", 3.0, 4.0).unwrap(),
            Err(BookmarkError::Duplicate)
        );
        assert_eq!(auth.save_bookmark(other, "Gate", 5.0, 6.0).unwrap(), Ok(()));
        let gate = auth.bookmarks(id).unwrap().remove(0);
        assert_eq!((gate.name.as_str(), gate.x, gate.z), ("Gate", 1.0, 2.0));
        assert_eq!(auth.bookmark(other, gate.id).unwrap(), None);

        assert_eq!(auth.save_bookmark(id, "Mill", 0.0, 0.0).unwrap(), Ok(()));
        assert_eq!(
            auth.rename_bookmark(id, gate.id, "mill").unwrap(),
            Err(BookmarkError::Duplicate)
        );
        assert_eq!(auth.rename_bookmark(id, gate.id, "Bridge").unwrap(), Ok(()));
        assert_eq!(
            auth.rename_bookmark(other, gate.id, "Bridge").unwrap(),
            Err(BookmarkError::Unknown)
        );
        assert_eq!(
            auth.delete_bookmark(other, gate.id).unwrap(),
            Err(BookmarkError::Unknown)
        );
        assert_eq!(auth.delete_bookmark(id, gate.id).unwrap(), Ok(()));

        for n in auth.bookmarks(id).unwrap().len()..MAX_BOOKMARKS {
            assert_eq!(
                auth.save_bookmark(id, &format!("b{n}"), 0.0, 0.0).unwrap(),
                Ok(())
            );
        }
        assert_eq!(
            auth.save_bookmark(id, "one too many", 0.0, 0.0).unwrap(),
            Err(BookmarkError::Full)
        );
    }

    #[test]
    fn a_plot_is_foreign_only_when_another_account_outside_the_family_owns_it() {
        let (auth, _) = temp_auth("auth_bookmark_plots");
        let owner_account = auth.login_google("sub-owner").unwrap();
        let owner = create(&auth, &owner_account, "Owner").unwrap().id;
        let alt = create(&auth, &owner_account, "Alt").unwrap().id;
        let visitor_account = auth.login_google("sub-visitor").unwrap();
        let visitor = create(&auth, &visitor_account, "Visitor").unwrap().id;
        auth.open_connection()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO land_estates (id, owner_id, account_name, grade, created_at)
                 VALUES (1, {owner}, '{owner_account}', 1, 0);
                 INSERT INTO land_plots (tile_x, tile_z, quadrant, estate_id) VALUES (3, 4, 2, 1);"
            ))
            .unwrap();

        assert!(auth.plot_is_foreign(visitor, (3, 4, 2)).unwrap());
        assert!(!auth.plot_is_foreign(visitor, (3, 4, 1)).unwrap());
        assert!(!auth.plot_is_foreign(owner, (3, 4, 2)).unwrap());
        assert!(!auth.plot_is_foreign(alt, (3, 4, 2)).unwrap());
    }
}
