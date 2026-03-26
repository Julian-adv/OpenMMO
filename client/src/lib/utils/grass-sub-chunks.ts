/**
 * Sub-chunk partitioning and tile-range utilities for the grass system.
 * Extracted from GameSceneGrassLayer so the logic is unit-testable.
 */

import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'

export const SUB_CHUNK_SIZE = 32

export interface SubChunkRange {
  scMinX: number
  scMaxX: number
  scMinZ: number
  scMaxZ: number
}

/** Compute the sub-chunk index range covered by a terrain tile. */
export function tileSubChunkRange(tileX: number, tileZ: number): SubChunkRange {
  const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const tileMaxX = tileX * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
  const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const tileMaxZ = tileZ * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
  return {
    scMinX: Math.floor(tileMinX / SUB_CHUNK_SIZE),
    scMaxX: Math.floor((tileMaxX - 1) / SUB_CHUNK_SIZE),
    scMinZ: Math.floor(tileMinZ / SUB_CHUNK_SIZE),
    scMaxZ: Math.floor((tileMaxZ - 1) / SUB_CHUNK_SIZE),
  }
}

/** Check if a sub-chunk key falls within a tile's sub-chunk range. */
export function isKeyInTileRange(key: string, range: SubChunkRange): boolean {
  const [sx, sz] = key.split(',').map(Number)
  return (
    sx >= range.scMinX &&
    sx <= range.scMaxX &&
    sz >= range.scMinZ &&
    sz <= range.scMaxZ
  )
}

/**
 * Partition raw grass instance data (Float32Array of [x, y, z, rot, scale] × N)
 * into sub-chunk keys based on world XZ position.
 * Returns a Map from "scx,scz" → array of instance indices.
 */
export function partitionKeysFromRawData(
  rawData: Float32Array,
  subChunkSize = SUB_CHUNK_SIZE
): Map<string, number[]> {
  const count = rawData.length / 5
  const groups = new Map<string, number[]>()
  for (let i = 0; i < count; i++) {
    const x = rawData[i * 5]
    const z = rawData[i * 5 + 2]
    const key = `${Math.floor(x / subChunkSize)},${Math.floor(z / subChunkSize)}`
    let list = groups.get(key)
    if (!list) {
      list = []
      groups.set(key, list)
    }
    list.push(i)
  }
  return groups
}

/**
 * Filter sub-chunk keys to only those within a tile's spatial range.
 * Discards boundary spillover caused by placement jitter.
 */
export function filterKeysToTileRange<T>(
  chunks: Map<string, T>,
  tileX: number,
  tileZ: number
): Map<string, T> {
  const range = tileSubChunkRange(tileX, tileZ)
  const filtered = new Map<string, T>()
  for (const [key, value] of chunks) {
    if (isKeyInTileRange(key, range)) {
      filtered.set(key, value)
    }
  }
  return filtered
}
