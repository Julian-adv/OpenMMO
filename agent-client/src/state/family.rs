use super::*;
use onlinerpg_shared::family::{
    permission, FamilyInfo, FamilyMergeProposal, MergeSide, FAMILY_INVITE_TTL, FAMILY_MERGE_TTL,
    FAMILY_RANK_HEAD,
};

pub(super) const MAX_PENDING_FAMILY_INVITES: usize = 3;
pub(super) const MAX_PENDING_FAMILY_MERGES: usize = 3;
const LISTED_MEMBERS: usize = 10;
const LISTED_ESTATES: usize = 6;

pub struct PendingFamilyInvite {
    pub inviter_id: PlayerId,
    pub inviter_name: String,
    pub family_name: String,
    pub expires_at: std::time::Instant,
}

pub struct PendingFamilyMerge {
    pub proposal: FamilyMergeProposal,
    pub expires_at: std::time::Instant,
}

/// What a merge offer leaves behind, from the receiving head's side.
pub fn merge_outcome(p: &FamilyMergeProposal) -> String {
    let side = |s: MergeSide, theirs: &str, ours: &str| match s {
        MergeSide::Proposer => theirs.to_string(),
        MergeSide::Target => ours.to_string(),
    };
    format!(
        "the merged family is named {}, shows {} emblem, is led by {}, and has {} members",
        side(p.terms.keep_name, &p.proposer_family, &p.target_family),
        side(
            p.terms.keep_emblem,
            &format!("{}'s", p.proposer_family),
            "your"
        ),
        side(p.terms.keep_head, &p.proposer_name, "you"),
        p.members
    )
}

impl SharedState {
    /// Our family rank's permission bits; the head holds them all.
    pub fn family_permissions(&self) -> u32 {
        let Some(f) = &self.family else {
            return 0;
        };
        if f.my_rank == FAMILY_RANK_HEAD {
            return permission::ALL;
        }
        f.ranks
            .get(usize::from(f.my_rank))
            .map_or(0, |r| r.permissions)
    }

    pub fn family_may(&self, bit: u32) -> bool {
        self.family_permissions() & bit != 0
    }

    pub fn is_family_head(&self) -> bool {
        self.family
            .as_ref()
            .is_some_and(|f| f.my_rank == FAMILY_RANK_HEAD)
    }

    /// Whether estate `estate_id` is attached to our family.
    pub fn family_estate(&self, estate_id: i64) -> bool {
        self.family
            .as_ref()
            .is_some_and(|f| f.estates.iter().any(|e| e.estate_id == estate_id))
    }

    pub fn prune_expired_family_invites(&mut self) {
        let now = std::time::Instant::now();
        self.pending_family_invites.retain(|i| i.expires_at > now);
    }

    pub fn live_family_invites(&self) -> impl Iterator<Item = &PendingFamilyInvite> {
        let now = std::time::Instant::now();
        self.pending_family_invites
            .iter()
            .filter(move |i| i.expires_at > now)
    }

    pub fn prune_expired_family_merges(&mut self) {
        let now = std::time::Instant::now();
        self.pending_family_merges.retain(|m| m.expires_at > now);
    }

    pub fn live_family_merges(&self) -> impl Iterator<Item = &PendingFamilyMerge> {
        let now = std::time::Instant::now();
        self.pending_family_merges
            .iter()
            .filter(move |m| m.expires_at > now)
    }

    pub(super) fn on_family_state(&mut self, family: &Option<FamilyInfo>) {
        if family.is_some() {
            self.pending_family_invites.clear();
        }
        if family
            .as_ref()
            .is_none_or(|f| f.my_rank != FAMILY_RANK_HEAD)
        {
            self.pending_family_merges.clear();
        }
        self.family.clone_from(family);
    }

