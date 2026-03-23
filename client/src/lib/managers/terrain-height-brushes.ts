import {
  TILE_DIM,
  VERTS_PER_SIDE,
  tileKey,
  encodeHeight,
  decodeHeight,
  worldToTileCoord,
  type TerrainHeightState,
  type AffectedTile,
} from './terrain-height-types'
import {
  applyHeightToGeometry,
  refreshAdjacentTileEdges,
} from './terrain-height-geometry'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'

function finalizeBrush(
  state: TerrainHeightState,
  affected: AffectedTile[]
): void {
  for (const { tileX: tx, tileZ: tz } of affected) {
    refreshAdjacentTileEdges(state, tx, tz)
  }
}

export function applyBrush(
  state: TerrainHeightState,
  worldX: number,
  worldZ: number,
  radius: number,
  strengthPerSec: number,
  raise: boolean,
  deltaTimeSec: number
): AffectedTile[] {
  const affected: AffectedTile[] = []
  const delta = strengthPerSec * deltaTimeSec * (raise ? 1 : -1)
  const sigma = radius / 2.5

  const minWorldX = worldX - radius
  const maxWorldX = worldX + radius
  const minWorldZ = worldZ - radius
  const maxWorldZ = worldZ + radius

  const minTileX = Math.floor(
    (minWorldX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const maxTileX = Math.floor(
    (maxWorldX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const minTileZ = Math.floor(
    (minWorldZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const maxTileZ = Math.floor(
    (maxWorldZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )

  const affectedKeys = new Set<string>()

  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      const key = tileKey(tx, tz)
      const data = state.heightmaps.get(key)
      if (!data) continue

      const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

      const startCX = Math.max(0, Math.floor(minWorldX - tileMinX))
      const endCX = Math.min(TILE_DIM - 1, Math.floor(maxWorldX - tileMinX))
      const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
      const endCZ = Math.min(TILE_DIM - 1, Math.floor(maxWorldZ - tileMinZ))

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const vertexWorldX = tileMinX + cx
          const vertexWorldZ = tileMinZ + cz

          const dx = vertexWorldX - worldX
          const dz = vertexWorldZ - worldZ
          const dist = Math.sqrt(dx * dx + dz * dz)

          if (dist > radius) continue

          const weight = Math.exp(-(dist * dist) / (2 * sigma * sigma))
          const heightDelta = delta * weight

          const idx = cz * VERTS_PER_SIDE + cx
          const currentHeight = decodeHeight(data[idx])
          const steps = Math.trunc(heightDelta / 0.05)
          if (steps === 0) continue
          const newHeight = currentHeight + steps * 0.05
          const newValue = Math.max(0, Math.min(65535, encodeHeight(newHeight)))
          data[idx] = newValue

          // Sync to original heightmap
          const origData = state.originalHeightmaps.get(key)
          if (origData) {
            origData[idx] = newValue
            state.dirtyOriginalTiles.add(key)
          }

          if (!affectedKeys.has(key)) {
            affectedKeys.add(key)
            affected.push({ tileX: tx, tileZ: tz })
            state.dirtyTiles.add(key)
          }
        }
      }

      const geometry = state.geometries.get(key)
      if (geometry) {
        applyHeightToGeometry(state, tx, tz, geometry)
      }
    }
  }

  finalizeBrush(state, affected)
  return affected
}

export function applyFlatten(
  state: TerrainHeightState,
  worldX: number,
  worldZ: number,
  radius: number
): AffectedTile[] {
  const affected: AffectedTile[] = []
  const sigma = radius / 2.5

  const minWorldX = worldX - radius
  const maxWorldX = worldX + radius
  const minWorldZ = worldZ - radius
  const maxWorldZ = worldZ + radius

  const minTileX = Math.floor(
    (minWorldX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const maxTileX = Math.floor(
    (maxWorldX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const minTileZ = Math.floor(
    (minWorldZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )
  const maxTileZ = Math.floor(
    (maxWorldZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
  )

  const affectedKeys = new Set<string>()

  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      const key = tileKey(tx, tz)
      const data = state.heightmaps.get(key)
      if (!data) continue

      const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const startCX = Math.max(0, Math.floor(minWorldX - tileMinX))
      const endCX = Math.min(TILE_DIM - 1, Math.floor(maxWorldX - tileMinX))
      const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
      const endCZ = Math.min(TILE_DIM - 1, Math.floor(maxWorldZ - tileMinZ))

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const dx = tileMinX + cx - worldX
          const dz = tileMinZ + cz - worldZ
          const dist = Math.sqrt(dx * dx + dz * dz)
          if (dist > radius) continue

          let nSum = 0
          let nCount = 0
          for (let nz = -1; nz <= 1; nz++) {
            for (let nx = -1; nx <= 1; nx++) {
              if (nx === 0 && nz === 0) continue
              const ncx = cx + nx
              const ncz = cz + nz
              if (ncx >= 0 && ncx < TILE_DIM && ncz >= 0 && ncz < TILE_DIM) {
                nSum += decodeHeight(data[ncz * VERTS_PER_SIDE + ncx])
                nCount++
              }
            }
          }
          if (nCount === 0) continue
          const neighborAvg = nSum / nCount

          const weight = Math.exp(-(dist * dist) / (2 * sigma * sigma))
          const idx = cz * VERTS_PER_SIDE + cx
          const currentHeight = decodeHeight(data[idx])
          const heightDelta = (neighborAvg - currentHeight) * weight

          const steps = Math.trunc(heightDelta / 0.05)
          if (steps === 0) continue
          const newHeight = currentHeight + steps * 0.05
          const newValue = Math.max(0, Math.min(65535, encodeHeight(newHeight)))
          data[idx] = newValue

          const origData = state.originalHeightmaps.get(key)
          if (origData) {
            origData[idx] = newValue
            state.dirtyOriginalTiles.add(key)
          }

          if (!affectedKeys.has(key)) {
            affectedKeys.add(key)
            affected.push({ tileX: tx, tileZ: tz })
            state.dirtyTiles.add(key)
          }
        }
      }

      const geometry = state.geometries.get(key)
      if (geometry) {
        applyHeightToGeometry(state, tx, tz, geometry)
      }
    }
  }

  finalizeBrush(state, affected)
  return affected
}

export function flattenArea(
  state: TerrainHeightState,
  minX: number,
  minZ: number,
  maxX: number,
  maxZ: number,
  targetHeight: number,
  blendRadius: number,
  ensureOriginal: (tileX: number, tileZ: number) => void,
  isProtected?: (worldX: number, worldZ: number) => boolean
): AffectedTile[] {
  const affected: AffectedTile[] = []
  const affectedKeys = new Set<string>()
  const targetEncoded = encodeHeight(targetHeight)

  const expandedMinX = minX - blendRadius
  const expandedMinZ = minZ - blendRadius
  const expandedMaxX = maxX + blendRadius
  const expandedMaxZ = maxZ + blendRadius

  const minTileX = worldToTileCoord(expandedMinX)
  const maxTileX = worldToTileCoord(expandedMaxX)
  const minTileZ = worldToTileCoord(expandedMinZ)
  const maxTileZ = worldToTileCoord(expandedMaxZ)

  // Snapshot original heightmaps before modification
  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      ensureOriginal(tx, tz)
    }
  }

  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      const key = tileKey(tx, tz)
      const data = state.heightmaps.get(key)
      if (!data) continue

      const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

      const startCX = Math.max(0, Math.floor(expandedMinX - tileMinX))
      const endCX = Math.min(TILE_DIM - 1, Math.floor(expandedMaxX - tileMinX))
      const startCZ = Math.max(0, Math.floor(expandedMinZ - tileMinZ))
      const endCZ = Math.min(TILE_DIM - 1, Math.floor(expandedMaxZ - tileMinZ))

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const worldCX = tileMinX + cx
          const worldCZ = tileMinZ + cz

          const dx = Math.max(minX - worldCX, 0, worldCX - maxX)
          const dz = Math.max(minZ - worldCZ, 0, worldCZ - maxZ)
          const distFromEdge = Math.sqrt(dx * dx + dz * dz)

          const idx = cz * VERTS_PER_SIDE + cx

          if (isProtected && isProtected(worldCX, worldCZ)) continue

          if (distFromEdge <= 0) {
            data[idx] = Math.max(0, Math.min(65535, targetEncoded))
          } else if (distFromEdge < blendRadius) {
            const t = distFromEdge / blendRadius
            const blend = 1 - t * t * (3 - 2 * t)
            const currentHeight = decodeHeight(data[idx])
            const newHeight =
              currentHeight + (targetHeight - currentHeight) * blend
            const newValue = Math.max(
              0,
              Math.min(65535, encodeHeight(newHeight))
            )
            data[idx] = newValue
          } else {
            continue
          }

          if (!affectedKeys.has(key)) {
            affectedKeys.add(key)
            affected.push({ tileX: tx, tileZ: tz })
            state.dirtyTiles.add(key)
          }
        }
      }

      const geometry = state.geometries.get(key)
      if (geometry) {
        applyHeightToGeometry(state, tx, tz, geometry)
      }
    }
  }

  finalizeBrush(state, affected)
  return affected
}

