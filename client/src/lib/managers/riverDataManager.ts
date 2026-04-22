import { getTerrainApiUrl } from '../utils/networkUtils'
import { decodeRiverData, type RiverTileData } from '../utils/river-data'
import { tileKey } from './terrain-height-types'

export class RiverDataManager {
  private cache = new Map<string, RiverTileData | null>()
  private inflight = new Map<string, Promise<RiverTileData | null>>()
  private terrainApiUrl = getTerrainApiUrl()

  async loadRiverData(
    tileX: number,
    tileZ: number
  ): Promise<RiverTileData | null> {
    const key = tileKey(tileX, tileZ)

    if (this.cache.has(key)) return this.cache.get(key) ?? null
    const existing = this.inflight.get(key)
    if (existing) return existing

    const promise = (async () => {
      try {
        const url = `${this.terrainApiUrl}/api/terrain/rivers/${tileX}/${tileZ}`
        const response = await fetch(url)
        if (response.status === 404) {
          this.cache.set(key, null)
          return null
        }
        if (!response.ok) {
          console.error(
            `Failed to load river data (${tileX}, ${tileZ}): ${response.status}`
          )
          return null
        }
        const buffer = await response.arrayBuffer()
        const data = decodeRiverData(buffer)
        this.cache.set(key, data)
        return data
      } catch (e) {
        console.error(`River data fetch error (${tileX}, ${tileZ}):`, e)
        return null
      } finally {
        this.inflight.delete(key)
      }
    })()
    this.inflight.set(key, promise)
    return promise
  }
}
