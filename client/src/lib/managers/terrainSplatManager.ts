import * as THREE from 'three'
import { getTerrainApiUrl } from '../utils/networkUtils'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'

const TILE_DIM = 64
const CHANNELS = 4 // RGBA

function tileKey(tileX: number, tileZ: number): string {
  return `${tileX},${tileZ}`
}

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
    texture.minFilter = THREE.LinearFilter
    texture.magFilter = THREE.LinearFilter
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

    // Deduplicate in-flight requests
    const inflight = this.inflightSplatmaps.get(key)
    if (inflight) return inflight

    const defaultFallback = (): THREE.DataTexture => {
      const data = new Uint8Array(TILE_DIM * TILE_DIM * CHANNELS)
      for (let i = 0; i < TILE_DIM * TILE_DIM; i++) {
        data[i * CHANNELS] = 255 // R = grass
      }
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

  applySplatBrush(
    worldX: number,
    worldZ: number,
    radius: number,
    layerIndex: number,
    strength: number
  ) {
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
            const vertexWorldX = tileMinX + cx
            const vertexWorldZ = tileMinZ + cz

            const dx = vertexWorldX - worldX
            const dz = vertexWorldZ - worldZ
            const dist = Math.sqrt(dx * dx + dz * dz)

            if (dist > radius) continue

            const weight = Math.exp(-(dist * dist) / (2 * sigma * sigma))
            const addAmount = weight * strength * 255

            const pixelIdx = (cz * TILE_DIM + cx) * CHANNELS

            // Increase target channel
            const current = data[pixelIdx + layerIndex]
            const target = Math.min(255, current + addAmount)
            data[pixelIdx + layerIndex] = target

            // Redistribute other channels so sum = 255
            let otherSum = 0
            for (let c = 0; c < CHANNELS; c++) {
              if (c !== layerIndex) otherSum += data[pixelIdx + c]
            }

            const total = data[pixelIdx + layerIndex] + otherSum
            if (total > 255 && otherSum > 0) {
              const scale = (255 - data[pixelIdx + layerIndex]) / otherSum
              for (let c = 0; c < CHANNELS; c++) {
                if (c !== layerIndex) {
                  data[pixelIdx + c] = Math.round(data[pixelIdx + c] * scale)
                }
              }
            }

            changed = true
          }
        }

        if (changed) {
          const texture = this.textures.get(key)
          if (texture) {
            texture.needsUpdate = true
          }
          this.dirtyTiles.add(key)
        }
      }
    }

    if (this.dirtyTiles.size > 0) {
      this.scheduleSave()
    }
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
      const data = this.splatmaps.get(key)
      if (!data) continue

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
      } catch (e) {
        console.error(`Failed to save splatmap for tile (${tx}, ${tz}):`, e)
        this.dirtyTiles.add(key)
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

  destroy() {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer)
    }
    if (this.dirtyTiles.size > 0) {
      this.saveDirtyTiles()
    }
    for (const texture of this.textures.values()) {
      texture.dispose()
    }
  }
}
