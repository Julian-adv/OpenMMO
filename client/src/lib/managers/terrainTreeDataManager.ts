import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  decodeTreeData,
  encodeTreeBuffer,
  type TreePlacementData,
} from '../utils/tree-data'
import { tileKey } from './terrain-height-types'
import type { TerrainHeightManager } from './terrainHeightManager'

export class TerrainTreeDataManager {
  private cache = new Map<string, TreePlacementData>()
  private inflight = new Map<string, Promise<TreePlacementData | null>>()
  private missingTiles = new Set<string>()
  private terrainApiUrl: string
  private generation = 0
  private heightManager: TerrainHeightManager
  private invalidateListeners: (() => void)[] = []
  private tileUpdateListeners: ((tileX: number, tileZ: number) => void)[] = []

  constructor(heightManager: TerrainHeightManager) {
    this.terrainApiUrl = getTerrainApiUrl()
    this.heightManager = heightManager
  }

  async loadTreeData(
    tileX: number,
    tileZ: number
  ): Promise<TreePlacementData | null> {
    const key = tileKey(tileX, tileZ)

    const cached = this.cache.get(key)
    if (cached) return cached

    if (this.missingTiles.has(key)) return null

    const existing = this.inflight.get(key)
    if (existing) return existing

    const gen = this.generation
    const promise = (async () => {
      try {
        const url = `${this.terrainApiUrl}/api/terrain/trees/${tileX}/${tileZ}`
        const response = await fetch(url)
        if (gen !== this.generation) return null
        if (response.status === 404) {
          this.missingTiles.add(key)
          return null
        }
        if (!response.ok) {
          console.error(
            `Failed to load tree data (${tileX}, ${tileZ}): ${response.status}`
          )
          return null
        }
        const buffer = await response.arrayBuffer()
        if (gen !== this.generation) return null
        let heightmap = this.heightManager.getHeightmap(tileX, tileZ)
        if (!heightmap) {
          heightmap = await this.heightManager.loadHeightmap(tileX, tileZ)
          if (gen !== this.generation) return null
        }
        const data = decodeTreeData(buffer, tileX, tileZ, heightmap)
        this.cache.set(key, data)
        return data
      } catch (e) {
        console.error(`Tree data fetch error (${tileX}, ${tileZ}):`, e)
        return null
      } finally {
        this.inflight.delete(key)
      }
    })()
    this.inflight.set(key, promise)
    return promise
  }

  async saveTreeData(
    tileX: number,
    tileZ: number,
    data: TreePlacementData
  ): Promise<void> {
    const key = tileKey(tileX, tileZ)
    this.cache.set(key, data)
    this.missingTiles.delete(key)

    for (const cb of this.tileUpdateListeners) cb(tileX, tileZ)

    try {
      const url = `${this.terrainApiUrl}/api/terrain/trees/${tileX}/${tileZ}`
      const wireBuffer = encodeTreeBuffer(data, tileX, tileZ)
      const response = await fetch(url, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: wireBuffer,
      })
      if (!response.ok) {
        console.error(
          `Failed to save tree data (${tileX}, ${tileZ}): ${response.status}`
        )
      }
    } catch (e) {
      console.error(`Tree data save error (${tileX}, ${tileZ}):`, e)
    }
  }

  getCachedTreeData(tileX: number, tileZ: number): TreePlacementData | null {
    return this.cache.get(tileKey(tileX, tileZ)) ?? null
  }

  invalidate(tileX: number, tileZ: number): void {
    const key = tileKey(tileX, tileZ)
    this.cache.delete(key)
    this.missingTiles.delete(key)
  }

  /** Subscribe to per-tile data updates. Returns unsubscribe function. */
  onTileUpdated(cb: (tileX: number, tileZ: number) => void): () => void {
    this.tileUpdateListeners.push(cb)
    return () => {
      this.tileUpdateListeners = this.tileUpdateListeners.filter(
        (l) => l !== cb
      )
    }
  }

  onInvalidateAll(cb: () => void): () => void {
    this.invalidateListeners.push(cb)
    return () => {
      this.invalidateListeners = this.invalidateListeners.filter(
        (l) => l !== cb
      )
    }
  }

  invalidateAll(): void {
    this.generation++
    this.cache.clear()
    this.missingTiles.clear()
    this.inflight.clear()
    for (const cb of this.invalidateListeners) cb()
  }

  evictExcept(keepKeys: Set<string>): void {
    for (const key of this.cache.keys()) {
      if (!keepKeys.has(key)) {
        this.cache.delete(key)
      }
    }
  }
}
