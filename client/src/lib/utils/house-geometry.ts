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

const WALL_THICKNESS = 0.15
export const FLOOR_THICKNESS = 0.1
const DOOR_WIDTH = 1.0
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 1.0
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2

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

export interface HouseGroupResult {
  houseGroup: THREE.Group
  frontGroup: THREE.Group
  backGroup: THREE.Group
  aabb: THREE.Box3
  /** JSON hash of rooms for change detection */
  roomsHash: string
}

const _aabbVec = new THREE.Vector3()
const _tmpMatrix = new THREE.Matrix4()

interface GeoEntry {
  geo: THREE.BufferGeometry
  textureIndex: number
}

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

  const frontEntries: GeoEntry[] = []
  const backEntries: GeoEntry[] = []

  for (const room of house.rooms) {
    collectRoomGeometries(room, frontEntries, backEntries)
  }

  // Group by texture index and merge
  addMergedMeshes(frontGroup, frontEntries)
  addMergedMeshes(backGroup, backEntries)

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

function collectRoomGeometries(
  room: RoomData,
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[]
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight, floorLevel } = room
  const yBase = floorLevel * wallHeight

  // Floor → back
  const floorIdx = room.floorTexture % HOUSING_TEXTURES.length
  backEntries.push({
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

  // Roof → front
  const roofIdx = room.roofTexture % HOUSING_TEXTURES.length
  const roofPlane = new THREE.PlaneGeometry(sizeX, sizeZ)
  roofPlane.rotateX(-Math.PI / 2)
  frontEntries.push({
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

  // Walls — each is an array of 1m segments
  collectWallSegments(room.wallNorth, 'north', room, frontEntries, backEntries)
  collectWallSegments(room.wallSouth, 'south', room, frontEntries, backEntries)
  collectWallSegments(room.wallEast, 'east', room, frontEntries, backEntries)
  collectWallSegments(room.wallWest, 'west', room, frontEntries, backEntries)
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
  const yBase = room.floorLevel * wh + FLOOR_THICKNESS / 2
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
