import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  TERRAIN_TILE_SIZE,
  getTerrainChunkFromPosition,
} from '../components/game-scene/terrain-utils'
import type { HouseData } from '../types/housing'
import { floorYBase } from '../utils/house-geometry'

function chunkKey(cx: number, cz: number): string {
  return `${cx},${cz}`
}

export class HousingManager {
  private apiUrl: string
  private chunkCache = new Map<string, HouseData[]>()
  private housesById = new Map<string, HouseData>()
  private inflight = new Set<string>()

  /** Callback when houses are loaded/changed. */
  onHousesChanged: ((houses: HouseData[]) => void) | null = null

  constructor() {
    this.apiUrl = getTerrainApiUrl()
  }

  /** Load houses for chunks around a world position. */
  loadChunksAround(wx: number, wz: number, radius: number = 1) {
    const { x: ccx, z: ccz } = getTerrainChunkFromPosition(
      { x: wx, y: 0, z: wz },
      TERRAIN_TILE_SIZE
    )
    for (let dx = -radius; dx <= radius; dx++) {
      for (let dz = -radius; dz <= radius; dz++) {
        this.ensureChunkLoaded(ccx + dx, ccz + dz)
      }
    }
  }

  private ensureChunkLoaded(cx: number, cz: number) {
    const key = chunkKey(cx, cz)
    if (this.chunkCache.has(key) || this.inflight.has(key)) return

    this.inflight.add(key)
    this.fetchChunk(cx, cz, key)
  }

  private async fetchChunk(cx: number, cz: number, key: string) {
    try {
      const resp = await fetch(`${this.apiUrl}/api/housing/area/${cx}/${cz}`)
      if (!resp.ok) {
        this.chunkCache.set(key, []) // Cache as empty to prevent retry storm
        return
      }
      const houses: HouseData[] = await resp.json()
      this.chunkCache.set(key, houses)
      for (const h of houses) {
        this.housesById.set(h.id, h)
      }
      this.notifyChanged()
    } catch {
      this.chunkCache.set(key, []) // Cache as empty to prevent retry storm
    } finally {
      this.inflight.delete(key)
    }
  }

  /** Create a house on the server (ID assigned by server) and add to local cache. */
  async saveHouse(house: HouseData): Promise<HouseData | null> {
    return this.sendHouse('POST', `${this.apiUrl}/api/housing`, house)
  }

  /** Update an existing house on the server (e.g. add room). */
  async updateHouse(house: HouseData): Promise<HouseData | null> {
    return this.sendHouse(
      'PUT',
      `${this.apiUrl}/api/housing/${house.id}`,
      house
    )
  }

