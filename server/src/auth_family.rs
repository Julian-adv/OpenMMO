//! Family persistence (doc/FAMILY_SYSTEM.md). The DB is the only copy of
//! membership, ranks and estate links; every rule is re-checked inside the
//! write transaction.

use super::{
    class_from_row, unix_now, valid_character_name, valid_name_char, AuthError, AuthService,
    CharacterSaveData, ItemRow,
};
use onlinerpg_shared::character::CharacterClass;
use onlinerpg_shared::family::{
    default_rank_label, default_rank_permissions, permission, FamilyMergeTerms, FamilyRank,
    MergeSide, FAMILY_HEAD_INACTIVE_SECS, FAMILY_MAX_MEMBERS, FAMILY_NAME_RETIRE_SECS,
    FAMILY_RANK_COUNT, FAMILY_RANK_ELDER, FAMILY_RANK_HEAD, FAMILY_RANK_LABEL_MAX_CHARS,
    FAMILY_RANK_MEMBER, FAMILY_REJOIN_COOLDOWN_SECS, FAMILY_RENAME_INTERVAL_SECS,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

type Verdict<T> = Result<Result<T, &'static str>, AuthError>;

pub struct FamilyMemberRow {
    pub character_id: i64,
    pub account_name: String,
    pub name: String,
    pub level: u32,
    pub class: CharacterClass,
    pub rank: u8,
    pub last_seen_at: i64,
    /// Their cape shows the family emblem rather than their own print.
    pub cape_emblem: bool,
}

pub struct FamilyEstateRow {
    pub estate_id: i64,
    pub account_name: String,
    pub owner_name: String,
    pub plots: u32,
    pub missed: u32,
}

pub struct FamilyRoster {
    pub name: String,
    pub ranks: Vec<FamilyRank>,
    /// Succession order.
    pub members: Vec<FamilyMemberRow>,
    pub estates: Vec<FamilyEstateRow>,
    /// Member accounts holding an estate no family has.
    pub unattached_accounts: Vec<String>,
    pub renamed_at: Option<i64>,
    pub emblem: FamilyEmblem,
    pub seat_estate_id: Option<i64>,
}

impl FamilyRoster {
    /// Who may take over from a head unseen for `FAMILY_HEAD_INACTIVE_SECS`:
    /// the first member in line who is not just as absent. `online` members
    /// count as seen now.
    pub fn successor(&self, now: i64, online: &[i64]) -> Option<i64> {
        let cutoff = now - FAMILY_HEAD_INACTIVE_SECS;
        let seen =
            |m: &FamilyMemberRow| m.last_seen_at >= cutoff || online.contains(&m.character_id);
        let head = self.members.iter().find(|m| m.rank == FAMILY_RANK_HEAD)?;
        if seen(head) {
            return None;
        }
        self.members
            .iter()
            .find(|m| m.rank != FAMILY_RANK_HEAD && seen(m))
            .map(|m| m.character_id)
    }
}

pub struct FamilyDeparture {
    pub family_id: i64,
    pub family_name: String,
    pub target_name: String,
    pub dissolved: bool,
}

/// Why an invite cannot go through, worded for whoever is told.
#[derive(Clone, Copy)]
pub enum InviteBlock {
    InviterNotMember,
    InviterMayNotInvite,
    TargetInFamily,
    TargetCoolingDown,
    Full,
}

impl InviteBlock {
    pub fn to_inviter(self) -> &'static str {
        match self {
            Self::InviterNotMember => "You do not belong to a family.",
            Self::InviterMayNotInvite => "Your rank may not invite.",
            Self::TargetInFamily => "They already belong to a family.",
            Self::TargetCoolingDown => "They left a family less than a day ago.",
            Self::Full => "Your family is full.",
        }
    }

    pub fn to_target(self) -> &'static str {
        match self {
            Self::InviterNotMember => "The inviter no longer belongs to a family.",
            Self::InviterMayNotInvite => "The inviter may no longer invite.",
            Self::TargetInFamily => "You already belong to a family.",
            Self::TargetCoolingDown => "You left a family less than a day ago.",
            Self::Full => "The family is full.",
        }
    }
}

/// SQL predicate: character `param` may use estate `e` — their account owns
/// it, or (with `family_bits`) it is attached to their family and their rank
/// holds one of the bits. `None` keeps the estate to its own account. Both
/// arms compare against a constant, so each keeps its index.
pub(super) fn estate_access_sql(param: &str, family_bits: Option<u32>) -> String {
    let account = format!("e.account_name=(SELECT account_name FROM characters WHERE id={param})");
    match family_bits {
        None => account,
        Some(bits) => format!(
            "({account} OR e.family_id=(
                SELECT fm.family_id FROM family_members fm
                JOIN family_ranks fr ON fr.family_id=fm.family_id AND fr.rank=fm.rank
                WHERE fm.character_id={param} AND fr.permissions & {bits} != 0))"
        ),
    }
}

/// Family name per character of `account_name`, for the character list.
pub(super) fn read_family_names(
    conn: &Connection,
    account_name: &str,
) -> Result<std::collections::HashMap<i64, String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT c.id, f.name FROM characters c
         JOIN family_members m ON m.character_id=c.id JOIN families f ON f.id=m.family_id
         WHERE c.account_name=?1",
    )?;
    let names = stmt
        .query_map([account_name], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect();
    names
}

