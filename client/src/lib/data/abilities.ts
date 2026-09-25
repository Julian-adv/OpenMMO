import {
  ability_mana_cost,
  max_cast_distance_m,
} from '../wasm/onlinerpg_shared'
import { translate } from '../i18n'
import { getItemDef } from './itemDefs'
import { DAGGER_SKILL } from './daggerSkill'
import type {
  CharacterClass,
  PlayerInventory,
  SkillId,
} from '../network/networkTypes'

export const GUARDIAN_WARD = {
  get manaCost() {
    return ability_mana_cost('guardian_ward')
  },
  id: 'guardian_ward',
  name: 'Guardian Ward',
  icon: '/icons/skills/guardian-ward.png',
  description: 'Protect yourself and nearby party members.',
  buffDescription: 'Guard +10%',
  get stats() {
    return [
      { label: 'Guard', value: '+10%' },
      { label: 'Class', value: 'Knight' },
      { label: 'Weapon', value: 'Sword or Mace' },
      { label: 'Off hand', value: 'Shield' },
      { label: 'Type', value: 'Magic' },
      { label: 'Cost', value: `${this.manaCost} MP` },
      { label: 'Radius', value: '20 m' },
      { label: 'Duration', value: '60 s' },
      { label: 'Cooldown', value: '45 s' },
    ]
  },
  details: ['Reapplying refreshes the duration. Does not stack.'],
} as const

export const RADIANCE = {
  get manaCost() {
    return ability_mana_cost('radiance')
  },
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
  get manaCost() {
    return ability_mana_cost('bow_mark')
  },
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
export type AbilityId =
  | (typeof BUFF_ABILITIES)[number]['id']
  | typeof AUSCULTATION.id
export type AbilityTimer = {
  ability: AbilityId | typeof DAGGER_SKILL.clip
  remaining_ms: number
}

export const DOUBLE_SLASH = {
  get manaCost() {
    return ability_mana_cost(DAGGER_SKILL.clip)
  },
  id: DAGGER_SKILL.clip,
  name: 'Double Slash',
  icon: DAGGER_SKILL.icon,
  description: 'Strike twice in quick succession.',
  stats: [
    { label: 'Type', value: 'Combat Skill' },
    { label: 'Damage', value: '100% × 2' },
    { label: 'Class', value: 'Rogue' },
    { label: 'Weapon', value: 'Dagger' },
    { label: 'Cooldown', value: `${DAGGER_SKILL.cooldownMs / 1000} s` },
  ],
} as const

export const AUSCULTATION = {
  id: 'auscultation',
  name: 'Auscultation',
  icon: '/items/objects/stethoscope.png',
  get manaCost() {
    return ability_mana_cost('auscultation')
  },
  description: 'Examine a nearby player or monster with your stethoscope.',
  stats: [
    { label: 'Equipment', value: 'Stethoscope (Neck)' },
    { label: 'Range', value: '2 m' },
    { label: 'Cost', value: '0 MP' },
    { label: 'Cooldown', value: '0.8 s' },
  ],
  details: [
    'Activate, then left-click a target. Press Escape to cancel.',
    'Shows level, HP, guard and equipped items. No training required.',
  ],
} as const

export const FISHING = {
  id: 'fishing',
  name: 'Fishing',
  icon: '/items/weapons/fishing_rod.png',
  manaCost: 0,
  description: 'Cast your line into water to catch fish.',
  get stats() {
    return [
      { label: 'Equipment', value: 'Fishing Rod (Main hand)' },
      { label: 'Range', value: `${max_cast_distance_m()} m` },
      { label: 'Cost', value: '0 MP' },
    ]
  },
  details: [
    'Activate, then left-click water. Press Escape to cancel.',
    'You can also click water directly with a fishing rod equipped.',
    'Learn by watching Tobin complete a catch nearby.',
  ],
} as const

export const ABILITIES = [
  GUARDIAN_WARD,
  DOUBLE_SLASH,
  RADIANCE,
  TRUE_AIM,
  AUSCULTATION,
  FISHING,
] as const

export function isAbilityAvailable(
  id: string,
  characterClass: CharacterClass | undefined,
  learned: readonly SkillId[] = []
) {
  return (
    (id === FISHING.id && learned.includes(FISHING.id)) ||
    id === AUSCULTATION.id ||
    (id === DOUBLE_SLASH.id && characterClass === 'rogue') ||
    (id === GUARDIAN_WARD.id && characterClass === 'knight')
  )
}

export function getAbility(id: string) {
  return ABILITIES.find((ability) => ability.id === id)
}

export function abilityEquipmentAllowed(
  id: (typeof ABILITIES)[number]['id'],
  equipped: PlayerInventory['equipped']
) {
  if (id === FISHING.id)
    return (
      getItemDef(equipped.main_hand?.item_def_id ?? '')?.category ===
      'fishing_rod'
    )
  if (id === AUSCULTATION.id)
    return equipped.neck?.item_def_id === 'stethoscope'
  if (id === TRUE_AIM.id || id === DOUBLE_SLASH.id)
    return (
      getItemDef(equipped.main_hand?.item_def_id ?? '')?.weaponType ===
      (id === DOUBLE_SLASH.id ? DAGGER_SKILL.weaponType : 'bow')
    )
  return id === RADIANCE.id || guardianWardEquipment(equipped)
}

export function abilityDisplayName(id: string): string {
  const ability = getAbility(id)
  return ability ? translate(`ability.${ability.id}.name`) : id
}

export function abilityRequirementsNotMet(name: string) {
  return translate('skillFailure.requirements', { name })
}

export function abilityEquipmentNotMet(id: string) {
  if (id === FISHING.id) return translate('skillFailure.fishingEquipment')
  if (id === AUSCULTATION.id)
    return translate('skillFailure.auscultationEquipment')
  if (id === DOUBLE_SLASH.id) return translate('skillFailure.daggerEquipment')
  return abilityRequirementsNotMet(abilityDisplayName(id))
}

export function guardianWardEquipment(equipped: PlayerInventory['equipped']) {
  const main = getItemDef(equipped.main_hand?.item_def_id ?? '')
  const off = getItemDef(equipped.off_hand?.item_def_id ?? '')
  return (
    (main?.weaponType === 'sword' || main?.weaponType === 'mace') &&
    off?.armorType === 'shield'
  )
}