  private async sendHouse(
    method: 'POST' | 'PUT',
    url: string,
    house: HouseData
  ): Promise<HouseData | null> {
    try {
      const resp = await fetch(url, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(house),
      })
      if (!resp.ok) return null

      const saved: HouseData = await resp.json()
      this.addToCache(saved)
      this.notifyChanged()
      return saved
    } catch {
      return null
    }
  }

  /** Delete a house from the server and remove from local cache. */
  async deleteHouse(houseId: string): Promise<boolean> {
    try {
      const resp = await fetch(`${this.apiUrl}/api/housing/${houseId}`, {
        method: 'DELETE',
      })
      if (!resp.ok) return false

      this.removeFromCache(houseId)
      this.notifyChanged()
      return true
    } catch {
      return false
    }
  }

  /** Handle a batch of houses from WebSocket (HousesInArea, etc.). */
  handleRemoteHousesBatch(houses: HouseData[]) {
    for (const h of houses) this.addToCache(h)
    this.notifyChanged()
  }

  /** Handle a single house spawned/updated by another player. */
  handleRemoteHouseSpawned(house: HouseData) {
    this.addToCache(house)
    this.notifyChanged()
  }

  /** Handle a house removed by another player. */
  handleRemoteHouseRemoved(houseId: string) {
    this.removeFromCache(houseId)
    this.notifyChanged()
  }

  /** Get all currently loaded houses. */
  getAllHouses(): HouseData[] {
    return Array.from(this.housesById.values())
  }

  /** Get a house by its ID, or undefined if not loaded. */
  getHouseById(id: string): HouseData | undefined {
    return this.housesById.get(id)
  }

  /** Find the house whose room contains a world point, or null. */
  findHouseAtPoint(x: number, y: number, z: number): HouseData | null {
    const result = this.findRoomAtPoint(x, y, z)
    return result ? result.house : null
  }

  /** Find the first room containing a world point (fast, no allocation). */
  findRoomAtPoint(
    x: number,
    y: number,
    z: number
  ): { house: HouseData; roomIndex: number } | null {
    for (const house of this.housesById.values()) {
      for (let i = 0; i < house.rooms.length; i++) {
        const room = house.rooms[i]
        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ
        const ryBase =
          house.origin.y + floorYBase(room.floorLevel, room.wallHeight)
        if (
          x >= rx &&
          x <= rx + room.sizeX &&
          z >= rz &&
          z <= rz + room.sizeZ &&
          y >= ryBase - 1 &&
          y <= ryBase + room.wallHeight + 1
        ) {
          return { house, roomIndex: i }
        }
      }
    }
    return null
  }

  /** Find ALL rooms containing a world point (for overlapping stairwells etc). */
  findAllRoomsAtPoint(
    x: number,
    y: number,
    z: number
  ): { house: HouseData; roomIndex: number }[] {
    const results: { house: HouseData; roomIndex: number }[] = []
    for (const house of this.housesById.values()) {
      for (let i = 0; i < house.rooms.length; i++) {
        const room = house.rooms[i]
        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ
        const ryBase =
          house.origin.y + floorYBase(room.floorLevel, room.wallHeight)
        const roomHeight = room.wallHeight
        if (
          x >= rx &&
          x <= rx + room.sizeX &&
          z >= rz &&
          z <= rz + room.sizeZ &&
          y >= ryBase - 1 &&
          y <= ryBase + roomHeight + 1
        ) {
          results.push({ house, roomIndex: i })
        }
      }
    }
    return results
  }

  /** Update local cache without server call (triggers geometry rebuild). */
  updateLocalCache(house: HouseData) {
    this.addToCache(house)
    this.notifyChanged()
  }

  /** Find an existing house that shares an edge with the given room footprint. */
  findAdjacentHouse(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number
  ): HouseData | null {
    for (const house of this.housesById.values()) {
      for (const room of house.rooms) {
        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ
        // Rooms share an edge if they overlap on one axis and touch exactly on the other
        const overlapX = originX < rx + room.sizeX && originX + sizeX > rx
        const overlapZ = originZ < rz + room.sizeZ && originZ + sizeZ > rz
        const touchN = originZ === rz + room.sizeZ
        const touchS = originZ + sizeZ === rz
        const touchE = originX === rx + room.sizeX
        const touchW = originX + sizeX === rx

        if (
          (overlapX && (touchN || touchS)) ||
          (overlapZ && (touchE || touchW))
        ) {
          return house
        }
      }
    }
    return null
  }

  /** Check if a room footprint overlaps any existing house on the same floor level. */
  checkOverlap(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number,
    floorLevel: number = 0
  ): boolean {
    for (const house of this.housesById.values()) {
      for (const room of house.rooms) {
        if (room.floorLevel !== floorLevel) continue
        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ
        if (
          originX < rx + room.sizeX &&
          originX + sizeX > rx &&
          originZ < rz + room.sizeZ &&
          originZ + sizeZ > rz
        ) {
          return true
        }
      }
    }
    return false
  }

  /**
   * Check if a 2F room footprint is fully supported by 1F rooms in a given house.
   * Returns true if the entire XZ rectangle is covered by floor_level=0 rooms.
   */
  hasFloorSupport(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number,
    houseId?: string
  ): boolean {
    // Check each 1m² cell of the proposed 2F footprint
    for (let x = originX; x < originX + sizeX; x++) {
      for (let z = originZ; z < originZ + sizeZ; z++) {
        let supported = false
        for (const house of this.housesById.values()) {
          if (houseId && house.id !== houseId) continue
          for (const room of house.rooms) {
            if (room.floorLevel !== 0) continue
            const rx = house.origin.x + room.localX
            const rz = house.origin.z + room.localZ
            if (
              x >= rx &&
              x < rx + room.sizeX &&
              z >= rz &&
              z < rz + room.sizeZ
            ) {
              supported = true
              break
            }
          }
          if (supported) break
        }
        if (!supported) return false
      }
    }
    return true
  }

  /**
   * Find a house that has 1F rooms supporting the given 2F footprint.
   */
  findSupportingHouse(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number
  ): HouseData | null {
    for (const house of this.housesById.values()) {
      if (this.hasFloorSupport(originX, originZ, sizeX, sizeZ, house.id)) {
        return house
      }
    }
    return null
  }

  private addToCache(house: HouseData) {
    this.housesById.set(house.id, house)
    const { x: cx, z: cz } = getTerrainChunkFromPosition(
      house.origin,
      TERRAIN_TILE_SIZE
    )
    const key = chunkKey(cx, cz)
    const chunk = this.chunkCache.get(key)
    if (chunk) {
      const idx = chunk.findIndex((h) => h.id === house.id)
      if (idx >= 0) {
        chunk[idx] = house
      } else {
        chunk.push(house)
      }
    } else {
      this.chunkCache.set(key, [house])
    }
  }

  private removeFromCache(houseId: string) {
    const house = this.housesById.get(houseId)
    if (!house) return
    this.housesById.delete(houseId)
    const { x: cx, z: cz } = getTerrainChunkFromPosition(
      house.origin,
      TERRAIN_TILE_SIZE
    )
    const key = chunkKey(cx, cz)
    const chunk = this.chunkCache.get(key)
    if (chunk) {
      const idx = chunk.findIndex((h) => h.id === houseId)
      if (idx >= 0) chunk.splice(idx, 1)
    }
  }

  private notifyChanged() {
    this.onHousesChanged?.(this.getAllHouses())
  }
}

export const housingManager = new HousingManager()
