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
  updateDoorEdge,
  type RuntimePassability,
} from './housing-passability'
import {
  passability_add_house,
  passability_remove_house,
  passability_update_door,
  passability_is_movement_blocked,
} from '../wasm/onlinerpg_shared'
import {
  checkOverlap,
  collectRoomAABBsInRegion,
  findAdjacentHouse,
  findAllRoomsAtPoint,
  findHouseAtPoint,
  findNearestDoor,
  findRoomAtPoint,
  findSupportingHouse,
  hasFloorSupport,
  isPointUnderHouseXZ,
  type RoomAABB,
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
  private inflight = new Map<string, Promise<void>>()

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

    this.inflight.set(key, this.fetchChunk(cx, cz, key))
  }

  /** Wait for all currently in-flight chunk fetches to complete. */
  async waitForPending(): Promise<void> {
    if (this.inflight.size === 0) return
    await Promise.all(this.inflight.values())
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
    // Only update passability for doors (windows remain blocking when open)
    if (wall[segmentIndex].variant === 'door') {
      passability_update_door(houseId, room, wallDir, segmentIndex, isOpen)
    }
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

  /** Check if (x, z) falls inside any house room footprint, ignoring Y. */
  isPointUnderHouseXZ(x: number, z: number): boolean {
    return isPointUnderHouseXZ(this.housesById, x, z)
  }

  /** Collect XZ AABBs of all rooms whose footprint intersects the given region. */
  collectRoomAABBsInRegion(
    minX: number,
    maxX: number,
    minZ: number,
    maxZ: number
  ): RoomAABB[] {
    return collectRoomAABBsInRegion(this.housesById, minX, maxX, minZ, maxZ)
  }

  /** Find ALL rooms containing a world point (for overlapping stairwells etc). */
  findAllRoomsAtPoint(x: number, y: number, z: number) {
    return findAllRoomsAtPoint(this.housesById, x, y, z)
  }

  /**
   * Check if movement from→to crosses any blocked cell edge.
   */
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    return passability_is_movement_blocked(fromX, fromZ, toX, toZ, y)
  }

  /** Update local cache without server call (triggers geometry rebuild). */
  updateLocalCache(house: HouseData) {
    this.addToCache(house)
    this.notifyChanged()
  }

  /** Build passability entries on the fly for debug visualization. */
  getPassabilityEntries(): Map<string, RuntimePassability> {
    const map = new Map<string, RuntimePassability>()
    for (const house of this.housesById.values()) {
      map.set(house.id, buildRuntimePassability(house))
      for (const room of house.rooms) {
        for (const dir of ALL_WALL_DIRS) {
          const segs = getWallByDir(room, dir)
          for (let i = 0; i < segs.length; i++) {
            if (segs[i].variant === 'door' && segs[i].isOpen) {
              updateDoorEdge(map, house.id, room, dir, i, true)
            }
          }
        }
      }
    }
    return map
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

    // Ensure passability grids exist (compute from room data if missing)
    if (!house.passability?.length) {
      house.passability = buildPassability(house)
    }
    passability_add_house(house)
  }

  private removeFromCache(houseId: string) {
    const house = this.housesById.get(houseId)
    if (!house) return
    this.housesById.delete(houseId)
    passability_remove_house(houseId)
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
