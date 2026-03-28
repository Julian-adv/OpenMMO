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
export const EDGE_N = 1 // -Z edge (north wall)
export const EDGE_E = 2 // +X edge (east wall)
export const EDGE_S = 4 // +Z edge (south wall)
export const EDGE_W = 8 // -X edge (west wall)

export const ALL_WALL_DIRS: WallDirection[] = ['north', 'south', 'east', 'west']

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
      room.roomType === 'stairwell'
        ? [room.floorLevel, room.floorLevel + 1] // stairwell registers on both its floor and the floor above
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

      if (room.roomType === 'stairwell') {
        if (
          floorLevel === room.floorLevel ||
          floorLevel === room.floorLevel + 1
        ) {
          const blockExitEnd =
            floorLevel === room.floorLevel &&
            !hasOverlappingStairwell(room, house.rooms, 'exit')
          const blockEntryEnd =
            floorLevel === room.floorLevel + 1 &&
            !hasOverlappingStairwell(room, house.rooms, 'entry')
          buildStairwellEdges(
            room,
            rx,
            rz,
            floorLevel,
            setEdge,
            blockExitEnd,
            blockEntryEnd
          )
        }
        continue
      }

      if (room.floorLevel !== floorLevel) continue

      for (let i = 0; i < room.sizeX; i++) {
        if (i < room.wallNorth.length && isWallBlocking(room.wallNorth[i])) {
          setEdge(rx + i, rz, EDGE_N)
          setEdge(rx + i, rz - 1, EDGE_S)
        }
      }
      for (let i = 0; i < room.sizeX; i++) {
        if (i < room.wallSouth.length && isWallBlocking(room.wallSouth[i])) {
          setEdge(rx + i, rz + room.sizeZ - 1, EDGE_S)
          setEdge(rx + i, rz + room.sizeZ, EDGE_N)
        }
      }
      for (let i = 0; i < room.sizeZ; i++) {
        if (i < room.wallWest.length && isWallBlocking(room.wallWest[i])) {
          setEdge(rx, rz + i, EDGE_W)
          setEdge(rx - 1, rz + i, EDGE_E)
        }
      }
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
 * Both ends along the stair axis are always open (no end walls).
 * Only side walls on the stair-run rows are blocked, skipping the
 * landing row for this floor:
 * - Entry floor: skip row 0 (entry landing)
 * - Exit floor: skip last row (exit landing)
 */
function buildStairwellEdges(
  room: RoomData,
  rx: number,
  rz: number,
  floorLevel: number,
  setEdge: (cx: number, cz: number, edge: number) => void,
  blockExitEnd: boolean,
  blockEntryEnd: boolean
) {
  const alongZ = room.sizeZ >= room.sizeX
  const alongSize = alongZ ? room.sizeZ : room.sizeX
  const reversed = room.stairReversed ?? false

  // Skip the landing row for this floor's open end
  const isEntryFloor = floorLevel === room.floorLevel
  const isExitFloor = floorLevel === room.floorLevel + 1
  // Skip landing row when it connects to an adjacent floor (open end)
  const skipEntryLanding = isEntryFloor || (isExitFloor && !blockEntryEnd)
  const skipExitLanding = isExitFloor || (isEntryFloor && !blockExitEnd)

  // When reversed, entry/exit physical positions swap (first row ↔ last row)
  const skipFirstRow = reversed ? skipExitLanding : skipEntryLanding
  const skipLastRow = reversed ? skipEntryLanding : skipExitLanding
  const blockFirstRow = reversed ? blockExitEnd : blockEntryEnd
  const blockLastRow = reversed ? blockEntryEnd : blockExitEnd

  const sideStart = skipFirstRow ? 1 : 0
  const sideEnd = skipLastRow ? alongSize - 1 : alongSize

  if (alongZ) {
    for (let i = sideStart; i < sideEnd; i++) {
      setEdge(rx, rz + i, EDGE_W)
      setEdge(rx - 1, rz + i, EDGE_E)
      setEdge(rx + room.sizeX - 1, rz + i, EDGE_E)
      setEdge(rx + room.sizeX, rz + i, EDGE_W)
    }
    if (blockLastRow) {
      for (let x = 0; x < room.sizeX; x++) {
        setEdge(rx + x, rz + room.sizeZ - 1, EDGE_S)
        setEdge(rx + x, rz + room.sizeZ, EDGE_N)
      }
    }
    if (blockFirstRow) {
      for (let x = 0; x < room.sizeX; x++) {
        setEdge(rx + x, rz, EDGE_N)
        setEdge(rx + x, rz - 1, EDGE_S)
      }
    }
  } else {
    for (let i = sideStart; i < sideEnd; i++) {
      setEdge(rx + i, rz, EDGE_N)
      setEdge(rx + i, rz - 1, EDGE_S)
      setEdge(rx + i, rz + room.sizeZ - 1, EDGE_S)
      setEdge(rx + i, rz + room.sizeZ, EDGE_N)
    }
    if (blockLastRow) {
      for (let z = 0; z < room.sizeZ; z++) {
        setEdge(rx + room.sizeX - 1, rz + z, EDGE_E)
        setEdge(rx + room.sizeX, rz + z, EDGE_W)
      }
    }
    if (blockFirstRow) {
      for (let z = 0; z < room.sizeZ; z++) {
        setEdge(rx, rz + z, EDGE_W)
        setEdge(rx - 1, rz + z, EDGE_E)
      }
    }
  }
}

