import { getTerrainApiUrl } from '../utils/networkUtils'
import type { FurnitureDef, FurnitureRegionData } from '../stores/editorStore'

function regionKey(rx: number, rz: number): string {
  return `${rx},${rz}`
}

export class FurnitureManager {
  private cache = new Map<string, FurnitureRegionData>()
  private terrainApiUrl: string
  private catalogCache: FurnitureDef[] | null = null

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  async fetchCatalog(): Promise<FurnitureDef[]> {
    if (this.catalogCache) return this.catalogCache
    const resp = await fetch('/models/furniture/catalog.json')
    const data: FurnitureDef[] = await resp.json()
    this.catalogCache = data
    return data
  }

  async fetchFurniture(rx: number, rz: number): Promise<FurnitureRegionData> {
    const key = regionKey(rx, rz)
    const cached = this.cache.get(key)
    if (cached) return cached

    try {
      const resp = await fetch(
        `${this.terrainApiUrl}/api/terrain/furniture/${rx}/${rz}`
      )
      const json = await resp.json()
      const data: FurnitureRegionData = {
        placements: json.placements ?? [],
      }
      this.cache.set(key, data)
      return data
    } catch {
      const data: FurnitureRegionData = { placements: [] }
      this.cache.set(key, data)
      return data
    }
  }

  async saveFurniture(
    rx: number,
    rz: number,
    data: FurnitureRegionData
  ): Promise<void> {
    await fetch(`${this.terrainApiUrl}/api/terrain/furniture/${rx}/${rz}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    })
    this.cache.set(regionKey(rx, rz), data)
  }

  getCached(rx: number, rz: number): FurnitureRegionData | null {
    return this.cache.get(regionKey(rx, rz)) ?? null
  }

  invalidate(rx: number, rz: number): void {
    this.cache.delete(regionKey(rx, rz))
  }
}

/** Shared singleton instance */
export const furnitureManager = new FurnitureManager()
