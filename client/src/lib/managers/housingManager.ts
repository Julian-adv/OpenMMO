import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  TERRAIN_TILE_SIZE,
  getTerrainChunkFromPosition,
} from '../components/game-scene/terrain-utils'
import type {
  HouseData,
  PassabilityGrid,
  RoomData,
  WallConfig,
} from '../types/housing'
import { floorYBase, type WallDirection } from '../utils/house-geometry'

export function getWallByDir(room: RoomData, dir: WallDirection): WallConfig[] {
  switch (dir) {
    case 'north':
      return room.wallNorth
    case 'south':
      return room.wallSouth
    case 'east':
      return room.wallEast
    case 'west':
      return room.wallWest
  }
}

/** Virtual wall half-thickness — player stops this far from the wall plane */
const WALL_HALF_THICKNESS = 0.3

// Cell edge bitmask constants
const EDGE_N = 1 // -Z edge (north wall)
const EDGE_E = 2 // +X edge (east wall)
const EDGE_S = 4 // +Z edge (south wall)
const EDGE_W = 8 // -X edge (west wall)

const ALL_WALL_DIRS: WallDirection[] = ['north', 'south', 'east', 'west']

/** Check if a wall segment blocks passage (everything except 'open') */
function isWallBlocking(seg: WallConfig): boolean {
  return seg.variant !== 'open'
}

/**
 * Build passability grids for a house. Stores static structure (all doors treated as blocked).
 * Returns one grid per floor level (including stairwell entries on both floors).
 */
export function buildPassability(house: HouseData): PassabilityGrid[] {
  // Group rooms by floor level, collecting bounding boxes
  const floorMap = new Map<
    number,
    { minX: number; minZ: number; maxX: number; maxZ: number }
  >()

  for (const room of house.rooms) {
    const rx = room.localX
    const rz = room.localZ
    const levels =
      room.roomType === 'stairwell' && room.floorLevel === 0
        ? [0, 1] // stairwell on 1F registers on both 1F and 2F grids
        : [room.floorLevel]

    for (const fl of levels) {
      const existing = floorMap.get(fl)
      if (existing) {
        existing.minX = Math.min(existing.minX, rx)
        existing.minZ = Math.min(existing.minZ, rz)
        existing.maxX = Math.max(existing.maxX, rx + room.sizeX)
        existing.maxZ = Math.max(existing.maxZ, rz + room.sizeZ)
      } else {
        floorMap.set(fl, {
          minX: rx,
          minZ: rz,
          maxX: rx + room.sizeX,
          maxZ: rz + room.sizeZ,
        })
      }
    }
  }

  const grids: PassabilityGrid[] = []

  for (const [floorLevel, bounds] of floorMap) {
    const originX = bounds.minX
    const originZ = bounds.minZ
    const width = bounds.maxX - bounds.minX
    const depth = bounds.maxZ - bounds.minZ
    const cells = new Array<number>(width * depth).fill(0)

    const setEdge = (cx: number, cz: number, edge: number) => {
      const gx = cx - originX
      const gz = cz - originZ
      if (gx >= 0 && gx < width && gz >= 0 && gz < depth) {
        cells[gx + gz * width] |= edge
      }
    }

    for (const room of house.rooms) {
      const rx = room.localX
      const rz = room.localZ

      if (room.roomType === 'stairwell' && room.floorLevel === 0) {
        // Stairwell: block all outer edges, open only the appropriate landing
        buildStairwellEdges(room, rx, rz, floorLevel, setEdge)
        continue
      }

      if (room.floorLevel !== floorLevel) continue

      // North wall at rz (sizeX segments)
      for (let i = 0; i < room.sizeX; i++) {
        if (i < room.wallNorth.length && isWallBlocking(room.wallNorth[i])) {
          setEdge(rx + i, rz, EDGE_N)
          setEdge(rx + i, rz - 1, EDGE_S)
        }
      }
      // South wall at rz + sizeZ (sizeX segments)
      for (let i = 0; i < room.sizeX; i++) {
        if (i < room.wallSouth.length && isWallBlocking(room.wallSouth[i])) {
          setEdge(rx + i, rz + room.sizeZ - 1, EDGE_S)
          setEdge(rx + i, rz + room.sizeZ, EDGE_N)
        }
      }
      // West wall at rx (sizeZ segments)
      for (let i = 0; i < room.sizeZ; i++) {
        if (i < room.wallWest.length && isWallBlocking(room.wallWest[i])) {
          setEdge(rx, rz + i, EDGE_W)
          setEdge(rx - 1, rz + i, EDGE_E)
        }
      }
      // East wall at rx + sizeX (sizeZ segments)
      for (let i = 0; i < room.sizeZ; i++) {
        if (i < room.wallEast.length && isWallBlocking(room.wallEast[i])) {
          setEdge(rx + room.sizeX - 1, rz + i, EDGE_E)
          setEdge(rx + room.sizeX, rz + i, EDGE_W)
        }
      }
    }

    grids.push({ floorLevel, originX, originZ, width, depth, cells })
  }

  return grids
}

