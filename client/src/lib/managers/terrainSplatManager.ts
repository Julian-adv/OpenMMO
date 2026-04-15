import * as THREE from 'three'
import { getTerrainApiUrl } from '../utils/networkUtils'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'
import { worldToTileCoord } from './terrain-height-types'
import { smoothstep } from '../terrain/terrain-constants'
import {
  BYTES_PER_CELL,
  applyBrush,
  readCell,
  writeCell,
} from '../terrain/splat-encoding'

const TILE_DIM = 64

function tileKey(tileX: number, tileZ: number): string {
  return `${tileX},${tileZ}`
}

function paintCell(
  data: Uint8Array,
  cellIdx: number,
  paintIdx: number,
  strength: number
): boolean {
  if (strength <= 0) return false
  const before = readCell(data, cellIdx)
  const after = applyBrush(before, paintIdx, strength)
  if (
    before.primaryIdx === after.primaryIdx &&
    before.secondaryIdx === after.secondaryIdx &&
    before.blend === after.blend
  ) {
    return false
  }
  writeCell(data, cellIdx, after)
  return true
}

// Cells adjacent to a painted cell need paintIdx in one of their palette slots
// so the shader's weight-space bilerp has a matching P/S pair across the
// boundary. Without this, `cellWeight` collapses for cells that have no
// matching slot, and when `nearestBlend` flips between cells with disjoint
// palettes the rendered mix jumps — a hard edge.
//
// A slot is considered "free" when its bilerp contribution is small enough
// (≤10%) that overwriting it with paintIdx produces at most a subtle tint
// shift but unlocks smooth blending across the road boundary. Genuinely
// mixed cells (both slots contributing >10%) are preserved and may still
// show a hard edge — the tradeoff favors user-painted mixes.
const FRINGE_REMAP_THRESHOLD = 25 // ≈10% of 255
function paintCellFringe(
  data: Uint8Array,
  cellIdx: number,
  paintIdx: number
): boolean {
  const base = cellIdx * BYTES_PER_CELL
  const indices = data[base]
  const primary = (indices >> 4) & 0x0f
  const secondary = indices & 0x0f
  if (primary === paintIdx || secondary === paintIdx) return false
  const blend = data[base + 2]
  if (primary === secondary || blend <= FRINGE_REMAP_THRESHOLD) {
    data[base] = (primary << 4) | (paintIdx & 0x0f)
    return true
  }
  if (blend >= 255 - FRINGE_REMAP_THRESHOLD) {
    data[base] = ((paintIdx & 0x0f) << 4) | secondary
    return true
  }
  return false
}

// Covers 8-connected neighbors (max diagonal distance √2, rounded up).
const FRINGE_PAD = 1.5

export class TerrainSplatManager {
  private splatmaps = new Map<string, Uint8Array>()
  private inflightSplatmaps = new Map<string, Promise<THREE.DataTexture>>()
  private textures = new Map<string, THREE.DataTexture>()
  private dirtyTiles = new Set<string>()
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private terrainApiUrl: string

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  private createTexture(data: Uint8Array): THREE.DataTexture {
    const texture = new THREE.DataTexture(
      data,
      TILE_DIM,
      TILE_DIM,
      THREE.RGBAFormat,
      THREE.UnsignedByteType
    )
    texture.wrapS = texture.wrapT = THREE.ClampToEdgeWrapping
    // V2 bytes 0/1/3 are integer fields — must not be bilinearly interpolated.
    texture.minFilter = THREE.NearestFilter
    texture.magFilter = THREE.NearestFilter
    texture.generateMipmaps = false
    // PlaneGeometry UV v=0 is maxZ, v=1 is minZ (v decreases with Z).
    // flipY=true so data row 0 maps to v=1 (minZ), matching cz=0 = minZ.
    texture.flipY = true
    texture.needsUpdate = true
    return texture
  }

