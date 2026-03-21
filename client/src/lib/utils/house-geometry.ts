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
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import type { HouseData, RoomData, WallConfig } from '../types/housing'
import { getHousingMaterial, HOUSING_TEXTURES } from './housing-textures'
import type { InstanceDescriptor } from './housing-instance-pool'

export const WALL_THICKNESS = 0.1
export const FLOOR_THICKNESS = 0.1
export const DEFAULT_WALL_HEIGHT = 3
const DOOR_WIDTH = 1.0
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 1.0
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2
const LANDING_DEPTH = 0.5

/** Y offset used to hide front walls instead of toggling visible (WebGPU workaround) */
export const OFFSCREEN_Y = -10000

/** Compute the Y base for a given floor level, accounting for floor thickness. */
export function floorYBase(floorLevel: number, wallHeight: number): number {
  return floorLevel * (wallHeight + FLOOR_THICKNESS)
}

// Wall direction descriptors
interface WallDirInfo {
  isNS: boolean
  isFront: boolean
}

const WALL_DIR_INFO: Record<WallDirection, WallDirInfo> = {
  north: { isNS: true, isFront: false },
  south: { isNS: true, isFront: true },
  east: { isNS: false, isFront: false },
  west: { isNS: false, isFront: true },
}

type WallDirection = 'north' | 'south' | 'east' | 'west'

export interface HouseGroupResult {
  houseGroup: THREE.Group
  /** Per-floor groups: key = floorLevel, value = { front, back } */
  floorGroups: Map<number, { front: THREE.Group; back: THREE.Group }>
  aabb: THREE.Box3
  /** JSON hash of rooms for change detection */
  roomsHash: string
}

export interface HouseGeometryResult {
  /** Instance descriptors for the pool (local-space positions). */
  instances: InstanceDescriptor[]
  /** Merged group for non-instanceable parts (door/window frames, stairwells). */
  mergedGroup: THREE.Group
  /** Per-floor groups within mergedGroup for visibility control. */
  mergedFloorGroups: Map<number, { front: THREE.Group; back: THREE.Group }>
  aabb: THREE.Box3
  roomsHash: string
}

const _aabbVec = new THREE.Vector3()
const _tmpMatrix = new THREE.Matrix4()

interface GeoEntry {
  geo: THREE.BufferGeometry
  textureIndex: number
}

interface RoomFootprint {
  x: number
  z: number
  sx: number
  sz: number
}

function collectFootprints(
  rooms: RoomData[],
  predicate: (room: RoomData) => boolean
): RoomFootprint[] {
  const result: RoomFootprint[] = []
  for (const room of rooms) {
    if (predicate(room)) {
      result.push({
        x: room.localX,
        z: room.localZ,
        sx: room.sizeX,
        sz: room.sizeZ,
      })
    }
  }
  return result
}

function cellInFootprint(cx: number, cz: number, fp: RoomFootprint): boolean {
  return cx >= fp.x && cx < fp.x + fp.sx && cz >= fp.z && cz < fp.z + fp.sz
}

type FloorEntries = { front: GeoEntry[]; back: GeoEntry[] }

function getOrCreateFloorEntries(
  perFloor: Map<number, FloorEntries>,
  fl: number
): FloorEntries {
  let entries = perFloor.get(fl)
  if (!entries) {
    entries = { front: [], back: [] }
    perFloor.set(fl, entries)
  }
  return entries
}

function computeHouseAABB(house: HouseData): THREE.Box3 {
  const aabb = new THREE.Box3()
  for (const room of house.rooms) {
    const yBase = floorYBase(room.floorLevel, room.wallHeight)
    const minX = house.origin.x + room.localX
    const minZ = house.origin.z + room.localZ
    _aabbVec.set(minX, house.origin.y + yBase, minZ)
    aabb.expandByPoint(_aabbVec)
    _aabbVec.set(
      minX + room.sizeX,
      house.origin.y + yBase + room.wallHeight,
      minZ + room.sizeZ
    )
    aabb.expandByPoint(_aabbVec)
  }
  return aabb
}

