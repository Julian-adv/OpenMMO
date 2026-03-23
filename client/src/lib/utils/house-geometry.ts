/**
 * house-geometry.ts — Assembles a THREE.Group from HouseData.
 *
 * Geometries are grouped by (isFront, textureIndex) and merged into one mesh
 * per group. Each mesh uses a shared MeshStandardMaterial from housing-textures.ts.
 *
 * Front group: south walls + west walls + roofs (hidden when player is inside)
 * Back group:  north walls + east walls + floors (always visible)
 */
import * as THREE from 'three'
import type { HouseData, RoomData } from '../types/housing'
import {
  addMergedMeshes,
  collectFootprints,
  computeHouseAABB,
  getOrCreateFloorEntries,
  type DoorMeshInfo,
  type FloorEntries,
  type GeoEntry,
  type HouseGroupResult,
  type RoomFootprint,
} from './house-geo-utils'
import { collectFloorGeometry } from './house-geo-floor'
import { collectRoofGeometry, shouldSuppressRoof } from './house-geo-roof'
import { collectStairwellGeometries } from './house-geo-stairwell'
import { collectWallSegments } from './house-geo-walls'

// Re-export public API so existing imports continue to work
export {
  WALL_THICKNESS,
  FLOOR_THICKNESS,
  DEFAULT_WALL_HEIGHT,
  LANDING_DEPTH,
  MAX_FLOOR_LEVEL,
  OFFSCREEN_Y,
  floorYBase,
  type WallDirection,
  type DoorMeshInfo,
  type HouseGroupResult,
} from './house-geo-utils'
export { getStairwellYOffset } from './house-geo-stairwell'

export function buildHouseGroup(
  house: HouseData,
  roomsHash?: string
): HouseGroupResult {
  const houseGroup = new THREE.Group()
  houseGroup.position.set(house.origin.x, house.origin.y, house.origin.z)
  houseGroup.name = `house_${house.id}`

  const stairwellFootprints = collectFootprints(
    house.rooms,
    (r) => r.roomType === 'stairwell'
  )

  // Pre-compute footprints per floor level for roof suppression checks
  const footprintsByFloor = new Map<number, RoomFootprint[]>()
  for (const room of house.rooms) {
    if (!footprintsByFloor.has(room.floorLevel)) {
      footprintsByFloor.set(
        room.floorLevel,
        collectFootprints(house.rooms, (r) => r.floorLevel === room.floorLevel)
      )
    }
  }

  const perFloor = new Map<number, FloorEntries>()

  for (let ri = 0; ri < house.rooms.length; ri++) {
    const room = house.rooms[ri]
    const fl = room.floorLevel
    const entries = getOrCreateFloorEntries(perFloor, fl)

    collectRoomGeometries(
      room,
      ri,
      entries.front,
      entries.back,
      entries.doors,
      shouldSuppressRoof(room, footprintsByFloor.get(fl + 1) ?? []),
      house.rooms,
      stairwellFootprints
    )
  }

  const floorGroups = new Map<
    number,
    { front: THREE.Group; back: THREE.Group }
  >()

  let mergedMeshCount = 0
  const allDoors: DoorMeshInfo[] = []

  for (const [fl, entries] of perFloor) {
    const front = new THREE.Group()
    front.name = `front_f${fl}`
    const back = new THREE.Group()
    back.name = `back_f${fl}`
    mergedMeshCount += addMergedMeshes(front, entries.front)
    mergedMeshCount += addMergedMeshes(back, entries.back)

    for (const door of entries.doors) {
      allDoors.push(door)
    }

    houseGroup.add(front)
    houseGroup.add(back)
    floorGroups.set(fl, { front, back })
  }

  for (const door of allDoors) {
    door.pivot.userData = {
      doorHouseId: house.id,
      doorRoomIndex: door.roomIndex,
      doorWallDir: door.wallDir,
      doorSegmentIndex: door.segmentIndex,
      doorFloorLevel: door.floorLevel,
    }
    houseGroup.add(door.pivot)
  }

  return {
    houseGroup,
    floorGroups,
    aabb: computeHouseAABB(house),
    roomsHash: roomsHash ?? JSON.stringify(house.rooms),
    mergedMeshCount,
    doors: allDoors,
  }
}

function collectRoomGeometries(
  room: RoomData,
  roomIndex: number,
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[],
  doors: DoorMeshInfo[],
  suppressRoof: boolean,
  allRooms: RoomData[],
  stairwellFootprints: RoomFootprint[]
) {
  if (room.roomType === 'stairwell') {
    collectStairwellGeometries(room, backEntries, allRooms)
    return
  }

  collectFloorGeometry(room, backEntries, stairwellFootprints)
  if (!suppressRoof) collectRoofGeometry(room, frontEntries, backEntries)

  collectWallSegments(
    room.wallNorth,
    'north',
    room,
    roomIndex,
    frontEntries,
    backEntries,
    doors
  )
  collectWallSegments(
    room.wallSouth,
    'south',
    room,
    roomIndex,
    frontEntries,
    backEntries,
    doors
  )
  collectWallSegments(
    room.wallEast,
    'east',
    room,
    roomIndex,
    frontEntries,
    backEntries,
    doors
  )
  collectWallSegments(
    room.wallWest,
    'west',
    room,
    roomIndex,
    frontEntries,
    backEntries,
    doors
  )
}

/** Dispose merged geometries in a house group */
export function disposeHouseGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) {
      obj.geometry?.dispose()
    }
  })
}
