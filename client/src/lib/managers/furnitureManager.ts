import { getTerrainApiUrl } from '../utils/networkUtils'
import type {
  FurnitureDef,
  FurniturePlacement,
  FurnitureRegionData,
} from '../stores/editorStore'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'
import { tileToRegion } from '../terrain/terrain-constants'

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

  /** Look up a furniture definition by type id (e.g. "bed"). Returns null if catalog not loaded or not found. */
  getCatalogEntry(furnitureType: string): FurnitureDef | null {
    if (!this.catalogCache) return null
    return this.catalogCache.find((d) => d.id === furnitureType) ?? null
  }

  /** Find the nearest furniture placement of the given type to a world position, searching all cached regions. */
  findNearestPlacement(
    furnitureType: string,
    wx: number,
    wz: number
  ): FurniturePlacement | null {
    let best: FurniturePlacement | null = null
    let bestDist = Infinity
    for (const region of this.cache.values()) {
      for (const p of region.placements) {
        if (p.type !== furnitureType) continue
        const dx = p.x - wx
        const dz = p.z - wz
        const dist = dx * dx + dz * dz
        if (dist < bestDist) {
          bestDist = dist
          best = p
        }
      }
    }
    return best
  }

  /** Like findNearestPlacement but fetches the region first if not cached. */
  async findNearestPlacementAsync(
    furnitureType: string,
    wx: number,
    wz: number
  ): Promise<FurniturePlacement | null> {
    // Ensure the region containing this position is loaded
    const tileX = Math.floor(wx / TERRAIN_TILE_SIZE)
    const tileZ = Math.floor(wz / TERRAIN_TILE_SIZE)
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    await this.fetchFurniture(rx, rz)
    return this.findNearestPlacement(furnitureType, wx, wz)
  }
}

/** Shared singleton instance */
export const furnitureManager = new FurnitureManager()
