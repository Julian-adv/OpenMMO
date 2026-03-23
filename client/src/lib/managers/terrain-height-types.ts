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

export function worldToTileCoord(worldCoord: number): number {
  return Math.floor((worldCoord + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE)
}
