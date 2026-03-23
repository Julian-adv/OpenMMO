import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  TERRAIN_TILE_SIZE,
  getTerrainChunkFromPosition,
} from '../components/game-scene/terrain-utils'
import type { HouseData } from '../types/housing'
import type { WallDirection } from '../utils/house-geometry'
import {
  ALL_WALL_DIRS,
  buildPassability,
  buildRuntimePassability,
  getWallByDir,
  isMovementBlocked,
  updateDoorEdge,
} from './housing-passability'

type RuntimePassability = ReturnType<typeof buildRuntimePassability>
import {
  checkOverlap,
  findAdjacentHouse,
  findAllRoomsAtPoint,
  findHouseAtPoint,
  findNearestDoor,
  findRoomAtPoint,
  findSupportingHouse,
  hasFloorSupport,
} from './housing-queries'

// Re-export for external consumers
export { getWallByDir } from './housing-passability'

function chunkKey(cx: number, cz: number): string {
  return `${cx},${cz}`
}

export class HousingManager {
  private apiUrl: string
  private chunkCache = new Map<string, HouseData[]>()
  private housesById = new Map<string, HouseData>()
  private inflight = new Set<string>()
  private passabilityCache = new Map<string, RuntimePassability>()

  private housesChangedListeners: ((houses: HouseData[]) => void)[] = []

  /** Subscribe to house data changes. Returns an unsubscribe function. */
  onHousesChanged(cb: (houses: HouseData[]) => void): () => void {
    this.housesChangedListeners.push(cb)
    return () => {
      this.housesChangedListeners = this.housesChangedListeners.filter(
        (l) => l !== cb
      )
    }
  }

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
      for (const h of houses) this.addToCache(h)
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
      const payload = { ...house, passability: buildPassability(house) }
      const resp = await fetch(url, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
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

  /** Handle a door toggle from the server (authoritative state). */
  handleDoorToggled(
    houseId: string,
    roomIndex: number,
    wallDir: WallDirection,
    segmentIndex: number,
    isOpen: boolean
  ) {
    const house = this.housesById.get(houseId)
    if (!house) return
    const room = house.rooms[roomIndex]
    if (!room) return

    const wall = getWallByDir(room, wallDir)
    if (!wall[segmentIndex]) return

    wall[segmentIndex].isOpen = isOpen
    updateDoorEdge(
      this.passabilityCache,
      houseId,
      room,
      wallDir,
      segmentIndex,
      isOpen
    )
    this.notifyChanged()
  }

  /** Find the nearest door segment within maxDist of (x, z). */
  findNearestDoor(x: number, z: number, y: number, maxDist: number) {
    return findNearestDoor(this.housesById, x, z, y, maxDist)
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
  findHouseAtPoint(x: number, y: number, z: number) {
    return findHouseAtPoint(this.housesById, x, y, z)
  }

  /** Find the first room containing a world point (fast, no allocation). */
  findRoomAtPoint(x: number, y: number, z: number) {
    return findRoomAtPoint(this.housesById, x, y, z)
  }

  /** Find ALL rooms containing a world point (for overlapping stairwells etc). */
  findAllRoomsAtPoint(x: number, y: number, z: number) {
    return findAllRoomsAtPoint(this.housesById, x, y, z)
  }

  /**
   * Check if movement from→to is blocked by any cell edge.
   * Uses precomputed passability grids with WALL_HALF_THICKNESS proximity buffer.
   */
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    return isMovementBlocked(this.passabilityCache, fromX, fromZ, toX, toZ, y)
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
  ) {
    return findAdjacentHouse(this.housesById, originX, originZ, sizeX, sizeZ)
  }

  /** Check if a room footprint overlaps any existing house on the same floor level. */
  checkOverlap(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number,
    floorLevel: number = 0
  ): boolean {
    return checkOverlap(
      this.housesById,
      originX,
      originZ,
      sizeX,
      sizeZ,
      floorLevel
    )
  }

  /**
   * Check if a room footprint is fully supported by rooms on the floor below.
   */
  hasFloorSupport(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number,
    opts?: { houseId?: string; floorLevel?: number }
  ): boolean {
    return hasFloorSupport(
      this.housesById,
      originX,
      originZ,
      sizeX,
      sizeZ,
      opts
    )
  }

  /**
   * Find a house that has rooms on the floor below supporting the given footprint.
   */
  findSupportingHouse(
    originX: number,
    originZ: number,
    sizeX: number,
    sizeZ: number,
    floorLevel: number = 1
  ) {
    return findSupportingHouse(
      this.housesById,
      originX,
      originZ,
      sizeX,
      sizeZ,
      floorLevel
    )
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

    // Build runtime passability and apply door overlays
    const rp = buildRuntimePassability(house)
    this.passabilityCache.set(house.id, rp)

    for (const room of house.rooms) {
      for (const dir of ALL_WALL_DIRS) {
        const segs = getWallByDir(room, dir)
        for (let i = 0; i < segs.length; i++) {
          if (segs[i].variant === 'door' && segs[i].isOpen) {
            updateDoorEdge(this.passabilityCache, house.id, room, dir, i, true)
          }
        }
      }
    }
  }

  private removeFromCache(houseId: string) {
    const house = this.housesById.get(houseId)
    if (!house) return
    this.housesById.delete(houseId)
    this.passabilityCache.delete(houseId)
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
    const all = this.getAllHouses()
    for (const cb of this.housesChangedListeners) cb(all)
  }
}

export const housingManager = new HousingManager()
