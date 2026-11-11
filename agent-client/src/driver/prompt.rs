//! Prompt construction: turn the current `SharedState`, the queue of
//! pending server events, and the active schedule entry into a single
//! string suitable for sending to an LLM. Also resolves which schedule
//! entry should currently be active based on game time.

use onlinerpg_shared::pricing::{PricingNotice, Trend};
use onlinerpg_shared::{PlayerId, ServerMessage};

use crate::state::{storey_name, SharedState, FLOOR_ZERO_HINT};
use onlinerpg_shared::schedule::ScheduleEntry;

pub(crate) fn player_within_event_range(state: &SharedState, player_id: &PlayerId) -> bool {
    state.self_player_id.as_ref() == Some(player_id) || state.nearby_players.contains_key(player_id)
}

/// Build a prompt string from current state and events. `memory` is the
/// tail of the NPC's memory file, re-read per prompt so notes written this
/// session reach a stateless backend without a restart; `terrain` is
/// the surface summary a `TerrainSummaryJob` rendered outside the state lock.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_prompt(
    state: &SharedState,
    events: &[ServerMessage],
    agent_events: &[String],
    schedule: &[ScheduleEntry],
    active_schedule_idx: Option<usize>,
    memory: Option<&str>,
    terrain: Option<&str>,
    tale: Option<String>,
) -> String {
    let mut prompt = String::new();
    let dialogue_only = state.dialogue_only();
    let relevant = |line: &str| !dialogue_only || !crate::monster_defs::is_combat_report(line);

    prompt.push_str("=== CURRENT STATE ===\n");
    prompt.push_str(&state.format_world_state());
    prompt.push_str("\nUse player names without titles in actions and favor updates.\n");
    if !dialogue_only && state.schedule_combat_active() {
        prompt.push_str("Your routine is paused during combat. Defend yourself; patrols and excavation resume when safe.\n");
    }
    if let Some(terrain) = terrain {
        prompt.push_str(terrain.trim_end());
        prompt.push('\n');
    }

    if let Some(memory) = memory {
        let memory = memory
            .lines()
            .filter(|line| relevant(line))
            .collect::<Vec<_>>()
            .join("\n");
        if !memory.is_empty() {
            prompt.push_str("\n=== YOUR MEMORIES (notes you wrote in past turns) ===\n");
            prompt.push_str(&memory);
            prompt.push('\n');
        }
    }

    // A customer is mid-trade: stay put and keep helping them. Movement is
    // suppressed server-side regardless, but telling the LLM keeps its
    // narration consistent.
    if state.trade_busy {
        prompt.push_str(
            "\nA customer is trading with you right now. Stay where you are and keep \
             helping them — do not walk away or wander off.\n",
        );
    }

    if state.self_fishing {
        prompt.push_str(
            "\nYou are fishing right now. Hooking and fighting the fish is automatic — \
             stay put and wait for the [Fishing] outcome; moving or attacking cancels \
             the session.\n",
        );
    }

    // Resident-trader Personal Trading section, rebuilt every turn from
    // the live bag and the favor ledger. It needs a regular in sight —
    // favor past the trade threshold, outside the decline cooldown — and
    // its buy-list half additionally sits out the post-purchase
    // satiation. Strangers hear small talk and cost no section tokens.
    let satiated = state
        .trade_satiated_until
        .is_some_and(|until| std::time::Instant::now() < until);
    if let Some(p) = state.self_player.as_ref() {
        let audience = state.trade_worthy_players();
        if let Some(section) = crate::shop_info::resident_trade_prompt_for(
            &p.name,
            &state.self_bag,
            !satiated,
            &audience,
        ) {
            prompt.push('\n');
            prompt.push_str(&section);
        }
    }

    if let (Some(info), Some(p)) = (state.pricing.as_ref(), state.self_player.as_ref()) {
        if !dialogue_only && crate::shop_info::merchant_shop(&p.name).is_some() {
            let at_meeting = active_schedule_idx.is_some_and(|i| {
                schedule[i].condition
                    == Some(onlinerpg_shared::schedule::ScheduleCondition::Meeting)
            });
            prompt.push('\n');
            prompt.push_str(&market_prompt(info, at_meeting));
        }
    }

    if let Some(ctx) = format_schedule_context(schedule, active_schedule_idx) {
        prompt.push_str(&ctx);
        prompt.push('\n');
    }

    if let Some(tale) = tale {
        prompt.push_str(&tale);
    }

    if !state.chat_history().is_empty() {
        prompt.push_str(
            "\n=== RECENT CONVERSATION (background only; do not answer old lines again) ===\n",
        );
        for line in state.chat_history() {
            if !relevant(line) {
                continue;
            }
            prompt.push_str(line);
            prompt.push('\n');
        }
    }

    if !state.pending_chat().is_empty() {
        prompt
            .push_str("\n=== NEW CONVERSATION (reply when a response fits naturally, even without your name; do not reply to every line) ===\n");
        for line in state.pending_chat() {
            prompt.push_str(line);
            prompt.push('\n');
        }
    }

    let has_server_events = events.iter().any(|e| format_event(state, e).is_some());
    if has_server_events || !agent_events.is_empty() {
        prompt.push_str("\n=== EVENTS ===\nRespond to [Urgent] triggers using the conversation above; other events inform your routine.\n");
        for event in events {
            if let Some(line) = format_event(state, event) {
                if state.classify_event(event) == crate::state::EventUrgency::Urgent {
                    prompt.push_str("[Urgent] ");
                }
                prompt.push_str(&line);
                prompt.push('\n');
            }
        }
        for line in agent_events {
            if dialogue_only && (line.starts_with("[CombatEnded]") || !relevant(line)) {
                continue;
            }
            prompt.push_str(line);
            prompt.push('\n');
        }
    }

    prompt.push_str("\nWhat do you do?");
    prompt
}

