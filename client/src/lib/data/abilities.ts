import {
  ability_mana_cost,
  max_cast_distance_m,
} from '../wasm/onlinerpg_shared'
import { translate } from '../i18n'
import { getItemDef, isTwoHanded } from './itemDefs'
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
  buffDescription: 'Guard up',
  get stats() {
    return [
      { label: 'Guard', value: '+2, +1 per 20 levels' },
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
  // Borrows Healing Prayer's icon until its own is redrawn.
  icon: '/icons/skills/healing-prayer.png',
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

const RAGE_WEAPON_TYPES = [
  'sword',
  'great_sword',
  'axe',
  'mace',
  'club',
  'spear',
  'dagger',
] as const

export const RAGE = {
  get manaCost() {
    return ability_mana_cost('rage')
  },
  id: 'rage',
  name: 'Rage',
  icon: '/icons/skills/rage.png',
  description: 'Melee attacks deal more damage, but your guard drops.',
  buffDescription: 'Melee damage up, Guard -2',
} as const

export const PRIMAL_ROAR = {
  get manaCost() {
    return ability_mana_cost('primal_roar')
  },
  id: 'primal_roar',
  name: 'Primal Roar',
  icon: '/icons/skills/primal-roar.png',
  description: 'Roar to stun nearby monsters and gain temporary HP.',
  buffDescription: 'Temporary HP takes damage first',
} as const

export const VALHALLA_CHARGE = {
  get manaCost() {
    return ability_mana_cost('valhalla_charge')
  },
  id: 'valhalla_charge',
  target: 'monster',
  name: 'Valhalla Charge',
  icon: '/icons/skills/valhalla-charge.png',
  description: 'Charge a monster, strike it once, and slow it down.',
} as const

export const HEALING_PRAYER = {
  get manaCost() {
    return ability_mana_cost('healing_prayer')
  },
  id: 'healing_prayer',
  name: 'Healing Prayer',
  icon: '/icons/skills/healing-prayer.png',
  description: 'Heal yourself or another player, or sear an undead monster.',
} as const

export const BATTLE_HYMN = {
  get manaCost() {
    return ability_mana_cost('battle_hymn')
  },
  id: 'battle_hymn',
  name: 'Battle Hymn',
  icon: '/icons/skills/battle-hymn.png',
  description: 'Play to speed up yourself and nearby party members.',
} as const

export const BUFF_ABILITIES = [
  GUARDIAN_WARD,
  RAGE,
  PRIMAL_ROAR,
  BATTLE_HYMN,
  RADIANCE,
  TRUE_AIM,
] as const
export type AbilityId =
  | (typeof BUFF_ABILITIES)[number]['id']
  | typeof DOUBLE_SLASH.id
  | typeof AUSCULTATION.id
  | typeof VALHALLA_CHARGE.id
  | typeof HEALING_PRAYER.id
export type AbilityTimer = {
  ability: AbilityId
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
  RAGE,
  PRIMAL_ROAR,
  VALHALLA_CHARGE,
  HEALING_PRAYER,
  BATTLE_HYMN,
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
    (id === GUARDIAN_WARD.id && characterClass === 'knight') ||
    (id === RAGE.id && characterClass === 'barbarian') ||
    (id === PRIMAL_ROAR.id && characterClass === 'caveman') ||
    (id === VALHALLA_CHARGE.id && characterClass === 'valkyrie') ||
    (id === HEALING_PRAYER.id && characterClass === 'priest') ||
    (id === BATTLE_HYMN.id && characterClass === 'bard')
  )
}

export function getAbility(id: string) {
  return ABILITIES.find((ability) => ability.id === id)
}

/** Battle Hymn alone looks in the bag: an instrument plays from anywhere. */
export function abilityEquipmentAllowed(
  id: (typeof ABILITIES)[number]['id'],
  equipped: PlayerInventory['equipped'],
  bag: PlayerInventory['bag'] = []
) {
  if (id === BATTLE_HYMN.id) {
    const instrument = (item?: { item_def_id: string }) =>
      getItemDef(item?.item_def_id ?? '')?.category === 'instrument'
    return bag.some(instrument) || Object.values(equipped).some(instrument)
  }
  if (id === FISHING.id)
    return (
      getItemDef(equipped.main_hand?.item_def_id ?? '')?.category ===
      'fishing_rod'
    )
  if (id === AUSCULTATION.id)
    return equipped.neck?.item_def_id === 'stethoscope'
  if (id === PRIMAL_ROAR.id || id === HEALING_PRAYER.id) return true
  const weaponType = getItemDef(
    equipped.main_hand?.item_def_id ?? ''
  )?.weaponType
  if (id === TRUE_AIM.id || id === DOUBLE_SLASH.id)
    return (
      weaponType === (id === DOUBLE_SLASH.id ? DAGGER_SKILL.weaponType : 'bow')
    )
  if (id === RAGE.id)
    return RAGE_WEAPON_TYPES.some((type) => type === weaponType)
  if (id === VALHALLA_CHARGE.id)
    return (
      (weaponType === 'sword' || weaponType === 'spear') &&
      !isTwoHanded(equipped.main_hand?.item_def_id)
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
  if (id === VALHALLA_CHARGE.id)
    return translate('skillFailure.valhallaChargeEquipment')
  if (id === BATTLE_HYMN.id)
    return translate('skillFailure.battleHymnEquipment')
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
