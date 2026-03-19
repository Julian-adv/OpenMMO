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
import type { HouseData, RoomData, WallConfig } from '../types/housing'

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
  /** JSON hash of rooms for change detection */
  roomsHash: string
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

  return {
    houseGroup,
    frontGroup,
    backGroup,
    aabb,
    roomsHash: JSON.stringify(house.rooms),
  }
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

  // Walls — each is an array of 1m segments
  collectWallSegments(room.wallNorth, 'north', room, frontGeos, backGeos)
  collectWallSegments(room.wallSouth, 'south', room, frontGeos, backGeos)
  collectWallSegments(room.wallEast, 'east', room, frontGeos, backGeos)
  collectWallSegments(room.wallWest, 'west', room, frontGeos, backGeos)
}

/** Render 1m wall segments along a wall direction. */
function collectWallSegments(
  segments: WallConfig[],
  dir: WallDirection,
  room: RoomData,
  frontGeos: THREE.BufferGeometry[],
  backGeos: THREE.BufferGeometry[]
) {
  const dirInfo = WALL_DIR_INFO[dir]
  const target = dirInfo.isFront ? frontGeos : backGeos
  const wh = room.wallHeight
  const yBase = room.floorLevel * wh
  const { localX, localZ, sizeX, sizeZ } = room

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    if (seg.variant === 'open') continue

    const color = WALL_COLORS[seg.texture % WALL_COLORS.length]

    // Position: center of this 1m segment along the wall
    const segCenter = i + 0.5 // 0.5, 1.5, 2.5, ...
    let x: number, z: number, rotY: number

    switch (dir) {
      case 'north': {
        x = localX + segCenter
        z = localZ
        rotY = 0
        break
      }
      case 'south': {
        x = localX + segCenter
        z = localZ + sizeZ
        rotY = 0
        break
      }
      case 'east': {
        x = localX + sizeX
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
      case 'west': {
        x = localX
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
    }

    if (seg.variant === 'solid') {
      target.push(
        bakedGeo(
          new THREE.BoxGeometry(1, wh, WALL_THICKNESS),
          color,
          x,
          yBase + wh / 2,
          z,
          rotY
        )
      )
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
          target.push(
            bakedGeo(
              new THREE.BoxGeometry(sideW, wh, WALL_THICKNESS),
              color,
              sx,
              yBase + wh / 2,
              sz,
              rotY
            )
          )
        }
      }

      // Bottom strip (windows)
      if (openBot > 0.01) {
        target.push(
          bakedGeo(
            new THREE.BoxGeometry(openW, openBot, WALL_THICKNESS),
            color,
            x,
            yBase + openBot / 2,
            z,
            rotY
          )
        )
      }

      // Top strip
      const topH = wh - openBot - openH
      if (topH > 0.01) {
        target.push(
          bakedGeo(
            new THREE.BoxGeometry(openW, topH, WALL_THICKNESS),
            color,
            x,
            yBase + openBot + openH + topH / 2,
            z,
            rotY
          )
        )
      }
    }
  }
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
