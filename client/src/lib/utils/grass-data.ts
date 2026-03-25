/**
 * Grass placement data: binary encode/decode and placement computation.
 *
 * Binary format per tile (v2):
 *   [u32 shortCount] [u32 tallCount] [u32 flowerCount]
 *   [shortCount × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
 *   [tallCount   × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
 *   [flowerCount × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
 *
 * 12-byte header + 20 bytes per instance.
 */

import {
  SHORT_GRASS_R_MIN,
  SHORT_GRASS_R_MAX,
  TALL_GRASS_R_MIN,
  TALL_GRASS_R_MAX,
} from '../shaders/grass-material'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'
import { createRng } from './simplex-noise'
import type { TerrainHeightManager } from '../managers/terrainHeightManager'

const TILE_DIM = 64
const VERTS_PER_SIDE = 65
const CHANNELS = 4
const FLOATS_PER_INSTANCE = 5 // x, y, z, rotation, scale

const SHORT_SCALE_MIN = 0.4
const SHORT_SCALE_RANGE = 0.3
const TALL_SCALE_MIN = 0.5
const TALL_SCALE_RANGE = 1.0

export interface GrassPlacementData {
  shortCount: number
  tallCount: number
  flowerCount: number
  /** Interleaved f32: [x, y, z, rotation, scale] × shortCount, then × tallCount, then × flowerCount */
  buffer: ArrayBuffer
}

function tileSeed(tileX: number, tileZ: number): number {
  return ((tileX * 73856093) ^ (tileZ * 19349663)) | 0
}

interface VegParams {
  rMin: number
  rMax: number
  scaleMin: number
  scaleRange: number
  bladesPerAxis: number
}

const SHORT_BLADES_PER_AXIS = 14
const TALL_BLADES_PER_AXIS = 10

const SHORT_PARAMS: VegParams = {
  rMin: SHORT_GRASS_R_MIN,
  rMax: SHORT_GRASS_R_MAX,
  scaleMin: SHORT_SCALE_MIN,
  scaleRange: SHORT_SCALE_RANGE,
  bladesPerAxis: SHORT_BLADES_PER_AXIS,
}

const TALL_PARAMS: VegParams = {
  rMin: TALL_GRASS_R_MIN,
  rMax: TALL_GRASS_R_MAX,
  scaleMin: TALL_SCALE_MIN,
  scaleRange: TALL_SCALE_RANGE,
  bladesPerAxis: TALL_BLADES_PER_AXIS,
}

/** Inline bilinear height sampling — avoids Map lookups and string allocations. */
function sampleHeight(
  heightmap: Uint16Array,
  localX: number,
  localZ: number
): number {
  const cx = Math.min(Math.max(localX, 0), TILE_DIM - 1)
  const cz = Math.min(Math.max(localZ, 0), TILE_DIM - 1)
  const ix = cx | 0
  const iz = cz | 0
  const fx = cx - ix
  const fz = cz - iz

  const ix1 = Math.min(ix + 1, TILE_DIM)
  const iz1 = Math.min(iz + 1, TILE_DIM)

  const h00 = heightmap[iz * VERTS_PER_SIDE + ix] * 0.05 - 500.0
  const h10 = heightmap[iz * VERTS_PER_SIDE + ix1] * 0.05 - 500.0
  const h01 = heightmap[iz1 * VERTS_PER_SIDE + ix] * 0.05 - 500.0
  const h11 = heightmap[iz1 * VERTS_PER_SIDE + ix1] * 0.05 - 500.0

  const h0 = h00 + (h10 - h00) * fx
  const h1 = h01 + (h11 - h01) * fx
  return h0 + (h1 - h0) * fz
}

