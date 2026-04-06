import * as THREE from 'three'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'

export const TILE_DIM = 64
export const VERTS_PER_SIDE = TILE_DIM + 1 // 65 vertices per axis
export const PADDED_SIDE = VERTS_PER_SIDE + 2 // 67 — padded grid for analytical normals

export interface AffectedTile {
  tileX: number
  tileZ: number
}

export type HeightChangedCallback = (tiles: AffectedTile[]) => void

export interface TerrainHeightState {
  heightmaps: Map<string, Uint16Array>
  originalHeightmaps: Map<string, Uint16Array>
  missingOriginalTiles: Set<string>
  geometries: Map<string, THREE.BufferGeometry>
  dirtyTiles: Set<string>
  dirtyOriginalTiles: Set<string>
}

export function tileKey(tileX: number, tileZ: number): string {
  return `${tileX},${tileZ}`
}

export function encodeHeight(meters: number): number {
  return Math.round((meters + 500.0) / 0.05)
}

export function decodeHeight(value: number): number {
  return value * 0.05 - 500.0
}

/** Bilinear height sampling from a heightmap at local tile coordinates. */
export function sampleHeight(
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

  const h00 = decodeHeight(heightmap[iz * VERTS_PER_SIDE + ix])
  const h10 = decodeHeight(heightmap[iz * VERTS_PER_SIDE + ix1])
  const h01 = decodeHeight(heightmap[iz1 * VERTS_PER_SIDE + ix])
  const h11 = decodeHeight(heightmap[iz1 * VERTS_PER_SIDE + ix1])

  const h0 = h00 + (h10 - h00) * fx
  const h1 = h01 + (h11 - h01) * fx
  return h0 + (h1 - h0) * fz
}

export function worldToTileCoord(worldCoord: number): number {
  return Math.floor((worldCoord + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE)
}