function shouldSuppressRoof(
  room: RoomData,
  secondFloorFootprints: RoomFootprint[]
): boolean {
  if (room.floorLevel !== 0 || secondFloorFootprints.length === 0) return false
  for (let x = room.localX; x < room.localX + room.sizeX; x++) {
    for (let z = room.localZ; z < room.localZ + room.sizeZ; z++) {
      if (!secondFloorFootprints.some((fp) => cellInFootprint(x, z, fp))) {
        return false
      }
    }
  }
  return true
}

export function buildHouseGroup(house: HouseData): HouseGroupResult {
  const houseGroup = new THREE.Group()
  houseGroup.position.set(house.origin.x, house.origin.y, house.origin.z)
  houseGroup.name = `house_${house.id}`

  // Build footprint sets for roof suppression and floor hole punching
  const secondFloorFootprints = collectFootprints(
    house.rooms,
    (r) => r.floorLevel >= 1
  )
  const stairwellFootprints = collectFootprints(
    house.rooms,
    (r) => r.roomType === 'stairwell'
  )

  // Collect geometry entries per floor level
  const perFloor = new Map<number, FloorEntries>()

  for (const room of house.rooms) {
    const fl = room.roomType === 'stairwell' ? 0 : room.floorLevel
    const entries = getOrCreateFloorEntries(perFloor, fl)

    collectRoomGeometries(
      room,
      entries.front,
      entries.back,
      shouldSuppressRoof(room, secondFloorFootprints),
      house.rooms,
      stairwellFootprints
    )
  }

  // Create per-floor groups and merge geometry
  const floorGroups = new Map<
    number,
    { front: THREE.Group; back: THREE.Group }
  >()

  for (const [fl, entries] of perFloor) {
    const front = new THREE.Group()
    front.name = `front_f${fl}`
    const back = new THREE.Group()
    back.name = `back_f${fl}`
    addMergedMeshes(front, entries.front)
    addMergedMeshes(back, entries.back)
    houseGroup.add(front)
    houseGroup.add(back)
    floorGroups.set(fl, { front, back })
  }

  return {
    houseGroup,
    floorGroups,
    aabb: computeHouseAABB(house),
    roomsHash: JSON.stringify(house.rooms),
  }
}

/**
 * Build instance descriptors + merged geometry for a house.
 * Solid walls → InstanceDescriptor (pool).
 * Floors, roofs, door/window frames, stairwell steps → merged geometry (per-house group).
 */
export function buildHouseGeometry(house: HouseData): HouseGeometryResult {
  const mergedGroup = new THREE.Group()
  mergedGroup.position.set(house.origin.x, house.origin.y, house.origin.z)
  mergedGroup.name = `house_merged_${house.id}`

  const instances: InstanceDescriptor[] = []

  const secondFloorFootprints = collectFootprints(
    house.rooms,
    (r) => r.floorLevel >= 1
  )
  const stairwellFootprints = collectFootprints(
    house.rooms,
    (r) => r.roomType === 'stairwell'
  )

  // Per-floor merged geometry entries (door/window frames + stairwells only)
  const perFloor = new Map<number, FloorEntries>()

  for (const room of house.rooms) {
    const fl = room.roomType === 'stairwell' ? 0 : room.floorLevel
    const entries = getOrCreateFloorEntries(perFloor, fl)

    if (room.roomType === 'stairwell') {
      // Stairwell steps → merged (variable geometry)
      collectStairwellGeometries(room, entries.back, house.rooms)
      continue
    }

    // Floor → merged geometry (back)
    collectFloorGeometry(room, entries.back, stairwellFootprints)

    // Roof → merged geometry (front), suppressed if covered by 2F
    if (!shouldSuppressRoof(room, secondFloorFootprints)) {
      collectRoofGeometry(room, entries.front)
    }

    // Walls: solid → instance, door/window → merged
    collectWallSegmentsInstanced(
      room.wallNorth,
      'north',
      room,
      instances,
      entries.front,
      entries.back
    )
    collectWallSegmentsInstanced(
      room.wallSouth,
      'south',
      room,
      instances,
      entries.front,
      entries.back
    )
    collectWallSegmentsInstanced(
      room.wallEast,
      'east',
      room,
      instances,
      entries.front,
      entries.back
    )
    collectWallSegmentsInstanced(
      room.wallWest,
      'west',
      room,
      instances,
      entries.front,
      entries.back
    )
  }

  // Build merged per-floor groups (door/window frames + stairwells)
  const mergedFloorGroups = new Map<
    number,
    { front: THREE.Group; back: THREE.Group }
  >()

  for (const [fl, entries] of perFloor) {
    const front = new THREE.Group()
    front.name = `merged_front_f${fl}`
    const back = new THREE.Group()
    back.name = `merged_back_f${fl}`
    addMergedMeshes(front, entries.front)
    addMergedMeshes(back, entries.back)
    mergedGroup.add(front)
    mergedGroup.add(back)
    mergedFloorGroups.set(fl, { front, back })
  }

  return {
    instances,
    mergedGroup,
    mergedFloorGroups,
    aabb: computeHouseAABB(house),
    roomsHash: JSON.stringify(house.rooms),
  }
}