    pub(super) fn on_family_invite(
        &mut self,
        inviter_id: PlayerId,
        inviter_name: &str,
        family_name: &str,
    ) {
        self.prune_expired_family_invites();
        let queue = &mut self.pending_family_invites;
        if queue.len() < MAX_PENDING_FAMILY_INVITES
            && !queue.iter().any(|i| i.inviter_id == inviter_id)
        {
            queue.push(PendingFamilyInvite {
                inviter_id,
                inviter_name: inviter_name.to_string(),
                family_name: family_name.to_string(),
                expires_at: std::time::Instant::now() + FAMILY_INVITE_TTL,
            });
        }
    }

    pub(super) fn on_family_merge(&mut self, proposal: &FamilyMergeProposal) {
        self.prune_expired_family_merges();
        let queue = &mut self.pending_family_merges;
        queue.retain(|m| m.proposal.proposer_id != proposal.proposer_id);
        if queue.len() < MAX_PENDING_FAMILY_MERGES {
            queue.push(PendingFamilyMerge {
                proposal: proposal.clone(),
                expires_at: std::time::Instant::now() + FAMILY_MERGE_TTL,
            });
        }
    }

    pub(super) fn format_family(&self, lines: &mut Vec<String>) {
        for invite in self.live_family_invites() {
            lines.push(format!(
                "Pending family invite from {} to join the {} family — answer with family_accept or family_decline",
                invite.inviter_name, invite.family_name
            ));
        }
        for merge in self.live_family_merges() {
            let p = &merge.proposal;
            lines.push(format!(
                "Merge offer from {}, head of {}: {}; it cannot be undone — answer with family_merge_accept or family_merge_decline",
                p.proposer_name,
                p.proposer_family,
                merge_outcome(p)
            ));
        }
        let Some(f) = &self.family else {
            return;
        };
        let rank = f
            .ranks
            .get(usize::from(f.my_rank))
            .map_or("member", |r| r.label.as_str());
        let mut may = Vec::new();
        for (bit, what) in [
            (permission::INVITE, "invite"),
            (permission::STORAGE_DEPOSIT, "store in family chests"),
            (permission::STORAGE_WITHDRAW, "take from family chests"),
            (permission::BOARD_WRITE, "post on family boards"),
        ] {
            if self.family_may(bit) {
                may.push(what);
            }
        }
        if f.my_rank == FAMILY_RANK_HEAD {
            may.push("propose merges");
        }
        lines.push(format!(
            "Your family: {} — you are {rank} (rank {}){}",
            f.name,
            f.my_rank,
            if may.is_empty() {
                String::new()
            } else {
                format!("; you may {}", may.join(", "))
            }
        ));
        let me = self.self_player.as_ref().map(|p| p.name.as_str());
        let online: Vec<&str> = f
            .members
            .iter()
            .filter(|m| m.online && Some(m.name.as_str()) != me)
            .map(|m| m.name.as_str())
            .collect();
        let shown = online.len().min(LISTED_MEMBERS);
        lines.push(format!(
            "  {} members; online besides you: {}{}",
            f.members.len(),
            if online.is_empty() {
                "nobody".to_string()
            } else {
                online[..shown].join(", ")
            },
            if online.len() > shown {
                format!(" (+{} more)", online.len() - shown)
            } else {
                String::new()
            }
        ));
        if !f.estates.is_empty() {
            let estates: Vec<String> = f
                .estates
                .iter()
                .take(LISTED_ESTATES)
                .map(|e| {
                    let mut s = if e.mine {
                        "yours".to_string()
                    } else {
                        format!("{}'s", e.owner_name)
                    };
                    if e.missed > 0 {
                        s.push_str(" (overdue)");
                    }
                    s
                })
                .collect();
            let more = f.estates.len().saturating_sub(LISTED_ESTATES);
            lines.push(format!(
                "  Family estates: {}{}",
                estates.join(", "),
                if more > 0 {
                    format!(" (+{more} more)")
                } else {
                    String::new()
                }
            ));
        }
        if f.emblem_texture.is_some() || f.emblem_color.is_some() {
            lines.push(format!(
                "  Your cape shows {} — family_cape_emblem switches it",
                if f.my_cape_emblem {
                    "the family emblem"
                } else {
                    "its own print, not the family emblem"
                }
            ));
        }
    }
}