function computeInstances(
  params: VegParams,
  tileX: number,
  tileZ: number,
  splatData: Uint8Array,
  heightmap: Uint16Array
): Float32Array {
  const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const bpa = params.bladesPerAxis
  const step = 1.0 / bpa
  const densityRange = params.rMax - params.rMin
  const rand = createRng(tileSeed(tileX, tileZ) ^ params.rMin)

  const instances: number[] = []

  for (let cz = 0; cz < TILE_DIM; cz++) {
    for (let cx = 0; cx < TILE_DIM; cx++) {
      const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
      if (rVal < params.rMin || rVal > params.rMax) continue
      const density = densityRange > 0 ? (rVal - params.rMin) / densityRange : 1

      for (let dz = 0; dz < bpa; dz++) {
        for (let dx = 0; dx < bpa; dx++) {
          const localX = cx + dx * step + rand() * step
          const localZ = cz + dz * step + rand() * step
          if (rand() >= density) continue
          const worldY = sampleHeight(heightmap, localX, localZ)
          if (worldY < 0.05) continue

          const rotation = rand() * Math.PI * 2
          const scale = params.scaleMin + rand() * params.scaleRange

          instances.push(
            tileMinX + localX,
            worldY,
            tileMinZ + localZ,
            rotation,
            scale
          )
        }
      }
    }
  }

  return new Float32Array(instances)
}

const FLOWER_SCALE_MIN = 0.42
const FLOWER_SCALE_RANGE = 0.18

/**
 * Scatter flowers within short grass cells.
 * Lower grass density (lower R value) → higher flower probability.
 * R=230 (sparsest) → ~50%, R=239 (densest) → ~1%
 */
function computeFlowerInstances(
  tileX: number,
  tileZ: number,
  splatData: Uint8Array,
  heightmap: Uint16Array
): Float32Array {
  const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const rand = createRng(tileSeed(tileX, tileZ) ^ 0xf10e)
  const densityRange = SHORT_GRASS_R_MAX - SHORT_GRASS_R_MIN

  const instances: number[] = []

  for (let cz = 0; cz < TILE_DIM; cz++) {
    for (let cx = 0; cx < TILE_DIM; cx++) {
      const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
      if (rVal < SHORT_GRASS_R_MIN || rVal > SHORT_GRASS_R_MAX) continue

      // Flower probability: high when grass is sparse, low when dense
      // t=0 (R=230, sparse) → 80%, t=1 (R=239, dense) → 10%
      const t = (rVal - SHORT_GRASS_R_MIN) / densityRange
      const flowerProb = 0.4 * Math.pow(0.125, t) // 0.40 → 0.05

      // One flower chance per cell
      const localX = cx + 0.5 + (rand() - 0.5) * 0.8
      const localZ = cz + 0.5 + (rand() - 0.5) * 0.8
      if (rand() >= flowerProb) continue

      const worldY = sampleHeight(heightmap, localX, localZ)
      if (worldY < 0.05) continue

      const rotation = rand() * Math.PI * 2
      const scale = FLOWER_SCALE_MIN + rand() * FLOWER_SCALE_RANGE

      instances.push(
        tileMinX + localX,
        worldY,
        tileMinZ + localZ,
        rotation,
        scale
      )
    }
  }

  return new Float32Array(instances)
}

const HEADER_BYTES = 12 // 3 × u32

/** Pack three instance arrays into a single GrassPlacementData buffer. */
function packGrassBuffer(
  shortData: Float32Array,
  tallData: Float32Array,
  flowerData: Float32Array
): GrassPlacementData {
  const shortCount = shortData.length / FLOATS_PER_INSTANCE
  const tallCount = tallData.length / FLOATS_PER_INSTANCE
  const flowerCount = flowerData.length / FLOATS_PER_INSTANCE

  const totalBytes =
    HEADER_BYTES + (shortData.length + tallData.length + flowerData.length) * 4
  const buffer = new ArrayBuffer(totalBytes)
  const header = new Uint32Array(buffer, 0, 3)
  header[0] = shortCount
  header[1] = tallCount
  header[2] = flowerCount

  const body = new Float32Array(buffer, HEADER_BYTES)
  body.set(shortData, 0)
  body.set(tallData, shortData.length)
  body.set(flowerData, shortData.length + tallData.length)

  return { shortCount, tallCount, flowerCount, buffer }
}