pub use onlinerpg_shared::family::Emblem as FamilyEmblem;

/// Furniture a return scroll lands on.
const ESTATE_RETURN_RUG: &str = "furniture_hearthbound_rug";

/// An estate's plots and the hearthbound rugs on it (position, rotation in
/// degrees, floor).
pub type EstateDestination = (
    Vec<(i32, i32, u8)>,
    Vec<(onlinerpg_shared::Position, f32, i8)>,
);

/// The family seat: the estate the head chose while it stays attached, else
/// the head's attached homestead, else the earliest attached estate.
pub(super) fn family_seat(
    conn: &Connection,
    family_id: i64,
) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(
            (SELECT e.id FROM families f JOIN land_estates e
                ON e.id=f.seat_estate_id AND e.family_id=f.id WHERE f.id=?1),
            (SELECT e.id FROM family_members m JOIN characters c ON c.id=m.character_id
                JOIN land_estates e ON e.account_name=c.account_name AND e.family_id=?1
                WHERE m.family_id=?1 AND m.rank=?2 ORDER BY e.grade LIMIT 1),
            (SELECT id FROM land_estates WHERE family_id=?1 ORDER BY id LIMIT 1))",
        params![family_id, FAMILY_RANK_HEAD],
        |row| row.get(0),
    )
}