/**
 * Wall segment collector for instanced path.
 * Solid → InstanceDescriptor, door/window frames → GeoEntry (merged).
 */
function collectWallSegmentsInstanced(
  segments: WallConfig[],
  dir: WallDirection,
  room: RoomData,
  instances: InstanceDescriptor[],
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[]
) {
  const dirInfo = WALL_DIR_INFO[dir]
  const mergedTarget = dirInfo.isFront ? frontEntries : backEntries
  const wh = room.wallHeight
  const yBase = floorYBase(room.floorLevel, wh) + FLOOR_THICKNESS / 2
  const { localX, localZ, sizeX, sizeZ } = room

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    if (seg.variant === 'open') continue

    const texIdx = seg.texture % HOUSING_TEXTURES.length
    const segCenter = i + 0.5
    let x: number, z: number, rotY: number

    const halfT = WALL_THICKNESS / 2
    switch (dir) {
      case 'north': {
        x = localX + segCenter
        z = localZ + halfT
        rotY = 0
        break
      }
      case 'south': {
        x = localX + segCenter
        z = localZ + sizeZ - halfT
        rotY = 0
        break
      }
      case 'east': {
        x = localX + sizeX - halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
      case 'west': {
        x = localX + halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
    }

    if (seg.variant === 'solid') {
      // Solid wall → instance
      instances.push({
        template: 'wall',
        textureIndex: texIdx,
        x,
        y: yBase + wh / 2,
        z,
        rotY,
        floorLevel: room.floorLevel,
        isFront: dirInfo.isFront,
      })
    } else {
      // Door/window frame pieces → merged geometry (variable shapes)
      const openW = seg.variant === 'door' ? DOOR_WIDTH : WINDOW_WIDTH
      const openH = seg.variant === 'door' ? DOOR_HEIGHT : WINDOW_HEIGHT
      const openBot = seg.variant === 'door' ? 0 : WINDOW_BOTTOM
      const sideW = (1 - openW) / 2

      if (sideW > 0.01) {
        for (const sign of [-1, 1]) {
          const offset = sign * (0.5 - sideW / 2)
          const sx = dir === 'north' || dir === 'south' ? x + offset : x
          const sz = dir === 'east' || dir === 'west' ? z + offset : z
          const uOffX = sign === -1 ? 0 : 1 - sideW
          mergedTarget.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(sideW, wh, WALL_THICKNESS),
              sx,
              yBase + wh / 2,
              sz,
              rotY,
              sideW,
              wh,
              uOffX,
              0
            ),
            textureIndex: texIdx,
          })
        }
      }

      if (openBot > 0.01) {
        mergedTarget.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, openBot, WALL_THICKNESS),
            x,
            yBase + openBot / 2,
            z,
            rotY,
            openW,
            openBot,
            sideW,
            0
          ),
          textureIndex: texIdx,
        })
      }

      const topH = wh - openBot - openH
      if (topH > 0.01) {
        mergedTarget.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, topH, WALL_THICKNESS),
            x,
            yBase + openBot + openH + topH / 2,
            z,
            rotY,
            openW,
            topH,
            sideW,
            openBot + openH
          ),
          textureIndex: texIdx,
        })
      }
    }
  }
}

