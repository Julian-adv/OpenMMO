export const WARRIOR_CHARACTER_MODEL_PATH = '/models/maria.glb'
export const KNIGHT_CHARACTER_MODEL_PATH = '/models/medieval_knight.glb'
export const CHARACTER_ANIMATION_SOURCE_MODEL_PATH =
  '/models/animations/locomotion.glb'

export const CHARACTER_ANIMATION_PACK_PATHS = {
  locomotion: '/models/animations/locomotion.glb',
  combatMelee: '/models/animations/combat_melee.glb',
} as const

export function getCharacterModelPath(
  characterClass: 'warrior' | 'knight'
): string {
  return characterClass === 'warrior'
    ? WARRIOR_CHARACTER_MODEL_PATH
    : KNIGHT_CHARACTER_MODEL_PATH
}
