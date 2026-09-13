import { getItemDef } from './itemDefs'
import { DAGGER_SKILL } from './daggerSkill'
import type { PlayerInventory } from '../network/networkTypes'

export const GUARDIAN_WARD = {
  id: 'guardian_ward',
  name: 'Guardian Ward',
  icon: '/icons/skills/guardian-ward.png',
  description: 'Protect yourself and nearby party members.',
  buffDescription: 'Guard +10%',
  stats: [
    { label: 'Guard', value: '+10%' },
    { label: 'Weapon', value: 'Sword or Mace' },
    { label: 'Off hand', value: 'Shield' },
    { label: 'Radius', value: '20 m' },
    { label: 'Duration', value: '60 s' },
    { label: 'Cooldown', value: '45 s' },
  ],
  details: ['Reapplying refreshes the duration. Does not stack.'],
} as const

export const RADIANCE = {
  id: 'radiance',
  name: 'Radiance',
  icon: '/icons/skills/radiance.png',
  description: 'Illuminate your surroundings with magical light.',
  buffDescription: 'Illuminates nearby surroundings.',
  stats: [
    { label: 'Weapon', value: 'Any' },
    { label: 'Light', value: 'Torch range' },
    { label: 'Duration', value: '120 s' },
    { label: 'Cooldown', value: '0.8 s' },
    { label: 'Type', value: 'Toggle' },
  ],
  details: ['Use again to turn off.'],
} as const

export const TRUE_AIM = {
  id: 'bow_mark',
  target: 'monster',
  name: 'True Aim',
  icon: '/icons/skills/true-aim.png',
  description: 'Your attacks against the marked target always hit.',
  buffDescription: 'Your attacks against the marked target always hit.',
  stats: [
    { label: 'Weapon', value: 'Bow' },
    { label: 'Range', value: '10 m' },
    { label: 'Duration', value: '5 s' },
    { label: 'Cooldown', value: '10 s' },
  ],
  details: [
    'Uses the enemy under your cursor, or your selected target. Applies only to your attacks.',
  ],
} as const

export const BUFF_ABILITIES = [GUARDIAN_WARD, RADIANCE, TRUE_AIM] as const
export type AbilityId = (typeof BUFF_ABILITIES)[number]['id']
export type AbilityTimer = {
  ability: AbilityId | typeof DAGGER_SKILL.clip
  remaining_ms: number
}

export const DOUBLE_SLASH = {
  id: DAGGER_SKILL.clip,
  name: 'Double Slash',
  icon: DAGGER_SKILL.icon,
  description: 'Strike twice in quick succession.',
  stats: [
    { label: 'Damage', value: '100% × 2' },
    { label: 'Weapon', value: 'Dagger' },
    { label: 'Cooldown', value: `${DAGGER_SKILL.cooldownMs / 1000} s` },
  ],
} as const

export function getAbility(id: string) {
  if (id === GUARDIAN_WARD.id) return GUARDIAN_WARD
  if (id === RADIANCE.id) return RADIANCE
  if (id === TRUE_AIM.id) return TRUE_AIM
  if (id === DOUBLE_SLASH.id) return DOUBLE_SLASH
  return undefined
}

export function abilityEquipmentAllowed(
  id: AbilityId,
  equipped: PlayerInventory['equipped']
) {
  if (id === TRUE_AIM.id)
    return (
      getItemDef(equipped.main_hand?.item_def_id ?? '')?.weaponType === 'bow'
    )
  return id === RADIANCE.id || guardianWardEquipment(equipped)
}

export function abilityRequirementsNotMet(name: string) {
  return `Cannot use ${name}.`
}

export function guardianWardEquipment(equipped: PlayerInventory['equipped']) {
  const main = getItemDef(equipped.main_hand?.item_def_id ?? '')
  const off = getItemDef(equipped.off_hand?.item_def_id ?? '')
  return (
    (main?.weaponType === 'sword' || main?.weaponType === 'mace') &&
    off?.armorType === 'shield'
  )
}
