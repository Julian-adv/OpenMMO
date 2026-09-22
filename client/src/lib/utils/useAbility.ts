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
  isAbilityAvailable,
} from '../data/abilities'
import { combatController } from '../managers/combatController'
import { monsterManager } from '../managers/monsterManager'
import { networkManager } from '../network/socket'
import { abilityCooldowns, beginAbility } from '../stores/abilityStore'
import { daggerSkillState, queueDaggerSkill } from '../stores/daggerSkillStore'
import {
  gameStore,
  hoveredMonsterId,
  reportSkillFailure,
} from '../stores/gameStore'
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
  if (player.health <= 0) {
    reportSkillFailure(translate('skillFailure.dead'))
    return
  }
  if (isMounted(player) && !(id === FISHING.id && player.mount === 'rowboat')) {
    reportSkillFailure(translate('skillFailure.mounted'))
    return
  }
  const { equipped } = get(inventoryStore)
  if (!abilityEquipmentAllowed(ability.id, equipped)) {
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
  const needsTarget = 'target' in ability && ability.target === 'monster'
  const target = needsTarget
    ? combatController.getAbilityTarget(get(hoveredMonsterId), (monsterId) =>
        monsterManager.monsters.get(monsterId)
      )
    : null
  if (needsTarget && !target) {
    reportSkillFailure(
      translate('skillFailure.target', { name: abilityDisplayName(ability.id) })
    )
    return
  }
  if (beginAbility(ability.id))
    networkManager.sendUseAbility(ability.id, target)
}
