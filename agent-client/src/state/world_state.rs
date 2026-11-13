use super::*;
use onlinerpg_shared::ability::{
    BATTLE_HYMN_CAST_MS, BATTLE_HYMN_DURATION_MS, BATTLE_HYMN_RADIUS, BATTLE_HYMN_SPEED_MULT,
    GUARDIAN_WARD_COOLDOWN_MS, GUARDIAN_WARD_DURATION_MS, GUARDIAN_WARD_RADIUS,
    HEALING_PRAYER_COOLDOWN_MS, HEALING_PRAYER_DICE, HEALING_PRAYER_RANGE, PRIMAL_ROAR_CAST_MS,
    PRIMAL_ROAR_COOLDOWN_MS, PRIMAL_ROAR_DURATION_MS, PRIMAL_ROAR_RADIUS, PRIMAL_ROAR_STAGGER_MS,
    PRIMAL_ROAR_STOMP_MS, RAGE_COOLDOWN_MS, RAGE_DURATION_MS, RAGE_GUARD_PENALTY,
    VALHALLA_CHARGE_COOLDOWN_MS, VALHALLA_CHARGE_DASH_MS, VALHALLA_CHARGE_MAX_RANGE,
    VALHALLA_CHARGE_MIN_RANGE, VALHALLA_CHARGE_SHORT_SLOW_MS, VALHALLA_CHARGE_SLOW_MS,
    VALHALLA_CHARGE_SLOW_MULT,
};

pub struct BuffAbility {
    pub tag: &'static str,
    pub name: &'static str,
    pub requires: &'static str,
    pub equipment: &'static str,
}

pub fn buff_ability(ability: AbilityId) -> Option<BuffAbility> {
    match ability {
        AbilityId::GuardianWard => Some(BuffAbility {
            tag: "GuardianWard",
            name: "Guardian Ward",
            requires: "a living, unmounted Knight with an MP pool",
            equipment: "a one-handed sword or mace and an off-hand shield",
        }),
        AbilityId::Rage => Some(BuffAbility {
            tag: "Rage",
            name: "Rage",
            requires: "a living, unmounted Barbarian",
            equipment: "a melee weapon in your main hand",
        }),
        AbilityId::PrimalRoar => Some(BuffAbility {
            tag: "PrimalRoar",
            name: "Primal Roar",
            requires: "a living, unmounted Caveman",
            equipment: "nothing",
        }),
        AbilityId::ValhallaCharge => Some(BuffAbility {
            tag: "ValhallaCharge",
            name: "Valhalla Charge",
            requires: "a living, unmounted Valkyrie and a living monster target on your floor",
            equipment: "a one-handed sword or spear in your main hand",
        }),
        AbilityId::HealingPrayer => Some(BuffAbility {
            tag: "HealingPrayer",
            name: "Healing Prayer",
            requires: "a living, unmounted Priest and a living player or undead monster target in sight on your floor",
            equipment: "nothing",
        }),
        AbilityId::BattleHymn => Some(BuffAbility {
            tag: "BattleHymn",
            name: "Battle Hymn",
            requires: "a living, unmounted Bard",
            equipment: "an instrument in your bag or hands",
        }),
        _ => None,
    }
}

/// Exact only when the server disclosed `max_health`; otherwise alive or dead.
pub fn hp_text(health: u32, max_health: u32) -> String {
    if max_health > 0 {
        format!("HP {health}/{max_health}")
    } else if health > 0 {
        "alive (HP hidden)".to_string()
    } else {
        "dead".to_string()
    }
}

impl SharedState {
    fn buff_ability_status(
        &self,
        player: &Player,
        ability: AbilityId,
        equipped: bool,
    ) -> (String, String) {
        let wait = self.ability_wait(ability);
        let mana_cost = ability.mana_cost();
        let readiness = if player.health == 0 {
            "respawn first".to_string()
        } else if player.is_mounted() {
            "dismount first".to_string()
        } else if !equipped {
            let equipment = buff_ability(ability).map_or("the required gear", |a| a.equipment);
            format!("equip {equipment} first")
        } else if !wait.is_zero() {
            format!("ready in {:.1}s", wait.as_secs_f32())
        } else {
            match self.self_mana {
                Some((mana, _)) if mana >= mana_cost => "ready".to_string(),
                Some(_) => format!("not enough MP; requires {mana_cost} MP"),
                None => "waiting for MP state".to_string(),
            }
        };
        let remaining = self
            .buff_expires_at
            .get(&ability)
            .map_or(std::time::Duration::ZERO, |expires| {
                expires.saturating_duration_since(tokio::time::Instant::now())
            });
        let buff = if remaining.is_zero() {
            "not active".to_string()
        } else {
            format!("active for {:.1}s", remaining.as_secs_f32())
        };
        (readiness, buff)
    }

