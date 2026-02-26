export const WARRIOR_CHARACTER_MODEL_PATH = '/models/female_knight.glb'
export const KNIGHT_CHARACTER_MODEL_PATH = '/models/medieval_knight.glb'
export const THIEF_CHARACTER_MODEL_PATH = '/models/female_thief.glb'

export const CHARACTER_ANIMATION_PACK_PATHS = {
  locomotion: '/models/animations/locomotion.glb',
  combatMelee: '/models/animations/combat_melee.glb',
} as const

export function getCharacterModelPath(
  characterClass: 'warrior' | 'knight' | 'thief'
): string {
  switch (characterClass) {
    case 'warrior':
      return WARRIOR_CHARACTER_MODEL_PATH
    case 'thief':
      return THIEF_CHARACTER_MODEL_PATH
    default:
      return KNIGHT_CHARACTER_MODEL_PATH
  }
}