/** Group entries by texture index, merge geometries per group, create meshes. */
function addMergedMeshes(group: THREE.Group, entries: GeoEntry[]) {
  if (entries.length === 0) return

  const byTex = new Map<number, THREE.BufferGeometry[]>()
  for (const e of entries) {
    const list = byTex.get(e.textureIndex)
    if (list) {
      list.push(e.geo)
    } else {
      byTex.set(e.textureIndex, [e.geo])
    }
  }

  for (const [texIdx, geos] of byTex) {
    const merged = mergeGeometries(geos, false)
    if (merged) {
      const mesh = new THREE.Mesh(merged, getHousingMaterial(texIdx))
      mesh.castShadow = true
      mesh.receiveShadow = true
      group.add(mesh)
    }
  }
}

/**
 * Create geometry with baked position and tiled UVs for a single piece.
 */
function bakedGeo(
  baseGeo: THREE.BufferGeometry,
  px: number,
  py: number,
  pz: number,
  rotY: number = 0,
  uvScaleX: number = 1,
  uvScaleY: number = 1,
  uvOffsetX: number = 0,
  uvOffsetY: number = 0
): THREE.BufferGeometry {
  // Apply position and rotation by modifying vertices directly
  if (rotY !== 0) {
    _tmpMatrix.makeRotationY(rotY)
    _tmpMatrix.setPosition(px, py, pz)
  } else {
    _tmpMatrix.makeTranslation(px, py, pz)
  }
  baseGeo.applyMatrix4(_tmpMatrix)

  // Scale and offset UVs for texture tiling (1 repeat per meter)
  const uv = baseGeo.getAttribute('uv')
  if (uv) {
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(
        i,
        uv.getX(i) * uvScaleX + uvOffsetX,
        uv.getY(i) * uvScaleY + uvOffsetY
      )
    }
  }

  return baseGeo
}

/** Generate floor geometry for a room, punching stairwell holes on 2F+. */
function collectFloorGeometry(
  room: RoomData,
  target: GeoEntry[],
  stairwellFootprints: RoomFootprint[]
) {
  const { localX, localZ, sizeX, sizeZ, floorLevel } = room
  const yBase = floorYBase(floorLevel, room.wallHeight)
  const floorIdx = room.floorTexture % HOUSING_TEXTURES.length

  const hasStairwellOverlap =
    floorLevel >= 1 &&
    stairwellFootprints.some(
      (fp) =>
        localX < fp.x + fp.sx &&
        localX + sizeX > fp.x &&
        localZ < fp.z + fp.sz &&
        localZ + sizeZ > fp.z
    )

  if (hasStairwellOverlap) {
    for (let cx = localX; cx < localX + sizeX; cx++) {
      for (let cz = localZ; cz < localZ + sizeZ; cz++) {
        if (stairwellFootprints.some((fp) => cellInFootprint(cx, cz, fp))) {
          continue
        }
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(1, FLOOR_THICKNESS, 1),
            cx + 0.5,
            yBase,
            cz + 0.5,
            0,
            1,
            1,
            cx - localX,
            cz - localZ
          ),
          textureIndex: floorIdx,
        })
      }
    }
  } else {
    target.push({
      geo: bakedGeo(
        new THREE.BoxGeometry(sizeX, FLOOR_THICKNESS, sizeZ),
        localX + sizeX / 2,
        yBase,
        localZ + sizeZ / 2,
        0,
        sizeX,
        sizeZ
      ),
      textureIndex: floorIdx,
    })
  }
}

/** Generate roof geometry for a room. */
function collectRoofGeometry(room: RoomData, target: GeoEntry[]) {
  const { localX, localZ, sizeX, sizeZ, wallHeight } = room
  const yBase = floorYBase(room.floorLevel, wallHeight)
  const roofIdx = room.roofTexture % HOUSING_TEXTURES.length
  const roofPlane = new THREE.PlaneGeometry(sizeX, sizeZ)
  roofPlane.rotateX(-Math.PI / 2)
  target.push({
    geo: bakedGeo(
      roofPlane,
      localX + sizeX / 2,
      yBase + FLOOR_THICKNESS / 2 + wallHeight + 0.001,
      localZ + sizeZ / 2,
      0,
      sizeX,
      sizeZ
    ),
    textureIndex: roofIdx,
  })
}

