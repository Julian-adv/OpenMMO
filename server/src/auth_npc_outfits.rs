use super::{unix_now, AuthError, AuthService};
use onlinerpg_shared::messages::OutfitGrant;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

fn handovers_on(
    conn: &Connection,
    npc_id: &str,
    how: OutfitHandover,
    game_day: i64,
) -> Result<u32, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM npc_outfits \
         WHERE npc_id = ?1 AND how = ?2 AND game_day = ?3",
        params![npc_id, how.as_str(), game_day],
        |row| row.get(0),
    )
}

/// How an account came to hold an NPC's outfit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutfitHandover {
    Bought,
    Gifted,
}

impl OutfitHandover {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bought => "bought",
            Self::Gifted => "gifted",
        }
    }
}

/// The outcome of `claim_npc_outfit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutfitClaim {
    /// `prior` is the standing to restore if the handover fails; `last`
    /// means this claim used up the day's share.
    Claimed { prior: OutfitGrant, last: bool },
    /// The account already has it, or a gift came before a declined sale.
    Refused,
    /// The NPC already sold or gave away its daily share of this kind.
    SoldOut,
}

impl AuthService {
    pub(super) fn ensure_npc_outfits_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS npc_outfits (
                account_name TEXT NOT NULL,
                npc_id TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('declined', 'received')),
                how TEXT,
                character_id INTEGER NOT NULL,
                game_day INTEGER,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (account_name, npc_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_npc_outfits_day \
             ON npc_outfits (npc_id, how, game_day)",
            [],
        )?;
        Ok(())
    }

    /// Every NPC outfit this account declined or received.
    pub fn load_npc_outfits(
        &self,
        account_name: &str,
    ) -> Result<HashMap<String, OutfitGrant>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt =
            conn.prepare("SELECT npc_id, status FROM npc_outfits WHERE account_name = ?1")?;
        let rows = stmt.query_map(params![account_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (npc_id, status) = row?;
            let grant = if status == "received" {
                OutfitGrant::Received
            } else {
                OutfitGrant::Declined
            };
            out.insert(npc_id, grant);
        }
        Ok(out)
    }

    /// Record a waved-off sale; false when the account already had a row.
    pub fn decline_npc_outfit(
        &self,
        account_name: &str,
        npc_id: &str,
        character_id: i64,
    ) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO npc_outfits \
             (account_name, npc_id, status, character_id, updated_at) \
             VALUES (?1, ?2, 'declined', ?3, ?4)",
            params![account_name, npc_id, character_id, unix_now()],
        )?;
        Ok(inserted == 1)
    }

    /// Accounts that got this NPC's outfit `how` on `game_day`.
    pub fn npc_outfit_handovers_on(
        &self,
        npc_id: &str,
        how: OutfitHandover,
        game_day: i64,
    ) -> Result<u32, AuthError> {
        let conn = self.open_connection()?;
        Ok(handovers_on(&conn, npc_id, how, game_day)?)
    }

    /// Claim the outfit for this account. A sale needs an open account, a
    /// gift a declined sale, and either fewer than `daily` of its kind on
    /// `game_day`.
    pub fn claim_npc_outfit(
        &self,
        account_name: &str,
        npc_id: &str,
        character_id: i64,
        how: OutfitHandover,
        game_day: i64,
        daily: u32,
    ) -> Result<OutfitClaim, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        let prior: Option<String> = tx
            .query_row(
                "SELECT status FROM npc_outfits WHERE account_name = ?1 AND npc_id = ?2",
                params![account_name, npc_id],
                |row| row.get(0),
            )
            .optional()?;
        let prior = match (prior.as_deref(), how) {
            (None, OutfitHandover::Bought) => OutfitGrant::Open,
            (Some("declined"), OutfitHandover::Gifted) => OutfitGrant::Declined,
            _ => return Ok(OutfitClaim::Refused),
        };
        let done = handovers_on(&tx, npc_id, how, game_day)?;
        if done >= daily {
            return Ok(OutfitClaim::SoldOut);
        }
        tx.execute(
            "INSERT INTO npc_outfits \
             (account_name, npc_id, status, how, character_id, game_day, updated_at) \
             VALUES (?1, ?2, 'received', ?3, ?4, ?5, ?6) \
             ON CONFLICT (account_name, npc_id) DO UPDATE SET \
             status = 'received', how = excluded.how, character_id = excluded.character_id, \
             game_day = excluded.game_day, updated_at = excluded.updated_at",
            params![
                account_name,
                npc_id,
                how.as_str(),
                character_id,
                game_day,
                unix_now()
            ],
        )?;
        tx.commit()?;
        Ok(OutfitClaim::Claimed {
            prior,
            last: done + 1 >= daily,
        })
    }

    /// Undo a claim whose handover failed, back to `prior`.
    pub fn release_npc_outfit(
        &self,
        account_name: &str,
        npc_id: &str,
        prior: OutfitGrant,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        if prior == OutfitGrant::Declined {
            conn.execute(
                "UPDATE npc_outfits SET status = 'declined', how = NULL \
                 WHERE account_name = ?1 AND npc_id = ?2",
                params![account_name, npc_id],
            )?;
        } else {
            conn.execute(
                "DELETE FROM npc_outfits WHERE account_name = ?1 AND npc_id = ?2",
                params![account_name, npc_id],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::OutfitClaim::{self, Refused, SoldOut};
    use super::OutfitHandover::{Bought, Gifted};
    use crate::test_util::temp_auth;
    use onlinerpg_shared::messages::OutfitGrant;

    fn claimed(prior: OutfitGrant) -> OutfitClaim {
        OutfitClaim::Claimed { prior, last: true }
    }

    #[test]
    fn an_outfit_goes_to_an_account_once() {
        let (auth, _) = temp_auth("auth_npc_outfits");
        assert!(auth.load_npc_outfits("acct").unwrap().is_empty());
        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 1, Gifted, 7, 1)
                .unwrap(),
            Refused,
            "a gift waits for a declined sale"
        );

        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 1, Bought, 7, 1)
                .unwrap(),
            claimed(OutfitGrant::Open)
        );
        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 2, Bought, 7, 1)
                .unwrap(),
            Refused
        );
        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 2, Gifted, 7, 1)
                .unwrap(),
            Refused
        );
        assert!(!auth.decline_npc_outfit("acct", "rica", 2).unwrap());
        assert_eq!(
            auth.load_npc_outfits("acct").unwrap()["rica"],
            OutfitGrant::Received
        );
        assert_eq!(auth.npc_outfit_handovers_on("rica", Bought, 7).unwrap(), 1);
        assert_eq!(
            auth.claim_npc_outfit("other", "rica", 3, Bought, 7, 1)
                .unwrap(),
            SoldOut
        );
        assert_eq!(
            auth.claim_npc_outfit("other", "rica", 3, Bought, 8, 1)
                .unwrap(),
            claimed(OutfitGrant::Open)
        );

        auth.release_npc_outfit("acct", "rica", OutfitGrant::Open)
            .unwrap();
        assert!(auth.load_npc_outfits("acct").unwrap().is_empty());
    }

    #[test]
    fn a_declined_sale_still_allows_one_gift() {
        let (auth, _) = temp_auth("auth_npc_outfits_decline");
        assert!(auth.decline_npc_outfit("acct", "rica", 1).unwrap());
        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 1, Bought, 7, 1)
                .unwrap(),
            Refused
        );
        assert_eq!(
            auth.claim_npc_outfit("acct", "rica", 1, Gifted, 7, 1)
                .unwrap(),
            claimed(OutfitGrant::Declined)
        );

        auth.release_npc_outfit("acct", "rica", OutfitGrant::Declined)
            .unwrap();
        assert_eq!(
            auth.load_npc_outfits("acct").unwrap()["rica"],
            OutfitGrant::Declined
        );
        assert_eq!(auth.load_npc_outfits("other").unwrap().len(), 0);
    }
}
