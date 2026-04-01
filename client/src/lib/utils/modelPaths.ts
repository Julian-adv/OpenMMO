import type { CharacterClass } from '../network/networkTypes'

export type WeaponType = 'sword' | 'spear'

export const WARRIOR_CHARACTER_MODEL_PATH = '/models/female_knight.glb'
export const KNIGHT_CHARACTER_MODEL_PATH = '/models/knight.glb'
export const THIEF_CHARACTER_MODEL_PATH = '/models/female_thief.glb'
export const MERCHANT_CHARACTER_MODEL_PATH = '/models/npc_woman.glb'
export const GUARD_CHARACTER_MODEL_PATH = '/models/guard.glb'

export const CHARACTER_ANIMATION_PACK_PATHS = {
  locomotion: '/models/animations/locomotion.glb',
  combatMelee: '/models/animations/combat_melee.glb',
} as const

export const WEAPON_MODEL_PATHS: Record<WeaponType, string> = {
  sword: '/models/sword.glb',
  spear: '/models/spear.glb',
} as const

export function getCharacterModelPath(characterClass: CharacterClass): string {
  switch (characterClass) {
    case 'warrior':
      return WARRIOR_CHARACTER_MODEL_PATH
    case 'thief':
      return THIEF_CHARACTER_MODEL_PATH
    case 'merchant':
      return MERCHANT_CHARACTER_MODEL_PATH
    case 'guard':
      return GUARD_CHARACTER_MODEL_PATH
    default:
      return KNIGHT_CHARACTER_MODEL_PATH
  }
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