function collectRoomGeometries(
  room: RoomData,
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[],
  suppressRoof: boolean = false,
  allRooms: RoomData[] = [],
  stairwellFootprints: RoomFootprint[] = []
) {
  if (room.roomType === 'stairwell') {
    collectStairwellGeometries(room, backEntries, allRooms)
    return
  }

  collectFloorGeometry(room, backEntries, stairwellFootprints)
  if (!suppressRoof) collectRoofGeometry(room, frontEntries)

  collectWallSegments(room.wallNorth, 'north', room, frontEntries, backEntries)
  collectWallSegments(room.wallSouth, 'south', room, frontEntries, backEntries)
  collectWallSegments(room.wallEast, 'east', room, frontEntries, backEntries)
  collectWallSegments(room.wallWest, 'west', room, frontEntries, backEntries)
}

/**
 * Generate stairwell geometry: steps ascending along the longer axis,
 * within 1 floor height. No walls, no roof. Includes landings at top/bottom.
 * Placed inside an existing room.
 */
function collectStairwellGeometries(
  room: RoomData,
  backEntries: GeoEntry[],
  allRooms: RoomData[]
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight } = room
  // Stairwells always connect floor 0 → floor 1
  const yBase = FLOOR_THICKNESS / 2
  const totalRise = floorYBase(1, wallHeight)
  const floorIdx = room.floorTexture % HOUSING_TEXTURES.length

  // Steps ascend along the longer axis
  const alongZ = sizeZ >= sizeX
  const stairLen = alongZ ? sizeZ : sizeX
  const stairWidth = alongZ ? sizeX : sizeZ

  // Detect solid walls on each side of the stairwell to inset geometry.
  // Each edge can have a wall from the containing room (same-side) or adjacent room (opposite-side).
  const hasSolidWall = (segs: WallConfig[]) =>
    segs.some((s) => s.variant !== 'open')
  const edgeChecks: {
    dir: 'north' | 'south' | 'east' | 'west'
    edge: number
    overlapAxis: 'x' | 'z'
    matches: {
      otherEdge: (o: RoomData) => number
      wall: (o: RoomData) => WallConfig[]
    }[]
  }[] = [
    {
      dir: 'north',
      edge: localZ,
      overlapAxis: 'x',
      matches: [
        { otherEdge: (o) => o.localZ, wall: (o) => o.wallNorth },
        { otherEdge: (o) => o.localZ + o.sizeZ, wall: (o) => o.wallSouth },
      ],
    },
    {
      dir: 'south',
      edge: localZ + sizeZ,
      overlapAxis: 'x',
      matches: [
        { otherEdge: (o) => o.localZ + o.sizeZ, wall: (o) => o.wallSouth },
        { otherEdge: (o) => o.localZ, wall: (o) => o.wallNorth },
      ],
    },
    {
      dir: 'west',
      edge: localX,
      overlapAxis: 'z',
      matches: [
        { otherEdge: (o) => o.localX, wall: (o) => o.wallWest },
        { otherEdge: (o) => o.localX + o.sizeX, wall: (o) => o.wallEast },
      ],
    },
    {
      dir: 'east',
      edge: localX + sizeX,
      overlapAxis: 'z',
      matches: [
        { otherEdge: (o) => o.localX + o.sizeX, wall: (o) => o.wallEast },
        { otherEdge: (o) => o.localX, wall: (o) => o.wallWest },
      ],
    },
  ]

  const inset = { north: 0, south: 0, east: 0, west: 0 }
  for (const other of allRooms) {
    if (other === room || other.roomType === 'stairwell') continue
    const xOverlap =
      localX < other.localX + other.sizeX && localX + sizeX > other.localX
    const zOverlap =
      localZ < other.localZ + other.sizeZ && localZ + sizeZ > other.localZ

    for (const check of edgeChecks) {
      if (!(check.overlapAxis === 'x' ? xOverlap : zOverlap)) continue
      for (const m of check.matches) {
        if (check.edge === m.otherEdge(other) && hasSolidWall(m.wall(other))) {
          inset[check.dir] = WALL_THICKNESS
        }
      }
    }
  }

  // Compute effective insets along stair axes
  // "left/right" = perpendicular to stair direction, "start/end" = along stair direction
  const insetLeft = alongZ ? inset.west : inset.north
  const insetRight = alongZ ? inset.east : inset.south
  const insetStart = alongZ ? inset.north : inset.west
  const insetEnd = alongZ ? inset.south : inset.east
  const effectiveWidth = stairWidth - insetLeft - insetRight
  const widthOffset = (insetLeft - insetRight) / 2
  const effectiveLen = stairLen - insetStart - insetEnd
  const lenOffset = (insetEnd - insetStart) / 2

  const stairRun = effectiveLen - LANDING_DEPTH * 2
  const stepCount = Math.round(totalRise / 0.25)
  const stepHeight = totalRise / stepCount
  const stepDepth = stairRun / stepCount

  // Helper: create a step box with world-tiled UVs (1 repeat/meter)
  // BoxGeometry(w,h,d) vertices: 0-3 +X, 4-7 -X, 8-11 +Y, 12-15 -Y, 16-19 +Z, 20-23 -Z
  const addBox = (
    w: number,
    h: number,
    d: number,
    cx: number,
    cy: number,
    cz: number
  ) => {
    const bw = alongZ ? w : d
    const bd = alongZ ? d : w
    const geo = new THREE.BoxGeometry(bw, h, bd)
    const uv = geo.getAttribute('uv')
    const pos = geo.getAttribute('position')
    for (let vi = 0; vi < pos.count; vi++) {
      const px = pos.getX(vi) + cx
      const py = pos.getY(vi) + cy
      const pz = pos.getZ(vi) + cz
      const face = Math.floor(vi / 4)
      // 0,1: ±X → (Z, Y)  2,3: ±Y → (X, Z)  4,5: ±Z → (X, Y)
      if (face <= 1) {
        uv.setXY(vi, pz, py)
      } else if (face <= 3) {
        uv.setXY(vi, px, pz)
      } else {
        uv.setXY(vi, px, py)
      }
    }
    backEntries.push({
      geo: bakedGeo(geo, cx, cy, cz, 0, 1, 1),
      textureIndex: floorIdx,
    })
  }

  // Center offset accounting for wall insets
  const baseCx = localX + sizeX / 2 + (alongZ ? widthOffset : -lenOffset)
  const baseCz = localZ + sizeZ / 2 + (alongZ ? -lenOffset : widthOffset)

  // Bottom landing
  {
    const offset = -(effectiveLen / 2) + LANDING_DEPTH / 2
    addBox(
      effectiveWidth,
      FLOOR_THICKNESS,
      LANDING_DEPTH,
      alongZ ? baseCx : baseCx + offset,
      yBase,
      alongZ ? baseCz + offset : baseCz
    )
  }

  // Steps
  for (let i = 0; i < stepCount; i++) {
    const stepY = yBase + i * stepHeight + stepHeight / 2
    const offset =
      -(effectiveLen / 2) + LANDING_DEPTH + i * stepDepth + stepDepth / 2
    addBox(
      effectiveWidth,
      stepHeight,
      stepDepth,
      alongZ ? baseCx : baseCx + offset,
      stepY,
      alongZ ? baseCz + offset : baseCz
    )
  }

  // Top landing
  {
    const offset = effectiveLen / 2 - LANDING_DEPTH / 2
    addBox(
      effectiveWidth,
      FLOOR_THICKNESS,
      LANDING_DEPTH,
      alongZ ? baseCx : baseCx + offset,
      yBase + totalRise,
      alongZ ? baseCz + offset : baseCz
    )
  }
}

