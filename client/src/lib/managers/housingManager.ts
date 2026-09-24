import { apiFetch, getTerrainApiUrl } from '../utils/networkUtils'
import type { HouseData } from '../types/housing'
import type { WallDirection } from '../utils/house-geometry'
import { setHouseMapFootprints } from '../stores/housingMapStore'
import { worldView } from '../network/worldView'
import {
  ALL_WALL_DIRS,
  buildPassability,
  buildRuntimePassability,
  doorPartnerRef,
  getWallByDir,
  isDoorVariant,
  updateDoorEdge,
  type RuntimePassability,
} from './housing-passability'
import {
  passability_add_house,
  passability_remove_house,
  passability_update_door,
  passability_is_movement_blocked,
  passability_attack_line_blocked,
  passability_is_circle_blocked,
} from '../wasm/onlinerpg_shared'
import {
  assistStairMovementDirection,
  checkOverlap,
  collectRoomAABBsInRegion,
  findAdjacentHouse,
  findClosedDoorOnSegment,
  findClosedDoorOnPath,
  findHouseAtPoint,
  findNearestDoor,
  findRoomAtPoint,
  findSupportingHouse,
  hasFloorSupport,
  houseFloorHeightAt,
  isHouseWallBlockingSegment,
  isPointUnderHouseXZ,
  stairLandingTargetAt,
  stopPathAtHouseEntrance,
  type RoomAABB,
  type ClosedHouseDoor,
} from './housing-queries'

// Re-export for external consumers
export { getWallByDir } from './housing-passability'

export class HousingManager {
  private apiUrl: string
  private housesById = new Map<string, HouseData>()
  private synchronized = false
  private pending: (() => void)[] = []

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

  isSynchronized(x: number, z: number) {
    return (
      this.synchronized && worldView.floorLevel >= 0 && worldView.covers(x, z)
    )
  }

  resetView() {
    for (const id of this.housesById.keys()) passability_remove_house(id)
    this.housesById.clear()
    this.synchronized = false
    this.notifyChanged()
  }

  completeSnapshot() {
    this.synchronized = true
    for (const resolve of this.pending.splice(0)) resolve()
    this.notifyChanged()
  }

  waitForSnapshot(): Promise<void> {
    return this.synchronized
      ? Promise.resolve()
      : new Promise((resolve) => this.pending.push(resolve))
  }

