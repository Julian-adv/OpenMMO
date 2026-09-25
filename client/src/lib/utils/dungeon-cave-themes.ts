import { HOUSING_TEXTURES } from './housing-textures'

export const DUNGEON_CAVE_THEMES = ['limestone', 'moss', 'masonry'].map(
  (id) => ({
    id,
    wallTexture: HOUSING_TEXTURES.findIndex(
      (entry) => entry.mapUrl === `/textures/dungeon/cave-${id}-wall.webp`
    ),
    floorTexture: HOUSING_TEXTURES.findIndex(
      (entry) => entry.mapUrl === `/textures/dungeon/cave-${id}-floor.webp`
    ),
  })
)

export function dungeonCaveSeed(dungeonId: string, depth: number): number {
  let hash = 2166136261
  for (const char of dungeonId) {
    hash = Math.imul(hash ^ char.charCodeAt(0), 16777619)
  }
  return ((hash >>> 0) + depth - 1) >>> 0
}

export function dungeonCaveTheme(dungeonId: string, depth: number) {
  return DUNGEON_CAVE_THEMES[
    dungeonCaveSeed(dungeonId, depth) % DUNGEON_CAVE_THEMES.length
  ]
}