/**
 * Check if a stairwell landing overlaps with any stairwell on an adjacent floor.
 * 'exit' checks exit landing vs floor below; 'entry' checks entry landing vs floor above.
 */
function hasOverlappingStairwell(
  stairwell: RoomData,
  rooms: RoomData[],
  end: 'entry' | 'exit'
): boolean {
  const alongZ = stairwell.sizeZ >= stairwell.sizeX
  const reversed = stairwell.stairReversed ?? false
  const rx = stairwell.localX
  const rz = stairwell.localZ

  // Landing bounding box: entry = first row, exit = last row along stair axis
  // When reversed, physical positions swap
  const physicalEnd = reversed ? (end === 'exit' ? 'entry' : 'exit') : end
  let minX: number, maxX: number, minZ: number, maxZ: number
  if (physicalEnd === 'exit') {
    if (alongZ) {
      minX = rx
      maxX = rx + stairwell.sizeX
      minZ = rz + stairwell.sizeZ - 1
      maxZ = rz + stairwell.sizeZ
    } else {
      minX = rx + stairwell.sizeX - 1
      maxX = rx + stairwell.sizeX
      minZ = rz
      maxZ = rz + stairwell.sizeZ
    }
  } else {
    if (alongZ) {
      minX = rx
      maxX = rx + stairwell.sizeX
      minZ = rz
      maxZ = rz + 1
    } else {
      minX = rx
      maxX = rx + 1
      minZ = rz
      maxZ = rz + stairwell.sizeZ
    }
  }

  const targetFloor =
    end === 'exit' ? stairwell.floorLevel - 1 : stairwell.floorLevel + 1

  for (const other of rooms) {
    if (other === stairwell) continue
    if (other.roomType !== 'stairwell') continue
    if (other.floorLevel !== targetFloor) continue

    if (
      minX < other.localX + other.sizeX &&
      maxX > other.localX &&
      minZ < other.localZ + other.sizeZ &&
      maxZ > other.localZ
    ) {
      return true
    }
  }
  return false
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

export interface StairwellInfo {
  /** House-local cell bounds (integers, max exclusive) */
  localMinX: number
  localMinZ: number
  localMaxX: number
  localMaxZ: number
  lowerFloor: number
  upperFloor: number
}

export interface RuntimePassability {
  houseOriginX: number
  houseOriginZ: number
  minX: number
  maxX: number
  minZ: number
  maxZ: number
  floors: RuntimeFloorGrid[]
  stairwells: StairwellInfo[]
}

/** Build runtime passability from stored grids (or compute if missing). */
export function buildRuntimePassability(house: HouseData): RuntimePassability {
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
      // For upper-floor grid derived from a stairwell
      if (
        room.roomType === 'stairwell' &&
        g.floorLevel === room.floorLevel + 1
      ) {
        wallHeight = room.wallHeight
        yBase = house.origin.y + floorYBase(g.floorLevel, room.wallHeight)
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

  const stairwells: StairwellInfo[] = []
  for (const room of house.rooms) {
    if (room.roomType === 'stairwell') {
      stairwells.push({
        localMinX: room.localX,
        localMinZ: room.localZ,
        localMaxX: room.localX + room.sizeX,
        localMaxZ: room.localZ + room.sizeZ,
        lowerFloor: room.floorLevel,
        upperFloor: room.floorLevel + 1,
      })
    }
  }

  return {
    houseOriginX: house.origin.x,
    houseOriginZ: house.origin.z,
    minX,
    maxX,
    minZ,
    maxZ,
    floors,
    stairwells,
  }
}

/**
 * Check if movement from→to is blocked by any cell edge.
 * Uses precomputed passability grids with WALL_HALF_THICKNESS proximity buffer.
 */
export function isMovementBlocked(
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
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

  for (const rp of passabilityCache.values()) {
    // AABB fast rejection
    if (maxX < rp.minX || minX > rp.maxX || maxZ < rp.minZ || minZ > rp.maxZ)
      continue

    for (const floor of rp.floors) {
      if (y < floor.yBase - 0.5 || y >= floor.yBase + floor.wallHeight) continue

      // Convert world coords to grid-local coords
      const localFromX = fromX - rp.houseOriginX - floor.originX
      const localFromZ = fromZ - rp.houseOriginZ - floor.originZ
      const localToX = toX - rp.houseOriginX - floor.originX
      const localToZ = toZ - rp.houseOriginZ - floor.originZ

      // Check X-axis edge crossings
      if (
        edgeBlocksAxis(localFromX, localToX, localFromZ, localToZ, floor, true)
      )
        return true

      // Check Z-axis edge crossings
      if (
        edgeBlocksAxis(localFromZ, localToZ, localFromX, localToX, floor, false)
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
function edgeBlocksAxis(
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
  if (toDist < WALL_HALF_THICKNESS && toDist < Math.abs(fromA - nearestEdge)) {
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

/**
 * Check if a cardinal (1-cell) move is blocked on a specific floor level.
 * Unlike isMovementBlocked, this matches by floorLevel directly (no Y range check)
 * and has no proximity buffer — designed for A* cell-to-cell expansion.
 */
export function isCardinalMoveBlocked(
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  cellX: number,
  cellZ: number,
  dx: number,
  dz: number,
  floorLevel: number
): boolean {
  const nx = cellX + dx
  const nz = cellZ + dz
  let leaveBit: number, enterBit: number
  if (dx === 1) {
    leaveBit = EDGE_E
    enterBit = EDGE_W
  } else if (dx === -1) {
    leaveBit = EDGE_W
    enterBit = EDGE_E
  } else if (dz === 1) {
    leaveBit = EDGE_S
    enterBit = EDGE_N
  } else {
    leaveBit = EDGE_N
    enterBit = EDGE_S
  }

  for (const rp of passabilityCache.values()) {
    if (cellX < rp.minX || nx > rp.maxX || cellZ < rp.minZ || nz > rp.maxZ)
      continue
    const oX = rp.houseOriginX
    const oZ = rp.houseOriginZ
    for (const floor of rp.floors) {
      if (floor.floorLevel !== floorLevel) continue
      const fX = oX + floor.originX
      const fZ = oZ + floor.originZ

      const gx = cellX - fX
      const gz = cellZ - fZ
      if (gx >= 0 && gx < floor.width && gz >= 0 && gz < floor.depth) {
        if (floor.cells[gx + gz * floor.width] & leaveBit) return true
      }

      const ngx = nx - fX
      const ngz = nz - fZ
      if (ngx >= 0 && ngx < floor.width && ngz >= 0 && ngz < floor.depth) {
        if (floor.cells[ngx + ngz * floor.width] & enterBit) return true
      }
    }
  }
  return false
}

/**
 * Get the floor level at a world position based on Y height.
 * Returns 0 (ground) if outside any house.
 */
export function getFloorAtPosition(
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  x: number,
  z: number,
  y: number
): number {
  const cx = Math.floor(x)
  const cz = Math.floor(z)
  for (const rp of passabilityCache.values()) {
    if (x < rp.minX || x > rp.maxX || z < rp.minZ || z > rp.maxZ) continue
    for (const floor of rp.floors) {
      if (y < floor.yBase - 0.5 || y >= floor.yBase + floor.wallHeight) continue
      const gx = cx - rp.houseOriginX - floor.originX
      const gz = cz - rp.houseOriginZ - floor.originZ
      if (gx >= 0 && gx < floor.width && gz >= 0 && gz < floor.depth) {
        return floor.floorLevel
      }
    }
  }
  return 0
}

/**
 * Get the yBase for a given floor level at a world position.
 * Returns undefined if no house floor matches.
 */
export function getFloorYBase(
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  x: number,
  z: number,
  floorLevel: number
): number | undefined {
  for (const rp of passabilityCache.values()) {
    if (x < rp.minX || x > rp.maxX || z < rp.minZ || z > rp.maxZ) continue
    for (const floor of rp.floors) {
      if (floor.floorLevel !== floorLevel) continue
      const gx = Math.floor(x) - rp.houseOriginX - floor.originX
      const gz = Math.floor(z) - rp.houseOriginZ - floor.originZ
      if (gx >= 0 && gx < floor.width && gz >= 0 && gz < floor.depth) {
        return floor.yBase
      }
    }
  }
  return undefined
}

/** Update passability edge bits when a door is opened or closed. */
export function updateDoorEdge(
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  houseId: string,
  room: RoomData,
  wallDir: WallDirection,
  segmentIndex: number,
  isOpen: boolean
) {
  const rp = passabilityCache.get(houseId)
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