/** Render 1m wall segments along a wall direction. */
function collectWallSegments(
  segments: WallConfig[],
  dir: WallDirection,
  room: RoomData,
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[]
) {
  const dirInfo = WALL_DIR_INFO[dir]
  const target = dirInfo.isFront ? frontEntries : backEntries
  const wh = room.wallHeight
  const yBase = floorYBase(room.floorLevel, wh) + FLOOR_THICKNESS / 2
  const { localX, localZ, sizeX, sizeZ } = room

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    if (seg.variant === 'open') continue

    const texIdx = seg.texture % HOUSING_TEXTURES.length

    // Position: center of this 1m segment along the wall
    const segCenter = i + 0.5 // 0.5, 1.5, 2.5, ...
    let x: number, z: number, rotY: number

    const halfT = WALL_THICKNESS / 2
    switch (dir) {
      case 'north': {
        x = localX + segCenter
        z = localZ + halfT
        rotY = 0
        break
      }
      case 'south': {
        x = localX + segCenter
        z = localZ + sizeZ - halfT
        rotY = 0
        break
      }
      case 'east': {
        x = localX + sizeX - halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
      case 'west': {
        x = localX + halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
    }

    if (seg.variant === 'solid') {
      target.push({
        geo: bakedGeo(
          new THREE.BoxGeometry(1, wh, WALL_THICKNESS),
          x,
          yBase + wh / 2,
          z,
          rotY,
          1,
          wh
        ),
        textureIndex: texIdx,
      })
    } else {
      // door or window — opening centered in the 1m segment
      const openW = seg.variant === 'door' ? DOOR_WIDTH : WINDOW_WIDTH
      const openH = seg.variant === 'door' ? DOOR_HEIGHT : WINDOW_HEIGHT
      const openBot = seg.variant === 'door' ? 0 : WINDOW_BOTTOM
      const sideW = (1 - openW) / 2

      // Left and right solid strips
      if (sideW > 0.01) {
        for (const sign of [-1, 1]) {
          const offset = sign * (0.5 - sideW / 2)
          const sx = dir === 'north' || dir === 'south' ? x + offset : x
          const sz = dir === 'east' || dir === 'west' ? z + offset : z
          // Left strip: uvOffsetX=0, right strip: uvOffsetX=1-sideW
          const uOffX = sign === -1 ? 0 : 1 - sideW
          target.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(sideW, wh, WALL_THICKNESS),
              sx,
              yBase + wh / 2,
              sz,
              rotY,
              sideW,
              wh,
              uOffX,
              0
            ),
            textureIndex: texIdx,
          })
        }
      }

      // Bottom strip (windows)
      if (openBot > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, openBot, WALL_THICKNESS),
            x,
            yBase + openBot / 2,
            z,
            rotY,
            openW,
            openBot,
            sideW,
            0
          ),
          textureIndex: texIdx,
        })
      }

      // Top strip
      const topH = wh - openBot - openH
      if (topH > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, topH, WALL_THICKNESS),
            x,
            yBase + openBot + openH + topH / 2,
            z,
            rotY,
            openW,
            topH,
            sideW,
            openBot + openH
          ),
          textureIndex: texIdx,
        })
      }
    }
  }
}

