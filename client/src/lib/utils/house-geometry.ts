/**
 * house-geometry.ts — Assembles a THREE.Group from HouseData.
 *
 * Phase 1: Procedural placeholder geometry (BoxGeometry walls, PlaneGeometry floors/roofs).
 * Phase 2+: Replace with GLB wall meshes for door/window cutouts.
 *
 * Returns { houseGroup, frontGroup, backGroup } where:
 * - houseGroup: root THREE.Group containing everything
 * - frontGroup: south walls + west walls + roofs (hidden when player is inside)
 * - backGroup: north walls + east walls + floors (always visible)
 */
import * as THREE from 'three'
import type { HouseData, RoomData, WallVariant } from '../types/housing'

const WALL_THICKNESS = 0.15
const DOOR_WIDTH = 1.0
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 1.0
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2

// Placeholder colors per texture index (Phase 1)
const WALL_COLORS = [0xc8b090, 0xa85032, 0x8b6914, 0x888888]
const FLOOR_COLORS = [0x8b6914, 0xa0522d, 0xd2b48c, 0x808080]
const ROOF_COLORS = [0x8b4513, 0x654321, 0xa0522d, 0x696969]

// Geometry cache: keyed by "w,h,d" to share identical geometries
const geoCache = new Map<string, THREE.BoxGeometry>()
const planeCache = new Map<string, THREE.PlaneGeometry>()

function getBoxGeo(w: number, h: number, d: number): THREE.BoxGeometry {
  const key = `${w},${h},${d}`
  let geo = geoCache.get(key)
  if (!geo) {
    geo = new THREE.BoxGeometry(w, h, d)
    geoCache.set(key, geo)
  }
  return geo
}

function getPlaneGeo(w: number, h: number): THREE.PlaneGeometry {
  const key = `${w},${h}`
  let geo = planeCache.get(key)
  if (!geo) {
    geo = new THREE.PlaneGeometry(w, h)
    geo.rotateX(-Math.PI / 2)
    planeCache.set(key, geo)
  }
  return geo
}

// Material cache: keyed by "color,roughness,doubleSide"
const matCache = new Map<string, THREE.MeshStandardMaterial>()

function getMaterial(
  color: number,
  roughness: number,
  doubleSide: boolean
): THREE.MeshStandardMaterial {
  const key = `${color},${roughness},${doubleSide ? 1 : 0}`
  let mat = matCache.get(key)
  if (!mat) {
    mat = new THREE.MeshStandardMaterial({
      color,
      roughness,
      side: doubleSide ? THREE.DoubleSide : THREE.FrontSide,
    })
    matCache.set(key, mat)
  }
  return mat
}

// Wall direction descriptors to avoid scattered string checks
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
  frontGroup: THREE.Group
  backGroup: THREE.Group
  aabb: THREE.Box3
}

// Scratch vector for AABB computation (avoids per-room allocations)
const _aabbVec = new THREE.Vector3()

export function buildHouseGroup(house: HouseData): HouseGroupResult {
  const houseGroup = new THREE.Group()
  houseGroup.position.set(house.origin.x, house.origin.y, house.origin.z)
  houseGroup.name = `house_${house.id}`

  const frontGroup = new THREE.Group()
  frontGroup.name = 'front'
  const backGroup = new THREE.Group()
  backGroup.name = 'back'
  houseGroup.add(frontGroup)
  houseGroup.add(backGroup)

  for (const room of house.rooms) {
    buildRoom(room, frontGroup, backGroup)
  }

  // Compute world-space AABB
  const aabb = new THREE.Box3()
  for (const room of house.rooms) {
    const yBase = room.floorLevel * room.wallHeight
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

  return { houseGroup, frontGroup, backGroup, aabb }
}

function buildRoom(
  room: RoomData,
  frontGroup: THREE.Group,
  backGroup: THREE.Group
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight, floorLevel } = room
  const yBase = floorLevel * wallHeight

  // Floor
  const floorMat = getMaterial(
    FLOOR_COLORS[room.floorTexture % FLOOR_COLORS.length],
    0.8,
    false
  )
  const floorMesh = new THREE.Mesh(getPlaneGeo(sizeX, sizeZ), floorMat)
  floorMesh.position.set(localX + sizeX / 2, yBase, localZ + sizeZ / 2)
  floorMesh.receiveShadow = true
  backGroup.add(floorMesh)

  // Roof (in frontGroup — hidden when inside)
  const roofMat = getMaterial(
    ROOF_COLORS[room.roofTexture % ROOF_COLORS.length],
    0.7,
    true
  )
  const roofMesh = new THREE.Mesh(getPlaneGeo(sizeX, sizeZ), roofMat)
  roofMesh.position.set(
    localX + sizeX / 2,
    yBase + wallHeight,
    localZ + sizeZ / 2
  )
  roofMesh.receiveShadow = true
  roofMesh.castShadow = true
  frontGroup.add(roofMesh)

  // Walls
  buildWall(room.wallNorth, 'north', room, frontGroup, backGroup)
  buildWall(room.wallSouth, 'south', room, frontGroup, backGroup)
  buildWall(room.wallEast, 'east', room, frontGroup, backGroup)
  buildWall(room.wallWest, 'west', room, frontGroup, backGroup)
}

