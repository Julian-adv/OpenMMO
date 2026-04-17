import type { ReferenceImageData } from './referenceImageSampler'

// --- Constants ---

export const TILE_DIM = 64
export const VERTS_PER_SIDE = TILE_DIM + 1 // 65
export const REGION_SIZE = 16
export const REGION_CELLS = REGION_SIZE * TILE_DIM // 1024

/** Height threshold at/above which water is considered shallow sea (upper bound of sea) */
export const SHALLOW_WATER_THRESHOLD = -0.1

/** Height threshold below which water is considered deep sea */
export const DEEP_WATER_THRESHOLD = -1.5

/** Absolute height (meters) at which snow begins to blend in */
export const SNOW_START_HEIGHT = 300
/** Absolute height (meters) at which terrain is fully snow */
export const SNOW_FULL_HEIGHT = 350

// --- Types ---

export interface TerrainGenConfig {
  seed: number
  minHeight: number // meters (-500 ~ 0)
  maxHeight: number // meters (0 ~ 3276)
  seaProportion: number // 0..1
  plainProportion: number // 0..1
  mountainProportion: number // 0..1
  shallowSeaRatio: number // 0..1, fraction of sea area that is shallow
  riverCount: number // 0..5
  referenceImage?: ReferenceImageData // optional reference image for biome placement
}

export interface GeneratedTile {
  tileX: number
  tileZ: number
  heightmap: Uint16Array // 4225 values (65*65, vertex-based)
  splatmap: Uint8Array // 16384 values (64*64*4, cell-based)
}

export interface NeighborEdgeData {
  north?: Float32Array // 1024 heights (top row of the region above)
  south?: Float32Array // 1024 heights (bottom row of the region below)
  east?: Float32Array // 1024 heights (left column of the region to the right)
  west?: Float32Array // 1024 heights (right column of the region to the left)
}

// --- Utility functions ---

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t
}

export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}
