import { getTerrainApiUrl } from '../utils/networkUtils'
import { decodeGrassData, type GrassPlacementData } from '../utils/grass-data'

function tileKey(tileX: number, tileZ: number): string {
  return `${tileX},${tileZ}`
}

export class TerrainGrassDataManager {
  private cache = new Map<string, GrassPlacementData>()
  private inflight = new Map<string, Promise<GrassPlacementData | null>>()
  /** Tiles known to have no server data (404). Prevents repeated fetches. */
  private missingTiles = new Set<string>()
  private terrainApiUrl: string
  private generation = 0

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  /**
   * Load pre-computed grass data for a tile.
   * Returns null if no data exists on the server.
   */
  async loadGrassData(
    tileX: number,
    tileZ: number
  ): Promise<GrassPlacementData | null> {
    const key = tileKey(tileX, tileZ)

    const cached = this.cache.get(key)
    if (cached) return cached

    if (this.missingTiles.has(key)) return null

    const existing = this.inflight.get(key)
    if (existing) return existing

    const gen = this.generation
    const promise = (async () => {
      try {
        const url = `${this.terrainApiUrl}/api/terrain/grass/${tileX}/${tileZ}`
        const response = await fetch(url)
        if (gen !== this.generation) return null
        if (response.status === 404) {
          this.missingTiles.add(key)
          return null
        }
        if (!response.ok) {
          console.error(
            `Failed to load grass data (${tileX}, ${tileZ}): ${response.status}`
          )
          return null
        }
        const buffer = await response.arrayBuffer()
        if (gen !== this.generation) return null
        const data = decodeGrassData(buffer)
        this.cache.set(key, data)
        return data
      } catch (e) {
        console.error(`Grass data fetch error (${tileX}, ${tileZ}):`, e)
        return null
      } finally {
        this.inflight.delete(key)
      }
    })()
    this.inflight.set(key, promise)
    return promise
  }

  /** Save pre-computed grass data to the server. */
  async saveGrassData(
    tileX: number,
    tileZ: number,
    data: GrassPlacementData
  ): Promise<void> {
    const key = tileKey(tileX, tileZ)
    this.cache.set(key, data)
    this.missingTiles.delete(key)

    try {
      const url = `${this.terrainApiUrl}/api/terrain/grass/${tileX}/${tileZ}`
      const response = await fetch(url, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: data.buffer,
      })
      if (!response.ok) {
        console.error(
          `Failed to save grass data (${tileX}, ${tileZ}): ${response.status}`
        )
      }
    } catch (e) {
      console.error(`Grass data save error (${tileX}, ${tileZ}):`, e)
    }
  }

  /** Get cached grass data (synchronous). */
  getCachedGrassData(tileX: number, tileZ: number): GrassPlacementData | null {
    return this.cache.get(tileKey(tileX, tileZ)) ?? null
  }

  /** Invalidate cache for a tile. */
  invalidate(tileX: number, tileZ: number): void {
    const key = tileKey(tileX, tileZ)
    this.cache.delete(key)
    this.missingTiles.delete(key)
  }

  /** Clear all caches so every tile is re-fetched from the server. */
  invalidateAll(): void {
    this.generation++
    this.cache.clear()
    this.missingTiles.clear()
    this.inflight.clear()
  }

  /** Evict cached data for tiles not in the given set. */
  evictExcept(keepKeys: Set<string>): void {
    for (const key of this.cache.keys()) {
      if (!keepKeys.has(key)) this.cache.delete(key)
    }
  }
}
