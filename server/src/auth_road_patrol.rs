use super::fence::axis_id;
use super::{unix_now, AuthError, AuthService, CharacterSaveData, ItemRow};
use onlinerpg_shared::fence::{FenceEdge, ITEM_ID as FENCE_ITEM};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::BTreeMap;

/// Grace before enforcement: three game days in real seconds.
pub const PATROL_GRACE_SECONDS: i64 = 3 * crate::game_state::time::REAL_DAY_DURATION_SECONDS as i64;

/// Offences from this one on are forfeited instead of impounded.
pub const FORFEIT_FROM_OFFENCE: u32 = 2;

pub struct RoadSeizure {
    pub owner_name: String,
    pub offence: u32,
    /// False when an earlier sweep of the same patrol already counted it.
    pub new_offence: bool,
    pub edges: Vec<FenceEdge>,
}

impl RoadSeizure {
    pub fn forfeited(&self) -> bool {
        self.offence >= FORFEIT_FROM_OFFENCE
    }
}

#[derive(Clone)]
pub struct ImpoundedStack {
    pub item_def_id: String,
    pub quantity: u32,
}

/// Account and character name of the estate owner behind a fence.
fn fence_owner(
    conn: &Connection,
    edge: &FenceEdge,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT e.account_name, c.character_name FROM land_fences f
         JOIN land_estates e ON e.id=f.estate_id
         JOIN characters c ON c.id=e.owner_id
         WHERE f.x=?1 AND f.z=?2 AND f.axis=?3",
        params![edge.x, edge.z, axis_id(edge.axis)],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

impl AuthService {
    pub(super) fn ensure_road_patrol_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS guard_offences (
                account_name TEXT PRIMARY KEY
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                count INTEGER NOT NULL,
                last_patrol INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS guard_impounds (
                account_name TEXT NOT NULL
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                PRIMARY KEY (account_name, item_def_id)
            );
            CREATE TABLE IF NOT EXISTS road_patrol_state (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );",
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO road_patrol_state (key, value) VALUES ('enforce_from', ?1)",
            [unix_now() + PATROL_GRACE_SECONDS],
        )?;
        Ok(())
    }

    pub fn road_patrol_enforce_from(&self) -> Result<i64, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn.query_row(
            "SELECT value FROM road_patrol_state WHERE key='enforce_from'",
            [],
            |row| row.get(0),
        )?)
    }

    #[cfg(test)]
    pub fn set_road_patrol_enforce_from(&self, at: i64) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE road_patrol_state SET value=?1 WHERE key='enforce_from'",
            [at],
        )?;
        Ok(())
    }

    /// Names of the estate owners behind `edges`.
    pub fn road_fence_owners(&self, edges: &[FenceEdge]) -> Result<Vec<String>, AuthError> {
        let conn = self.open_connection()?;
        let mut names = Vec::new();
        for edge in edges {
            if let Some((_, name)) = fence_owner(&conn, edge)? {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }

    /// Remove estate fences on `edges`. One patrol is one offence per account;
    /// the first is impounded, later ones forfeited.
    pub fn seize_road_fences(
        &self,
        edges: &[FenceEdge],
        patrol_id: i64,
    ) -> Result<Vec<RoadSeizure>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut by_account: BTreeMap<String, RoadSeizure> = BTreeMap::new();
        for edge in edges {
            let Some((account, owner_name)) = fence_owner(&tx, edge)? else {
                continue;
            };
            tx.execute(
                "DELETE FROM land_fences WHERE x=?1 AND z=?2 AND axis=?3",
                params![edge.x, edge.z, axis_id(edge.axis)],
            )?;
            by_account
                .entry(account)
                .or_insert(RoadSeizure {
                    owner_name,
                    offence: 0,
                    new_offence: false,
                    edges: Vec::new(),
                })
                .edges
                .push(*edge);
        }
        for (account, seizure) in &mut by_account {
            let (count, last_patrol): (u32, Option<i64>) = tx
                .query_row(
                    "SELECT count, last_patrol FROM guard_offences WHERE account_name=?1",
                    [account.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .unwrap_or((0, None));
            seizure.new_offence = last_patrol != Some(patrol_id);
            seizure.offence = count + u32::from(seizure.new_offence);
            tx.execute(
                "INSERT OR REPLACE INTO guard_offences (account_name, count, last_patrol)
                 VALUES (?1, ?2, ?3)",
                params![account, seizure.offence, patrol_id],
            )?;
            if !seizure.forfeited() {
                tx.execute(
                    "INSERT INTO guard_impounds (account_name, item_def_id, quantity)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(account_name, item_def_id)
                     DO UPDATE SET quantity = quantity + excluded.quantity",
                    params![account, FENCE_ITEM, seizure.edges.len() as u32],
                )?;
            }
        }
        tx.commit()?;
        Ok(by_account.into_values().collect())
    }

    pub fn impounded_items(&self, character_id: i64) -> Result<Vec<ImpoundedStack>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT i.item_def_id, i.quantity FROM guard_impounds i
             JOIN characters c ON c.account_name=i.account_name
             WHERE c.id=?1 ORDER BY i.item_def_id",
        )?;
        let stacks = stmt
            .query_map([character_id], |row| {
                Ok(ImpoundedStack {
                    item_def_id: row.get(0)?,
                    quantity: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(stacks)
    }

    /// Hand `expected` back; refuses if the impound changed since it was priced.
    pub fn reclaim_impounds(
        &self,
        character: &CharacterSaveData,
        inventory: &[ItemRow],
        expected: &[ImpoundedStack],
    ) -> Result<Result<(), &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for stack in expected {
            let removed = tx.execute(
                "DELETE FROM guard_impounds
                 WHERE item_def_id=?2 AND quantity=?3
                   AND account_name=(SELECT account_name FROM characters WHERE id=?1)",
                params![character.character_id, stack.item_def_id, stack.quantity],
            )?;
            if removed != 1 {
                return Ok(Err("The impound changed. Ask again."));
            }
        }
        Self::write_character_states(&tx, std::slice::from_ref(character))?;
        Self::replace_inventories(&tx, [(character.character_id, inventory)])?;
        tx.commit()?;
        Ok(Ok(()))
    }
}
