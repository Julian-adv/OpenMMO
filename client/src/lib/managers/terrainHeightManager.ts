import * as THREE from 'three'
import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  TERRAIN_TILE_SIZE,
  SEA_LEVEL_ENCODED,
} from '../components/game-scene/terrain-utils'

const TILE_DIM = 64
const VERTS_PER_SIDE = TILE_DIM + 1 // 65 vertices per axis
const PADDED_SIDE = VERTS_PER_SIDE + 2 // 67 — padded grid for analytical normals
const _paddedHeights = new Float32Array(PADDED_SIDE * PADDED_SIDE) // reusable buffer

function tileKey(tileX: number, tileZ: number): string {
  return `${tileX},${tileZ}`
}

function encodeHeight(meters: number): number {
  return Math.round((meters + 500.0) / 0.05)
}

function decodeHeight(value: number): number {
  return value * 0.05 - 500.0
}

function worldToTileCoord(worldCoord: number): number {
  return Math.floor((worldCoord + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE)
}

export interface AffectedTile {
  tileX: number
  tileZ: number
}

export type HeightChangedCallback = (tiles: AffectedTile[]) => void

export class TerrainHeightManager {
  private heightmaps = new Map<string, Uint16Array>()
  private inflightHeightmaps = new Map<string, Promise<Uint16Array>>()
  private geometries = new Map<string, THREE.BufferGeometry>()
  private dirtyTiles = new Set<string>()
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private terrainApiUrl: string
  private heightChangedListeners: HeightChangedCallback[] = []

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  onHeightChanged(cb: HeightChangedCallback): () => void {
    this.heightChangedListeners.push(cb)
    return () => {
      this.heightChangedListeners = this.heightChangedListeners.filter(
        (l) => l !== cb
      )
    }
  }

  private notifyHeightChanged(tiles: AffectedTile[]) {
    for (const cb of this.heightChangedListeners) cb(tiles)
  }

  async loadHeightmap(tileX: number, tileZ: number): Promise<Uint16Array> {
    const key = tileKey(tileX, tileZ)
    const cached = this.heightmaps.get(key)
    if (cached) return cached

    // Deduplicate in-flight requests
    const inflight = this.inflightHeightmaps.get(key)
    if (inflight) return inflight

    const promise = (async () => {
      const url = `${this.terrainApiUrl}/api/terrain/height/${tileX}/${tileZ}`
      const response = await fetch(url)
      const buffer = await response.arrayBuffer()
      const data = new Uint16Array(buffer)
      this.heightmaps.set(key, data)
      this.inflightHeightmaps.delete(key)
      return data
    })()
    this.inflightHeightmaps.set(key, promise)
    return promise
  }

  getHeightmap(tileX: number, tileZ: number): Uint16Array | undefined {
    return this.heightmaps.get(tileKey(tileX, tileZ))
  }

  getHeightAtCell(
    tileX: number,
    tileZ: number,
    cellX: number,
    cellZ: number
  ): number {
    // Heightmap stores 65×65 vertices (0-64). Cross-tile only for padding positions.
    if (cellX >= VERTS_PER_SIDE) {
      return this.getHeightAtCell(tileX + 1, tileZ, cellX - TILE_DIM, cellZ)
    }
    if (cellZ >= VERTS_PER_SIDE) {
      return this.getHeightAtCell(tileX, tileZ + 1, cellX, cellZ - TILE_DIM)
    }
    if (cellX < 0) {
      return this.getHeightAtCell(tileX - 1, tileZ, cellX + TILE_DIM, cellZ)
    }
    if (cellZ < 0) {
      return this.getHeightAtCell(tileX, tileZ - 1, cellX, cellZ + TILE_DIM)
    }

    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return 0
    return decodeHeight(data[cellZ * VERTS_PER_SIDE + cellX])
  }

  hasHeightData(worldX: number, worldZ: number): boolean {
    return this.heightmaps.has(
      tileKey(worldToTileCoord(worldX), worldToTileCoord(worldZ))
    )
  }

  hasHeightDataForGrid(worldX: number, worldZ: number, radius = 1): boolean {
    const centerTileX = worldToTileCoord(worldX)
    const centerTileZ = worldToTileCoord(worldZ)
    for (let dz = -radius; dz <= radius; dz++) {
      for (let dx = -radius; dx <= radius; dx++) {
        if (!this.heightmaps.has(tileKey(centerTileX + dx, centerTileZ + dz))) {
          return false
        }
      }
    }
    return true
  }

  getHeightAtWorldPosition(worldX: number, worldZ: number): number {
    const tileX = worldToTileCoord(worldX)
    const tileZ = worldToTileCoord(worldZ)
    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const localX = worldX - tileMinX
    const localZ = worldZ - tileMinZ
    const cellX = Math.floor(localX)
    const cellZ = Math.floor(localZ)
    const fracX = localX - cellX
    const fracZ = localZ - cellZ

    const h00 = this.getHeightAtCell(tileX, tileZ, cellX, cellZ)
    const h10 = this.getHeightAtCell(tileX, tileZ, cellX + 1, cellZ)
    const h01 = this.getHeightAtCell(tileX, tileZ, cellX, cellZ + 1)
    const h11 = this.getHeightAtCell(tileX, tileZ, cellX + 1, cellZ + 1)

    const h0 = h00 + (h10 - h00) * fracX
    const h1 = h01 + (h11 - h01) * fracX
    return h0 + (h1 - h0) * fracZ
  }

  registerGeometry(
    tileX: number,
    tileZ: number,
    geometry: THREE.BufferGeometry
  ) {
    this.geometries.set(tileKey(tileX, tileZ), geometry)
  }

  unregisterGeometry(tileX: number, tileZ: number) {
    this.geometries.delete(tileKey(tileX, tileZ))
  }

  applyHeightToGeometry(
    tileX: number,
    tileZ: number,
    geometry: THREE.BufferGeometry
  ) {
    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return

    const posAttr = geometry.getAttribute('position') as THREE.BufferAttribute
    const positions = posAttr.array as Float32Array
    const normalAttr = geometry.getAttribute('normal') as THREE.BufferAttribute
    const normals = normalAttr.array as Float32Array

    // Reuse padded height grid (67×67) to avoid per-call allocation.
    // Rows/cols 0 and 66 come from neighbor tiles; 1-65 are this tile's data.
    const P = PADDED_SIDE
    const heights = _paddedHeights

    // Fill 65×65 vertices directly from heightmap data (includes edge vertices)
    for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
      const srcRow = vz * VERTS_PER_SIDE
      const dstRow = (vz + 1) * P + 1
      for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
        heights[dstRow + vx] = decodeHeight(data[srcRow + vx])
      }
    }

    // Padding edges for normal computation at boundaries
    for (let i = 0; i < VERTS_PER_SIDE; i++) {
      heights[(i + 1) * P] = this.getHeightAtCell(tileX, tileZ, -1, i) // left padding
      heights[(i + 1) * P + (P - 1)] = this.getHeightAtCell(
        tileX,
        tileZ,
        VERTS_PER_SIDE,
        i
      ) // right padding
      heights[i + 1] = this.getHeightAtCell(tileX, tileZ, i, -1) // top padding
      heights[(P - 1) * P + (i + 1)] = this.getHeightAtCell(
        tileX,
        tileZ,
        i,
        VERTS_PER_SIDE
      ) // bottom padding
    }
    // Four padding corners
    heights[0] = this.getHeightAtCell(tileX, tileZ, -1, -1)
    heights[P - 1] = this.getHeightAtCell(tileX, tileZ, VERTS_PER_SIDE, -1)
    heights[(P - 1) * P] = this.getHeightAtCell(
      tileX,
      tileZ,
      -1,
      VERTS_PER_SIDE
    )
    heights[(P - 1) * P + (P - 1)] = this.getHeightAtCell(
      tileX,
      tileZ,
      VERTS_PER_SIDE,
      VERTS_PER_SIDE
    )

    // Set positions and compute analytical normals via central differences
    for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
      for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
        const vertexIndex = vz * VERTS_PER_SIDE + vx
        const pi = (vz + 1) * P + (vx + 1) // index into padded grid

        const h = heights[pi]
        positions[vertexIndex * 3 + 1] = h

        // Central differences (cell spacing = 1.0)
        const dhdx = heights[pi + 1] - heights[pi - 1] // right - left
        const dhdz = heights[pi + P] - heights[pi - P] // forward - back

        // normal = normalize(-dhdx, 2, -dhdz)
        const nx = -dhdx
        const ny = 2.0
        const nz = -dhdz
        const invLen = 1.0 / Math.sqrt(nx * nx + ny * ny + nz * nz)
        normals[vertexIndex * 3] = nx * invLen
        normals[vertexIndex * 3 + 1] = ny * invLen
        normals[vertexIndex * 3 + 2] = nz * invLen
      }
    }

    posAttr.needsUpdate = true
    normalAttr.needsUpdate = true
  }

  /** Sync overlapping edge vertices to neighbor tiles' data, then refresh their geometry.
   *  This tile's column 0 → neighbor(tileX-1)'s column 64
   *  This tile's row 0 → neighbor(tileZ-1)'s row 64
   *  This tile's vertex(0,0) → neighbor(tileX-1, tileZ-1)'s vertex(64,64) */
  refreshAdjacentTileEdges(tileX: number, tileZ: number) {
    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return

    // Sync overlapping edge data to neighbors
    const leftData = this.heightmaps.get(tileKey(tileX - 1, tileZ))
    if (leftData) {
      for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
        leftData[vz * VERTS_PER_SIDE + TILE_DIM] = data[vz * VERTS_PER_SIDE + 0]
      }
    }

    const topData = this.heightmaps.get(tileKey(tileX, tileZ - 1))
    if (topData) {
      for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
        topData[TILE_DIM * VERTS_PER_SIDE + vx] = data[0 * VERTS_PER_SIDE + vx]
      }
    }

    const diagData = this.heightmaps.get(tileKey(tileX - 1, tileZ - 1))
    if (diagData) {
      diagData[TILE_DIM * VERTS_PER_SIDE + TILE_DIM] = data[0]
    }

    // Re-apply geometry for neighbors whose data was updated
    const neighbors = [
      { dx: -1, dz: 0 },
      { dx: 0, dz: -1 },
      { dx: -1, dz: -1 },
    ]
    for (const { dx, dz } of neighbors) {
      const nx = tileX + dx
      const nz = tileZ + dz
      const key = tileKey(nx, nz)
      const geo = this.geometries.get(key)
      if (geo && this.heightmaps.has(key)) {
        this.applyHeightToGeometry(nx, nz, geo)
      }
    }
  }

  applyBrush(
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

    // Determine which cells to iterate (world-space bounding box of brush)
    const minWorldX = worldX - radius
    const maxWorldX = worldX + radius
    const minWorldZ = worldZ - radius
    const maxWorldZ = worldZ + radius

    // Convert to tile/cell ranges
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
        const data = this.heightmaps.get(key)
        if (!data) continue

        const tileOriginX = tx * TERRAIN_TILE_SIZE
        const tileOriginZ = tz * TERRAIN_TILE_SIZE
        const tileMinX = tileOriginX - TERRAIN_TILE_SIZE / 2
        const tileMinZ = tileOriginZ - TERRAIN_TILE_SIZE / 2

        // Only iterate cells within the brush bounding box
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

            // Gaussian falloff
            const weight = Math.exp(-(dist * dist) / (2 * sigma * sigma))
            const heightDelta = delta * weight

            const idx = cz * VERTS_PER_SIDE + cx
            const currentHeight = decodeHeight(data[idx])
            // Quantize delta to 0.05m steps (1 uint16 unit)
            const steps = Math.trunc(heightDelta / 0.05)
            if (steps === 0) continue
            const newHeight = currentHeight + steps * 0.05
            const newValue = Math.max(
              0,
              Math.min(65535, encodeHeight(newHeight))
            )
            data[idx] = newValue

            if (!affectedKeys.has(key)) {
              affectedKeys.add(key)
              affected.push({ tileX: tx, tileZ: tz })
              this.dirtyTiles.add(key)
            }
          }
        }

        // Update geometry for this tile
        const geometry = this.geometries.get(key)
        if (geometry) {
          this.applyHeightToGeometry(tx, tz, geometry)
        }
      }
    }

    // Refresh edge vertices of adjacent tiles that reference modified tiles' data
    for (const { tileX: tx, tileZ: tz } of affected) {
      this.refreshAdjacentTileEdges(tx, tz)
    }

    if (affected.length > 0) {
      this.scheduleSave()
      this.notifyHeightChanged(affected)
    }

    return affected
  }

  applyFlatten(worldX: number, worldZ: number, radius: number): AffectedTile[] {
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

    // Box filter smooth: each cell blends toward the average of its 3x3 neighbors
    const affectedKeys = new Set<string>()

    for (let tz = minTileZ; tz <= maxTileZ; tz++) {
      for (let tx = minTileX; tx <= maxTileX; tx++) {
        const key = tileKey(tx, tz)
        const data = this.heightmaps.get(key)
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

            // Average of 8 surrounding neighbors (excluding self)
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
            const newValue = Math.max(
              0,
              Math.min(65535, encodeHeight(newHeight))
            )
            data[idx] = newValue

            if (!affectedKeys.has(key)) {
              affectedKeys.add(key)
              affected.push({ tileX: tx, tileZ: tz })
              this.dirtyTiles.add(key)
            }
          }
        }

        const geometry = this.geometries.get(key)
        if (geometry) {
          this.applyHeightToGeometry(tx, tz, geometry)
        }
      }
    }

    // Refresh edge vertices of adjacent tiles that reference modified tiles' data
    for (const { tileX: tx, tileZ: tz } of affected) {
      this.refreshAdjacentTileEdges(tx, tz)
    }

    if (affected.length > 0) {
      this.scheduleSave()
      this.notifyHeightChanged(affected)
    }

    return affected
  }

  private scheduleSave() {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer)
    }
    this.saveTimer = setTimeout(() => {
      this.saveDirtyTiles()
      this.saveTimer = null
    }, 1000)
  }

  private async saveDirtyTiles() {
    const tilesToSave = [...this.dirtyTiles]
    this.dirtyTiles.clear()

    for (const key of tilesToSave) {
      const data = this.heightmaps.get(key)
      if (!data) continue

      const [txStr, tzStr] = key.split(',')
      const tx = parseInt(txStr)
      const tz = parseInt(tzStr)

      const url = `${this.terrainApiUrl}/api/terrain/height/${tx}/${tz}`
      const body = (data.buffer as ArrayBuffer).slice(
        data.byteOffset,
        data.byteOffset + data.byteLength
      )

      try {
        await fetch(url, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/octet-stream' },
          body,
        })
      } catch (e) {
        console.error(`Failed to save heightmap for tile (${tx}, ${tz}):`, e)
        // Re-mark as dirty for retry
        this.dirtyTiles.add(key)
      }
    }
  }

  hasWater(tileX: number, tileZ: number): boolean {
    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return false
    for (let i = 0; i < data.length; i++) {
      if (data[i] < SEA_LEVEL_ENCODED) return true
    }
    return false
  }

  getHeightmapTexture(tileX: number, tileZ: number): THREE.DataTexture | null {
    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return null

    // Heightmap is already 65×65 — decode directly
    const W = VERTS_PER_SIDE
    const decoded = new Float32Array(W * W)
    for (let i = 0; i < W * W; i++) {
      decoded[i] = decodeHeight(data[i])
    }

    const tex = new THREE.DataTexture(
      decoded,
      W,
      W,
      THREE.RedFormat,
      THREE.FloatType
    )
    tex.flipY = true
    tex.minFilter = THREE.LinearFilter
    tex.magFilter = THREE.LinearFilter
    tex.needsUpdate = true
    return tex
  }

  /** Update an existing DataTexture in-place with current heightmap data.
   *  Returns true if the texture was updated, false if no heightmap data exists. */
  updateHeightmapTexture(
    tileX: number,
    tileZ: number,
    tex: THREE.DataTexture
  ): boolean {
    const data = this.heightmaps.get(tileKey(tileX, tileZ))
    if (!data) return false

    const W = VERTS_PER_SIDE
    const buf = tex.image.data as Float32Array
    for (let i = 0; i < W * W; i++) {
      buf[i] = decodeHeight(data[i])
    }
    tex.needsUpdate = true
    return true
  }

  /** Re-apply height to geometry if the tile has a registered geometry. */
  refreshTileGeometry(tileX: number, tileZ: number): void {
    const key = tileKey(tileX, tileZ)
    const geo = this.geometries.get(key)
    if (geo && this.heightmaps.has(key)) {
      this.applyHeightToGeometry(tileX, tileZ, geo)
    }
  }

  /** Directly set heightmap data for a tile (used by terrain generator). */
  setHeightmap(tileX: number, tileZ: number, data: Uint16Array): void {
    this.heightmaps.set(tileKey(tileX, tileZ), data)
  }

  /** Mark a tile as dirty so it will be saved on next save cycle. */
  markDirty(tileX: number, tileZ: number): void {
    this.dirtyTiles.add(tileKey(tileX, tileZ))
  }

  /** Force-save all dirty tiles immediately (cancels pending debounce). */
  async saveAllDirty(): Promise<void> {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    await this.saveDirtyTiles()
  }

  unloadTile(tileX: number, tileZ: number) {
    const key = tileKey(tileX, tileZ)
    this.heightmaps.delete(key)
    this.geometries.delete(key)
  }

  destroy() {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer)
    }
    // Save any remaining dirty tiles synchronously-ish
    if (this.dirtyTiles.size > 0) {
      this.saveDirtyTiles()
    }
  }
}
