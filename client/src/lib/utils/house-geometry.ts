/**
 * house-geometry.ts — Assembles a THREE.Group from HouseData.
 *
 * All geometry for a house is merged into 2 meshes:
 * - frontMesh: south walls + west walls + roofs (hidden when player is inside)
 * - backMesh: north walls + east walls + floors (always visible)
 *
 * This keeps draw calls to 2 per house regardless of room count.
 */
import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import type { HouseData, RoomData, WallVariant } from '../types/housing'

const WALL_THICKNESS = 0.15
const DOOR_WIDTH = 1.0
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 1.0
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2

// Placeholder colors per texture index (Phase 1)
export const WALL_COLORS = [0xc8b090, 0xa85032, 0x8b6914, 0x888888]
export const FLOOR_COLORS = [0x8b6914, 0xa0522d, 0xd2b48c, 0x808080]
export const ROOF_COLORS = [0x8b4513, 0x654321, 0xa0522d, 0x696969]

/** Y offset used to hide front walls instead of toggling visible (WebGPU workaround) */
export const OFFSCREEN_Y = -10000

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

// Single shared material for all housing (Phase 1 placeholder)
let _housingMat: THREE.MeshBasicMaterial | null = null
function getHousingMaterial(): THREE.MeshBasicMaterial {
  if (!_housingMat) {
    _housingMat = new THREE.MeshBasicMaterial({
      vertexColors: true,
      side: THREE.DoubleSide,
    })
  }
  return _housingMat
}

export interface HouseGroupResult {
  houseGroup: THREE.Group
  frontGroup: THREE.Group
  backGroup: THREE.Group
  aabb: THREE.Box3
}

const _aabbVec = new THREE.Vector3()
const _tmpColor = new THREE.Color()
const _tmpMatrix = new THREE.Matrix4()

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

  // Collect geometries with baked positions and vertex colors
  const frontGeos: THREE.BufferGeometry[] = []
  const backGeos: THREE.BufferGeometry[] = []

  for (const room of house.rooms) {
    collectRoomGeometries(room, frontGeos, backGeos)
  }

  const mat = getHousingMaterial()

  if (frontGeos.length > 0) {
    const merged = mergeGeometries(frontGeos, false)
    if (merged) {
      frontGroup.add(new THREE.Mesh(merged, mat))
    }
  }

  if (backGeos.length > 0) {
    const merged = mergeGeometries(backGeos, false)
    if (merged) {
      backGroup.add(new THREE.Mesh(merged, mat))
    }
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

/**
 * Create geometry with baked position + vertex colors for a single piece,
 * then add it to the front or back geometry list.
 */
function bakedGeo(
  baseGeo: THREE.BufferGeometry,
  color: number,
  px: number,
  py: number,
  pz: number,
  rotY: number = 0
): THREE.BufferGeometry {
  const geo = baseGeo.clone()

  // Apply position and rotation by modifying vertices directly
  if (rotY !== 0) {
    _tmpMatrix.makeRotationY(rotY)
    _tmpMatrix.setPosition(px, py, pz)
  } else {
    _tmpMatrix.makeTranslation(px, py, pz)
  }
  geo.applyMatrix4(_tmpMatrix)

  // Add vertex colors
  const count = geo.getAttribute('position').count
  const colors = new Float32Array(count * 3)
  _tmpColor.set(color)
  for (let i = 0; i < count; i++) {
    colors[i * 3] = _tmpColor.r
    colors[i * 3 + 1] = _tmpColor.g
    colors[i * 3 + 2] = _tmpColor.b
  }
  geo.setAttribute('color', new THREE.BufferAttribute(colors, 3))

  return geo
}

function collectRoomGeometries(
  room: RoomData,
  frontGeos: THREE.BufferGeometry[],
  backGeos: THREE.BufferGeometry[]
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight, floorLevel } = room
  const yBase = floorLevel * wallHeight

  // Floor → back
  const floorColor = FLOOR_COLORS[room.floorTexture % FLOOR_COLORS.length]
  const floorPlane = new THREE.PlaneGeometry(sizeX, sizeZ)
  floorPlane.rotateX(-Math.PI / 2)
  backGeos.push(
    bakedGeo(
      floorPlane,
      floorColor,
      localX + sizeX / 2,
      yBase,
      localZ + sizeZ / 2
    )
  )

  // Roof → front
  const roofColor = ROOF_COLORS[room.roofTexture % ROOF_COLORS.length]
  const roofPlane = new THREE.PlaneGeometry(sizeX, sizeZ)
  roofPlane.rotateX(-Math.PI / 2)
  frontGeos.push(
    bakedGeo(
      roofPlane,
      roofColor,
      localX + sizeX / 2,
      yBase + wallHeight,
      localZ + sizeZ / 2
    )
  )

  // Walls
  collectWallGeos(room.wallNorth, 'north', room, frontGeos, backGeos)
  collectWallGeos(room.wallSouth, 'south', room, frontGeos, backGeos)
  collectWallGeos(room.wallEast, 'east', room, frontGeos, backGeos)
  collectWallGeos(room.wallWest, 'west', room, frontGeos, backGeos)
}