export function restoreFromOriginal(
  state: TerrainHeightState,
  minX: number,
  minZ: number,
  maxX: number,
  maxZ: number
): AffectedTile[] {
  const affected: AffectedTile[] = []

  const minTileX = worldToTileCoord(minX)
  const maxTileX = worldToTileCoord(maxX)
  const minTileZ = worldToTileCoord(minZ)
  const maxTileZ = worldToTileCoord(maxZ)

  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      const key = tileKey(tx, tz)
      const original = state.originalHeightmaps.get(key)
      const current = state.heightmaps.get(key)
      if (!original || !current) continue

      const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

      const startCX = Math.max(0, Math.floor(minX - tileMinX))
      const endCX = Math.min(TILE_DIM, Math.floor(maxX - tileMinX))
      const startCZ = Math.max(0, Math.floor(minZ - tileMinZ))
      const endCZ = Math.min(TILE_DIM, Math.floor(maxZ - tileMinZ))

      let changed = false
      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const idx = cz * VERTS_PER_SIDE + cx
          if (current[idx] !== original[idx]) {
            current[idx] = original[idx]
            changed = true
          }
        }
      }

      if (changed) {
        affected.push({ tileX: tx, tileZ: tz })
        state.dirtyTiles.add(key)
        const geometry = state.geometries.get(key)
        if (geometry) {
          applyHeightToGeometry(state, tx, tz, geometry)
        }
      }
    }
  }

  finalizeBrush(state, affected)
  return affected
}
