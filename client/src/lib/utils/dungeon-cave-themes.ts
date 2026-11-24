import { DUNGEON_ENTRANCES } from '../data/dungeonDefs'
import { HOUSING_TEXTURES } from './housing-textures'

function theme(
  id: string,
  {
    masonry = false,
    propModels = {},
    treasureChest,
    hallColumn,
    entrance,
  }: {
    masonry?: boolean
    propModels?: Record<string, string>
    treasureChest?: string
    hallColumn?: string
    entrance?: { door: string; column: string }
  } = {}
) {
  const texture = (surface: string) =>
    HOUSING_TEXTURES.findIndex(
      (entry) => entry.mapUrl === `/textures/dungeon/cave-${id}-${surface}.webp`
    )
  return {
    id,
    /** Laid blocks and tiles rather than bare rock. */
    masonry,
    wallTexture: texture('wall'),
    floorTexture: texture('floor'),
    /** Catalog ids drawn in place of a prop's usual model. */
    propModels,
    /** The final floor's chest, when not the plain animated one. */
    treasureChest,
    /** Drawn over a hall's rock pillars, which still block as rock. */
    hallColumn,
    /** A temple-style surface entrance in this theme's stone. */
    entrance: entrance && {
      wallTexture: texture('wall'),
      roofTexture: texture('floor'),
      doorTexture: HOUSING_TEXTURES.findIndex(
        (entry) => entry.mapUrl === `/textures/dungeon/${entrance.door}.webp`
      ),
      column: entrance.column,
    },
  }
}

export const DUNGEON_CAVE_THEMES = [
  theme('limestone'),
  theme('moss'),
  theme('masonry', { masonry: true }),
]

/** Marble for every floor of a `layoutStyle=temple` dungeon, rooms included. */
export const DUNGEON_TEMPLE_THEME = theme('temple', {
  masonry: true,
  propModels: {
    barrel: 'temple_amphora',
    barrel_broken: 'temple_amphora_broken',
  },
  treasureChest: 'garden_relic_chest',
  hallColumn: 'temple_column',
  entrance: { door: 'temple-door', column: 'temple_column' },
})

const TEMPLES = new Set(
  DUNGEON_ENTRANCES.filter((def) => def.layoutStyle === 'temple').map(
    (def) => def.id
  )
)

export function dungeonCaveSeed(dungeonId: string, depth: number): number {
  let hash = 2166136261
  for (const char of dungeonId) {
    hash = Math.imul(hash ^ char.charCodeAt(0), 16777619)
  }
  return ((hash >>> 0) + depth - 1) >>> 0
}

export function dungeonCaveTheme(dungeonId: string, depth: number) {
  if (TEMPLES.has(dungeonId)) return DUNGEON_TEMPLE_THEME
  return DUNGEON_CAVE_THEMES[
    dungeonCaveSeed(dungeonId, depth) % DUNGEON_CAVE_THEMES.length
  ]
}