function collectWallGeos(
  config: { variant: WallVariant; texture: number },
  dir: WallDirection,
  room: RoomData,
  frontGeos: THREE.BufferGeometry[],
  backGeos: THREE.BufferGeometry[]
) {
  if (config.variant === 'open') return

  const dirInfo = WALL_DIR_INFO[dir]
  const target = dirInfo.isFront ? frontGeos : backGeos
  const color = WALL_COLORS[config.texture % WALL_COLORS.length]

  if (config.variant === 'solid') {
    target.push(createSolidWallGeo(dir, room, color))
  } else {
    const geos = createWallWithOpeningGeos(
      dir,
      room,
      color,
      config.variant as 'door' | 'window'
    )
    for (const g of geos) target.push(g)
  }
}

function getWallPos(
  dir: WallDirection,
  room: RoomData,
  widthOffset: number,
  yCenter: number,
  yBase: number
): { x: number; y: number; z: number; rotY: number } {
  const { localX, localZ, sizeX, sizeZ } = room
  const cx = localX + sizeX / 2
  const cz = localZ + sizeZ / 2

  switch (dir) {
    case 'north':
      return { x: cx + widthOffset, y: yBase + yCenter, z: localZ, rotY: 0 }
    case 'south':
      return {
        x: cx + widthOffset,
        y: yBase + yCenter,
        z: localZ + sizeZ,
        rotY: 0,
      }
    case 'east':
      return {
        x: localX + sizeX,
        y: yBase + yCenter,
        z: cz + widthOffset,
        rotY: Math.PI / 2,
      }
    case 'west':
      return {
        x: localX,
        y: yBase + yCenter,
        z: cz + widthOffset,
        rotY: Math.PI / 2,
      }
  }
}

function createSolidWallGeo(
  dir: WallDirection,
  room: RoomData,
  color: number
): THREE.BufferGeometry {
  const width = WALL_DIR_INFO[dir].isNS ? room.sizeX : room.sizeZ
  const wh = room.wallHeight
  const yBase = room.floorLevel * wh
  const pos = getWallPos(dir, room, 0, wh / 2, yBase)
  return bakedGeo(
    new THREE.BoxGeometry(width, wh, WALL_THICKNESS),
    color,
    pos.x,
    pos.y,
    pos.z,
    pos.rotY
  )
}

function createWallWithOpeningGeos(
  dir: WallDirection,
  room: RoomData,
  color: number,
  variant: 'door' | 'window'
): THREE.BufferGeometry[] {
  const wallWidth = WALL_DIR_INFO[dir].isNS ? room.sizeX : room.sizeZ
  const wh = room.wallHeight
  const yBase = room.floorLevel * wh

  const openingWidth = variant === 'door' ? DOOR_WIDTH : WINDOW_WIDTH
  const openingHeight = variant === 'door' ? DOOR_HEIGHT : WINDOW_HEIGHT
  const openingBottom = variant === 'door' ? 0 : WINDOW_BOTTOM

  const geos: THREE.BufferGeometry[] = []

  // Left and right segments
  const sideWidth = (wallWidth - openingWidth) / 2
  if (sideWidth > 0.01) {
    for (const sign of [-1, 1]) {
      const offset = sign * (wallWidth / 2 - sideWidth / 2)
      const pos = getWallPos(dir, room, offset, wh / 2, yBase)
      geos.push(
        bakedGeo(
          new THREE.BoxGeometry(sideWidth, wh, WALL_THICKNESS),
          color,
          pos.x,
          pos.y,
          pos.z,
          pos.rotY
        )
      )
    }
  }

  // Bottom segment (windows only)
  if (openingBottom > 0.01) {
    const pos = getWallPos(dir, room, 0, openingBottom / 2, yBase)
    geos.push(
      bakedGeo(
        new THREE.BoxGeometry(openingWidth, openingBottom, WALL_THICKNESS),
        color,
        pos.x,
        pos.y,
        pos.z,
        pos.rotY
      )
    )
  }

  // Top segment
  const topHeight = wh - openingBottom - openingHeight
  if (topHeight > 0.01) {
    const pos = getWallPos(
      dir,
      room,
      0,
      openingBottom + openingHeight + topHeight / 2,
      yBase
    )
    geos.push(
      bakedGeo(
        new THREE.BoxGeometry(openingWidth, topHeight, WALL_THICKNESS),
        color,
        pos.x,
        pos.y,
        pos.z,
        pos.rotY
      )
    )
  }

  return geos
}

/** Dispose merged geometries in a house group */
export function disposeHouseGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) {
      // Merged geometries are unique per house — dispose them
      obj.geometry?.dispose()
      // Material is shared singleton — don't dispose
    }
  })
}