/**
 * Build passability edges for a stairwell room on a specific floor level.
 *
 * Stairwell orientation:
 * - alongZ (sizeZ >= sizeX): entry(1F)=north, exit(2F)=south
 * - alongX (sizeX > sizeZ):  entry(1F)=west,  exit(2F)=east
 *
 * Landing cells (first/last row along stair axis) have NO edge bits —
 * they are open platforms for entry/exit. Only stair-run cells (middle rows)
 * get side-wall edges blocked.
 *
 * Cross-axis ends (perpendicular to stair direction):
 * - Entry end: open on this floor's grid (1F→north/west, 2F→south/east)
 * - Exit end: blocked on this floor's grid
 */
function buildStairwellEdges(
  room: RoomData,
  rx: number,
  rz: number,
  floorLevel: number,
  setEdge: (cx: number, cz: number, edge: number) => void
) {
  const alongZ = room.sizeZ >= room.sizeX

  // Side wall range: skip only the open landing row for this floor
  // 1F: entry (low) landing open → skip row 0, include row sizeN-1
  // 2F: exit (high) landing open → include row 0, skip row sizeN-1
  const sideStart = floorLevel === 0 ? 1 : 0
  const alongSize = alongZ ? room.sizeZ : room.sizeX
  const sideEnd = floorLevel === 1 ? alongSize - 1 : alongSize

  if (alongZ) {
    // Stair axis = Z. Side walls = east/west. Ends = north/south.
    for (let i = sideStart; i < sideEnd; i++) {
      setEdge(rx, rz + i, EDGE_W)
      setEdge(rx - 1, rz + i, EDGE_E)
      setEdge(rx + room.sizeX - 1, rz + i, EDGE_E)
      setEdge(rx + room.sizeX, rz + i, EDGE_W)
    }

    // North end (entry on 1F, blocked on 2F)
    if (floorLevel !== 0) {
      for (let i = 0; i < room.sizeX; i++) {
        setEdge(rx + i, rz, EDGE_N)
        setEdge(rx + i, rz - 1, EDGE_S)
      }
    }

    // South end (blocked on 1F, exit on 2F)
    if (floorLevel !== 1) {
      for (let i = 0; i < room.sizeX; i++) {
        setEdge(rx + i, rz + room.sizeZ - 1, EDGE_S)
        setEdge(rx + i, rz + room.sizeZ, EDGE_N)
      }
    }
  } else {
    // Stair axis = X. Side walls = north/south. Ends = west/east.
    for (let i = sideStart; i < sideEnd; i++) {
      setEdge(rx + i, rz, EDGE_N)
      setEdge(rx + i, rz - 1, EDGE_S)
      setEdge(rx + i, rz + room.sizeZ - 1, EDGE_S)
      setEdge(rx + i, rz + room.sizeZ, EDGE_N)
    }

    // West end (entry on 1F, blocked on 2F)
    if (floorLevel !== 0) {
      for (let i = 0; i < room.sizeZ; i++) {
        setEdge(rx, rz + i, EDGE_W)
        setEdge(rx - 1, rz + i, EDGE_E)
      }
    }

    // East end (blocked on 1F, exit on 2F)
    if (floorLevel !== 1) {
      for (let i = 0; i < room.sizeZ; i++) {
        setEdge(rx + room.sizeX - 1, rz + i, EDGE_E)
        setEdge(rx + room.sizeX, rz + i, EDGE_W)
      }
    }
  }
}

