import { HOUSING_TEXTURES } from './house-geo-utils'

export const DUNGEON_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
)
export const DUNGEON_MASONRY_BACK_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.mapUrl === '/textures/dungeon/cave-limestone-wall.webp'
)
export const DUNGEON_WALL_WEATHERING_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.mapUrl === '/textures/dungeon/wall-weathering-decals.webp'
)
export const DUNGEON_WALL_DETAILS_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.mapUrl === '/textures/dungeon/wall-cracks-cobwebs.webp'
)
/** Mossy plaster for the *surface* entrance building walls — distinct from the
 *  underground stone walls (DUNGEON_WALL_TEXTURE_IDX). */
export const DUNGEON_ENTRANCE_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/worn_mossy_plasterwall_1k'
)
export const DUNGEON_FLOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/grey_stone_path_1k'
)
export const DUNGEON_VOID_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.label === 'Void'
)
/** Grey roof tiles for the surface entrance roof. */
export const DUNGEON_CEILING_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/grey_roof_tiles_02_1k'
)
/** Stone blocks for the decorative entrance corner pillars (accent against the
 *  mossy-plaster entrance walls). */
export const DUNGEON_PILLAR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
)
/** Wooden garage-door texture for the entrance door (mapped 0→1 across it). */
export const DUNGEON_DOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'dungeon/wooden_garage_door_1k'
)
/** Rusty grid for doors that take a floor key (doc/DUNGEON_REWARD.md). */
export const DUNGEON_LOCKED_DOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'dungeon/rusty_metal_grid_1k'
)

// A small lift anchors contact shadows cast by the solids' back faces.
export const SHADOW_CONTACT_LIFT = 0.02

/** Name of the up-shaft stairs sub-group inside a floor group; the dungeon
 *  layer looks it up to fade it to a ghost when it occludes the player. */
export const UP_SHAFT_GROUP_NAME = 'upShaftStairs'

export const SLAB_THICKNESS = 0.15
/** Flat landing cells at shaft ends — must match dungeonManager.rampY. */
export const LANDING_CELLS = 1.0
export const STEP_RISE = 0.25
/** One texture repeat per two metres of dungeon floor/stairs. */
export const DUNGEON_FLOOR_UV_SCALE = 0.5

/** Room wall thickness, also used at cave wall connections. */
export const WALL_THICKNESS = 0.1
export const WALL_HALF_THICKNESS = WALL_THICKNESS / 2

export interface DungeonGeoCtx {
  grid: number
  /** Wall visual height (matches shared DUNGEON_WALL_HEIGHT). */
  wallHeight: number
  /** Vertical distance between floors (shared DUNGEON_FLOOR_HEIGHT). */
  floorHeight: number
  shaftW: number
  shaftLen: number
}

/** Which of a room's four walls a corridor mouth sits in. Packed into the door
 *  id, so the values double as encoding lanes (0..3). */
export const WALL_N = 0
export const WALL_E = 1
export const WALL_S = 2
export const WALL_W = 3
export type DungeonWall =
  | typeof WALL_N
  | typeof WALL_E
  | typeof WALL_S
  | typeof WALL_W