  stopPathAtHouseEntrance(
    current: { x: number; y: number; z: number },
    currentFloor: number,
    target: { x: number; y: number; z: number },
    waypoints: { x: number; z: number; floor: number }[]
  ): { x: number; z: number; floor: number }[] {
    return stopPathAtHouseEntrance(
      this.housesById,
      current,
      currentFloor,
      target,
      waypoints
    )
  }
  /** Create a house; its active state arrives through the world stream. */
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
      const resp = await apiFetch(url, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })
      if (!resp.ok) return null

      const saved: HouseData = await resp.json()
      return saved
    } catch {
      return null
    }
  }

  /** Delete a house from the server and remove from local cache. */
  async deleteHouse(houseId: string): Promise<boolean> {
    try {
      const resp = await apiFetch(`${this.apiUrl}/api/housing/${houseId}`, {
        method: 'DELETE',
      })
      if (!resp.ok) return false

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

    const refs = [{ roomIndex, segmentIndex }]
    const partner = doorPartnerRef(
      house.rooms,
      roomIndex,
      wallDir,
      segmentIndex
    )
    if (partner) refs.push(partner)
    for (const ref of refs) {
      const r = house.rooms[ref.roomIndex]
      const seg = getWallByDir(r, wallDir)[ref.segmentIndex]
      seg.isOpen = isOpen
      // Windows stay blocking when open
      if (isDoorVariant(seg.variant)) {
        passability_update_door(houseId, r, wallDir, ref.segmentIndex, isOpen)
      }
    }
    this.notifyChanged(false)
  }

  /** Find the nearest door segment within maxDist of (x, z). */
  findNearestDoor(x: number, z: number, y: number, maxDist: number) {
    return findNearestDoor(this.housesById, x, z, y, maxDist)
  }

  findClosedDoorOnSegment(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    floorLevel: number
  ): ClosedHouseDoor | null {
    return findClosedDoorOnSegment(
      this.housesById,
      fromX,
      fromZ,
      toX,
      toZ,
      floorLevel
    )
  }

  findClosedDoorOnPath(
    fromX: number,
    fromZ: number,
    waypoints: readonly { x: number; z: number }[],
    floorLevel: number
  ): ClosedHouseDoor | null {
    return findClosedDoorOnPath(
      this.housesById,
      fromX,
      fromZ,
      waypoints,
      floorLevel
    )
  }

  withClosedDoorsOpen<T>(floorLevel: number, fn: () => T): T {
    const closed: {
      houseId: string
      room: HouseData['rooms'][number]
      wallDir: WallDirection
      segmentIndex: number
    }[] = []

    try {
      for (const house of this.housesById.values()) {
        for (const room of house.rooms) {
          if (room.floorLevel !== floorLevel) continue
          for (const wallDir of ALL_WALL_DIRS) {
            const wall = getWallByDir(room, wallDir)
            for (
              let segmentIndex = 0;
              segmentIndex < wall.length;
              segmentIndex++
            ) {
              const segment = wall[segmentIndex]
              if (!isDoorVariant(segment.variant) || segment.isOpen) continue
              closed.push({ houseId: house.id, room, wallDir, segmentIndex })
              passability_update_door(
                house.id,
                room,
                wallDir,
                segmentIndex,
                true
              )
            }
          }
        }
      }
      return fn()
    } finally {
      for (const door of closed) {
        passability_update_door(
          door.houseId,
          door.room,
          door.wallDir,
          door.segmentIndex,
          false
        )
      }
    }
  }

  isDoorOpen(door: ClosedHouseDoor): boolean {
    const room = this.housesById.get(door.houseId)?.rooms[door.roomIndex]
    return (
      !!room && !!getWallByDir(room, door.wallDir)[door.segmentIndex]?.isOpen
    )
  }

  isHouseWallBlockingSegment(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    floorLevel: number
  ): boolean {
    return isHouseWallBlockingSegment(
      this.housesById,
      fromX,
      fromZ,
      toX,
      toZ,
      floorLevel
    )
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

  /** Ground Y on a given house floor at (x, z), stairwell ramps included. */
  floorHeightAt(floorLevel: number, x: number, z: number): number | null {
    return houseFloorHeightAt(this.housesById, floorLevel, x, z)
  }

  assistStairMovementDirection(
    floorLevel: number,
    position: { x: number; y: number; z: number },
    direction: { x: number; z: number }
  ) {
    return assistStairMovementDirection(
      this.housesById,
      floorLevel,
      position,
      direction
    )
  }

  stairLandingTargetAt(
    floorLevel: number,
    x: number,
    y: number,
    z: number,
    stairFloor?: number
  ) {
    return stairLandingTargetAt(
      this.housesById,
      floorLevel,
      x,
      y,
      z,
      stairFloor
    )
  }

  /**
   * Check if movement from→to crosses any blocked cell edge. `floorLevel` is
   * the passability floor index (see `dungeonManager.passabilityFloor`), not
   * the wire floor. A player furniture has sealed in gets one step out; walls
   * still refuse.
   */
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    floorLevel: number,
    y: number
  ): boolean {
    return passability_is_movement_blocked(
      fromX,
      fromZ,
      toX,
      toZ,
      floorLevel,
      y
    )
  }

  /** Attack collision on the passability floor, matching the server. */
  attackLineBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    floorLevel: number,
    ranged = false
  ): boolean {
    return passability_attack_line_blocked(
      fromX,
      fromZ,
      toX,
      toZ,
      floorLevel,
      ranged
    )
  }

  /** Check if a circle of radius r at (x, z) overlaps any blocking wall. */
  isCircleBlocked(
    x: number,
    z: number,
    r: number,
    floorLevel: number,
    y: number
  ): boolean {
    return passability_is_circle_blocked(x, z, r, floorLevel, y)
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
            if (isDoorVariant(segs[i].variant) && segs[i].isOpen) {
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
  }

  private notifyChanged(updateMap = true) {
    const all = this.getAllHouses()
    if (updateMap) setHouseMapFootprints(all)
    for (const cb of this.housesChangedListeners) cb(all)
  }
}

export const housingManager = new HousingManager()