/** Runtime passability grid with Y-range info for floor matching */
interface RuntimeFloorGrid {
  floorLevel: number
  originX: number
  originZ: number
  width: number
  depth: number
  yBase: number
  wallHeight: number
  cells: number[]
}

interface RuntimePassability {
  houseOriginX: number
  houseOriginZ: number
  minX: number
  maxX: number
  minZ: number
  maxZ: number
  floors: RuntimeFloorGrid[]
}

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

  /** Optimistic local toggle — flips isOpen and updates passability. */
  toggleDoor(
    houseId: string,
    roomIndex: number,
    wallDir: WallDirection,
    segmentIndex: number
  ) {
    const house = this.housesById.get(houseId)
    if (!house) return
    const room = house.rooms[roomIndex]
    if (!room) return

    const seg = getWallByDir(room, wallDir)[segmentIndex]
    if (!seg) return

    seg.isOpen = !seg.isOpen
    this.updateDoorEdge(houseId, room, wallDir, segmentIndex, seg.isOpen)
    this.notifyChanged()
  }

  /** Handle a door toggle from another player or server confirmation. */
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
    this.updateDoorEdge(houseId, room, wallDir, segmentIndex, isOpen)
    this.notifyChanged()
  }

  /** Find the nearest door segment within maxDist of (x, z). */
  findNearestDoor(
    x: number,
    z: number,
    y: number,
    maxDist: number
  ): {
    houseId: string
    roomIndex: number
    wallDir: WallDirection
    segmentIndex: number
    distance: number
  } | null {
    let best: ReturnType<typeof this.findNearestDoor> = null

    const dirs: [WallDirection, number][] = [
      ['north', 0],
      ['south', 0],
      ['east', 1],
      ['west', 1],
    ]

    for (const house of this.housesById.values()) {
      for (let ri = 0; ri < house.rooms.length; ri++) {
        const room = house.rooms[ri]
        const ryBase =
          house.origin.y + floorYBase(room.floorLevel, room.wallHeight)
        if (y < ryBase - 0.5 || y >= ryBase + room.wallHeight) continue

        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ

        for (const [dir, axis] of dirs) {
          const segs = getWallByDir(room, dir)
          const wallCoord =
            dir === 'north'
              ? rz
              : dir === 'south'
                ? rz + room.sizeZ
                : dir === 'east'
                  ? rx + room.sizeX
                  : rx

          for (let si = 0; si < segs.length; si++) {
            if (segs[si].variant !== 'door') continue

            const segCenter = si + 0.5
            const startB = axis === 0 ? rx : rz
            const doorB = startB + segCenter

            const dx = axis === 0 ? doorB - x : wallCoord - x
            const dz = axis === 0 ? wallCoord - z : doorB - z
            const dist = Math.sqrt(dx * dx + dz * dz)

            if (dist < maxDist && (!best || dist < best.distance)) {
              best = {
                houseId: house.id,
                roomIndex: ri,
                wallDir: dir,
                segmentIndex: si,
                distance: dist,
              }
            }
          }
        }
      }
    }

    return best
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
    const minX = Math.min(fromX, toX) - WALL_HALF_THICKNESS
    const maxX = Math.max(fromX, toX) + WALL_HALF_THICKNESS
    const minZ = Math.min(fromZ, toZ) - WALL_HALF_THICKNESS
    const maxZ = Math.max(fromZ, toZ) + WALL_HALF_THICKNESS

    for (const rp of this.passabilityCache.values()) {
      // AABB fast rejection
      if (maxX < rp.minX || minX > rp.maxX || maxZ < rp.minZ || minZ > rp.maxZ)
        continue

      for (const floor of rp.floors) {
        if (y < floor.yBase - 0.5 || y >= floor.yBase + floor.wallHeight)
          continue

        // Convert world coords to grid-local coords
        const localFromX = fromX - rp.houseOriginX - floor.originX
        const localFromZ = fromZ - rp.houseOriginZ - floor.originZ
        const localToX = toX - rp.houseOriginX - floor.originX
        const localToZ = toZ - rp.houseOriginZ - floor.originZ

        // Check X-axis edge crossings
        if (
          this.edgeBlocksAxis(
            localFromX,
            localToX,
            localFromZ,
            localToZ,
            floor,
            true
          )
        )
          return true

        // Check Z-axis edge crossings
        if (
          this.edgeBlocksAxis(
            localFromZ,
            localToZ,
            localFromX,
            localToX,
            floor,
            false
          )
        )
          return true
      }
    }

    return false
  }

  /**
   * Check if movement along one axis crosses a blocked cell edge.
   * When xAxis=true, checks east/west edges. When false, checks north/south edges.
   */
  private edgeBlocksAxis(
    fromA: number,
    toA: number,
    fromB: number,
    toB: number,
    floor: RuntimeFloorGrid,
    xAxis: boolean
  ): boolean {
    const sizeA = xAxis ? floor.width : floor.depth
    const sizeB = xAxis ? floor.depth : floor.width
    const w = floor.width
    const idx = xAxis
      ? (a: number, b: number) => a + b * w
      : (a: number, b: number) => b + a * w

    const fromCell = Math.floor(fromA)
    const toCell = Math.floor(toA)

    if (fromCell !== toCell) {
      const step = toCell > fromCell ? 1 : -1
      const leaveBit =
        step > 0 ? (xAxis ? EDGE_E : EDGE_S) : xAxis ? EDGE_W : EDGE_N
      const enterBit =
        step > 0 ? (xAxis ? EDGE_W : EDGE_N) : xAxis ? EDGE_E : EDGE_S
      let cell = fromCell
      while (cell !== toCell) {
        const edgeCoord = step > 0 ? cell + 1 : cell
        const nextCell = cell + step
        const t = (edgeCoord - fromA) / (toA - fromA)
        const cellB = Math.floor(fromB + t * (toB - fromB))
        if (cellB >= 0 && cellB < sizeB) {
          if (cell >= 0 && cell < sizeA) {
            if (floor.cells[idx(cell, cellB)] & leaveBit) return true
          }
          if (nextCell >= 0 && nextCell < sizeA) {
            if (floor.cells[idx(nextCell, cellB)] & enterBit) return true
          }
        }
        cell += step
      }
    }

    // Proximity check: approaching a cell edge within WALL_HALF_THICKNESS
    const nearestEdge = Math.round(toA)
    const toDist = Math.abs(toA - nearestEdge)
    if (
      toDist < WALL_HALF_THICKNESS &&
      toDist < Math.abs(fromA - nearestEdge)
    ) {
      const cellB = Math.floor(toB)
      if (cellB < 0 || cellB >= sizeB) return false
      const cellBefore = nearestEdge - 1
      const cellAfter = nearestEdge
      if (cellBefore >= 0 && cellBefore < sizeA) {
        if (floor.cells[idx(cellBefore, cellB)] & (xAxis ? EDGE_E : EDGE_S))
          return true
      }
      if (cellAfter >= 0 && cellAfter < sizeA) {
        if (floor.cells[idx(cellAfter, cellB)] & (xAxis ? EDGE_W : EDGE_N))
          return true
      }
    }

    return false
  }

  /** Update passability edge bits when a door is opened or closed. */
  private updateDoorEdge(
    houseId: string,
    room: RoomData,
    wallDir: WallDirection,
    segmentIndex: number,
    isOpen: boolean
  ) {
    const rp = this.passabilityCache.get(houseId)
    if (!rp) return

    const floor = rp.floors.find((f) => f.floorLevel === room.floorLevel)
    if (!floor) return

    const rx = room.localX - floor.originX
    const rz = room.localZ - floor.originZ

    let cx: number,
      cz: number,
      edge: number,
      adjCx: number,
      adjCz: number,
      adjEdge: number
    switch (wallDir) {
      case 'north': {
        cx = rx + segmentIndex
        cz = rz
        edge = EDGE_N
        adjCx = cx
        adjCz = cz - 1
        adjEdge = EDGE_S
        break
      }
      case 'south': {
        cx = rx + segmentIndex
        cz = rz + room.sizeZ - 1
        edge = EDGE_S
        adjCx = cx
        adjCz = cz + 1
        adjEdge = EDGE_N
        break
      }
      case 'west': {
        cx = rx
        cz = rz + segmentIndex
        edge = EDGE_W
        adjCx = cx - 1
        adjCz = cz
        adjEdge = EDGE_E
        break
      }
      case 'east': {
        cx = rx + room.sizeX - 1
        cz = rz + segmentIndex
        edge = EDGE_E
        adjCx = cx + 1
        adjCz = cz
        adjEdge = EDGE_W
        break
      }
    }

    const setOrClear = (gx: number, gz: number, bit: number) => {
      if (gx < 0 || gx >= floor.width || gz < 0 || gz >= floor.depth) return
      const idx = gx + gz * floor.width
      if (isOpen) {
        floor.cells[idx] &= ~bit
      } else {
        floor.cells[idx] |= bit
      }
    }

    setOrClear(cx, cz, edge)
    setOrClear(adjCx, adjCz, adjEdge)
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
    this.buildRuntimePassability(house)
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

  /** Build runtime passability from stored grids (or compute if missing). */
  private buildRuntimePassability(house: HouseData) {
    const grids = house.passability?.length
      ? house.passability
      : buildPassability(house)

    // Compute world-space AABB across all floors
    let minX = Infinity
    let maxX = -Infinity
    let minZ = Infinity
    let maxZ = -Infinity

    const floors: RuntimeFloorGrid[] = grids.map((g) => {
      const worldMinX = house.origin.x + g.originX
      const worldMinZ = house.origin.z + g.originZ
      const worldMaxX = worldMinX + g.width
      const worldMaxZ = worldMinZ + g.depth
      minX = Math.min(minX, worldMinX)
      maxX = Math.max(maxX, worldMaxX)
      minZ = Math.min(minZ, worldMinZ)
      maxZ = Math.max(maxZ, worldMaxZ)

      // Find wallHeight for this floor level from rooms
      let wallHeight = 3
      let yBase = house.origin.y
      for (const room of house.rooms) {
        if (room.floorLevel === g.floorLevel) {
          wallHeight = room.wallHeight
          yBase = house.origin.y + floorYBase(room.floorLevel, room.wallHeight)
          break
        }
        // For 2F stairwell grid derived from a 1F stairwell
        if (
          room.roomType === 'stairwell' &&
          room.floorLevel === 0 &&
          g.floorLevel === 1
        ) {
          wallHeight = room.wallHeight
          yBase = house.origin.y + floorYBase(1, room.wallHeight)
          break
        }
      }

      return {
        floorLevel: g.floorLevel,
        originX: g.originX,
        originZ: g.originZ,
        width: g.width,
        depth: g.depth,
        yBase,
        wallHeight,
        cells: g.cells,
      }
    })

    const rp: RuntimePassability = {
      houseOriginX: house.origin.x,
      houseOriginZ: house.origin.z,
      minX,
      maxX,
      minZ,
      maxZ,
      floors,
    }

    this.passabilityCache.set(house.id, rp)

    // Apply current door states as overlay (clear bits for open doors)
    for (const room of house.rooms) {
      for (const dir of ALL_WALL_DIRS) {
        const segs = getWallByDir(room, dir)
        for (let i = 0; i < segs.length; i++) {
          if (segs[i].variant === 'door' && segs[i].isOpen) {
            this.updateDoorEdge(house.id, room, dir, i, true)
          }
        }
      }
    }
  }

  private notifyChanged() {
    const all = this.getAllHouses()
    for (const cb of this.housesChangedListeners) cb(all)
  }
}

export const housingManager = new HousingManager()