pub(super) fn estate_destination(
    conn: &Connection,
    estate_id: i64,
) -> Result<EstateDestination, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT tile_x, tile_z, quadrant FROM land_plots WHERE estate_id=?1 ORDER BY rowid",
    )?;
    let plots = stmt
        .query_map([estate_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT x, y, z, rotation_deg, floor_level FROM estate_chests
         WHERE estate_id=?1 AND item_def_id=?2 ORDER BY id",
    )?;
    let rugs = stmt
        .query_map(params![estate_id, ESTATE_RETURN_RUG], |row| {
            Ok((
                onlinerpg_shared::Position {
                    x: row.get::<_, f64>(0)? as f32,
                    y: row.get::<_, f64>(1)? as f32,
                    z: row.get::<_, f64>(2)? as f32,
                },
                row.get::<_, f64>(3)? as f32,
                row.get::<_, i64>(4)? as i8,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((plots, rugs))
}

struct Membership {
    family_id: i64,
    rank: u8,
    permissions: u32,
}

fn membership(conn: &Connection, character_id: i64) -> Result<Option<Membership>, rusqlite::Error> {
    conn.query_row(
        "SELECT m.family_id, m.rank, r.permissions FROM family_members m
         JOIN family_ranks r ON r.family_id=m.family_id AND r.rank=m.rank
         WHERE m.character_id=?1",
        [character_id],
        |row| {
            Ok(Membership {
                family_id: row.get(0)?,
                rank: row.get(1)?,
                permissions: row.get(2)?,
            })
        },
    )
    .optional()
}

fn in_cooldown(conn: &Connection, character_id: i64) -> Result<bool, rusqlite::Error> {
    let left_at: Option<i64> = conn
        .query_row(
            "SELECT left_at FROM family_leave_cooldowns WHERE character_id=?1",
            [character_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(left_at.is_some_and(|at| unix_now() - at < FAMILY_REJOIN_COOLDOWN_SECS))
}

/// The inviter's family (id and name) when `target_id` may join it now.
fn invite_target(
    conn: &Connection,
    inviter_id: i64,
    target_id: i64,
) -> Result<Result<(i64, String), InviteBlock>, rusqlite::Error> {
    let Some(inviter) = membership(conn, inviter_id)? else {
        return Ok(Err(InviteBlock::InviterNotMember));
    };
    if inviter.permissions & permission::INVITE == 0 {
        return Ok(Err(InviteBlock::InviterMayNotInvite));
    }
    if membership(conn, target_id)?.is_some() {
        return Ok(Err(InviteBlock::TargetInFamily));
    }
    if in_cooldown(conn, target_id)? {
        return Ok(Err(InviteBlock::TargetCoolingDown));
    }
    let (name, members): (String, i64) = conn.query_row(
        "SELECT name, (SELECT COUNT(*) FROM family_members WHERE family_id=f.id)
         FROM families f WHERE id=?1",
        [inviter.family_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if members as usize >= FAMILY_MAX_MEMBERS {
        return Ok(Err(InviteBlock::Full));
    }
    Ok(Ok((inviter.family_id, name)))
}

#[derive(Clone, Copy)]
enum FamilyEvent {
    Founded,
    /// Absorbed into the family with this id.
    Merged(i64),
    Dissolved,
}

fn record_family_event(
    conn: &Connection,
    family_id: i64,
    name: &str,
    event: FamilyEvent,
) -> Result<(), rusqlite::Error> {
    let (event, into_family_id) = match event {
        FamilyEvent::Founded => ("founded", None),
        FamilyEvent::Merged(survivor) => ("merged", Some(survivor)),
        FamilyEvent::Dissolved => ("dissolved", None),
    };
    conn.execute(
        "INSERT INTO family_events (timestamp, family_id, name, event, into_family_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![unix_now(), family_id, name, event, into_family_id],
    )?;
    Ok(())
}

fn retire_name(conn: &Connection, name: &str) -> Result<(), rusqlite::Error> {
    let now = unix_now();
    conn.execute(
        "INSERT OR REPLACE INTO family_retired_names (name, successor_id, retired_at, reusable_at)
         VALUES (?1, NULL, ?2, ?3)",
        params![name, now, now + FAMILY_NAME_RETIRE_SECS],
    )?;
    Ok(())
}

/// Bring one family back to its invariants after members left by any route:
/// no members dissolves it, a missing head is succeeded, and estates whose
/// account has no member left are let go. True when it was dissolved.
fn settle_family(
    conn: &Connection,
    family_id: i64,
    last_head: Option<i64>,
) -> Result<bool, rusqlite::Error> {
    let members: i64 = conn.query_row(
        "SELECT COUNT(*) FROM family_members WHERE family_id=?1",
        [family_id],
        |row| row.get(0),
    )?;
    if members == 0 {
        let name: Option<String> = conn
            .query_row(
                "SELECT name FROM families WHERE id=?1",
                [family_id],
                |row| row.get(0),
            )
            .optional()?;
        conn.execute(
            "UPDATE land_estates SET family_id=NULL WHERE family_id=?1",
            [family_id],
        )?;
        // What is left in the family's chests goes to its last head.
        conn.execute(
            "UPDATE estate_chests SET family_id=NULL, owner_id=COALESCE(?2, owner_id)
             WHERE family_id=?1",
            params![family_id, last_head],
        )?;
        conn.execute("DELETE FROM families WHERE id=?1", [family_id])?;
        if let Some(name) = name {
            retire_name(conn, &name)?;
            record_family_event(conn, family_id, &name, FamilyEvent::Dissolved)?;
        }
        return Ok(true);
    }
    conn.execute(
        "UPDATE family_members SET rank=?2 WHERE character_id=(
            SELECT character_id FROM family_members WHERE family_id=?1
            ORDER BY rank, joined_at, character_id LIMIT 1)
         AND NOT EXISTS(SELECT 1 FROM family_members WHERE family_id=?1 AND rank=?2)",
        params![family_id, FAMILY_RANK_HEAD],
    )?;
    conn.execute(
        "UPDATE land_estates SET family_id=NULL WHERE family_id=?1 AND account_name NOT IN (
            SELECT c.account_name FROM family_members m JOIN characters c ON c.id=m.character_id
            WHERE m.family_id=?1)",
        [family_id],
    )?;
    Ok(false)
}

fn valid_rank_label(label: &str) -> bool {
    let chars = label.chars().count();
    (1..=FAMILY_RANK_LABEL_MAX_CHARS).contains(&chars) && label.chars().all(valid_name_char)
}

impl AuthService {
    pub(super) fn ensure_family_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS families (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                emblem_texture TEXT,
                emblem_color TEXT,
                renamed_at INTEGER,
                created_at INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_families_name_nocase
                ON families(name COLLATE NOCASE);
            CREATE TABLE IF NOT EXISTS family_members (
                character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
                family_id INTEGER NOT NULL REFERENCES families(id) ON DELETE CASCADE,
                rank INTEGER NOT NULL,
                joined_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_family_members_family ON family_members(family_id);
            CREATE TABLE IF NOT EXISTS family_ranks (
                family_id INTEGER NOT NULL REFERENCES families(id) ON DELETE CASCADE,
                rank INTEGER NOT NULL,
                label TEXT NOT NULL,
                permissions INTEGER NOT NULL,
                PRIMARY KEY (family_id, rank)
            );
            CREATE TABLE IF NOT EXISTS family_retired_names (
                name TEXT PRIMARY KEY COLLATE NOCASE,
                successor_id INTEGER REFERENCES families(id) ON DELETE SET NULL,
                retired_at INTEGER NOT NULL,
                reusable_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS family_leave_cooldowns (
                character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
                left_at INTEGER NOT NULL
            );",
        )?;
        if !Self::table_columns(conn, "land_estates")?.contains("family_id") {
            conn.execute(
                "ALTER TABLE land_estates
                 ADD COLUMN family_id INTEGER REFERENCES families(id) ON DELETE SET NULL",
                [],
            )?;
        }
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_land_estates_family
             ON land_estates(family_id) WHERE family_id IS NOT NULL",
            [],
        )?;
        if !Self::table_columns(conn, "families")?.contains("seat_estate_id") {
            conn.execute("ALTER TABLE families ADD COLUMN seat_estate_id INTEGER", [])?;
        }
        if !Self::table_columns(conn, "family_members")?.contains("cape_emblem") {
            conn.execute(
                "ALTER TABLE family_members ADD COLUMN cape_emblem INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        Self::ensure_family_events_schema(conn)
    }

    /// The head names which attached estate is the family seat.
    pub fn set_family_seat(&self, character_id: i64, estate_id: i64) -> Verdict<i64> {
        let conn = self.open_connection()?;
        let Some(member) = membership(&conn, character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if member.rank != FAMILY_RANK_HEAD {
            return Ok(Err("Only the head chooses the family seat."));
        }
        let changed = conn.execute(
            "UPDATE families SET seat_estate_id=?2 WHERE id=?1
             AND EXISTS(SELECT 1 FROM land_estates WHERE id=?2 AND family_id=?1)",
            params![member.family_id, estate_id],
        )?;
        if changed == 0 {
            return Ok(Err("That estate is not attached to the family."));
        }
        Ok(Ok(member.family_id))
    }

    /// Where a Scroll of Family Return lands: the seat's plots and its
    /// hearthbound rugs, or why there is nowhere to go.
    pub fn family_return_destination(&self, character_id: i64) -> Verdict<EstateDestination> {
        let conn = self.open_connection()?;
        let Some(member) = membership(&conn, character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        let Some(seat) = family_seat(&conn, member.family_id)? else {
            return Ok(Err("Your family has no estate to return to."));
        };
        let missed: i64 = conn.query_row(
            "SELECT missed FROM land_estates WHERE id=?1",
            [seat],
            |row| row.get(0),
        )?;
        if missed > 0 {
            return Ok(Err("The family seat has overdue taxes."));
        }
        Ok(Ok(estate_destination(&conn, seat)?))
    }

    /// Whether the member's cape shows the family emblem or their own print.
    pub fn set_family_cape_emblem(&self, character_id: i64, show: bool) -> Verdict<i64> {
        let conn = self.open_connection()?;
        let Some(member) = membership(&conn, character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        conn.execute(
            "UPDATE family_members SET cape_emblem=?2 WHERE character_id=?1",
            params![character_id, show],
        )?;
        Ok(Ok(member.family_id))
    }

    /// A live family or a name still held back keeps characters off it.
    pub(super) fn family_name_in_use(
        conn: &Connection,
        name: &str,
    ) -> Result<bool, rusqlite::Error> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM families WHERE name=?1 COLLATE NOCASE)
                 OR EXISTS(SELECT 1 FROM family_retired_names
                           WHERE name=?1 AND (reusable_at IS NULL OR reusable_at>?2))",
            params![name, unix_now()],
            |row| row.get(0),
        )
    }

    fn family_name_refusal(
        &self,
        conn: &Connection,
        account_name: &str,
        name: &str,
        own_family: Option<i64>,
    ) -> Result<Option<&'static str>, rusqlite::Error> {
        if !valid_character_name(name) {
            return Ok(Some(
                "Family names follow the character name rules: letters and digits, starting with a letter.",
            ));
        }
        if self.is_name_banned_for(account_name, name) {
            return Ok(Some("That family name is not allowed."));
        }
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM families WHERE name=?1 COLLATE NOCASE AND id IS NOT ?2)",
            params![name, own_family],
            |row| row.get(0),
        )?;
        if taken {
            return Ok(Some("Another family has that name."));
        }
        let character: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM characters WHERE character_name=?1 COLLATE NOCASE)",
            [name],
            |row| row.get(0),
        )?;
        if character {
            return Ok(Some("A character has that name."));
        }
        if Self::family_name_in_use(conn, name)? {
            return Ok(Some(
                "That family name was in use recently and is held back.",
            ));
        }
        conn.execute("DELETE FROM family_retired_names WHERE name=?1", [name])?;
        Ok(None)
    }

    /// The character's family as (id, name).
    pub fn family_of(&self, character_id: i64) -> Result<Option<(i64, String)>, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT f.id, f.name FROM family_members m JOIN families f ON f.id=m.family_id
                 WHERE m.character_id=?1",
                [character_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn family_roster(&self, family_id: i64) -> Result<Option<FamilyRoster>, AuthError> {
        let conn = self.open_connection()?;
        let head: Option<(String, Option<i64>, FamilyEmblem)> = conn
            .query_row(
                "SELECT name, renamed_at, emblem_texture, emblem_color FROM families WHERE id=?1",
                [family_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        FamilyEmblem {
                            texture: row.get(2)?,
                            color: row.get(3)?,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((name, renamed_at, emblem)) = head else {
            return Ok(None);
        };
        let mut stmt = conn.prepare(
            "SELECT label, permissions FROM family_ranks WHERE family_id=?1 ORDER BY rank",
        )?;
        let ranks = stmt
            .query_map([family_id], |row| {
                Ok(FamilyRank {
                    label: row.get(0)?,
                    permissions: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.account_name, c.character_name, c.level, c.class, m.rank,
                    COALESCE(c.last_seen_at, c.created_at), m.cape_emblem
             FROM family_members m JOIN characters c ON c.id=m.character_id
             WHERE m.family_id=?1 ORDER BY m.rank, m.joined_at, m.character_id",
        )?;
        let members = stmt
            .query_map([family_id], |row| {
                Ok(FamilyMemberRow {
                    character_id: row.get(0)?,
                    account_name: row.get(1)?,
                    name: row.get(2)?,
                    level: row.get(3)?,
                    class: class_from_row(row, 4)?,
                    rank: row.get(5)?,
                    last_seen_at: row.get(6)?,
                    cape_emblem: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = conn.prepare(
            // Named after the account's character in this family, so the
            // roster never ties a member to another character on their account.
            "SELECT e.id, e.account_name,
                    COALESCE((SELECT c.character_name FROM family_members m
                              JOIN characters c ON c.id=m.character_id
                              WHERE m.family_id=?1 AND c.account_name=e.account_name
                              ORDER BY m.rank, m.joined_at LIMIT 1), ''),
                    e.missed, (SELECT COUNT(*) FROM land_plots WHERE estate_id=e.id)
             FROM land_estates e
             WHERE e.family_id=?1 ORDER BY e.id",
        )?;
        let estates = stmt
            .query_map([family_id], |row| {
                Ok(FamilyEstateRow {
                    estate_id: row.get(0)?,
                    account_name: row.get(1)?,
                    owner_name: row.get(2)?,
                    missed: row.get(3)?,
                    plots: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT e.account_name FROM land_estates e
             WHERE e.family_id IS NULL AND e.account_name IN (
                SELECT c.account_name FROM family_members m JOIN characters c ON c.id=m.character_id
                WHERE m.family_id=?1)",
        )?;
        let unattached_accounts = stmt
            .query_map([family_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(FamilyRoster {
            name,
            ranks,
            members,
            estates,
            unattached_accounts,
            renamed_at,
            emblem,
            seat_estate_id: family_seat(&conn, family_id)?,
        }))
    }

    pub fn leads_family(&self, character_id: i64) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        Ok(membership(&conn, character_id)?.is_some_and(|m| m.rank == FAMILY_RANK_HEAD))
    }

    /// Both characters head different families that fit together: their
    /// family names and the merged member count.
    pub fn check_family_merge(&self, proposer: i64, target: i64) -> Verdict<(String, String, u32)> {
        let conn = self.open_connection()?;
        Self::merge_pair(&conn, proposer, target)
    }

    fn merge_pair(conn: &Connection, proposer: i64, target: i64) -> Verdict<(String, String, u32)> {
        let (Some(a), Some(b)) = (membership(conn, proposer)?, membership(conn, target)?) else {
            return Ok(Err("Both sides must lead a family."));
        };
        if a.rank != FAMILY_RANK_HEAD || b.rank != FAMILY_RANK_HEAD {
            return Ok(Err("Only heads of families can agree a merge."));
        }
        if a.family_id == b.family_id {
            return Ok(Err("That is your own family."));
        }
        let (name_a, name_b, members): (String, String, u32) = conn.query_row(
            "SELECT (SELECT name FROM families WHERE id=?1), (SELECT name FROM families WHERE id=?2),
                    (SELECT COUNT(*) FROM family_members WHERE family_id IN (?1, ?2))",
            params![a.family_id, b.family_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if members as usize > FAMILY_MAX_MEMBERS {
            return Ok(Err(
                "Together the families would have more than 30 members.",
            ));
        }
        Ok(Ok((name_a, name_b, members)))
    }

    /// Fold one family into the other on the agreed terms. Returns the
    /// surviving and the absorbed family ids.
    pub fn merge_families(
        &self,
        proposer: i64,
        target: i64,
        terms: FamilyMergeTerms,
    ) -> Verdict<(i64, i64)> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Err(reason) = Self::merge_pair(&tx, proposer, target)? {
            return Ok(Err(reason));
        }
        let family = |id: i64| -> Result<i64, rusqlite::Error> {
            tx.query_row(
                "SELECT family_id FROM family_members WHERE character_id=?1",
                [id],
                |row| row.get(0),
            )
        };
        let side = |which: MergeSide| match which {
            MergeSide::Proposer => proposer,
            MergeSide::Target => target,
        };
        let (survivor_head, absorbed_head) = match terms.keep_name {
            MergeSide::Proposer => (proposer, target),
            MergeSide::Target => (target, proposer),
        };
        let survivor = family(survivor_head)?;
        let absorbed = family(absorbed_head)?;
        if side(terms.keep_emblem) == absorbed_head {
            tx.execute(
                "UPDATE families SET (emblem_texture, emblem_color) =
                    (SELECT emblem_texture, emblem_color FROM families WHERE id=?2)
                 WHERE id=?1",
                params![survivor, absorbed],
            )?;
        }
        let stepping_down = if side(terms.keep_head) == survivor_head {
            absorbed_head
        } else {
            survivor_head
        };
        tx.execute(
            "UPDATE family_members SET rank=?2 WHERE character_id=?1",
            params![stepping_down, FAMILY_RANK_ELDER],
        )?;
        for table in [
            "family_members",
            "land_estates",
            "estate_chests",
            "family_storage_log",
        ] {
            tx.execute(
                &format!("UPDATE {table} SET family_id=?1 WHERE family_id=?2"),
                params![survivor, absorbed],
            )?;
        }
        let absorbed_name: String =
            tx.query_row("SELECT name FROM families WHERE id=?1", [absorbed], |row| {
                row.get(0)
            })?;
        tx.execute("DELETE FROM families WHERE id=?1", [absorbed])?;
        record_family_event(&tx, absorbed, &absorbed_name, FamilyEvent::Merged(survivor))?;
        tx.execute(
            "INSERT OR REPLACE INTO family_retired_names (name, successor_id, retired_at, reusable_at)
             VALUES (?1, ?2, ?3, NULL)",
            params![absorbed_name, survivor, unix_now()],
        )?;
        tx.commit()?;
        Ok(Ok((survivor, absorbed)))
    }

    /// Set or clear the family emblem; needs `EDIT_EMBLEM`.
    pub fn set_family_emblem(&self, character_id: i64, emblem: &FamilyEmblem) -> Verdict<i64> {
        let conn = self.open_connection()?;
        let Some(member) = membership(&conn, character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if member.permissions & permission::EDIT_EMBLEM == 0 {
            return Ok(Err("Your rank may not change the emblem."));
        }
        conn.execute(
            "UPDATE families SET emblem_texture=?2, emblem_color=?3 WHERE id=?1",
            params![member.family_id, emblem.texture, emblem.color],
        )?;
        Ok(Ok(member.family_id))
    }

    pub fn create_family(&self, id: i64, name: &str) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if membership(&tx, id)?.is_some() {
            return Ok(Err("You already belong to a family."));
        }
        if in_cooldown(&tx, id)? {
            return Ok(Err("You left a family less than a day ago."));
        }
        let account: String = tx.query_row(
            "SELECT account_name FROM characters WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        let (estates, attached): (i64, i64) = tx.query_row(
            "SELECT COUNT(*), COUNT(family_id) FROM land_estates
             WHERE account_name=?1 AND missed=0",
            [&account],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if estates == 0 {
            return Ok(Err(
                "Own an estate with no overdue taxes to found a family.",
            ));
        }
        if attached > 0 {
            return Ok(Err("Your estate already belongs to another family."));
        }
        if let Some(reason) = self.family_name_refusal(&tx, &account, name, None)? {
            return Ok(Err(reason));
        }
        let now = unix_now();
        tx.execute(
            "INSERT INTO families (name, created_at) VALUES (?1, ?2)",
            params![name, now],
        )?;
        let family_id = tx.last_insert_rowid();
        record_family_event(&tx, family_id, name, FamilyEvent::Founded)?;
        for rank in 0..FAMILY_RANK_COUNT {
            tx.execute(
                "INSERT INTO family_ranks (family_id, rank, label, permissions) VALUES (?1,?2,?3,?4)",
                params![
                    family_id,
                    rank,
                    default_rank_label(rank),
                    default_rank_permissions(rank)
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO family_members (character_id, family_id, rank, joined_at) VALUES (?1,?2,?3,?4)",
            params![id, family_id, FAMILY_RANK_HEAD, now],
        )?;
        tx.execute(
            "UPDATE land_estates SET family_id=?1 WHERE account_name=?2",
            params![family_id, account],
        )?;
        tx.commit()?;
        Ok(Ok(family_id))
    }

    /// Add `target_id` to the inviter's family; returns its id and name.
    pub fn join_family(
        &self,
        inviter_id: i64,
        target_id: i64,
    ) -> Result<Result<(i64, String), InviteBlock>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (family_id, name) = match invite_target(&tx, inviter_id, target_id)? {
            Ok(family) => family,
            Err(block) => return Ok(Err(block)),
        };
        tx.execute(
            "INSERT INTO family_members (character_id, family_id, rank, joined_at) VALUES (?1,?2,?3,?4)",
            params![target_id, family_id, FAMILY_RANK_MEMBER, unix_now()],
        )?;
        tx.commit()?;
        Ok(Ok((family_id, name)))
    }

    /// The family name `inviter_id` could add `target_id` to right now.
    pub fn check_family_invite(
        &self,
        inviter_id: i64,
        target_id: i64,
    ) -> Result<Result<String, InviteBlock>, AuthError> {
        let conn = self.open_connection()?;
        Ok(invite_target(&conn, inviter_id, target_id)?.map(|(_, name)| name))
    }

    /// Leave (`actor_id == target_id`) or kick. The head never leaves this
    /// way: they hand the family over, or dissolve it at the registrar.
    pub fn remove_family_member(&self, actor_id: i64, target_id: i64) -> Verdict<FamilyDeparture> {
        self.depart_family(actor_id, target_id, false)
    }

    /// The head, as the last member, ends the family.
    pub fn dissolve_family(&self, actor_id: i64) -> Verdict<FamilyDeparture> {
        self.depart_family(actor_id, actor_id, true)
    }

    fn depart_family(
        &self,
        actor_id: i64,
        target_id: i64,
        dissolve: bool,
    ) -> Verdict<FamilyDeparture> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(target) = membership(&tx, target_id)? else {
            return Ok(Err("Not a member of a family."));
        };
        if actor_id == target_id {
            let members: i64 = tx.query_row(
                "SELECT COUNT(*) FROM family_members WHERE family_id=?1",
                [target.family_id],
                |row| row.get(0),
            )?;
            let head = target.rank == FAMILY_RANK_HEAD;
            let refusal = match (dissolve, head, members > 1) {
                (true, false, _) => Some("Only the head can dissolve the family."),
                (true, true, true) => {
                    Some("Remove every other member before dissolving the family.")
                }
                (false, true, true) => Some("Hand the family to another member before leaving."),
                (false, true, false) => Some("Visit the Land Registrar to dissolve the family."),
                _ => None,
            };
            if let Some(reason) = refusal {
                return Ok(Err(reason));
            }
        } else {
            let allowed = membership(&tx, actor_id)?.is_some_and(|actor| {
                actor.family_id == target.family_id
                    && actor.permissions & permission::KICK != 0
                    && actor.rank < target.rank
            });
            if !allowed {
                return Ok(Err("Your rank may not remove that member."));
            }
        }
        let (family_name, target_name): (String, String) = tx.query_row(
            "SELECT (SELECT name FROM families WHERE id=?1),
                    (SELECT character_name FROM characters WHERE id=?2)",
            params![target.family_id, target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tx.execute(
            "DELETE FROM family_members WHERE character_id=?1",
            [target_id],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO family_leave_cooldowns (character_id, left_at) VALUES (?1, ?2)",
            params![target_id, unix_now()],
        )?;
        let dissolved = settle_family(&tx, target.family_id, Some(target_id))?;
        tx.commit()?;
        Ok(Ok(FamilyDeparture {
            family_id: target.family_id,
            family_name,
            target_name,
            dissolved,
        }))
    }

    /// Repair every family a cascade left without members or a head (character
    /// or account deletion). Returns the surviving families that changed.
    pub fn settle_families(&self) -> Result<Vec<i64>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stmt = tx.prepare(
            "SELECT f.id FROM families f WHERE NOT EXISTS(
                SELECT 1 FROM family_members m WHERE m.family_id=f.id AND m.rank=?1)
             UNION
             SELECT e.family_id FROM land_estates e WHERE e.family_id IS NOT NULL
               AND e.account_name NOT IN (
                SELECT c.account_name FROM family_members m JOIN characters c ON c.id=m.character_id
                WHERE m.family_id=e.family_id)",
        )?;
        let ids = stmt
            .query_map([FAMILY_RANK_HEAD], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut changed = Vec::new();
        for id in ids {
            if !settle_family(&tx, id, None)? {
                changed.push(id);
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn set_family_member_rank(&self, actor_id: i64, target_id: i64, rank: u8) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (Some(actor), Some(target)) = (membership(&tx, actor_id)?, membership(&tx, target_id)?)
        else {
            return Ok(Err("Not a member of your family."));
        };
        if actor.family_id != target.family_id {
            return Ok(Err("Not a member of your family."));
        }
        if actor.permissions & permission::EDIT_RANKS == 0 {
            return Ok(Err("Your rank may not change ranks."));
        }
        if rank == FAMILY_RANK_HEAD || rank >= FAMILY_RANK_COUNT {
            return Ok(Err("No such rank."));
        }
        if actor.rank >= target.rank || actor.rank >= rank {
            return Ok(Err("You can only change ranks below your own."));
        }
        tx.execute(
            "UPDATE family_members SET rank=?2 WHERE character_id=?1",
            params![target_id, rank],
        )?;
        tx.commit()?;
        Ok(Ok(actor.family_id))
    }

    fn swap_head(conn: &Connection, family_id: i64, new_head: i64) -> Result<(), rusqlite::Error> {
        conn.execute(
            "UPDATE family_members SET rank=?2 WHERE family_id=?1 AND rank=?3",
            params![family_id, FAMILY_RANK_ELDER, FAMILY_RANK_HEAD],
        )?;
        conn.execute(
            "UPDATE family_members SET rank=?2 WHERE character_id=?1",
            params![new_head, FAMILY_RANK_HEAD],
        )?;
        Ok(())
    }

    pub fn transfer_family_head(&self, actor_id: i64, target_id: i64) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (Some(actor), Some(target)) = (membership(&tx, actor_id)?, membership(&tx, target_id)?)
        else {
            return Ok(Err("Not a member of your family."));
        };
        if actor.family_id != target.family_id || actor_id == target_id {
            return Ok(Err("Not a member of your family."));
        }
        if actor.rank != FAMILY_RANK_HEAD {
            return Ok(Err("Only the head can hand the family over."));
        }
        Self::swap_head(&tx, actor.family_id, target_id)?;
        tx.commit()?;
        Ok(Ok(actor.family_id))
    }

    pub fn claim_family_head(&self, actor_id: i64, family_id: i64, online: &[i64]) -> Verdict<i64> {
        let Some(roster) = self.family_roster(family_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if roster.successor(unix_now(), online) != Some(actor_id) {
            return Ok(Err(
                "The head has not been away long enough, or another member is ahead of you.",
            ));
        }
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if membership(&tx, actor_id)?.is_none_or(|m| m.family_id != family_id) {
            return Ok(Err("You do not belong to a family."));
        }
        Self::swap_head(&tx, family_id, actor_id)?;
        tx.commit()?;
        Ok(Ok(family_id))
    }

    pub fn set_family_rank(
        &self,
        actor_id: i64,
        rank: u8,
        label: &str,
        permissions: u32,
    ) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(actor) = membership(&tx, actor_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if actor.permissions & permission::EDIT_RANKS == 0 {
            return Ok(Err("Your rank may not change ranks."));
        }
        if rank >= FAMILY_RANK_COUNT {
            return Ok(Err("No such rank."));
        }
        if !valid_rank_label(label) {
            return Ok(Err(
                "Rank names are 1 to 12 letters or digits, without spaces.",
            ));
        }
        let permissions = if rank == FAMILY_RANK_HEAD {
            if actor.rank != FAMILY_RANK_HEAD {
                return Ok(Err("You can only change ranks below your own."));
            }
            permission::ALL
        } else {
            if actor.rank >= rank {
                return Ok(Err("You can only change ranks below your own."));
            }
            let current: u32 = tx.query_row(
                "SELECT permissions FROM family_ranks WHERE family_id=?1 AND rank=?2",
                params![actor.family_id, rank],
                |row| row.get(0),
            )?;
            let permissions = permissions & permission::ALL;
            if (permissions ^ current) & !actor.permissions != 0 {
                return Ok(Err("You cannot change a permission you do not hold."));
            }
            permissions
        };
        tx.execute(
            "UPDATE family_ranks SET label=?3, permissions=?4 WHERE family_id=?1 AND rank=?2",
            params![actor.family_id, rank, label, permissions],
        )?;
        tx.commit()?;
        Ok(Ok(actor.family_id))
    }

    pub fn rename_family(&self, id: i64, name: &str) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(actor) = membership(&tx, id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if actor.rank != FAMILY_RANK_HEAD {
            return Ok(Err("Only the head can rename the family."));
        }
        let (old_name, renamed_at): (String, Option<i64>) = tx.query_row(
            "SELECT name, renamed_at FROM families WHERE id=?1",
            [actor.family_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let now = unix_now();
        if renamed_at.is_some_and(|at| now - at < FAMILY_RENAME_INTERVAL_SECS) {
            return Ok(Err("A family can be renamed once every 30 days."));
        }
        if old_name == name {
            return Ok(Err("That is already the family's name."));
        }
        let account: String = tx.query_row(
            "SELECT account_name FROM characters WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        if let Some(reason) =
            self.family_name_refusal(&tx, &account, name, Some(actor.family_id))?
        {
            return Ok(Err(reason));
        }
        tx.execute(
            "UPDATE families SET name=?2, renamed_at=?3 WHERE id=?1",
            params![actor.family_id, name, now],
        )?;
        if !old_name.eq_ignore_ascii_case(name) {
            retire_name(&tx, &old_name)?;
        }
        tx.commit()?;
        Ok(Ok(actor.family_id))
    }

    pub fn attach_family_estate(&self, character_id: i64, attach: bool) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(member) = membership(&tx, character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        let account: String = tx.query_row(
            "SELECT account_name FROM characters WHERE id=?1",
            [character_id],
            |row| row.get(0),
        )?;
        let changed = if attach {
            let elsewhere: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM land_estates WHERE account_name=?1
                               AND family_id IS NOT NULL AND family_id!=?2)",
                params![account, member.family_id],
                |row| row.get(0),
            )?;
            if elsewhere {
                return Ok(Err(
                    "Your estate belongs to another family. Detach it there first.",
                ));
            }
            tx.execute(
                "UPDATE land_estates SET family_id=?2 WHERE account_name=?1 AND family_id IS NULL",
                params![account, member.family_id],
            )?
        } else {
            let stocked: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM estate_chests c
                    JOIN land_estates e ON e.id=c.estate_id
                    JOIN estate_chest_items i ON i.chest_id=c.id
                    WHERE e.account_name=?1 AND c.family_id=?2)",
                params![account, member.family_id],
                |row| row.get(0),
            )?;
            if stocked {
                return Ok(Err(
                    "Empty the family chests on your estate before detaching it.",
                ));
            }
            tx.execute(
                "UPDATE land_estates SET family_id=NULL WHERE account_name=?1 AND family_id=?2",
                params![account, member.family_id],
            )?
        };
        if changed == 0 {
            return Ok(Err(if attach {
                "You have no estate to attach."
            } else {
                "Your estate is not attached to this family."
            }));
        }
        tx.commit()?;
        Ok(Ok(member.family_id))
    }

    /// `character.gold` is the balance before the deposit.
    pub fn deposit_family_estate(
        &self,
        character: CharacterSaveData,
        inventory: &[ItemRow],
        estate_id: i64,
        amount: i64,
    ) -> Verdict<i64> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(member) = membership(&tx, character.character_id)? else {
            return Ok(Err("You do not belong to a family."));
        };
        if member.permissions & permission::TREASURY_DEPOSIT == 0 {
            return Ok(Err("Your rank may not pay into estate tax accounts."));
        }
        let attached: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM land_estates WHERE id=?1 AND family_id=?2)",
            params![estate_id, member.family_id],
            |row| row.get(0),
        )?;
        if !attached {
            return Ok(Err("That estate is not attached to your family."));
        }
        if let Err(reason) =
            Self::transfer_estate_gold(&tx, estate_id, character, inventory, amount, true)?
        {
            return Ok(Err(reason));
        }
        tx.commit()?;
        Ok(Ok(member.family_id))
    }

    /// Owner ids the client treats as "mine" when offering to remove a fence
    /// or demolish a house: the account's characters, the family's members,
    /// and the owners of family estates. A hint only; each edit is re-checked.
    pub fn landscaping_owner_ids(&self, character_id: i64) -> Result<Vec<i64>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM characters
             WHERE account_name=(SELECT account_name FROM characters WHERE id=?1)
             UNION SELECT other.character_id FROM family_members me
                   JOIN family_members other ON other.family_id=me.family_id
                   WHERE me.character_id=?1
             UNION SELECT e.owner_id FROM family_members me
                   JOIN land_estates e ON e.family_id=me.family_id
                   WHERE me.character_id=?1
             ORDER BY 1",
        )?;
        let ids = stmt.query_map([character_id], |row| row.get(0))?;
        Ok(ids.collect::<Result<Vec<_>, _>>()?)
    }
}
