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

export function worldToTileCell(wx: number, wz: number) {
  const S = TERRAIN_TILE_SIZE
  const tileX = Math.round(wx / S)
  const tileZ = Math.round(wz / S)
  const cellX = Math.max(0, Math.min(S - 1, Math.floor(wx - tileX * S + S / 2)))
  const cellZ = Math.max(0, Math.min(S - 1, Math.floor(wz - tileZ * S + S / 2)))
  return { tileX, tileZ, cellX, cellZ }
}

/**
 * Create a 2×2 tile grid based on the player's floor-rounded chunk position.
 * The 4 tiles always cover the player: (fx,fz), (fx+1,fz), (fx,fz+1), (fx+1,fz+1).
 * This is sufficient for an orthographic camera viewport with 64-unit tiles.
 */
export function createTerrainTiles(
  floorChunkX: number,
  floorChunkZ: number,
  tileSize = TERRAIN_TILE_SIZE
): TerrainTile[] {
  const tiles: TerrainTile[] = []
  for (let dz = 0; dz <= 1; dz++) {
    for (let dx = 0; dx <= 1; dx++) {
      const cx = floorChunkX + dx
      const cz = floorChunkZ + dz
      tiles.push({
        id: `${cx}_${cz}`,
        position: [cx * tileSize, 0, cz * tileSize],
      })
    }
  }
  return tiles
}

/**
 * Get the floor-based chunk coordinate for a world position.
 * Combined with the 2×2 grid, this ensures the player is always
 * surrounded by terrain regardless of where they stand within a tile.
 */
export function getTerrainChunkFromPosition(
  position: Vector3Like,
  tileSize = TERRAIN_TILE_SIZE
): TerrainChunk {
  return {
    x: Math.floor(position.x / tileSize),
    z: Math.floor(position.z / tileSize),
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