/**
 * Compute grass placement data for a single tile.
 * Requires heightmap and splatmap data to be already loaded in the manager.
 */
export function computeGrassPlacement(
  tileX: number,
  tileZ: number,
  splatData: Uint8Array,
  hMgr: TerrainHeightManager
): GrassPlacementData {
  const heightmap = hMgr.getHeightmap(tileX, tileZ)
  if (!heightmap) {
    return packGrassBuffer(
      new Float32Array(0),
      new Float32Array(0),
      new Float32Array(0)
    )
  }

  const shortInstances = computeInstances(
    SHORT_PARAMS,
    tileX,
    tileZ,
    splatData,
    heightmap
  )
  const tallInstances = computeInstances(
    TALL_PARAMS,
    tileX,
    tileZ,
    splatData,
    heightmap
  )
  const flowerInstances = computeFlowerInstances(
    tileX,
    tileZ,
    splatData,
    heightmap
  )

  return packGrassBuffer(shortInstances, tallInstances, flowerInstances)
}

/**
 * Generate and save grass data for a batch of tiles via the manager.
 * Yields to the event loop periodically and saves in parallel batches.
 */
export async function generateAndSaveGrassData(
  tiles: { tileX: number; tileZ: number; splatmap: Uint8Array }[],
  hMgr: TerrainHeightManager,
  grassMgr: {
    saveGrassData(
      tileX: number,
      tileZ: number,
      data: GrassPlacementData
    ): Promise<void>
  },
  onProgress?: (label: string) => void
): Promise<void> {
  const BATCH_SIZE = 8
  const grassResults: {
    tileX: number
    tileZ: number
    data: GrassPlacementData
  }[] = []
  for (let i = 0; i < tiles.length; i++) {
    const tile = tiles[i]
    const data = computeGrassPlacement(
      tile.tileX,
      tile.tileZ,
      tile.splatmap,
      hMgr
    )
    grassResults.push({ tileX: tile.tileX, tileZ: tile.tileZ, data })
    if (i % 4 === 3) {
      onProgress?.(`Generating grass... ${i + 1}/${tiles.length}`)
      await new Promise((r) => setTimeout(r, 0))
    }
  }
  for (let i = 0; i < grassResults.length; i += BATCH_SIZE) {
    const batch = grassResults.slice(i, i + BATCH_SIZE)
    onProgress?.(
      `Saving grass... ${Math.min(i + BATCH_SIZE, grassResults.length)}/${grassResults.length}`
    )
    await Promise.all(
      batch.map((g) => grassMgr.saveGrassData(g.tileX, g.tileZ, g.data))
    )
  }
}

/**
 * Remove grass instances that fall within a world-space rectangle.
 * Returns null if no instances were removed (caller can skip saving).
 */
export function removeGrassInRect(
  data: GrassPlacementData,
  minX: number,
  minZ: number,
  maxX: number,
  maxZ: number
): GrassPlacementData | null {
  /** Two-pass filter: count survivors, then copy into a typed array. */
  function filterInstances(raw: Float32Array): Float32Array {
    const count = raw.length / FLOATS_PER_INSTANCE
    // Pass 1: count survivors
    let kept = 0
    for (let i = 0; i < count; i++) {
      const base = i * FLOATS_PER_INSTANCE
      const x = raw[base]
      const z = raw[base + 2]
      if (!(x >= minX && x <= maxX && z >= minZ && z <= maxZ)) kept++
    }
    if (kept === count) return raw // nothing removed
    // Pass 2: copy survivors
    const out = new Float32Array(kept * FLOATS_PER_INSTANCE)
    let offset = 0
    for (let i = 0; i < count; i++) {
      const base = i * FLOATS_PER_INSTANCE
      const x = raw[base]
      const z = raw[base + 2]
      if (x >= minX && x <= maxX && z >= minZ && z <= maxZ) continue
      out.set(raw.subarray(base, base + FLOATS_PER_INSTANCE), offset)
      offset += FLOATS_PER_INSTANCE
    }
    return out
  }

  const shortRaw = getInstanceData(data, 'short')
  const tallRaw = getInstanceData(data, 'tall')
  const flowerRaw = getInstanceData(data, 'flower')

  const shortFiltered = filterInstances(shortRaw)
  const tallFiltered = filterInstances(tallRaw)
  const flowerFiltered = filterInstances(flowerRaw)

  // Early-out: nothing was removed
  if (
    shortFiltered === shortRaw &&
    tallFiltered === tallRaw &&
    flowerFiltered === flowerRaw
  ) {
    return null
  }

  return packGrassBuffer(shortFiltered, tallFiltered, flowerFiltered)
}