function buildWall(
  config: { variant: WallVariant; texture: number },
  dir: WallDirection,
  room: RoomData,
  frontGroup: THREE.Group,
  backGroup: THREE.Group
) {
  if (config.variant === 'open') return

  const dirInfo = WALL_DIR_INFO[dir]
  const parent = dirInfo.isFront ? frontGroup : backGroup
  const wallMat = getMaterial(
    WALL_COLORS[config.texture % WALL_COLORS.length],
    0.85,
    true
  )

  const meshes =
    config.variant === 'solid'
      ? [createSolidWall(dir, room, wallMat)]
      : createWallWithOpening(dir, room, wallMat, config.variant)

  for (const m of meshes) parent.add(m)
}

function createSolidWall(
  dir: WallDirection,
  room: RoomData,
  mat: THREE.MeshStandardMaterial
): THREE.Mesh {
  const width = WALL_DIR_INFO[dir].isNS ? room.sizeX : room.sizeZ
  const mesh = new THREE.Mesh(
    getBoxGeo(width, room.wallHeight, WALL_THICKNESS),
    mat
  )
  const yBase = room.floorLevel * room.wallHeight
  positionWallSegment(mesh, dir, room, 0, room.wallHeight / 2, yBase)
  mesh.castShadow = true
  mesh.receiveShadow = true
  return mesh
}

/**
 * Creates wall segments around a rectangular opening (door or window).
 * Door: opening starts at floor (openingBottom = 0).
 * Window: opening starts above floor (openingBottom = WINDOW_BOTTOM).
 */
function createWallWithOpening(
  dir: WallDirection,
  room: RoomData,
  mat: THREE.MeshStandardMaterial,
  variant: 'door' | 'window'
): THREE.Mesh[] {
  const wallWidth = WALL_DIR_INFO[dir].isNS ? room.sizeX : room.sizeZ
  const wh = room.wallHeight
  const yBase = room.floorLevel * wh

  const openingWidth = variant === 'door' ? DOOR_WIDTH : WINDOW_WIDTH
  const openingHeight = variant === 'door' ? DOOR_HEIGHT : WINDOW_HEIGHT
  const openingBottom = variant === 'door' ? 0 : WINDOW_BOTTOM

  const meshes: THREE.Mesh[] = []

  // Left and right segments (full height)
  const sideWidth = (wallWidth - openingWidth) / 2
  if (sideWidth > 0.01) {
    for (const sign of [-1, 1]) {
      const m = makeShadowMesh(getBoxGeo(sideWidth, wh, WALL_THICKNESS), mat)
      const offset = sign * (wallWidth / 2 - sideWidth / 2)
      positionWallSegment(m, dir, room, offset, wh / 2, yBase)
      meshes.push(m)
    }
  }

  // Bottom segment (below opening — only for windows)
  if (openingBottom > 0.01) {
    const m = makeShadowMesh(
      getBoxGeo(openingWidth, openingBottom, WALL_THICKNESS),
      mat
    )
    positionWallSegment(m, dir, room, 0, openingBottom / 2, yBase)
    meshes.push(m)
  }

  // Top segment (above opening)
  const topHeight = wh - openingBottom - openingHeight
  if (topHeight > 0.01) {
    const m = makeShadowMesh(
      getBoxGeo(openingWidth, topHeight, WALL_THICKNESS),
      mat
    )
    positionWallSegment(
      m,
      dir,
      room,
      0,
      openingBottom + openingHeight + topHeight / 2,
      yBase
    )
    meshes.push(m)
  }

  return meshes
}

function makeShadowMesh(
  geo: THREE.BufferGeometry,
  mat: THREE.Material
): THREE.Mesh {
  const m = new THREE.Mesh(geo, mat)
  m.castShadow = true
  m.receiveShadow = true
  return m
}

/** Position a wall segment on the room edge with width/height offsets */
function positionWallSegment(
  mesh: THREE.Mesh,
  dir: WallDirection,
  room: RoomData,
  widthOffset: number,
  yCenter: number,
  yBase: number
) {
  const { localX, localZ, sizeX, sizeZ } = room
  const cx = localX + sizeX / 2
  const cz = localZ + sizeZ / 2

  switch (dir) {
    case 'north': {
      mesh.position.set(cx + widthOffset, yBase + yCenter, localZ)
      break
    }
    case 'south': {
      mesh.position.set(cx + widthOffset, yBase + yCenter, localZ + sizeZ)
      break
    }
    case 'east': {
      mesh.position.set(localX + sizeX, yBase + yCenter, cz + widthOffset)
      mesh.rotation.y = Math.PI / 2
      break
    }
    case 'west': {
      mesh.position.set(localX, yBase + yCenter, cz + widthOffset)
      mesh.rotation.y = Math.PI / 2
      break
    }
  }
}

/** Dispose all geometries and materials in a house group (skips shared/cached resources) */
export function disposeHouseGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) {
      // Geometry and materials are cached/shared — don't dispose them.
      // Just remove from parent so GC can collect the Mesh wrapper.
      obj.geometry = null!
      obj.material = null!
    }
  })
}