/**
 * Calculate the Y offset for a player standing on a stairwell.
 * Returns the height above ground based on position along the stair.
 * wx/wz are world coordinates, house is the containing house.
 */
export function getStairwellYOffset(
  room: RoomData,
  houseOriginX: number,
  houseOriginZ: number,
  wx: number,
  wz: number
): number {
  const { localX, localZ, sizeX, sizeZ, wallHeight } = room
  const alongZ = sizeZ >= sizeX
  const stairLen = alongZ ? sizeZ : sizeX
  const totalRise = floorYBase(1, wallHeight)

  // Player position along the stair axis (0 = start, stairLen = end)
  const roomStartX = houseOriginX + localX
  const roomStartZ = houseOriginZ + localZ
  const posAlongStair = alongZ ? wz - roomStartZ : wx - roomStartX

  // Clamp to [0, stairLen]
  const t = Math.max(0, Math.min(stairLen, posAlongStair))

  // Bottom landing: t in [0, LANDING_DEPTH] → height = 0
  if (t <= LANDING_DEPTH) return FLOOR_THICKNESS / 2

  // Top landing: t in [stairLen - LANDING_DEPTH, stairLen] → height = totalRise
  if (t >= stairLen - LANDING_DEPTH) return totalRise + FLOOR_THICKNESS / 2

  // Steps region: linear interpolation
  const stairT = (t - LANDING_DEPTH) / (stairLen - LANDING_DEPTH * 2)
  return stairT * totalRise + FLOOR_THICKNESS / 2
}

/** Dispose merged geometries in a house group */
export function disposeHouseGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) {
      // Merged geometries are unique per house — dispose them
      obj.geometry?.dispose()
      // Materials are shared singletons — don't dispose
    }
  })
}
