import * as THREE from 'three'

export interface TerrainTile {
  id: string
  position: [number, number, number]
}

export interface TerrainChunk {
  x: number
  z: number
}

export interface Vector3Like {
  x: number
  y: number
  z: number
}

export const SEA_LEVEL = 0.0
export const SEA_LEVEL_ENCODED = 10000

export const TERRAIN_TILE_SIZE = 64
export const TERRAIN_TILE_SEGMENTS = 64
export const TERRAIN_GRID_RADIUS = 1 // 1 => 3x3 tiles around player

export function createTerrainTiles(
  centerChunkX: number,
  centerChunkZ: number,
  tileSize = TERRAIN_TILE_SIZE,
  gridRadius = TERRAIN_GRID_RADIUS
): TerrainTile[] {
  const nextTiles: TerrainTile[] = []

  for (let dz = -gridRadius; dz <= gridRadius; dz++) {
    for (let dx = -gridRadius; dx <= gridRadius; dx++) {
      nextTiles.push({
        id: `${centerChunkX + dx}_${centerChunkZ + dz}`,
        position: [
          (centerChunkX + dx) * tileSize,
          0,
          (centerChunkZ + dz) * tileSize,
        ],
      })
    }
  }

  return nextTiles
}

export function getTerrainChunkFromPosition(
  position: Vector3Like,
  tileSize = TERRAIN_TILE_SIZE
): TerrainChunk {
  return {
    x: Math.round(position.x / tileSize),
    z: Math.round(position.z / tileSize),
  }
}

export function createTerrainGeometry(
  tileSize = TERRAIN_TILE_SIZE,
  tileSegments = TERRAIN_TILE_SEGMENTS
): THREE.BufferGeometry {
  const plane = new THREE.PlaneGeometry(
    tileSize,
    tileSize,
    tileSegments,
    tileSegments
  )
  plane.rotateX(-Math.PI / 2) // Lay flat on XZ
  return plane
}