/** Decode binary grass placement data. */
export function decodeGrassData(buffer: ArrayBuffer): GrassPlacementData {
  const header = new Uint32Array(buffer, 0, 3)
  return {
    shortCount: header[0],
    tallCount: header[1],
    flowerCount: header[2],
    buffer,
  }
}

/**
 * Global grass density multiplier (0–1). Applied at load time to thin out
 * pre-computed grass instances. Flowers are not affected.
 */
export const GRASS_DENSITY_SCALE = 1.0

/** Deterministically thin a Float32Array of instances by keeping only a fraction. */
function thinInstances(src: Float32Array, keep: number): Float32Array {
  if (keep >= 1) return src
  const count = src.length / FLOATS_PER_INSTANCE

  // Pass 1: count survivors
  let survived = 0
  for (let i = 0; i < count; i++) {
    const x = src[i * FLOATS_PER_INSTANCE]
    const z = src[i * FLOATS_PER_INSTANCE + 2]
    const h =
      ((Math.imul((x * 374761) | 0, 668265263) ^
        Math.imul((z * 550929) | 0, 374761393)) >>>
        0) /
      0xffffffff
    if (h < keep) survived++
  }
  if (survived === count) return src

  // Pass 2: copy survivors into pre-allocated buffer
  const out = new Float32Array(survived * FLOATS_PER_INSTANCE)
  let offset = 0
  for (let i = 0; i < count; i++) {
    const base = i * FLOATS_PER_INSTANCE
    const x = src[base]
    const z = src[base + 2]
    const h =
      ((Math.imul((x * 374761) | 0, 668265263) ^
        Math.imul((z * 550929) | 0, 374761393)) >>>
        0) /
      0xffffffff
    if (h < keep) {
      out.set(src.subarray(base, base + FLOATS_PER_INSTANCE), offset)
      offset += FLOATS_PER_INSTANCE
    }
  }
  return out
}

/** Extract raw instance Float32Array for a given type from decoded data. */
export function getInstanceData(
  data: GrassPlacementData,
  type: 'short' | 'tall' | 'flower'
): Float32Array {
  const headerBytes = 12
  const shortFloats = data.shortCount * FLOATS_PER_INSTANCE
  const tallFloats = data.tallCount * FLOATS_PER_INSTANCE

  switch (type) {
    case 'short':
      return new Float32Array(data.buffer, headerBytes, shortFloats)
    case 'tall':
      return new Float32Array(
        data.buffer,
        headerBytes + shortFloats * 4,
        tallFloats
      )
    case 'flower':
      return new Float32Array(
        data.buffer,
        headerBytes + (shortFloats + tallFloats) * 4,
        data.flowerCount * FLOATS_PER_INSTANCE
      )
  }
}

/**
 * Extract instance data with density thinning applied.
 * Use this for display; use getInstanceData() for data manipulation.
 */
export function getThinnedInstanceData(
  data: GrassPlacementData,
  type: 'short' | 'tall' | 'flower'
): Float32Array {
  const raw = getInstanceData(data, type)
  if (type === 'flower') return raw
  return thinInstances(raw, GRASS_DENSITY_SCALE)
}
