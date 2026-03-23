import {
  generateBaseHeights,
  classifyAndRemap,
  classifyAndRemapWithReference,
  carveRivers,
  blendBoundaries,
} from './terrain-height-gen'
import { computeCoastDistance, generateSplatMap } from './terrain-splat-gen'
import {
  TILE_DIM,
  VERTS_PER_SIDE,
  REGION_SIZE,
  REGION_CELLS,
} from './terrain-constants'

// Re-export constants and types for external consumers
export {
  SHALLOW_WATER_THRESHOLD,
  DEEP_WATER_THRESHOLD,
  REGION_CELLS,
  lerp,
  smoothstep,
  type TerrainGenConfig,
  type GeneratedTile,
  type NeighborEdgeData,
} from './terrain-constants'

import type {
  TerrainGenConfig,
  GeneratedTile,
  NeighborEdgeData,
} from './terrain-constants'

function encodeHeight(meters: number): number {
  return Math.round((meters + 500.0) / 0.05)
}

/**
 * Generate terrain for an entire region (16x16 tiles = 1024x1024 cells).
 */
export function generateRegionTerrain(
  regionX: number,
  regionZ: number,
  config: TerrainGenConfig,
  neighborEdges?: NeighborEdgeData
): GeneratedTile[] {
  const N = REGION_CELLS
  const worldOffsetX = regionX * N
  const worldOffsetZ = regionZ * N

  // --- Phase 1: Base elevation via fBm ---
  const rawHeights = generateBaseHeights(config, worldOffsetX, worldOffsetZ)

  // --- Phase 2: Classification & height remapping ---
  const heightField = config.referenceImage
    ? classifyAndRemapWithReference(
        rawHeights,
        config,
        worldOffsetX,
        worldOffsetZ
      )
    : classifyAndRemap(rawHeights, config)

  // --- Phase 3: River carving ---
  carveRivers(heightField, config)

  // --- Phase 4: Coast distance (used by splat map) ---
  const coastDist = computeCoastDistance(heightField)

  // --- Phase 5: Region boundary blending ---
  if (neighborEdges) {
    blendBoundaries(heightField, neighborEdges)
  }

  // --- Phase 6: Splat map generation ---
  const splatField = generateSplatMap(
    heightField,
    coastDist,
    config,
    regionX,
    regionZ
  )

  // --- Slice into per-tile data ---
  return sliceIntoTiles(regionX, regionZ, heightField, splatField)
}

function sliceIntoTiles(
  regionX: number,
  regionZ: number,
  heightField: Float32Array,
  splatField: Uint8Array
): GeneratedTile[] {
  const N = REGION_CELLS
  const tiles: GeneratedTile[] = []
  const baseTileX = regionX * REGION_SIZE
  const baseTileZ = regionZ * REGION_SIZE

  for (let tz = 0; tz < REGION_SIZE; tz++) {
    for (let tx = 0; tx < REGION_SIZE; tx++) {
      const heightmap = new Uint16Array(VERTS_PER_SIDE * VERTS_PER_SIDE)
      const splatmap = new Uint8Array(TILE_DIM * TILE_DIM * 4)

      // Height: 65x65 vertices (overlapping edges with adjacent tiles)
      for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
        for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
          const regionCX = Math.min(tx * TILE_DIM + vx, N - 1)
          const regionCZ = Math.min(tz * TILE_DIM + vz, N - 1)
          const ri = regionCZ * N + regionCX
          const ti = vz * VERTS_PER_SIDE + vx

          const h = heightField[ri]
          heightmap[ti] = Math.max(0, Math.min(65535, encodeHeight(h)))
        }
      }

      // Splat: 64x64 cells
      for (let cz = 0; cz < TILE_DIM; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const regionCX = tx * TILE_DIM + cx
          const regionCZ = tz * TILE_DIM + cz
          const ri = regionCZ * N + regionCX
          const ti = cz * TILE_DIM + cx

          const rsi = ri * 4
          const tsi = ti * 4
          splatmap[tsi] = splatField[rsi]
          splatmap[tsi + 1] = splatField[rsi + 1]
          splatmap[tsi + 2] = splatField[rsi + 2]
          splatmap[tsi + 3] = splatField[rsi + 3]
        }
      }

      tiles.push({
        tileX: baseTileX + tx,
        tileZ: baseTileZ + tz,
        heightmap,
        splatmap,
      })
    }
  }

  return tiles
}

/**
 * Regenerate only splatmaps for a region using existing heightmap data.
 * Returns per-tile splatmap data (heightmaps unchanged).
 */
export function regenerateRegionSplatmaps(
  regionX: number,
  regionZ: number,
  tileHeightmaps: { tileX: number; tileZ: number; heightmap: Uint16Array }[],
  config: TerrainGenConfig
): { tileX: number; tileZ: number; splatmap: Uint8Array }[] {
  const N = REGION_CELLS

  // Reconstruct region-wide heightField from per-tile heightmaps
  const heightField = new Float32Array(N * N)
  const baseTileX = regionX * REGION_SIZE
  const baseTileZ = regionZ * REGION_SIZE

  for (const tile of tileHeightmaps) {
    const tx = tile.tileX - baseTileX
    const tz = tile.tileZ - baseTileZ
    if (tx < 0 || tx >= REGION_SIZE || tz < 0 || tz >= REGION_SIZE) continue

    for (let cz = 0; cz < TILE_DIM; cz++) {
      for (let cx = 0; cx < TILE_DIM; cx++) {
        const regionCX = tx * TILE_DIM + cx
        const regionCZ = tz * TILE_DIM + cz
        const encoded = tile.heightmap[cz * VERTS_PER_SIDE + cx]
        heightField[regionCZ * N + regionCX] = encoded * 0.05 - 500.0
      }
    }
  }

  const coastDist = computeCoastDistance(heightField)
  const splatField = generateSplatMap(
    heightField,
    coastDist,
    config,
    regionX,
    regionZ
  )

  // Slice splatmap into per-tile data
  const results: { tileX: number; tileZ: number; splatmap: Uint8Array }[] = []
  for (let tz = 0; tz < REGION_SIZE; tz++) {
    for (let tx = 0; tx < REGION_SIZE; tx++) {
      const splatmap = new Uint8Array(TILE_DIM * TILE_DIM * 4)
      for (let cz = 0; cz < TILE_DIM; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const ri = (tz * TILE_DIM + cz) * N + (tx * TILE_DIM + cx)
          const ti = cz * TILE_DIM + cx
          splatmap[ti * 4] = splatField[ri * 4]
          splatmap[ti * 4 + 1] = splatField[ri * 4 + 1]
          splatmap[ti * 4 + 2] = splatField[ri * 4 + 2]
          splatmap[ti * 4 + 3] = splatField[ri * 4 + 3]
        }
      }
      results.push({ tileX: baseTileX + tx, tileZ: baseTileZ + tz, splatmap })
    }
  }

  return results
}