  async loadSplatmap(tileX: number, tileZ: number): Promise<THREE.DataTexture> {
    const key = tileKey(tileX, tileZ)
    const cached = this.textures.get(key)
    if (cached) return cached

    const inflight = this.inflightSplatmaps.get(key)
    if (inflight) return inflight

    const defaultFallback = (): THREE.DataTexture => {
      // V2 default: all zeros → every cell is 100% palette slot 0.
      const data = new Uint8Array(TILE_DIM * TILE_DIM * BYTES_PER_CELL)
      this.splatmaps.set(key, data)
      const texture = this.createTexture(data)
      this.textures.set(key, texture)
      return texture
    }

    const promise = (async () => {
      try {
        const url = `${this.terrainApiUrl}/api/terrain/splat/${tileX}/${tileZ}`
        const response = await fetch(url)
        if (!response.ok) {
          console.error(
            `Failed to load splatmap (${tileX}, ${tileZ}): ${response.status}`
          )
          return defaultFallback()
        }
        const buffer = await response.arrayBuffer()
        const data = new Uint8Array(buffer)
        this.splatmaps.set(key, data)
        const texture = this.createTexture(data)
        this.textures.set(key, texture)
        return texture
      } catch (e) {
        console.error(`Splatmap fetch error (${tileX}, ${tileZ}):`, e)
        return defaultFallback()
      } finally {
        this.inflightSplatmaps.delete(key)
      }
    })()
    this.inflightSplatmaps.set(key, promise)
    return promise
  }

  getSplatTexture(tileX: number, tileZ: number): THREE.DataTexture | null {
    return this.textures.get(tileKey(tileX, tileZ)) ?? null
  }

  /** Get raw splatmap bytes for a tile (64×64×4 Uint8Array, V2 encoding), or null if not loaded. */
  getSplatData(tileX: number, tileZ: number): Uint8Array | null {
    return this.splatmaps.get(tileKey(tileX, tileZ)) ?? null
  }

  /** Apply splat brush and return the list of tiles that were modified. */
  applySplatBrush(
    worldX: number,
    worldZ: number,
    radius: number,
    paintIdx: number,
    strength: number
  ): { tileX: number; tileZ: number }[] {
    const sigma = radius / 2.5
    const outerR = radius + FRINGE_PAD

    const minWorldX = worldX - outerR
    const maxWorldX = worldX + outerR
    const minWorldZ = worldZ - outerR
    const maxWorldZ = worldZ + outerR

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

    const affectedTiles: { tileX: number; tileZ: number }[] = []

    for (let tz = minTileZ; tz <= maxTileZ; tz++) {
      for (let tx = minTileX; tx <= maxTileX; tx++) {
        const key = tileKey(tx, tz)
        const data = this.splatmaps.get(key)
        if (!data) continue

        const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
        const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

        const startCX = Math.max(0, Math.floor(minWorldX - tileMinX))
        const endCX = Math.min(TILE_DIM - 1, Math.floor(maxWorldX - tileMinX))
        const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
        const endCZ = Math.min(TILE_DIM - 1, Math.floor(maxWorldZ - tileMinZ))

        let changed = false

        for (let cz = startCZ; cz <= endCZ; cz++) {
          for (let cx = startCX; cx <= endCX; cx++) {
            const dx = tileMinX + cx - worldX
            const dz = tileMinZ + cz - worldZ
            const dist = Math.sqrt(dx * dx + dz * dz)
            if (dist > outerR) continue

            const cellIdx = cz * TILE_DIM + cx
            if (dist > radius) {
              if (paintCellFringe(data, cellIdx, paintIdx)) changed = true
            } else {
              const weight = Math.exp(-(dist * dist) / (2 * sigma * sigma))
              if (paintCell(data, cellIdx, paintIdx, weight * strength)) {
                changed = true
              }
            }
          }
        }

        if (changed) {
          const texture = this.textures.get(key)
          if (texture) texture.needsUpdate = true
          this.dirtyTiles.add(key)
          affectedTiles.push({ tileX: tx, tileZ: tz })
        }
      }
    }

    if (this.dirtyTiles.size > 0) this.scheduleSave()
    return affectedTiles
  }

