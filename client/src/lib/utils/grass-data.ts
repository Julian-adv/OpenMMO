/**
 * Grass placement data: binary encode/decode and placement computation.
 *
 * Binary format per tile:
 *   [u32 shortCount] [u32 tallCount]
 *   [shortCount × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
 *   [tallCount   × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
 *
 * 8-byte header + 20 bytes per instance.
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
const BLADES_PER_AXIS = 10
const FLOATS_PER_INSTANCE = 5 // x, y, z, rotation, scale

const SHORT_SCALE_MIN = 0.7
const SHORT_SCALE_RANGE = 0.6
const TALL_SCALE_MIN = 0.8
const TALL_SCALE_RANGE = 0.5

export interface GrassPlacementData {
  shortCount: number
  tallCount: number
  /** Interleaved f32: [x, y, z, rotation, scale] × shortCount, then × tallCount */
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
}

const SHORT_PARAMS: VegParams = {
  rMin: SHORT_GRASS_R_MIN,
  rMax: SHORT_GRASS_R_MAX,
  scaleMin: SHORT_SCALE_MIN,
  scaleRange: SHORT_SCALE_RANGE,
}

const TALL_PARAMS: VegParams = {
  rMin: TALL_GRASS_R_MIN,
  rMax: TALL_GRASS_R_MAX,
  scaleMin: TALL_SCALE_MIN,
  scaleRange: TALL_SCALE_RANGE,
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
  const step = 1.0 / BLADES_PER_AXIS
  const densityRange = params.rMax - params.rMin
  const rand = createRng(tileSeed(tileX, tileZ) ^ params.rMin)

  const instances: number[] = []

  for (let cz = 0; cz < TILE_DIM; cz++) {
    for (let cx = 0; cx < TILE_DIM; cx++) {
      const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
      if (rVal < params.rMin || rVal > params.rMax) continue
      const density = densityRange > 0 ? (rVal - params.rMin) / densityRange : 1

      for (let dz = 0; dz < BLADES_PER_AXIS; dz++) {
        for (let dx = 0; dx < BLADES_PER_AXIS; dx++) {
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
    // No heightmap → empty grass
    const buffer = new ArrayBuffer(8)
    return { shortCount: 0, tallCount: 0, buffer }
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

  const shortCount = shortInstances.length / FLOATS_PER_INSTANCE
  const tallCount = tallInstances.length / FLOATS_PER_INSTANCE

  // Pack into binary: header (8 bytes) + instance data
  const totalBytes = 8 + (shortInstances.length + tallInstances.length) * 4
  const buffer = new ArrayBuffer(totalBytes)
  const header = new Uint32Array(buffer, 0, 2)
  header[0] = shortCount
  header[1] = tallCount

  const data = new Float32Array(buffer, 8)
  data.set(shortInstances, 0)
  data.set(tallInstances, shortInstances.length)

  return { shortCount, tallCount, buffer }
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

/** Decode binary grass placement data. */
export function decodeGrassData(buffer: ArrayBuffer): GrassPlacementData {
  const header = new Uint32Array(buffer, 0, 2)
  const shortCount = header[0]
  const tallCount = header[1]
  return { shortCount, tallCount, buffer }
}

/** Extract instance Float32Array for a given type from decoded data. */
export function getInstanceData(
  data: GrassPlacementData,
  type: 'short' | 'tall'
): Float32Array {
  const offset = 8 // skip header
  if (type === 'short') {
    return new Float32Array(
      data.buffer,
      offset,
      data.shortCount * FLOATS_PER_INSTANCE
    )
  }
  const shortBytes = data.shortCount * FLOATS_PER_INSTANCE * 4
  return new Float32Array(
    data.buffer,
    offset + shortBytes,
    data.tallCount * FLOATS_PER_INSTANCE
  )
}
