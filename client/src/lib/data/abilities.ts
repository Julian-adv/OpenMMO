import { getItemDef } from './itemDefs'
import { DAGGER_SKILL } from './daggerSkill'
import type { PlayerInventory } from '../network/networkTypes'

export const GUARDIAN_WARD = {
  id: 'guardian_ward',
  name: 'Guardian Ward',
  icon: '/icons/skills/guardian-ward.png',
  description: 'Protect yourself and nearby party members.',
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

export type AbilityId = typeof GUARDIAN_WARD.id
export type AbilityTimer = { ability: AbilityId; remaining_ms: number }

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
  if (id === DOUBLE_SLASH.id) return DOUBLE_SLASH
  return undefined
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