/// Format a server event as a human-readable line for LLM prompts.
/// Returns `None` for events that should not be forwarded to the LLM.
pub(crate) fn format_event(state: &SharedState, msg: &ServerMessage) -> Option<String> {
    if state.dialogue_ignores_event(msg) {
        return None;
    }
    match msg {
        ServerMessage::JoinSuccess { player, .. } => Some(format!(
            "[Join] You joined as {} at ({:.1}, {:.1}, {:.1})",
            player.name, player.position.x, player.position.y, player.position.z
        )),
        ServerMessage::GameState {
            players, monsters, ..
        } => {
            let dialogue_only = state.dialogue_only();
            let mut lines = vec![if dialogue_only {
                format!("[World] {} player(s)", players.len())
            } else {
                format!("[World] {} player(s), {} monster(s)", players.len(), monsters.len())
            }];
            for p in players.iter() {
                lines.push(format!(
                    "  Player: {} Lv.{} {}",
                    p.name,
                    p.level,
                    crate::state::hp_text(p.health, p.max_health)
                ));
            }
            if !dialogue_only {
                for m in monsters.values() {
                lines.push(format!(
                    "  Monster: {} [{}] HP {}/{}",
                    crate::monster_defs::monster_name(&m.monster_type),
                    m.id,
                    m.health,
                    m.max_health
                ));
                }
            }
            Some(lines.join("\n"))
        }
        ServerMessage::GameTimeSync { datetime, is_night } => Some(format!(
            "[Time] Y{} M{} D{} {:02}:{:02} ({})",
            datetime.year,
            datetime.month,
            datetime.day,
            datetime.hour,
            datetime.minute,
            if *is_night { "night" } else { "day" }
        )),
        ServerMessage::ChatMessage {
            player_id, message, ..
        } => {
            let is_npc = state
                .nearby_players
                .get(player_id)
                .is_some_and(|p| p.is_official_npc);
            let tag = if is_npc { "[NpcChat]" } else { "[Chat]" };
            Some(format!(
                "{tag} {}: {message}",
                player_name(state, player_id)
            ))
        }
        ServerMessage::WhisperMessage { from, message, .. } => {
            // Skip the echo of our own outgoing whisper.
            let self_name = state.self_player.as_ref().map(|p| p.name.as_str());
            if Some(from.as_str()) == self_name {
                return None;
            }
            Some(format!("[Whisper] {from}: {message}"))
        }
        ServerMessage::PartyChatMessage { from, message } => {
            // Skip the echo of our own outgoing party line.
            let self_name = state.self_player.as_ref().map(|p| p.name.as_str());
            if Some(from.as_str()) == self_name {
                return None;
            }
            Some(format!("[Party] {from}: {message}"))
        }
        ServerMessage::FamilyChatMessage { from, message } => {
            let self_name = state.self_player.as_ref().map(|p| p.name.as_str());
            if Some(from.as_str()) == self_name {
                return None;
            }
            Some(format!("[Family] {from}: {message}"))
        }
        ServerMessage::SystemMessage { message, .. } => Some(format!("[System] {message}")),
        ServerMessage::ServerAnnouncement { message } => Some(format!("[Announcement] {message}")),
        ServerMessage::ServerNotice { message } => {
            message.as_ref().map(|message| format!("[Notice] {message}"))
        }
        ServerMessage::InteractionRejected { reason } => {
            Some(format!("[InteractionRejected] {reason}"))
        }
        ServerMessage::DungeonChestOpened {
            player_id,
            item_def_ids,
            gold,
            ..
        } => {
            let who = if state.self_player_id.as_ref() == Some(player_id) {
                "You".to_string()
            } else {
                player_name(state, player_id)
            };
            let tail = if item_def_ids.is_empty() && *gold == 0 {
                "it was empty (it refills at nightfall).".to_string()
            } else {
                format!(
                    "{} burst out onto the ground nearby; the gold ({gold}) went to the opener.",
                    item_def_ids.join(", ")
                )
            };
            Some(format!("[Chest] {who} opened the treasure chest — {tail}"))
        }
        ServerMessage::DungeonPropBroken { depth, prop_id, .. } => Some(format!(
            "[Prop] Prop {prop_id} on floor {depth} was smashed — its cell is now walkable."
        )),
        ServerMessage::DungeonPropOpened { depth, prop_id, .. } => Some(format!(
            "[Prop] Chest prop {prop_id} on floor {depth} swung open."
        )),
        ServerMessage::BuybackUpdated {
            merchant_player_id,
            buyback,
        } => {
            let who = player_name(state, merchant_player_id);
            if buyback.is_empty() {
                return Some(format!(
                    "[Buyback] {who}'s buyback list is now empty (your last repurchase or \
                     sale cleared it)."
                ));
            }
            // Enchant is the only thing telling two entries of the same item
            // apart — the payout ignores it, so the prices match.
            let list: Vec<String> = buyback
                .iter()
                .take(6)
                .map(|e| {
                    let price = crate::shop_info::format_price(e.total_price());
                    let name = match e.enchant {
                        0 => e.item_def_id.clone(),
                        n => format!("{} +{n}", e.item_def_id),
                    };
                    match e.quantity {
                        1 => format!("{name} @{price}"),
                        n => format!("{name} x{n} (all together) @{price}"),
                    }
                })
                .collect();
            Some(format!(
                "[Buyback] {who} will sell back: {} — {{\"type\": \"buyback\", \"item\": ..., \
                 \"merchant\": \"{who}\"}}.",
                list.join(", ")
            ))
        }
        ServerMessage::PlayerMusicStarted {
            player_id, track, ..
        } => {
            if !player_within_event_range(state, player_id) {
                return None;
            }
            let who = if state.self_player_id.as_ref() == Some(player_id) {
                "You".to_string()
            } else {
                player_name(state, player_id)
            };
            // Everyone in earshot hears the same tune; it plays until the
            // performer moves or the track runs out.
            Some(format!("[PlayMusic] {who} started playing \"{track}\"."))
        }
        ServerMessage::TitleEarned { title } => Some(format!(
            "[TitleEarned] You earned the title \"{}\".",
            crate::title_defs::title_name(title)
        )),
        ServerMessage::PlayerJoined { player } => Some(format!("[PlayerJoined] {}", player.name)),
        ServerMessage::PlayerAppeared { player } => {
            Some(format!("[PlayerAppeared] {}", player.name))
        }
        ServerMessage::PlayerLeft { player_id } => {
            Some(format!("[PlayerLeft] {}", player_name(state, player_id)))
        }
        ServerMessage::PlayerDisappeared { player_id } => Some(format!(
            "[PlayerDisappeared] {}",
            player_name(state, player_id)
        )),
        ServerMessage::PlayerTeleported {
            player_id,
            position,
            floor_level,
            ..
        } => {
            if state.self_player_id.as_ref() != Some(player_id) {
                return None;
            }
            let floor = match *floor_level {
                0 => "the surface".to_string(),
                f if f < 0 => format!("dungeon floor {}", f.unsigned_abs()),
                f => format!("the {}", storey_name(f as u8)),
            };
            Some(format!(
                "[Teleported] You were teleported to ({:.0}, {:.0}) on {floor}. Your old \
                 walk targets no longer apply.",
                position.x, position.z
            ))
        }
        ServerMessage::PlayerMoved {
            player_id,
            position,
            ..
        } => Some(format!(
            "[Move] {} -> ({:.1}, {:.1}, {:.1})",
            player_name(state, player_id),
            position.x,
            position.y,
            position.z
        )),
        ServerMessage::MonsterSpawned { monster } => Some(format!(
            "[MonsterSpawned] {} ({})",
            monster.id,
            crate::monster_defs::monster_name(&monster.monster_type)
        )),
        ServerMessage::MonsterDead { monster_id, .. } => {
            Some(format!("[MonsterDead] {monster_id}"))
        }
        ServerMessage::PlayerAttacked {
            player_id,
            monster_id,
            hit,
            damage,
            critical,
            dagger_strike,
            ..
        } => Some(format!(
            "[Attack] {} -> {monster_id}: hit={hit} dmg={damage} critical={critical}{}",
            player_name(state, player_id),
            dagger_strike.map_or_else(String::new, |strike| format!(
                " (Double Slash strike {strike})"
            ))
        )),
        ServerMessage::AbilityUsed {
            ability: onlinerpg_shared::ability::AbilityId::GuardianWard,
            player_id,
            targets,
            ..
        } => {
            let self_id = state.self_player_id?;
            if self_id != *player_id && !targets.contains(&self_id) {
                return None;
            }
            let names = targets
                .iter()
                .map(|id| player_name(state, id))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!(
                "[GuardianWard] {} cast Guardian Ward on {names}.",
                player_name(state, player_id),
            ))
        }
        ServerMessage::AbilityUsed {
            ability: onlinerpg_shared::ability::AbilityId::Rage,
            player_id,
            ..
        } if state.self_player_id == Some(*player_id) => Some(format!(
            "[Rage] You fly into a rage for {}s.",
            onlinerpg_shared::ability::RAGE_DURATION_MS / 1000
        )),
        ServerMessage::AbilityUsed {
            ability: onlinerpg_shared::ability::AbilityId::PrimalRoar,
            player_id,
            ..
        } if state.self_player_id == Some(*player_id) => Some(format!(
            "[PrimalRoar] You roar: nearby monsters are stunned and turn on you; {} temporary HP for {}s.",
            state.self_player.as_ref().map_or(0, |p| {
                onlinerpg_shared::ability::primal_roar_temp_health(p.max_health)
            }),
            onlinerpg_shared::ability::PRIMAL_ROAR_DURATION_MS / 1000
        )),
        ServerMessage::BattleHymnStarted { player_id, .. }
            if state.self_player_id == Some(*player_id) =>
        {
            Some(format!(
                "[BattleHymn] You start the hymn; keep still for {}s.",
                onlinerpg_shared::ability::BATTLE_HYMN_CAST_MS / 1000
            ))
        }
        ServerMessage::AbilityUsed {
            ability: onlinerpg_shared::ability::AbilityId::BattleHymn,
            player_id,
            targets,
            ..
        } => {
            let self_id = state.self_player_id?;
            let minutes = onlinerpg_shared::ability::BATTLE_HYMN_DURATION_MS / 60_000;
            if *player_id == self_id {
                let others = targets.iter().filter(|id| **id != self_id).count();
                Some(format!(
                    "[BattleHymn] Your hymn is done: you and {others} party member(s) move and attack faster for {minutes} minutes."
                ))
            } else if targets.contains(&self_id) {
                Some(format!(
                    "[BattleHymn] {}'s hymn: you move and attack faster for {minutes} minutes.",
                    player_name(state, player_id)
                ))
            } else {
                None
            }
        }
        ServerMessage::PlayerCharged {
            player_id,
            monster_id,
            position,
            ..
        } if state.self_player_id == Some(*player_id) => Some(format!(
            "[ValhallaCharge] You charge {monster_id} and land at ({:.1}, {:.1}); your swing follows on arrival.",
            position.x, position.z
        )),
        ServerMessage::HealingPrayed {
            player_id,
            target_player_id,
            monster_id,
            amount,
        } => {
            let self_id = state.self_player_id?;
            match (monster_id, target_player_id) {
                (Some(monster_id), _) if *player_id == self_id => Some(format!(
                    "[HealingPrayer] Your prayer sears {monster_id} for {amount} damage."
                )),
                (None, Some(target)) if *player_id == self_id && *target == self_id => {
                    Some(format!("[HealingPrayer] You heal yourself for {amount} HP."))
                }
                (None, Some(target)) if *player_id == self_id => Some(format!(
                    "[HealingPrayer] You heal {} for {amount} HP.",
                    player_name(state, target)
                )),
                (None, Some(target)) if *target == self_id => Some(format!(
                    "[HealingPrayer] {} heals you for {amount} HP.",
                    player_name(state, player_id)
                )),
                _ => None,
            }
        }
        ServerMessage::AbilityRejected { ability, reason }
            if crate::state::buff_ability(*ability).is_some() =>
        {
            use onlinerpg_shared::ability::{AbilityId, AbilityRejectReason};
            let info = crate::state::buff_ability(*ability)?;
            let reason = match reason {
                AbilityRejectReason::Unavailable => {
                    format!("unavailable in your current state; requires {}", info.requires)
                }
                AbilityRejectReason::Equipment => format!("equip {}", info.equipment),
                AbilityRejectReason::Cooldown => "still on cooldown; check readiness".into(),
                AbilityRejectReason::NotEnoughMana => "not enough MP".into(),
                AbilityRejectReason::OutOfRange => "target is out of range".into(),
                AbilityRejectReason::TooClose => "target is too close".into(),
                AbilityRejectReason::Blocked if *ability == AbilityId::HealingPrayer => {
                    "a wall blocks your sight of the target".into()
                }
                AbilityRejectReason::Blocked => "the straight path is blocked".into(),
                AbilityRejectReason::FullHealth => {
                    "the target is already at full HP; nothing was spent".into()
                }
                AbilityRejectReason::NotUndead => {
                    "only undead monsters can be harmed by it; nothing was spent".into()
                }
                AbilityRejectReason::Interrupted => {
                    "you moved, attacked, stopped playing or were hit before it finished; nothing was spent, start again when ready".into()
                }
            };
            Some(format!("[{}Failed] {reason}.", info.tag))
        }
        ServerMessage::DaggerDoubleSlashStarted {
            player_id,
            monster_id,
            cooldown_ms,
        } => Some(format!(
            "[DoubleSlashStarted] {} -> {monster_id}; cooldown {:.1}s. Strike results follow.",
            player_name(state, player_id),
            *cooldown_ms as f64 / 1000.0
        )),
        ServerMessage::DaggerDoubleSlashRejected {
            monster_id,
            reason,
            cooldown_ms,
        } => Some(format!(
            "[DoubleSlashFailed] {monster_id}: {reason}; remaining cooldown {:.1}s.",
            *cooldown_ms as f64 / 1000.0
        )),
        ServerMessage::DaggerDoubleSlashSkipped {
            player_id,
            monster_id,
            strike,
            reason,
        } => Some(format!(
            "[DoubleSlashSkipped] {} -> {monster_id}, strike {strike}: {reason}.",
            player_name(state, player_id)
        )),
        ServerMessage::PlayerAttackRejected { monster_id, reason } => {
            Some(format!("[AttackRejected] {monster_id}: {reason}"))
        }
        ServerMessage::MonsterSkillStarted {
            monster_id,
            skill,
            range,
            spots,
            spot_radius,
            ..
        } => {
            use onlinerpg_shared::monster_ai::{skill_by_id, SkillArea, SkillDamage};
            let def = skill_by_id(*skill);
            let strike_s = def.strike_ms / 1000.0;
            let area = match def.area {
                SkillArea::Fan { .. } => format!(
                    "the {range:.0}m fan in front of it, so step behind it or out of range"
                ),
                SkillArea::Zone { .. } => {
                    format!("everyone within {range:.1}m of its body, walls or not")
                }
                SkillArea::Spots => "everyone on a marked circle, so step off them".into(),
            };
            let marks = if spots.is_empty() {
                String::new()
            } else {
                let at: Vec<_> = spots
                    .iter()
                    .map(|p| format!("({:.0}, {:.0})", p.x, p.z))
                    .collect();
                let after = match def.pools {
                    Some(pools) => format!(
                        "; they turn to magma for {:.0}s, burning {} each second",
                        pools.tick_ms * pools.ticks as f32 / 1000.0,
                        pools.damage
                    ),
                    None => String::new(),
                };
                format!(
                    " Marked {spot_radius:.1}m circles at x,z {}{after}.",
                    at.join(" ")
                )
            };
            let damage = match def.damage {
                SkillDamage::Fixed(damage) => format!(
                    " It deals {damage} whatever your armor; drink a potion now if your HP is {damage} or less."
                ),
                SkillDamage::Scaled { .. } => String::new(),
            };
            Some(format!(
                "[MonsterSkill] {monster_id} winds up {skill:?}: in {strike_s:.1}s it strikes {area}.{marks}{damage}"
            ))
        }
        ServerMessage::MonsterSkillStruck {
            monster_id,
            skill,
            victims,
            burn,
        } => Some(format!(
            "[MonsterSkillStruck] {monster_id} {skill:?}{} hit {}",
            if *burn { " magma" } else { "" },
            victims
                .iter()
                .map(|v| format!(
                    "{} dmg={} {}",
                    player_name(state, &v.player_id),
                    v.damage,
                    victim_hp(state, &v.player_id, v.current_health)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        ServerMessage::MonsterGroundStrike {
            monster_id,
            target_player_id,
            position,
            radius,
            impact_ms,
            ..
        } => Some(format!(
            "[MonsterGroundStrike] {monster_id} slams at {}: the ground at ({:.1}, {:.1}) erupts in {:.1}s, hitting anyone within {radius:.1}m. Step out of it.",
            player_name(state, target_player_id),
            position.x,
            position.z,
            *impact_ms as f32 / 1000.0
        )),
        ServerMessage::MonsterAttackedPlayer {
            monster_id,
            player_id,
            hit,
            damage,
            current_health,
            ..
        } => Some(format!(
            "[MonsterAttack] {monster_id} -> {}: hit={hit} dmg={damage} {}",
            player_name(state, player_id),
            victim_hp(state, player_id, *current_health)
        )),
        ServerMessage::PlayerDead { player_id } => {
            Some(format!("[PlayerDead] {}", player_name(state, player_id)))
        }
        ServerMessage::PlayerRespawned { player } => {
            let is_self = state.self_player_id.as_ref() == Some(&player.id);
            if !is_self {
                return Some(format!(
                    "[Respawn] {} {}",
                    player.name,
                    crate::state::hp_text(player.health, player.max_health)
                ));
            }
            let (x, z) = (player.position.x, player.position.z);
            let place = match state.storey_at(x, z, player.floor_level) {
                Some(storey) if player.floor_level > 0 => format!(
                    "in a sick-room bed on the {storey} of a building at ({x:.0}, {z:.0}) — \
                     {FLOOR_ZERO_HINT}"
                ),
                Some(storey) => {
                    format!("in a bed on the {storey} of a building at ({x:.0}, {z:.0})")
                }
                None => format!("at ({x:.0}, {z:.0}) on the surface"),
            };
            Some(format!(
                "[Respawn] You died and woke {place}. HP {}/{}.",
                player.health, player.max_health
            ))
        }
        ServerMessage::XpGained {
            xp_amount,
            total_xp,
            new_level,
            leveled_up,
            ..
        } => {
            let mut s = format!("[XP] +{xp_amount} (total: {total_xp}, level: {new_level})");
            if *leveled_up {
                s.push_str(" LEVEL UP!");
            }
            Some(s)
        }
        ServerMessage::CharacterError { message } => Some(format!("[CharacterError] {message}")),
        ServerMessage::CharacterCreated { character } => Some(format!(
            "[CharacterCreated] id={} {} Lv.{} {:?}",
            character.id, character.name, character.level, character.class
        )),
        ServerMessage::CharacterStatsRolled { attributes, max_hp } => Some(format!(
            "[StatsRolled] STR:{} DEX:{} CON:{} INT:{} WIS:{} CHA:{} HP:{}",
            attributes.r#str,
            attributes.dex,
            attributes.con,
            attributes.int,
            attributes.wis,
            attributes.cha,
            max_hp
        )),
        ServerMessage::MonsterMoved {
            monster_id,
            position,
            state: monster_state,
            ..
        } => Some(format!(
            "[MonsterMoved] {monster_id} -> ({:.1}, {:.1}, {:.1}) state={monster_state}",
            position.x, position.y, position.z
        )),
        ServerMessage::Kicked { reason, .. } => Some(format!("[Kicked] {reason}")),
        ServerMessage::DealResult {
            target_player_name,
            item_def_id,
            kind,
            accepted,
            applied_modifier_pct,
            message,
            ..
        } => Some(format!(
            "[DealResult] your offer on {item_def_id} ({kind:?}) for {target_player_name}: {} \
             at {applied_modifier_pct}% — {message}",
            if *accepted { "GRANTED" } else { "REJECTED" }
        )),
        ServerMessage::FurnitureSelectionNotice {
            player_id,
            player_name,
            item_def_id,
        } => {
            if !player_within_event_range(state, player_id) {
                return None;
            }
            use onlinerpg_shared::furniture_shop::FurnitureTip;
            let advice = match FurnitureTip::for_item(item_def_id)? {
                FurnitureTip::StorageChest =>
                    "The Estate Chest must be placed on the customer's own estate \
                     before it can be used; it holds up to 50 kg.",
                FurnitureTip::Bed =>
                    "Sleeping in a bed for a full 16 seconds doubles natural HP and mana recovery. \
                     Normal recovery restrictions still apply; the bed must be placed on the customer's own estate.",
            };
            Some(format!(
                "[FurnitureSelection] {player_name} added {item_def_id} from a display to their \
                 unpaid basket. Briefly explain this tip in one friendly sentence, addressing \
                 that customer in their language: {advice} This is not a completed purchase. \
                 Do not open a trade window or repeat the checkout instructions."
            ))
        }
        ServerMessage::TradeNotice {
            player_name,
            item_def_id,
            kind,
            price,
            npc_gold,
        } => Some(format!(
            "[Trade] {player_name} {} for {} — your gold is now {}",
            match kind {
                onlinerpg_shared::messages::DealKind::Buy =>
                    format!("bought {item_def_id} from you"),
                onlinerpg_shared::messages::DealKind::Sell => format!("sold you {item_def_id}"),
            },
            crate::shop_info::format_shop_price(
                state.self_player.as_ref().map_or("", |player| player.name.as_str()),
                *price,
            ),
            crate::shop_info::format_price(*npc_gold),
        )),
        ServerMessage::TradeError { message, .. } => Some(format!("[TradeError] {message}")),
        ServerMessage::TradeDeclined { player_name, .. } => Some(format!(
            "[TradeDeclined] {player_name} waved off your trade window — let \
             trading rest with them for a good while and just talk. Trade \
             pushes at them are blocked meanwhile."
        )),
        ServerMessage::PartyInviteReceived { inviter_name, .. } => Some(format!(
            "[PartyInvite] {inviter_name} invited you to their party. Accept with \
             {{\"type\":\"party_accept\"}} or decline with party_decline."
        )),
        ServerMessage::PartyInviteResult { message, .. } => Some(format!("[Party] {message}")),
        ServerMessage::FamilyInviteReceived {
            inviter_name,
            family_name,
            ..
        } => Some(format!(
            "[FamilyInvite] {inviter_name} invited you to join the {family_name} family. \
             Accept with {{\"type\":\"family_accept\"}} or decline with family_decline."
        )),
        ServerMessage::FamilyMergeProposal { proposal } => Some(format!(
            "[FamilyMerge] {}, head of {}, offers to merge with your {}: {}. A merge cannot \
             be undone. Accept with {{\"type\":\"family_merge_accept\"}} or decline with \
             family_merge_decline.",
            proposal.proposer_name,
            proposal.proposer_family,
            proposal.target_family,
            crate::state::merge_outcome(proposal)
        )),
        ServerMessage::PartySummonReceived { caster_name, .. } => Some(format!(
            "[PartySummon] {caster_name} calls you to their side (summoning scroll). Accept \
             with {{\"type\":\"summon_accept\"}} or decline with summon_decline."
        )),
        ServerMessage::PartyState { members, .. } => Some(if members.is_empty() {
            "[Party] You are no longer in a party.".to_string()
        } else {
            format!(
                "[Party] Members: {}",
                members
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        // Fishing: only the endings reach the LLM (reflexes answered the
        // rest). Word the outcome so the model knows what it can do next.
        ServerMessage::FishingEnded { player_id, outcome } => {
            if state.self_player_id.as_ref() != Some(player_id) {
                return None;
            }
            use onlinerpg_shared::fishing::FishingOutcome;
            Some(match outcome {
                FishingOutcome::Caught {
                    item_def_id,
                    size_cm,
                    trophy,
                } => caught_line(item_def_id, *size_cm, *trophy),
                FishingOutcome::Escaped => {
                    "[Fishing] The fish got away. You can cast again with the fish action."
                        .to_string()
                }
                FishingOutcome::Aborted => "[Fishing] You reeled in your line.".to_string(),
            })
        }
        ServerMessage::FishingError { message } => Some(format!("[FishingError] {message}")),
        // Hunger transitions arrive direct (owner-only), a few per game day.
        ServerMessage::HungerUpdate {
            satiation, state, ..
        } => {
            use onlinerpg_shared::hunger::{can_sprint, HungerState};
            let line = match state {
                HungerState::Normal if can_sprint(*satiation) => format!(
                    "[Hunger] You are adequately fed ({satiation}/1000) and can sprint."
                ),
                HungerState::Normal => format!(
                    "[Hunger] You are adequately fed ({satiation}/1000) but too low to sprint — your moves walk until you eat."
                ),
                HungerState::Hungry => format!(
                    "[Hunger] You are getting hungry ({satiation}/1000): sprinting is unavailable and natural healing is slower."
                ),
                HungerState::Weak => format!(
                    "[Hunger] You are weak from hunger ({satiation}/1000): slower movement and attacks, less carry weight, and no natural healing. Eat something."
                ),
            };
            Some(line)
        }
        ServerMessage::DebuffUpdate { debuffs } => Some(if debuffs.is_empty() {
            "[Debuff] You are free of ailments.".to_string()
        } else {
            let list: Vec<String> = debuffs
                .iter()
                .map(|d| match d.id.as_str() {
                    "food_poisoning" => format!(
                        "food poisoning for {} more minutes (heavy penalties — cooked food next time)",
                        d.remaining_ms.div_ceil(60_000)
                    ),
                    "bleed" => format!(
                        "bleeding for {} more seconds (losing HP each second, no natural healing)",
                        d.remaining_ms.div_ceil(1_000)
                    ),
                    other => format!("{other} for {} more seconds", d.remaining_ms.div_ceil(1_000)),
                })
                .collect();
            format!("[Debuff] Afflicted: {}.", list.join("; "))
        }),
        ServerMessage::GrillEnded {
            grilled_item_def_id,
        } => Some(match grilled_item_def_id {
            Some(id) => format!("[Grill] Your fish is done: {id} is in your bag."),
            None => "[Grill] Your grilling was interrupted.".to_string(),
        }),
        // Skip unknown/unhandled event types
        _ => None,
    }
}

/// The `[Fishing]` line for a landed catch, with category-aware next steps.
/// Keep the phrasing in sync with the browser client's `catchMessage`
/// (client/src/lib/network/fishingMessages.ts).
fn caught_line(item_def_id: &str, size_cm: u16, trophy: bool) -> String {
    match crate::item_defs::get(item_def_id).and_then(|d| d.category.as_deref()) {
        Some("coin_catch") => format!(
            "[Fishing] You hauled up a {item_def_id}. It is in your bag — use it to open it and collect the coins, or fish again."
        ),
        Some("fish") => format!(
            "[Fishing] You caught a {item_def_id} ({size_cm} cm){}. It is in your bag — you can eat it, sell it, or fish again.",
            if trophy { " — a TROPHY catch!" } else { "" }
        ),
        _ => format!(
            "[Fishing] You fished up a {item_def_id}. It is in your bag — junk like this can be sold if a merchant pays for it, or dropped."
        ),
    }
}

/// Resolve a player_id to a display name using SharedState.
/// Falls back to the raw ID if the player is not found.
pub(super) fn player_name(state: &SharedState, player_id: &PlayerId) -> String {
    state.player_display_name(player_id)
}

fn victim_hp(state: &SharedState, player_id: &PlayerId, health: u32) -> String {
    if state.knows_hp_of(player_id) {
        format!("hp={health}")
    } else {
        crate::state::hp_text(health, 0)
    }
}

/// Format current schedule context for inclusion in LLM prompts.
/// Per-turn merchant market section (doc/PRICING.md 연출).
fn market_prompt(info: &PricingNotice, at_meeting: bool) -> String {
    let mut s = format!(
        "## Market\nConsumable prices (potions, food, scrolls, oil) are at {}% of base right now",
        info.index_percent
    );
    match info.last_change_pct {
        0 => s.push_str(".\n"),
        d => s.push_str(&format!(
            "; the merchants' guild {} at the last meeting.\n",
            price_change_phrase(d, true)
        )),
    }
    let hint = match info.trend {
        Trend::Rising => "Gold has been piling up in adventurers' purses, so you expect the guild to raise prices next time",
        Trend::Falling => "Gold has been scarce lately, so you expect the guild to lower prices next time",
        Trend::Steady => "Gold has been flowing about as expected, so you expect prices to hold next time",
    };
    if at_meeting {
        s.push_str(&format!(
            "{hint} — and the meeting is happening right now. Stay focused on it: talk prices \
             and the market with the other merchants only. No selling, no shop talk to \
             passers-by, no stall, no wandering off until the guild head closes the meeting.\n"
        ));
    } else {
        s.push_str(&format!(
            "{hint}. The guild meets on the evening Serin goes dark, {}. \
             You may drop hints about it, but never promise a price — it is only your hunch.\n",
            match info.meeting_in_days {
                0 => "tonight".to_string(),
                1 => "tomorrow evening".to_string(),
                n => format!("in {n} days"),
            }
        ));
    }
    s
}

/// "raised them 4%" / "lower consumable prices by 4%" / "hold …".
fn price_change_phrase(pct: i32, past: bool) -> String {
    match (pct.signum(), past) {
        (0, true) => "held them".to_string(),
        (0, false) => "hold consumable prices where they are".to_string(),
        (1, true) => format!("raised them {pct}%"),
        (1, false) => format!("raise consumable prices by {pct}%"),
        (_, true) => format!("lowered them {}%", -pct),
        (_, false) => format!("lower consumable prices by {}%", -pct),
    }
}

pub(super) fn dining_arrival_event(label: &str, serves_self: bool) -> String {
    let order = if serves_self {
        "your own meal, once every guest has theirs. The kitchen is yours: serve yourself one \
         dish and one drink with your own name as the player"
    } else {
        "call your order to the inn maid in your own words — one dish and one drink from the \
         kitchen"
    };
    format!(
        "[Schedule] You have sat down for {label} — {order} ({}). The house feeds its \
         regulars: no coins, no haggling, nothing from your own bag. Stay seated; the plate \
         is eaten by itself once it lands.",
        crate::item_defs::menu_line()
    )
}

pub(super) fn meeting_arrival_event() -> String {
    "[Schedule] You have arrived at the merchants' price meeting. Greet the other \
     merchants and open the talk about where prices should go."
        .to_string()
}

/// The server already decided at sunset; the host reads it out, the rest
/// say goodbye.
pub(super) fn meeting_closing_event(host: bool, last_change_pct: i32) -> String {
    if !host {
        return "[Schedule] The price meeting is wrapping up. Thank the guild head, say your \
                goodbyes, and head back to your business."
            .to_string();
    }
    format!(
        "[Schedule] As head of the merchants' guild, close the meeting now: announce that the \
         guild has decided to {}, thank everyone, and send them home.",
        price_change_phrase(last_change_pct, false)
    )
}

fn format_schedule_context(
    schedule: &[ScheduleEntry],
    active_idx: Option<usize>,
) -> Option<String> {
    let entry = &schedule[active_idx?];
    let mut line = format!(
        "Schedule (automatic): {} at ({:.1}, {:.1}, {:.1}). Travel and scheduled \
         interactions are automatic; do not issue move actions to follow this schedule.",
        entry.display_label(),
        entry.pos[0],
        entry.pos[1],
        entry.pos[2]
    );
    if let Some(ref action) = entry.action {
        line.push_str(&format!(" — using {action} (DO NOT move, you are resting)"));
    }
    if entry.condition == Some(onlinerpg_shared::schedule::ScheduleCondition::Rain) {
        line.push_str(
            " Rain has paused your outdoor work. Rest quietly in the inn, chat or listen to \
             the rain. Keep your stall and instruments packed away until your schedule resumes.",
        );
    }
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::{build_prompt, caught_line, format_event};

    #[test]
    fn ysolde_dialogue_keeps_the_question_and_omits_incidental_combat_context() {
        let (mut state, _rx) = test_state();
        let mut me = test_player(0.0, 0.0);
        me.name = "Ysolde".into();
        me.is_official_npc = true;
        state.self_player_id = Some(me.id);
        state.self_player = Some(me.clone());
        let mut visitor = test_player(2.0, 0.0);
        visitor.id = PlayerId::from(me.id.get() + 1);
        visitor.name = "jake5".into();
        state.nearby_players.insert(visitor.id, visitor.clone());
        state.push_event(ServerMessage::ChatMessage {
            player_id: me.id,
            message: "The gnoll still prowls southeast of camp. Keep clear.".into(),
        });
        state.drain_events();
        state.finish_conversation();
        let mut gnoll = crate::state::tests::monster("m3");
        gnoll.monster_type = "gnoll".into();
        gnoll.aggressive = true;
        let combat = vec![
            ServerMessage::MonsterSpawned { monster: gnoll },
            ServerMessage::MonsterAttackedPlayer {
                monster_id: "m3".into(),
                player_id: me.id,
                hit: false,
                roll: 1,
                damage: 0,
                current_health: me.health,
                ranged: false,
            },
        ];
        for event in &combat {
            assert_eq!(
                state.push_event(event.clone()),
                crate::state::EventUrgency::Noise
            );
            assert!(format_event(&state, event).is_none());
        }
        assert!(state.schedule_combat_active());
        assert!(state.nearby_monsters.contains_key("m3"));
        let sightings = state.drain_agent_events();
        assert!(sightings.iter().all(|line| !line.starts_with("[Sighted]")));
        state.push_event(ServerMessage::ChatMessage {
            player_id: visitor.id,
            message: "여기서 뭐해?".into(),
        });
        let prompt = build_prompt(&state, &combat, &sightings, &[], None,
            Some("Warned jake5 about a gnoll.\njake5 prefers Korean.\nBarkan stole Terraclash's heart."), None, None);
        assert!(prompt.contains("여기서 뭐해?"));
        assert!(prompt.contains("jake5 prefers Korean"));
        assert!(prompt.contains("Barkan stole Terraclash's heart"));
        for excluded in [
            "Gnoll",
            "gnoll",
            "Monster:",
            "MonsterAttacked",
            "Defend yourself",
            "Warned",
        ] {
            assert!(!prompt.contains(excluded), "{excluded}: {prompt}");
        }
    }

    #[test]
    fn monster_events_use_display_names() {
        let (state, _rx) = test_state();
        let mut monster = crate::state::tests::monster("m21");
        monster.monster_type = "scp939".into();
        let spawned = ServerMessage::MonsterSpawned {
            monster: monster.clone(),
        };
        assert_eq!(
            format_event(&state, &spawned).unwrap(),
            "[MonsterSpawned] m21 (Stalker)"
        );
        let world = ServerMessage::GameState {
            players: Vec::new(),
            monsters: [(monster.id.clone(), monster)].into(),
            ground_items: Vec::new(),
            campfires: Vec::new(),
            stalls: Vec::new(),
            tip_hats: Vec::new(),
            meals: Vec::new(),
        };
        let prompt = build_prompt(&state, &[world, spawned], &[], &[], None, None, None, None);
        assert!(prompt.contains("Monster: Stalker [m21]"), "{prompt}");
        assert!(!prompt.contains("scp939"), "{prompt}");
    }

    #[test]
    fn the_dining_cue_names_the_meal_and_the_menu() {
        let cue = super::dining_arrival_event("eating dinner on the inn's ground floor", false);
        assert!(cue.contains("sat down for eating dinner"), "{cue}");
        assert!(
            cue.contains("chicken_curry") && cue.contains("beer"),
            "{cue}"
        );
        assert!(cue.contains("inn maid"), "{cue}");
        let own = super::dining_arrival_event("eating dinner on the inn's ground floor", true);
        assert!(own.contains("your own name"), "{own}");
    }

    #[test]
    fn the_host_closes_with_the_servers_decision() {
        assert!(super::meeting_closing_event(true, -4).contains("lower consumable prices by 4%"));
        assert!(super::meeting_closing_event(true, 0).contains("hold consumable prices"));
        assert!(super::meeting_closing_event(false, 3).contains("say your goodbyes"));
    }
    use crate::state::tests::{test_player, test_state};
    use crate::state::EVENT_DELIVERY_RADIUS;
    use onlinerpg_shared::{PlayerId, ServerMessage};

    #[test]
    fn furniture_selections_prompt_advice_without_claiming_a_purchase() {
        let (mut state, _rx) = test_state();
        let shopper = test_player(2.0, 0.0);
        let id = shopper.id;
        state.nearby_players.insert(id, shopper);
        for (item, expected) in [
            ("storage_chest", "own estate"),
            ("furniture_bed", "doubles natural HP and mana recovery"),
            (
                "furniture_rustic_bed",
                "doubles natural HP and mana recovery",
            ),
        ] {
            let event = ServerMessage::FurnitureSelectionNotice {
                player_id: id,
                player_name: "Shopper".into(),
                item_def_id: item.into(),
            };
            assert_eq!(
                state.push_event(event.clone()),
                crate::state::EventUrgency::Urgent
            );
            let text = format_event(&state, &event).unwrap();
            assert!(
                text.contains(expected) && text.contains("unpaid basket"),
                "{text}"
            );
            assert!(text.contains("not a completed purchase"), "{text}");
            state.nearby_players.remove(&id);
            assert!(format_event(&state, &event).is_none());
            state.nearby_players.insert(id, test_player(2.0, 0.0));
        }
    }

    /// Drained conversation returns as RECENT CONVERSATION in the next
    /// prompt — replayed as context, while transient events (a death, a
    /// move) stay out of it. Memory notes render under YOUR MEMORIES.
    #[test]
    fn drained_chat_returns_as_history() {
        let (mut state, _rx) = test_state();
        let me = test_player(0.0, 0.0);
        state.self_player_id = Some(me.id);
        state.self_player = Some(me);

        let mut jake = test_player(2.0, 0.0);
        jake.id = PlayerId::from(2);
        jake.name = "jake1".to_string();
        state.nearby_players.insert(jake.id, jake);

        let heard = vec![
            ServerMessage::ChatMessage {
                player_id: PlayerId::from(2),
                message: "first song please".to_string(),
            },
            ServerMessage::PlayerDead {
                player_id: PlayerId::from(2),
            },
        ];
        for event in heard {
            state.push_event(event);
        }
        state.drain_events();
        state.finish_conversation();

        let prompt = build_prompt(&state, &[], &[], &[], None, None, None, None);
        assert!(prompt.contains("RECENT CONVERSATION"), "{prompt}");
        assert!(
            prompt.contains("[Chat] jake1: first song please"),
            "{prompt}"
        );
        assert!(
            !prompt.contains("[PlayerDead]"),
            "combat noise is not conversation: {prompt}"
        );
        assert!(
            !prompt.contains("=== YOUR MEMORIES"),
            "no memories, no section: {prompt}"
        );

        let prompt = build_prompt(
            &state,
            &[],
            &[],
            &[],
            None,
            Some("jake1 tips well"),
            None,
            None,
        );
        assert!(prompt.contains("=== YOUR MEMORIES"), "{prompt}");
        assert!(prompt.contains("jake1 tips well"), "{prompt}");
    }

    /// The current deed rides in as its own section; with none handed
    /// over there is no section to sing from.
    #[test]
    fn a_tale_is_a_section_of_its_own() {
        let (state, _rx) = test_state();
        let deed = crate::tales::Deed::parse(
            "2026-09-02 | Alder | Alder slew the Ogre Warlord. Celebrate the victory.",
        )
        .unwrap();
        let section = crate::tales::prompt_section(&deed, crate::tales::Lang::Korean, true);
        let prompt = build_prompt(&state, &[], &[], &[], None, None, None, Some(section));
        assert!(prompt.contains("=== CURRENT TALE"), "{prompt}");
        assert!(prompt.contains("Automatic rotation: DUE"), "{prompt}");
        assert!(prompt.contains("Alder slew the Ogre Warlord"), "{prompt}");
        assert!(prompt.contains("Hero: Alder"), "{prompt}");
        assert!(prompt.contains("Language: Korean"), "{prompt}");
        let prompt = build_prompt(&state, &[], &[], &[], None, None, None, None);
        assert!(!prompt.contains("CURRENT TALE"), "{prompt}");
    }

    /// Waking in the inn's sick room is a change of place the LLM cannot see
    /// from a bare HP line: it must know it is upstairs and indoors before
    /// it aims at the hunting ground it died in.
    #[test]
    fn a_self_respawn_says_where_and_on_which_floor() {
        use crate::state::tests::{house, room};
        use onlinerpg_shared::housing::RoomType;
        use onlinerpg_shared::Position;

        let (mut state, _rx) = test_state();
        let mut me = test_player(0.0, 0.0);
        me.id = PlayerId::from(1);
        state.self_player_id = Some(me.id);
        state.self_player = Some(me.clone());
        let inn = house(
            "inn",
            Position {
                x: -1448.0,
                y: 0.0,
                z: 4753.0,
            },
            vec![room(0, 1, RoomType::Normal)],
        );
        state.world_cache.write().unwrap().add_house(inn);

        me.position = Position {
            x: -1446.7,
            y: 4.4,
            z: 4754.9,
        };
        me.floor_level = 1;
        let line = format_event(
            &state,
            &ServerMessage::PlayerRespawned { player: me.clone() },
        )
        .expect("own respawn is always reported");
        assert!(line.contains("sick-room bed on the 2nd floor"), "{line}");
        assert!(line.contains(crate::state::FLOOR_ZERO_HINT), "{line}");

        me.position = Position {
            x: 10.0,
            y: 0.0,
            z: 10.0,
        };
        me.floor_level = 0;
        let line = format_event(
            &state,
            &ServerMessage::PlayerRespawned { player: me.clone() },
        )
        .unwrap();
        assert!(line.contains("on the surface"), "{line}");

        let mut other = test_player(3.0, 0.0);
        other.id = PlayerId::from(2);
        other.name = "Brann".to_string();
        let line = format_event(&state, &ServerMessage::PlayerRespawned { player: other }).unwrap();
        assert!(line.starts_with("[Respawn] Brann HP"), "{line}");
    }

    /// A tune someone strikes up nearby is worth a prompt line — the title is
    /// what makes it something an NPC can talk about. Out of earshot it is
    /// not our business.
    #[test]
    fn music_from_an_active_performer_reaches_the_llm() {
        let (mut state, _rx) = test_state();
        let me = test_player(0.0, 0.0);
        state.self_player_id = Some(me.id);
        state.self_player = Some(me);

        let mut bard = test_player(5.0, 0.0);
        bard.id = PlayerId::from(2);
        bard.name = "Rica".to_string();
        state.nearby_players.insert(bard.id, bard);

        let music = |player_id| ServerMessage::PlayerMusicStarted {
            player_id,
            track: "Twilight Fields".to_string(),
            elapsed_secs: 0.0,
        };

        let line = format_event(&state, &music(PlayerId::from(2))).expect("bard is in earshot");
        assert!(line.contains("Rica"), "{line}");
        assert!(line.contains("Twilight Fields"), "{line}");

        state
            .nearby_players
            .get_mut(&PlayerId::from(2))
            .unwrap()
            .position
            .x = EVENT_DELIVERY_RADIUS + 10.0;
        assert!(format_event(&state, &music(PlayerId::from(2))).is_some());
        state.nearby_players.remove(&PlayerId::from(2));
        assert_eq!(format_event(&state, &music(PlayerId::from(2))), None);
    }

    // The wording contract with the LLM: each catch category tells the model
    // what it can actually do next (against the real embedded item defs).

    #[test]
    fn a_fish_is_edible_and_sellable() {
        let line = caught_line("raw_trout", 34, false);
        assert!(line.contains("caught a raw_trout (34 cm)"), "{line}");
        assert!(line.contains("eat it, sell it"), "{line}");
        assert!(!line.contains("TROPHY"), "{line}");
    }

    #[test]
    fn a_trophy_fish_celebrates() {
        let line = caught_line("golden_sturgeon", 120, true);
        assert!(line.contains("TROPHY"), "{line}");
        assert!(line.contains("eat it, sell it"), "{line}");
    }

    #[test]
    fn junk_is_not_presented_as_edible() {
        let line = caught_line("old_boot", 40, false);
        assert!(line.contains("fished up a old_boot"), "{line}");
        assert!(
            !line.contains("eat"),
            "junk must not be offered as food: {line}"
        );
    }

    #[test]
    fn a_coin_catch_points_at_the_bag_and_the_use_action() {
        let line = caught_line("sunken_coin_pouch", 12, false);
        assert!(
            line.contains("bag"),
            "the pouch lands in the bag sealed: {line}"
        );
        assert!(
            line.contains("use it to open it"),
            "the model must learn the `use` action opens it: {line}"
        );
    }
}
