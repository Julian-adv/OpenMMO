import { get } from 'svelte/store'
import { translate } from '../i18n'
import {
  AUSCULTATION,
  DOUBLE_SLASH,
  FISHING,
  abilityDisplayName,
  abilityEquipmentAllowed,
  abilityEquipmentNotMet,
  getAbility,
  HEALING_PRAYER,
  isAbilityAvailable,
  VALHALLA_CHARGE,
} from '../data/abilities'
import { getMonsterDef } from '../data/monsterDefs'
import {
  cancelChargeTargeting,
  chargeTargeting,
} from '../stores/chargeStrikeStore'
import { combatController } from '../managers/combatController'
import { monsterManager } from '../managers/monsterManager'
import { networkManager } from '../network/socket'
import { abilityCooldowns, beginAbility } from '../stores/abilityStore'
import { daggerSkillState, queueDaggerSkill } from '../stores/daggerSkillStore'
import {
  gameStore,
  hoveredMonsterId,
  hoveredPlayerId,
  reportSkillFailure,
} from '../stores/gameStore'
import { hoveredPartyMemberId, partyRoster } from '../stores/partyStore'
import { cancelInspection, queueInspection } from '../stores/inspectionStore'
import { inventoryStore } from '../stores/inventoryStore'
import { manaState } from '../stores/manaStore'
import { skillsStore } from '../stores/skillsStore'
import { currentDungeonDepth } from '../stores/dungeonStore'
import { playerVisualFloorLevel } from '../stores/housingStore'
import {
  cancelFishingTargeting,
  myFishing,
  queueFishingTarget,
} from '../stores/fishingStore'
import { isMounted } from './mounts'

export function useAbility(id: string) {
  const ability = getAbility(id)
  const player = get(gameStore).currentPlayer
  if (
    !ability ||
    !player ||
    !isAbilityAvailable(id, player.characterClass, get(skillsStore).learned)
  )
    return
  if (id !== AUSCULTATION.id) cancelInspection()
  if (id !== FISHING.id) cancelFishingTargeting()
  if (id !== VALHALLA_CHARGE.id) cancelChargeTargeting()
  if (player.health <= 0) {
    reportSkillFailure(translate('skillFailure.dead'))
    return
  }
  if (player.petrified) {
    reportSkillFailure(translate('skillFailure.petrified'))
    return
  }
  if (isMounted(player) && !(id === FISHING.id && player.mount === 'rowboat')) {
    reportSkillFailure(translate('skillFailure.mounted'))
    return
  }
  const { equipped, bag } = get(inventoryStore)
  if (!abilityEquipmentAllowed(ability.id, equipped, bag)) {
    reportSkillFailure(abilityEquipmentNotMet(id))
    return
  }
  if (ability.id === FISHING.id) {
    if (get(currentDungeonDepth) > 0 || get(playerVisualFloorLevel) !== 0) {
      reportSkillFailure(translate('fishing.outdoors'))
      return
    }
    if (get(myFishing).phase !== 'idle') {
      reportSkillFailure(translate('fishing.already'))
      return
    }
    queueFishingTarget()
    return
  }
  if (ability.manaCost > (get(manaState)?.mana ?? 0)) {
    reportSkillFailure(translate('skillFailure.mana'))
    return
  }
  const cooldownUntil =
    ability.id === DOUBLE_SLASH.id
      ? get(daggerSkillState).cooldownUntil
      : (get(abilityCooldowns)[ability.id] ?? 0)
  if (cooldownUntil > Date.now()) {
    reportSkillFailure(
      translate('skillFailure.cooldown', {
        name: abilityDisplayName(ability.id),
      })
    )
    return
  }
  if (ability.id === AUSCULTATION.id) {
    queueInspection(equipped)
    return
  }
  if (ability.id === DOUBLE_SLASH.id) {
    queueDaggerSkill()
    return
  }
  if (ability.id === HEALING_PRAYER.id) {
    const target = healingPrayerTarget()
    if (target && beginAbility(ability.id))
      networkManager.sendUseAbility(ability.id, target.monster, target.player)
    return
  }
  const needsTarget = 'target' in ability && ability.target === 'monster'
  const target = needsTarget
    ? combatController.getAbilityTarget(get(hoveredMonsterId), (monsterId) =>
        monsterManager.monsters.get(monsterId)
      )
    : null
  if (ability.id === VALHALLA_CHARGE.id && !target) {
    chargeTargeting.update((on) => !on)
    return
  }
  cancelChargeTargeting()
  if (needsTarget && !target) {
    reportSkillFailure(
      translate('skillFailure.target', { name: abilityDisplayName(ability.id) })
    )
    return
  }
  if (beginAbility(ability.id))
    networkManager.sendUseAbility(ability.id, target)
}

/** A hovered party row or player, then a hovered undead, else yourself. */
function healingPrayerTarget(): {
  monster: string | null
  player: number | null
} | null {
  const state = get(gameStore)
  const self = state.currentPlayer
  const memberId = get(hoveredPartyMemberId)
  const member = get(partyRoster)?.members.find((m) => m.id === memberId)
  const playerId = get(hoveredPlayerId)
  const player =
    playerId === null ? undefined : state.otherPlayers.get(playerId)
  const known =
    member && member.id !== self?.id
      ? { id: member.id, health: member.hp, maxHealth: member.max_hp }
      : player && player.health > 0
        ? { id: player.id, health: player.health, maxHealth: player.maxHealth }
        : null
  if (known) {
    if (known.maxHealth > 0 && known.health >= known.maxHealth) {
      reportSkillFailure(translate('skillFailure.fullHealth'))
      return null
    }
    return { monster: null, player: known.id }
  }
  const monsterId = get(hoveredMonsterId)
  const monster = monsterId ? monsterManager.monsters.get(monsterId) : undefined
  if (
    monsterId &&
    monster &&
    monster.state !== 'dead' &&
    !monster.isDeadPending &&
    getMonsterDef(monster.type)?.undead
  )
    return { monster: monsterId, player: null }
  if (self && self.health >= self.maxHealth) {
    reportSkillFailure(translate('skillFailure.fullHealth'))
    return null
  }
  return { monster: null, player: null }
}
