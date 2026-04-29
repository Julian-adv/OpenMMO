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
import { smoothstep } from '../terrain/terrain-constants'
import { rotatedRectAabb } from '../utils/objectFootprint'

function finalizeBrush(
  state: TerrainHeightState,
  affected: AffectedTile[]
): void {
  for (const { tileX: tx, tileZ: tz } of affected) {
    refreshAdjacentTileEdges(state, tx, tz)
  }
}

function sampleNeighborHeight(
  state: TerrainHeightState,
  tx: number,
  tz: number,
  cx: number,
  cz: number
): number | null {
  let ntx = tx
  let ntz = tz
  let ncx = cx
  let ncz = cz
  if (ncx < 0) {
    ntx -= 1
    ncx += TILE_DIM
  } else if (ncx >= VERTS_PER_SIDE) {
    ntx += 1
    ncx -= TILE_DIM
  }
  if (ncz < 0) {
    ntz -= 1
    ncz += TILE_DIM
  } else if (ncz >= VERTS_PER_SIDE) {
    ntz += 1
    ncz -= TILE_DIM
  }
  const data = state.heightmaps.get(tileKey(ntx, ntz))
  if (!data) return null
  return decodeHeight(data[ncz * VERTS_PER_SIDE + ncx])
}

export function applyBrush(
  state: TerrainHeightState,
  worldX: number,
  worldZ: number,
  radius: number,
  strengthPerSec: number,
  raise: boolean,
  deltaTimeSec: number,
  isProtected?: (worldX: number, worldZ: number) => boolean
): AffectedTile[] {
  const affected: AffectedTile[] = []
  const delta = strengthPerSec * deltaTimeSec * (raise ? 1 : -1)
  const plateauR = radius * 0.4

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
      const endCX = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldX - tileMinX)
      )
      const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
      const endCZ = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldZ - tileMinZ)
      )

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const vertexWorldX = tileMinX + cx
          const vertexWorldZ = tileMinZ + cz

          const dx = vertexWorldX - worldX
          const dz = vertexWorldZ - worldZ
          const dist = Math.sqrt(dx * dx + dz * dz)

          if (dist > radius) continue
          if (isProtected && isProtected(vertexWorldX, vertexWorldZ)) continue

          const weight = 1 - smoothstep(plateauR, radius, dist)
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
  radius: number,
  isProtected?: (worldX: number, worldZ: number) => boolean
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
      const endCX = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldX - tileMinX)
      )
      const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
      const endCZ = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldZ - tileMinZ)
      )

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const wx = tileMinX + cx
          const wz = tileMinZ + cz
          const dx = wx - worldX
          const dz = wz - worldZ
          const dist = Math.sqrt(dx * dx + dz * dz)
          if (dist > radius) continue
          if (isProtected && isProtected(wx, wz)) continue

          let nSum = 0
          let nCount = 0
          for (let nz = -1; nz <= 1; nz++) {
            for (let nx = -1; nx <= 1; nx++) {
              if (nx === 0 && nz === 0) continue
              const ncx = cx + nx
              const ncz = cz + nz
              const inBounds =
                ncx >= 0 &&
                ncz >= 0 &&
                ncx < VERTS_PER_SIDE &&
                ncz < VERTS_PER_SIDE
              if (inBounds) {
                nSum += decodeHeight(data[ncz * VERTS_PER_SIDE + ncx])
                nCount++
              } else {
                const h = sampleNeighborHeight(state, tx, tz, ncx, ncz)
                if (h !== null) {
                  nSum += h
                  nCount++
                }
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

export function applyFlattenLine(
  state: TerrainHeightState,
  x1: number,
  z1: number,
  x2: number,
  z2: number,
  radius: number,
  isProtected?: (worldX: number, worldZ: number) => boolean
): AffectedTile[] {
  const affected: AffectedTile[] = []

  const lineDx = x2 - x1
  const lineDz = z2 - z1
  const lenSq = lineDx * lineDx + lineDz * lineDz
  if (lenSq < 1e-6) return affected
  const lineLen = Math.sqrt(lenSq)

  const perpX = -lineDz / lineLen
  const perpZ = lineDx / lineLen

  // Per-t target is the mean of the cross-section, so length-wise variation
  // is preserved while the left/right edges converge to the local mean.
  const numSamples = Math.max(2, Math.ceil(lineLen) + 1)
  const targets = new Float32Array(numSamples)
  for (let i = 0; i < numSamples; i++) {
    const tt = i / (numSamples - 1)
    const sx = x1 + lineDx * tt
    const sz = z1 + lineDz * tt
    let sum = 0
    let cnt = 0
    for (let d = -radius; d <= radius + 1e-6; d += 1) {
      const h = sampleHeightAtWorld(state, sx + perpX * d, sz + perpZ * d)
      if (h !== null) {
        sum += h
        cnt++
      }
    }
    if (cnt === 0) return affected
    targets[i] = sum / cnt
  }

  // Road core flattens mostly (not fully) toward the target so subtle
  // original terrain variation remains, then eases out over a blend skirt.
  const coreBlend = 0.5
  const blendRadius = radius * 2
  const minWorldX = Math.min(x1, x2) - blendRadius
  const maxWorldX = Math.max(x1, x2) + blendRadius
  const minWorldZ = Math.min(z1, z2) - blendRadius
  const maxWorldZ = Math.max(z1, z2) + blendRadius

  const minTileX = worldToTileCoord(minWorldX)
  const maxTileX = worldToTileCoord(maxWorldX)
  const minTileZ = worldToTileCoord(minWorldZ)
  const maxTileZ = worldToTileCoord(maxWorldZ)

  const affectedKeys = new Set<string>()

  for (let tz = minTileZ; tz <= maxTileZ; tz++) {
    for (let tx = minTileX; tx <= maxTileX; tx++) {
      const key = tileKey(tx, tz)
      const data = state.heightmaps.get(key)
      if (!data) continue

      const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
      const startCX = Math.max(0, Math.floor(minWorldX - tileMinX))
      const endCX = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldX - tileMinX)
      )
      const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
      const endCZ = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(maxWorldZ - tileMinZ)
      )

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const wx = tileMinX + cx
          const wz = tileMinZ + cz

          // Closest point on segment
          const vx = wx - x1
          const vz = wz - z1
          let t = (vx * lineDx + vz * lineDz) / lenSq
          if (t < 0) t = 0
          else if (t > 1) t = 1
          const ddx = wx - (x1 + t * lineDx)
          const ddz = wz - (z1 + t * lineDz)
          const dist = Math.sqrt(ddx * ddx + ddz * ddz)
          if (dist > blendRadius) continue
          if (isProtected && isProtected(wx, wz)) continue

          const ti = Math.round(t * (numSamples - 1))
          const target = targets[ti]
          const blend = coreBlend * (1 - smoothstep(radius, blendRadius, dist))

          const idx = cz * VERTS_PER_SIDE + cx
          const currentHeight = decodeHeight(data[idx])
          const newHeight = currentHeight + (target - currentHeight) * blend
          const newValue = Math.max(0, Math.min(65535, encodeHeight(newHeight)))
          if (newValue === data[idx]) continue
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

function sampleHeightAtWorld(
  state: TerrainHeightState,
  worldX: number,
  worldZ: number
): number | null {
  const tileX = worldToTileCoord(worldX)
  const tileZ = worldToTileCoord(worldZ)
  const data = state.heightmaps.get(tileKey(tileX, tileZ))
  if (!data) return null
  const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
  const cx = Math.max(0, Math.min(TILE_DIM - 1, Math.floor(worldX - tileMinX)))
  const cz = Math.max(0, Math.min(TILE_DIM - 1, Math.floor(worldZ - tileMinZ)))
  return decodeHeight(data[cz * VERTS_PER_SIDE + cx])
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
      const endCX = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(expandedMaxX - tileMinX)
      )
      const startCZ = Math.max(0, Math.floor(expandedMinZ - tileMinZ))
      const endCZ = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(expandedMaxZ - tileMinZ)
      )

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

/**
 * Flatten a rectangle that's rotated around (centerX, centerZ). Same blend
 * behaviour as flattenArea, but distance is measured to the rotated rect's
 * edges (not its world AABB), so a 45° footprint doesn't bleed terrain into
 * the AABB corners that fall outside the actual rect.
 */
export function flattenRotatedRect(
  state: TerrainHeightState,
  centerX: number,
  centerZ: number,
  rotationDeg: number,
  localMinX: number,
  localMaxX: number,
  localMinZ: number,
  localMaxZ: number,
  targetHeight: number,
  blendRadius: number,
  ensureOriginal: (tileX: number, tileZ: number) => void,
  isProtected?: (worldX: number, worldZ: number) => boolean
): AffectedTile[] {
  const affected: AffectedTile[] = []
  const affectedKeys = new Set<string>()
  const targetEncoded = encodeHeight(targetHeight)

  const rot = (rotationDeg * Math.PI) / 180
  const cos = Math.cos(rot)
  const sin = Math.sin(rot)

  const aabb = rotatedRectAabb(localMinX, localMaxX, localMinZ, localMaxZ, rot)
  const expandedMinX = centerX + aabb.minX - blendRadius
  const expandedMinZ = centerZ + aabb.minZ - blendRadius
  const expandedMaxX = centerX + aabb.maxX + blendRadius
  const expandedMaxZ = centerZ + aabb.maxZ + blendRadius

  const minTileX = worldToTileCoord(expandedMinX)
  const maxTileX = worldToTileCoord(expandedMaxX)
  const minTileZ = worldToTileCoord(expandedMinZ)
  const maxTileZ = worldToTileCoord(expandedMaxZ)

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
      const endCX = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(expandedMaxX - tileMinX)
      )
      const startCZ = Math.max(0, Math.floor(expandedMinZ - tileMinZ))
      const endCZ = Math.min(
        VERTS_PER_SIDE - 1,
        Math.floor(expandedMaxZ - tileMinZ)
      )

      for (let cz = startCZ; cz <= endCZ; cz++) {
        for (let cx = startCX; cx <= endCX; cx++) {
          const worldCX = tileMinX + cx
          const worldCZ = tileMinZ + cz

          // World → local: lx = dx*cos - dz*sin, lz = dx*sin + dz*cos
          const dx = worldCX - centerX
          const dz = worldCZ - centerZ
          const lx = dx * cos - dz * sin
          const lz = dx * sin + dz * cos

          const ddx = Math.max(localMinX - lx, 0, lx - localMaxX)
          const ddz = Math.max(localMinZ - lz, 0, lz - localMaxZ)
          const distFromEdge = Math.sqrt(ddx * ddx + ddz * ddz)

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
