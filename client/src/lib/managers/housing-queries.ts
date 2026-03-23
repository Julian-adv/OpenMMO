import type { HouseData } from '../types/housing'
import { floorYBase, type WallDirection } from '../utils/house-geometry'
import { getWallByDir } from './housing-passability'

/** Find the first room containing a world point (fast, no allocation). */
export function findRoomAtPoint(
  housesById: ReadonlyMap<string, HouseData>,
  x: number,
  y: number,
  z: number
): { house: HouseData; roomIndex: number } | null {
  for (const house of housesById.values()) {
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
export function findAllRoomsAtPoint(
  housesById: ReadonlyMap<string, HouseData>,
  x: number,
  y: number,
  z: number
): { house: HouseData; roomIndex: number }[] {
  const results: { house: HouseData; roomIndex: number }[] = []
  for (const house of housesById.values()) {
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
        results.push({ house, roomIndex: i })
      }
    }
  }
  return results
}

/** Find the house whose room contains a world point, or null. */
export function findHouseAtPoint(
  housesById: ReadonlyMap<string, HouseData>,
  x: number,
  y: number,
  z: number
): HouseData | null {
  const result = findRoomAtPoint(housesById, x, y, z)
  return result ? result.house : null
}

/** Find the nearest door segment within maxDist of (x, z). */
export function findNearestDoor(
  housesById: ReadonlyMap<string, HouseData>,
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
  let best: ReturnType<typeof findNearestDoor> = null

  const dirs: [WallDirection, number][] = [
    ['north', 0],
    ['south', 0],
    ['east', 1],
    ['west', 1],
  ]

  for (const house of housesById.values()) {
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

/** Find an existing house that shares an edge with the given room footprint. */
export function findAdjacentHouse(
  housesById: ReadonlyMap<string, HouseData>,
  originX: number,
  originZ: number,
  sizeX: number,
  sizeZ: number
): HouseData | null {
  for (const house of housesById.values()) {
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
export function checkOverlap(
  housesById: ReadonlyMap<string, HouseData>,
  originX: number,
  originZ: number,
  sizeX: number,
  sizeZ: number,
  floorLevel: number = 0
): boolean {
  for (const house of housesById.values()) {
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
export function hasFloorSupport(
  housesById: ReadonlyMap<string, HouseData>,
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
      for (const house of housesById.values()) {
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
export function findSupportingHouse(
  housesById: ReadonlyMap<string, HouseData>,
  originX: number,
  originZ: number,
  sizeX: number,
  sizeZ: number
): HouseData | null {
  for (const house of housesById.values()) {
    if (hasFloorSupport(housesById, originX, originZ, sizeX, sizeZ, house.id)) {
      return house
    }
  }
  return null
}