    /// Whether `health` values for this player arrive exact.
    pub fn knows_hp_of(&self, player_id: &PlayerId) -> bool {
        self.self_player_id.as_ref() == Some(player_id)
            || self
                .nearby_players
                .get(player_id)
                .is_some_and(|p| p.max_health > 0)
    }

    /// Party members' exact HP stands in for a hidden nearby value.
    fn player_hp_text(&self, player: &Player) -> String {
        match self.party_members.iter().find(|m| m.id == player.id) {
            Some(member) if player.max_health == 0 && member.max_hp > 0 => {
                hp_text(member.hp, member.max_hp)
            }
            _ => hp_text(player.health, player.max_health),
        }
    }

    pub fn main_hand_def(&self) -> Option<&'static crate::item_defs::ItemDef> {
        self.self_equipped
            .get(&onlinerpg_shared::inventory::EquipSlot::MainHand)
            .and_then(|item| crate::item_defs::get(&item.item_def_id))
    }

    pub(super) fn main_hand_weapon(&self) -> Option<&'static crate::item_defs::ItemDef> {
        self.main_hand_def()
            .filter(|def| def.category.as_deref() == Some("weapon"))
    }

    pub fn attack_interval(&self) -> std::time::Duration {
        self.main_hand_weapon()
            .and_then(|def| def.attack_interval_ms)
            .map_or(self.attack_cooldown, std::time::Duration::from_millis)
    }

    pub fn attack_approach_range(&self) -> f32 {
        self.main_hand_weapon()
            .and_then(|def| def.range)
            .unwrap_or(onlinerpg_shared::reach::PLAYER_MELEE_REACH_METERS)
    }

    /// Current game time snapshot for schedule resolution.
    pub fn time_context(
        &self,
    ) -> (
        Option<onlinerpg_shared::schedule::SchedulePeriod>,
        Option<u32>,
        Option<u32>,
        Option<bool>,
    ) {
        (
            self.schedule_period,
            self.game_hour,
            self.game_minute,
            // A played-out meeting stops matching; the NPC goes home.
            self.is_serin_dark_day.filter(|_| !self.meeting_done()),
        )
    }

    /// This agent's own laid-out stall, if one is out.
    pub fn own_stall(&self) -> Option<&onlinerpg_shared::stall::Stall> {
        self.self_player_id
            .and_then(|id| self.stalls.values().find(|s| s.owner == id))
    }

    /// The storey of the building standing at `(x, z)` on `floor_level`,
    /// named; `None` outdoors or underground.
    pub fn storey_at(&self, x: f32, z: f32, floor_level: i8) -> Option<&'static str> {
        let floor = u8::try_from(floor_level).ok()?;
        let world = self.world_cache.read().unwrap();
        world.is_indoors(x, z, floor).then(|| storey_name(floor))
    }

    fn indoor_floor(&self, position: &Position) -> Option<&'static str> {
        self.storey_at(position.x, position.z, self.self_floor_level)
    }

    pub fn format_world_state(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref p) = self.self_player {
            lines.push(format!(
                "You: {} Lv.{} {:?} HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.name,
                p.level,
                p.class,
                p.health,
                p.max_health,
                p.position.x,
                p.position.y,
                p.position.z
            ));
            if let Some(floor) = self.indoor_floor(&p.position) {
                lines.push(format!("You are indoors ({floor})"));
            }
            if p.health == 0 {
                lines.push(
                    "You are DEFEATED (HP 0). You do NOT recover on your own and most \
                     actions stay blocked. Respawn now with {\"type\": \"respawn\"} — \
                     the death penalty was already paid when you fell; respawning \
                     costs nothing more."
                        .to_string(),
                );
            }
        }
        if let Some(line) = self.format_dungeon_state() {
            lines.push(line);
        }
        if let Some((mana, max_mana)) = self.self_mana {
            lines.push(format!("MP: {mana}/{max_mana}"));
        }
        if let Some(trade) = self.format_player_trade() {
            lines.push(trade);
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Rogue)
        {
            let dagger_equipped = self
                .main_hand_def()
                .is_some_and(|def| def.weapon_type.as_deref() == Some("dagger"));
            let wait = self.double_slash_wait().max(self.player_attack_wait());
            let readiness = if player.health == 0 {
                "respawn first".to_string()
            } else if !dagger_equipped {
                "equip a dagger in your main hand first".to_string()
            } else if player.is_mounted() || player.object_type.is_some() {
                "dismount or leave your current pose first".to_string()
            } else if !wait.is_zero() {
                format!("ready in {:.1}s", wait.as_secs_f32())
            } else {
                "ready".to_string()
            };
            lines.push(format!(
                r#"Double Slash: {readiness}.
With a dagger equipped, you can choose Double Slash for the opening strike:
{{"type": "attack", "target": "m21", "skill": "double_slash"}}
This attempts the skill once, then continues with ordinary attacks.
Check readiness above; it shares normal attack recovery and has its own
cooldown. Moving, changing weapons or mounting can interrupt its two strikes.
The server reports starts, failures and skipped strikes; a start alone does
not mean both strikes hit."#
            ));
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Knight)
        {
            let main = self.main_hand_def();
            let off = self
                .self_equipped
                .get(&onlinerpg_shared::inventory::EquipSlot::OffHand)
                .and_then(|item| crate::item_defs::get(&item.item_def_id));
            let equipped = main.is_some_and(|def| {
                matches!(def.weapon_type.as_deref(), Some("sword" | "mace")) && !def.is_two_handed()
            }) && off.is_some_and(|def| def.armor_type.as_deref() == Some("shield"));
            let (readiness, buff) =
                self.buff_ability_status(player, AbilityId::GuardianWard, equipped);
            let mana_cost = AbilityId::GuardianWard.mana_cost();
            let ward_bonus = onlinerpg_shared::ability::guardian_ward_guard_bonus(player.level);
            lines.push(format!(
                r#"Guardian Ward: {readiness}. Your ward: {buff}.
As a Knight, cast with {{"type": "use_ability", "ability": "guardian_ward"}}.
Requires a one-handed sword or mace, an off-hand shield and {mana_cost} MP.
Grants Guard +{ward_bonus} (2, +1 per 20 caster levels) to you and living party members within {GUARDIAN_WARD_RADIUS}m on the same floor
for {}s; cooldown {}s. No target is needed. Recasting refreshes the duration; it does not stack.
Use the readiness and ward status above. Wait for server success or failure feedback."#,
                GUARDIAN_WARD_DURATION_MS / 1000,
                GUARDIAN_WARD_COOLDOWN_MS / 1000,
            ));
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Barbarian)
        {
            let equipped = self.main_hand_def().is_some_and(|def| {
                matches!(
                    def.weapon_type.as_deref(),
                    Some("sword" | "great_sword" | "axe" | "mace" | "club" | "spear" | "dagger")
                )
            });
            let (readiness, buff) = self.buff_ability_status(player, AbilityId::Rage, equipped);
            let mana_cost = AbilityId::Rage.mana_cost();
            let bonus = onlinerpg_shared::ability::rage_damage_bonus(player.level);
            lines.push(format!(
                r#"Rage: {readiness}. Your rage: {buff}.
As a Barbarian, cast with {{"type": "use_ability", "ability": "rage"}}.
Requires a melee weapon in your main hand and {mana_cost} MP.
For {}s your melee hits deal +{bonus} damage and your Guard drops by {RAGE_GUARD_PENALTY};
cooldown {}s. No target is needed. Unequipping your melee weapon ends it.
Use the readiness and rage status above. Wait for server success or failure feedback."#,
                RAGE_DURATION_MS / 1000,
                RAGE_COOLDOWN_MS / 1000,
            ));
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Caveman)
        {
            let (readiness, _) = self.buff_ability_status(player, AbilityId::PrimalRoar, true);
            let mana_cost = AbilityId::PrimalRoar.mana_cost();
            let shield = onlinerpg_shared::ability::primal_roar_temp_health(player.max_health);
            let temp = self.self_temp_health;
            lines.push(format!(
                r#"Primal Roar: {readiness}. Your temporary HP: {temp}.
As a Caveman, cast with {{"type": "use_ability", "ability": "primal_roar"}}.
Needs {mana_cost} MP and no weapon. You roar for {:.1}s without swinging. At the stomp, {:.1}s in,
living monsters within {PRIMAL_ROAR_RADIUS}m on your floor, not behind walls, are stunned for {:.1}s
and turn on you. You gain {shield} temporary HP for {}s at once; it takes damage before your HP
and does not stack. Cooldown {}s. No target is needed.
Use the readiness status above. Wait for server success or failure feedback."#,
                PRIMAL_ROAR_CAST_MS as f32 / 1000.0,
                PRIMAL_ROAR_STOMP_MS as f32 / 1000.0,
                PRIMAL_ROAR_STAGGER_MS as f32 / 1000.0,
                PRIMAL_ROAR_DURATION_MS / 1000,
                PRIMAL_ROAR_COOLDOWN_MS / 1000,
            ));
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Valkyrie)
        {
            let equipped = self.main_hand_def().is_some_and(|def| {
                matches!(def.weapon_type.as_deref(), Some("sword" | "spear"))
                    && !def.is_two_handed()
            });
            let (readiness, _) =
                self.buff_ability_status(player, AbilityId::ValhallaCharge, equipped);
            let mana_cost = AbilityId::ValhallaCharge.mana_cost();
            lines.push(format!(
                r#"Valhalla Charge: {readiness}.
As a Valkyrie, cast with {{"type": "use_ability", "ability": "valhalla_charge", "target": "<monster id>"}}.
Requires a one-handed sword or spear and {mana_cost} MP, and your basic attack must be ready.
The monster must be {VALHALLA_CHARGE_MIN_RANGE}-{VALHALLA_CHARGE_MAX_RANGE}m away on your floor with a clear,
walkable straight line. You dash into melee reach in {:.1}s and swing once on arrival; a hit slows
its movement to {}% for {}s ({}s for bosses). Cooldown {}s. Keep fighting with "attack" afterwards.
Use the readiness status above. Wait for server success or failure feedback."#,
                VALHALLA_CHARGE_DASH_MS as f32 / 1000.0,
                (VALHALLA_CHARGE_SLOW_MULT * 100.0).round(),
                VALHALLA_CHARGE_SLOW_MS / 1000,
                VALHALLA_CHARGE_SHORT_SLOW_MS as f32 / 1000.0,
                VALHALLA_CHARGE_COOLDOWN_MS / 1000,
            ));
        }
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Priest)
        {
            let (readiness, _) = self.buff_ability_status(player, AbilityId::HealingPrayer, true);
            let mana_cost = AbilityId::HealingPrayer.mana_cost();
            let roll = self
                .characters
                .iter()
                .find(|c| c.name == player.name)
                .map_or_else(
                    || format!("{HEALING_PRAYER_DICE} + Level/3 + WIS modifier"),
                    |c| {
                        let bonus = onlinerpg_shared::ability::healing_prayer_bonus(
                            player.level,
                            c.attributes.wis,
                        );
                        format!("{HEALING_PRAYER_DICE}{bonus:+}")
                    },
                );
            let undead = crate::monster_defs::undead_names().join(", ");
            lines.push(format!(
                r#"Healing Prayer: {readiness}.
As a Priest, heal yourself with {{"type": "use_ability", "ability": "healing_prayer"}}.
Add "target": "<player name>" to heal another living player, or "target": "<monster id>" to harm
an undead monster. Needs {mana_cost} MP and works with any gear. The target must be within {HEALING_PRAYER_RANGE}m
on your floor with no wall between you. It restores {roll} HP (at least 1), never above max HP;
a target already at full HP refuses it without cost. It harms only undead monsters ({undead}),
for the same roll; any other monster refuses it. Cooldown {}s. Other players show only alive or
dead unless they are in your party or you wear a stethoscope.
Use the readiness status above. Wait for server success or failure feedback."#,
                HEALING_PRAYER_COOLDOWN_MS / 1000,
            ));
        }
        let speed_percent = ((BATTLE_HYMN_SPEED_MULT - 1.0) * 100.0).round();
        if let Some(player) = self
            .self_player
            .as_ref()
            .filter(|player| player.class == onlinerpg_shared::CharacterClass::Bard)
        {
            let (readiness, _) =
                self.buff_ability_status(player, AbilityId::BattleHymn, self.holds_instrument());
            let wait = self.ability_wait(AbilityId::BattleHymn);
            let performing = player.object_type.as_deref() == Some(MUSIC_EMOTE);
            // The cast runs as the cooldown, so a wait while strumming is the hymn.
            let status = if performing && !wait.is_zero() {
                format!(
                    "playing; done in {:.1}s if you keep still",
                    wait.as_secs_f32()
                )
            } else {
                readiness
            };
            let mana_cost = AbilityId::BattleHymn.mana_cost();
            lines.push(format!(
                r#"Battle Hymn: {status}.
As a Bard, play it with {{"type": "use_ability", "ability": "battle_hymn"}}.
Needs an instrument in your bag or hands and {mana_cost} MP, spent when it finishes. You play the hymn's
own melody for {}s, taking over any tune you are playing. Moving, attacking or being hit before
the end cancels it at no cost, and you may start again at once. Then you and living party members
within {BATTLE_HYMN_RADIUS}m on your floor move and attack {speed_percent}% faster for {} minutes, wherever they go.
Playing again renews it; it does not stack. Best played between fights.
Use the readiness status above. Wait for server success or failure feedback."#,
                BATTLE_HYMN_CAST_MS / 1000,
                BATTLE_HYMN_DURATION_MS / 60_000,
            ));
        }
        if let Some(until) = self.buff_expires_at.get(&AbilityId::BattleHymn) {
            let left = until.saturating_duration_since(tokio::time::Instant::now());
            if !left.is_zero() {
                lines.push(format!(
                    "Battle Hymn: you move and attack {speed_percent}% faster for {}s more.",
                    left.as_secs()
                ));
            }
        }
        if let Some((satiation, state)) = self.self_hunger {
            let mut line = format!("Hunger: {state:?} ({satiation}/1000)");
            if !self.self_debuffs.is_empty() {
                line.push_str(&format!(", debuffs: {}", self.self_debuffs.join(", ")));
            }
            lines.push(line);
        }
        if let Some(p) = &self.self_player {
            let nearest_fire = self
                .campfires
                .values()
                .filter(|c| c.floor_level == p.floor_level)
                .map(|c| c.position.dist_xz_sq(&p.position))
                .min_by(f32::total_cmp);
            if let Some(d2) = nearest_fire {
                lines.push(format!(
                    "Campfire nearby: {:.1}m away (use a raw fish within 3m to grill it)",
                    d2.sqrt()
                ));
            }
            if let Some(own_stall) = self.own_stall() {
                lines.push(format!(
                    "Your stall is laid out {:.1}m away",
                    own_stall.position.dist_xz_sq(&p.position).sqrt()
                ));
            }
        }
        if let Some(gold) = self.self_gold {
            lines.push(format!(
                "Your gold: {}",
                crate::shop_info::format_price(gold)
            ));
        }
        self.format_stalls(&mut lines);
        self.format_notice_boards(&mut lines);
        self.format_mailboxes(&mut lines);
        self.format_family_chests(&mut lines);
        if !self.self_bag.is_empty() {
            let items: Vec<String> = self
                .self_bag
                .iter()
                .map(|i| {
                    if i.quantity > 1 {
                        format!("{} x{}", i.item_def_id, i.quantity)
                    } else {
                        i.item_def_id.clone()
                    }
                })
                .collect();
            lines.push(format!("Your bag: {}", items.join(", ")));
        }
        if !self.self_equipped.is_empty() {
            let mut worn: Vec<String> = self
                .self_equipped
                .iter()
                .map(|(slot, i)| format!("{}: {}", slot.as_str(), i.item_def_id))
                .collect();
            worn.sort();
            lines.push(format!("You are wearing: {}", worn.join(", ")));
        }
        if !self.self_titles.is_empty() {
            let shown = self.self_player.as_ref().and_then(|me| me.title.as_deref());
            let list: Vec<String> = self
                .self_titles
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let mark = if shown == Some(id.as_str()) {
                        " (shown)"
                    } else {
                        ""
                    };
                    format!("{}. {}{mark}", i + 1, crate::title_defs::title_name(id))
                })
                .collect();
            lines.push(format!(
                "Your titles: {} — say \"/title N\" to show one, \"/title off\" to hide",
                list.join("; ")
            ));
        }
        // Data only — what to do with the list is the role template's call
        // (bard.txt: prefer something fresh, unless a listener asks again).
        if self.plays_music && !self.recent_songs.is_empty() {
            let list: Vec<&str> = self.recent_songs.iter().map(String::as_str).collect();
            lines.push(format!(
                "Songs you played recently, oldest first: {}",
                list.join(", ")
            ));
        }
        if !self.song_requests.is_empty() {
            let requests: Vec<_> = self
                .song_requests
                .iter()
                .take(10)
                .map(|(requester, track)| format!("\"{track}\" for {requester}"))
                .collect();
            lines.push(format!(
                "Tip-hat song queue ({} pending, played automatically in order before any other song or tale): {}",
                self.song_requests.len(), requests.join("; ")
            ));
        }

        if !self.party_members.is_empty() {
            let names: Vec<String> = self
                .party_members
                .iter()
                .map(|m| {
                    let leader = if Some(m.id) == self.party_leader {
                        " (leader)"
                    } else {
                        ""
                    };
                    format!("{}{leader} {}", m.name, hp_text(m.hp, m.max_hp))
                })
                .collect();
            lines.push(format!("Your party: {}", names.join(", ")));
        }
        for invite in self.live_party_invites() {
            lines.push(format!(
                "Pending party invite from {} — answer with party_accept or party_decline",
                invite.inviter_name
            ));
        }
        for summon in self.live_party_summons() {
            lines.push(format!(
                "{} calls you to their side (summoning scroll) — answer with summon_accept or summon_decline",
                summon.caster_name
            ));
        }
        for request in self.live_friend_requests() {
            lines.push(format!(
                "Pending friend request from {} — answer with friend_accept or friend_decline",
                request.requester_name
            ));
        }
        if !self.friends.is_empty() {
            let names: Vec<&str> = self.friends.iter().map(|f| f.name.as_str()).collect();
            lines.push(format!("Your friends: {}", names.join(", ")));
        }
        self.format_family(&mut lines);
        if let Some(offer) = self.pushed_trade.as_ref().filter(|t| t.is_live()) {
            let name = &offer.merchant_name;
            lines.push(format!(
                "{name}'s trade window is open on your screen — buy, sell, or decline_trade"
            ));
        }

        // Nearby players (exclude self and humans beyond the sight radius)
        let sp = self.self_player.as_ref();
        let index = self.pricing.as_ref().map_or(100, |p| p.index_percent);
        for (_, p) in self.players_on_my_floor() {
            if self.self_player_id.as_ref() == Some(&p.id) {
                continue;
            }
            let npc_tag = if p.is_official_npc { " (NPC)" } else { "" };
            let favor_tag = match self.favor.get(&p.name) {
                Some(v) if !p.is_official_npc && *v != 0 => format!(" (favor {v:+})"),
                _ => String::new(),
            };
            lines.push(format!(
                "Player: {}{npc_tag}{favor_tag} Lv.{} {} at ({:.1}, {:.1}, {:.1})",
                p.name,
                p.level,
                self.player_hp_text(p),
                p.position.x,
                p.position.y,
                p.position.z
            ));
            if let Some(title) = p.title.as_deref() {
                lines.push(format!(
                    "  Title: \"{}\"",
                    crate::title_defs::title_name(title)
                ));
            }
            if let Some(family) = p.family.as_deref() {
                lines.push(format!("  Family: {family}"));
            }
            if p.is_official_npc {
                if let Some(shop) = crate::shop_info::shop_line_for(&p.name, index) {
                    lines.push(shop);
                }
            }
        }

        if !self.dialogue_only() {
            let priest = sp.is_some_and(|p| p.class == onlinerpg_shared::CharacterClass::Priest);
            for m in self.monsters_on_my_floor() {
                let undead = if priest && crate::monster_defs::is_undead(&m.monster_type) {
                    " (undead)"
                } else {
                    ""
                };
                lines.push(format!(
                    "Monster: {} [{}]{undead} HP {}/{} state={} at ({:.1}, {:.1}, {:.1})",
                    crate::monster_defs::monster_name(&m.monster_type),
                    m.id,
                    m.health,
                    m.max_health,
                    m.state,
                    m.position.x,
                    m.position.y,
                    m.position.z
                ));
            }
        }

        // Items on the ground. Drops linger for minutes, so a busy hunting
        // ground would otherwise stack dozens of lines into every prompt.
        let ground = self.ground_items_in_sight();
        let hidden = ground.len().saturating_sub(MAX_LISTED_GROUND_ITEMS);
        for (d_sq, i) in ground.into_iter().take(MAX_LISTED_GROUND_ITEMS) {
            let dropped_by = match i.dropped_by.as_ref() {
                Some(id) if self.self_player_id.as_ref() == Some(id) => {
                    ", dropped by you".to_string()
                }
                Some(id) => format!(", dropped by {}", self.visible_name(id)),
                None => String::new(),
            };
            let amount = if i.quantity > 1 {
                format!(" x{}", i.quantity)
            } else {
                String::new()
            };
            let note = if let Some(owner) = self.loot_claimant(i) {
                format!(" — {owner}'s loot for now, leave it")
            } else if crate::item_defs::get(&i.item_def_id).is_some_and(|d| d.is_dungeon_key()) {
                " — DUNGEON KEY, pick it up first".to_string()
            } else {
                String::new()
            };
            lines.push(format!(
                "Item on ground: {}{amount} ({:.1}m away) [id {}]{dropped_by}{note}",
                i.item_def_id,
                d_sq.sqrt(),
                i.instance_id
            ));
        }
        if hidden > 0 {
            lines.push(format!("(and {hidden} more items further away)"));
        }

        // Tip hats within sight on our floor: where to drop coins after a
        // performance. Our own hat is listed too (it shows as yours).
        if let Some(sp) = sp {
            for hat in self.tip_hats.values() {
                if hat.floor_level != sp.floor_level {
                    continue;
                }
                let d_sq = hat.position.dist_xz_sq(&sp.position);
                let whose = if Some(hat.owner) == self.self_player_id {
                    "yours".to_string()
                } else {
                    format!("{}'s — drop coins in it with tip_hat", hat.owner_name)
                };
                lines.push(format!(
                    "Tip hat [id {}] {:.1}m away: {whose}",
                    hat.id,
                    d_sq.sqrt()
                ));
            }
            for meal in self.meals.values() {
                if meal.floor_level != sp.floor_level {
                    continue;
                }
                let d_sq = meal.position.dist_xz_sq(&sp.position);
                let whose = if Some(meal.for_player) == self.self_player_id {
                    "served to you — you eat it on your own once settled in the chair".to_string()
                } else {
                    format!("served to {}", self.visible_name(&meal.for_player))
                };
                let state = if meal.eaten { "Empty plate" } else { "Plate" };
                lines.push(format!(
                    "{state} of {} [id {}] on the table {:.1}m away: {whose}",
                    meal.item_def_id,
                    meal.id,
                    d_sq.sqrt()
                ));
            }
        }

        if lines.is_empty() {
            "No state available yet.".to_string()
        } else {
            lines.join("\n")
        }
    }
}

/// How a move leaves an upper storey, quoted wherever the LLM is told it is
/// upstairs.
pub const FLOOR_ZERO_HINT: &str =
    "add \"floor\": 0 to your move to reach the outdoors — you take the stairs on your own";

pub fn storey_name(floor: u8) -> &'static str {
    match floor {
        0 => "ground floor",
        1 => "2nd floor",
        2 => "3rd floor",
        _ => "an upper floor",
    }
}
