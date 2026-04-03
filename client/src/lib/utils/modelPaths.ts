import type { CharacterClass, Gender } from '../network/networkTypes'

export type WeaponType = 'sword' | 'spear'

export const KNIGHT_CHARACTER_MODEL_PATH = '/models/knight.glb'
export const FEMALE_KNIGHT_CHARACTER_MODEL_PATH = '/models/female_knight.glb'
export const ROGUE_CHARACTER_MODEL_PATH = '/models/female_thief.glb'
export const MERCHANT_CHARACTER_MODEL_PATH = '/models/npc_woman.glb'
export const FEMALE_BARBARIAN_CHARACTER_MODEL_PATH =
  '/models/female_barbarian.glb'
export const GUARD_CHARACTER_MODEL_PATH = '/models/guard.glb'

export const CHARACTER_ANIMATION_PACK_PATHS = {
  locomotion: '/models/animations/locomotion.glb',
  combatMelee: '/models/animations/combat_melee.glb',
  social: '/models/animations/social.glb',
} as const

export const WEAPON_MODEL_PATHS: Record<WeaponType, string> = {
  sword: '/models/sword.glb',
  spear: '/models/spear.glb',
} as const

const CLASS_GENDER_MODELS: Partial<
  Record<CharacterClass, Partial<Record<Gender, string>>>
> = {
  knight: {
    male: KNIGHT_CHARACTER_MODEL_PATH,
    female: FEMALE_KNIGHT_CHARACTER_MODEL_PATH,
  },
  barbarian: { female: FEMALE_BARBARIAN_CHARACTER_MODEL_PATH },
  rogue: { female: ROGUE_CHARACTER_MODEL_PATH },
}

export function getAvailableGenders(characterClass: CharacterClass): Gender[] {
  const genders = CLASS_GENDER_MODELS[characterClass]
  if (!genders) return ['male', 'female']
  return Object.keys(genders) as Gender[]
}

export function getCharacterModelPath(
  characterClass: CharacterClass,
  gender?: Gender
): string {
  const genders = CLASS_GENDER_MODELS[characterClass]
  if (genders) {
    if (gender && genders[gender]) return genders[gender]
    return Object.values(genders)[0]
  }
  return KNIGHT_CHARACTER_MODEL_PATH
}

const MODEL_Y_OFFSETS: Partial<
  Record<CharacterClass, Partial<Record<Gender, number>>>
> = {
  knight: { female: 0.13 },
  barbarian: { female: 0.06 },
  rogue: { female: 0.06 },
}

export function getCharacterModelYOffset(
  characterClass: CharacterClass,
  gender?: Gender
): number {
  if (!gender) return 0
  return MODEL_Y_OFFSETS[characterClass]?.[gender] ?? 0
}

export function getWeaponType(
  characterClass: CharacterClass
): WeaponType | null {
  switch (characterClass) {
    case 'merchant':
      return null
    case 'guard':
      return 'spear'
    default:
      return 'sword'
  }
}