  /** Apply splat brush along a line segment (used by the road tool). */
  applySplatLine(
    x1: number,
    z1: number,
    x2: number,
    z2: number,
    radius: number,
    paintIdx: number,
    strength: number
  ): { tileX: number; tileZ: number }[] {
    const lineDx = x2 - x1
    const lineDz = z2 - z1
    const lenSq = lineDx * lineDx + lineDz * lineDz
    if (lenSq < 1e-6) return []

    // Flat-core falloff: fully saturate within innerR, smoothstep to 0 at radius.
    const innerR = radius * 0.6
    const outerR = radius + FRINGE_PAD

    const minWorldX = Math.min(x1, x2) - outerR
    const maxWorldX = Math.max(x1, x2) + outerR
    const minWorldZ = Math.min(z1, z2) - outerR
    const maxWorldZ = Math.max(z1, z2) + outerR

    const minTileX = worldToTileCoord(minWorldX)
    const maxTileX = worldToTileCoord(maxWorldX)
    const minTileZ = worldToTileCoord(minWorldZ)
    const maxTileZ = worldToTileCoord(maxWorldZ)

    const affectedTiles: { tileX: number; tileZ: number }[] = []

    for (let tz = minTileZ; tz <= maxTileZ; tz++) {
      for (let tx = minTileX; tx <= maxTileX; tx++) {
        const key = tileKey(tx, tz)
        const data = this.splatmaps.get(key)
        if (!data) continue

        const tileMinX = tx * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
        const tileMinZ = tz * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

        const startCX = Math.max(0, Math.floor(minWorldX - tileMinX))
        const endCX = Math.min(TILE_DIM - 1, Math.floor(maxWorldX - tileMinX))
        const startCZ = Math.max(0, Math.floor(minWorldZ - tileMinZ))
        const endCZ = Math.min(TILE_DIM - 1, Math.floor(maxWorldZ - tileMinZ))

        let changed = false

        for (let cz = startCZ; cz <= endCZ; cz++) {
          for (let cx = startCX; cx <= endCX; cx++) {
            const wx = tileMinX + cx
            const wz = tileMinZ + cz

            const vx = wx - x1
            const vz = wz - z1
            let t = (vx * lineDx + vz * lineDz) / lenSq
            if (t < 0) t = 0
            else if (t > 1) t = 1
            const ddx = wx - (x1 + t * lineDx)
            const ddz = wz - (z1 + t * lineDz)
            const dist = Math.sqrt(ddx * ddx + ddz * ddz)
            if (dist > outerR) continue

            const cellIdx = cz * TILE_DIM + cx
            if (dist > radius) {
              if (paintCellFringe(data, cellIdx, paintIdx)) changed = true
            } else {
              const weight = 1 - smoothstep(innerR, radius, dist)
              if (paintCell(data, cellIdx, paintIdx, weight * strength)) {
                changed = true
              }
            }
          }
        }

        if (changed) {
          const texture = this.textures.get(key)
          if (texture) texture.needsUpdate = true
          this.dirtyTiles.add(key)
          affectedTiles.push({ tileX: tx, tileZ: tz })
        }
      }
    }

    if (this.dirtyTiles.size > 0) this.scheduleSave()
    return affectedTiles
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

    for (const key of tilesToSave) {
      const data = this.splatmaps.get(key)
      if (!data) {
        this.dirtyTiles.delete(key)
        continue
      }

      const [txStr, tzStr] = key.split(',')
      const tx = parseInt(txStr)
      const tz = parseInt(tzStr)

      const url = `${this.terrainApiUrl}/api/terrain/splat/${tx}/${tz}`
      const body = new Uint8Array(data).buffer as ArrayBuffer

      try {
        await fetch(url, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/octet-stream' },
          body,
        })
        this.dirtyTiles.delete(key)
      } catch (e) {
        console.error(`Failed to save splatmap for tile (${tx}, ${tz}):`, e)
      }
    }
  }

  /** Directly set splatmap data for a tile (used by terrain generator). */
  setSplatmap(tileX: number, tileZ: number, data: Uint8Array): void {
    const key = tileKey(tileX, tileZ)
    this.splatmaps.set(key, data)
    const existing = this.textures.get(key)
    if (existing) {
      ;(existing.image as unknown as { data: Uint8Array }).data.set(data)
      existing.needsUpdate = true
    }
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
    const texture = this.textures.get(key)
    if (texture) {
      texture.dispose()
    }
    this.splatmaps.delete(key)
    this.textures.delete(key)
  }

  /** Remove cached data without disposing GPU textures (they may still be rendered).
   *  Refuses to evict if the tile has unsaved changes. */
  evictCachedData(tileX: number, tileZ: number) {
    const key = tileKey(tileX, tileZ)
    if (this.dirtyTiles.has(key)) return
    this.splatmaps.delete(key)
    this.textures.delete(key)
  }

  async destroy() {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    if (this.dirtyTiles.size > 0) {
      await this.saveDirtyTiles()
    }
    for (const texture of this.textures.values()) {
      texture.dispose()
    }
  }
}
