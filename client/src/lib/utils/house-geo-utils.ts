/**
 * house-geo-utils.ts — Shared constants, types, and geometry helpers for house building.
 */
import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import type { RoomData } from '../types/housing'
import { getHousingMaterial, HOUSING_TEXTURES } from './housing-textures'

export const WALL_THICKNESS = 0.1
export const FLOOR_THICKNESS = 0.1
export const DEFAULT_WALL_HEIGHT = 3
export const LANDING_DEPTH = 0.5
export const MAX_FLOOR_LEVEL = 3
export const ROOF_OVERHANG = 0.3
export const FLOOR_OVERHANG_PER_LEVEL = 0.15
export const ROOF_PITCH: Record<string, number> = {
  gabled: 0.8,
  steep: 1.4,
}

export { HOUSING_TEXTURES }

export const FRAME_PROTRUSION = 0.04
export const FRAME_DEPTH = WALL_THICKNESS + FRAME_PROTRUSION * 2
export const WOOD_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/wood_shutter_1k'
)

/** Compute floor overhang for a given floor level (upper floors extend beyond walls). */
export function floorOverhang(floorLevel: number): number {
  return floorLevel * FLOOR_OVERHANG_PER_LEVEL
}

/** Y offset used to hide front walls instead of toggling visible (WebGPU workaround) */
export const OFFSCREEN_Y = -10000

export type WallDirection = 'north' | 'south' | 'east' | 'west'

/** Compute the Y base for a given floor level, accounting for floor thickness. */
export function floorYBase(floorLevel: number, wallHeight: number): number {
  return floorLevel * (wallHeight + FLOOR_THICKNESS)
}

// Wall direction descriptors
export interface WallDirInfo {
  isNS: boolean
  isFront: boolean
}

export const WALL_DIR_INFO: Record<WallDirection, WallDirInfo> = {
  north: { isNS: true, isFront: false },
  south: { isNS: true, isFront: true },
  east: { isNS: false, isFront: false },
  west: { isNS: false, isFront: true },
}

export interface DoorMeshInfo {
  /** Hinge pivot group (rotate .rotation.y to open/close) */
  pivot: THREE.Group
  roomIndex: number
  wallDir: WallDirection
  segmentIndex: number
  floorLevel: number
  isOpen: boolean
}

export interface HouseGroupResult {
  houseGroup: THREE.Group
  /** Per-floor groups: key = floorLevel, value = { front, back } */
  floorGroups: Map<number, { front: THREE.Group; back: THREE.Group }>
  aabb: THREE.Box3
  /** JSON hash of rooms for change detection */
  roomsHash: string
  /** Number of merged meshes (for profiling). */
  mergedMeshCount: number
  /** Door panel meshes with hinge pivots for animation */
  doors: DoorMeshInfo[]
}

export interface GeoEntry {
  geo: THREE.BufferGeometry
  textureIndex: number
}

export interface RoomFootprint {
  x: number
  z: number
  sx: number
  sz: number
  fl: number
}

export type FloorEntries = {
  front: GeoEntry[]
  back: GeoEntry[]
  doors: DoorMeshInfo[]
}

const _tmpMatrix = new THREE.Matrix4()
const _aabbVec = new THREE.Vector3()

/**
 * Create geometry with baked position and tiled UVs for a single piece.
 */
export function bakedGeo(
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
  if (rotY !== 0) {
    _tmpMatrix.makeRotationY(rotY)
    _tmpMatrix.setPosition(px, py, pz)
  } else {
    _tmpMatrix.makeTranslation(px, py, pz)
  }
  baseGeo.applyMatrix4(_tmpMatrix)

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

/** Group entries by texture index, merge geometries per group, create meshes. Returns mesh count. */
export function addMergedMeshes(
  group: THREE.Group,
  entries: GeoEntry[]
): number {
  if (entries.length === 0) return 0

  const byTex = new Map<number, THREE.BufferGeometry[]>()
  for (const e of entries) {
    const list = byTex.get(e.textureIndex)
    if (list) {
      list.push(e.geo)
    } else {
      byTex.set(e.textureIndex, [e.geo])
    }
  }

  let count = 0
  for (const [texIdx, geos] of byTex) {
    const merged = mergeGeometries(geos, false)
    for (const g of geos) g.dispose()
    if (merged) {
      const mesh = new THREE.Mesh(merged, getHousingMaterial(texIdx))
      mesh.castShadow = true
      mesh.receiveShadow = true
      group.add(mesh)
      count++
    }
  }
  return count
}

export function collectFootprints(
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
        fl: room.floorLevel,
      })
    }
  }
  return result
}

export function cellInFootprint(
  cx: number,
  cz: number,
  fp: RoomFootprint
): boolean {
  return cx >= fp.x && cx < fp.x + fp.sx && cz >= fp.z && cz < fp.z + fp.sz
}

export function getOrCreateFloorEntries(
  perFloor: Map<number, FloorEntries>,
  fl: number
): FloorEntries {
  let entries = perFloor.get(fl)
  if (!entries) {
    entries = { front: [], back: [], doors: [] }
    perFloor.set(fl, entries)
  }
  return entries
}

export function computeHouseAABB(house: {
  origin: { x: number; y: number; z: number }
  rooms: RoomData[]
}): THREE.Box3 {
  const aabb = new THREE.Box3()
  for (const room of house.rooms) {
    const yBase = floorYBase(room.floorLevel, room.wallHeight)
    const minX = house.origin.x + room.localX
    const minZ = house.origin.z + room.localZ
    let maxY = room.wallHeight
    let roofOh = 0
    if (room.roofType && room.roofType !== 'flat') {
      const { ridgeHeight } = gabledRoofDims(room)
      maxY += ridgeHeight
      roofOh = ROOF_OVERHANG
    }
    const flOh = floorOverhang(room.floorLevel)
    const oh = Math.max(roofOh, flOh)
    _aabbVec.set(minX - oh, house.origin.y + yBase, minZ - oh)
    aabb.expandByPoint(_aabbVec)
    _aabbVec.set(
      minX + room.sizeX + oh,
      house.origin.y + yBase + maxY,
      minZ + room.sizeZ + oh
    )
    aabb.expandByPoint(_aabbVec)
  }
  return aabb
}

/** Compute gabled roof dimensions from room data. */
export function gabledRoofDims(room: RoomData) {
  const dir = room.roofRidgeDir ?? 'auto'
  const ridgeAlongX =
    dir === 'x' ? true : dir === 'z' ? false : room.sizeX >= room.sizeZ
  const shortDim = ridgeAlongX ? room.sizeZ : room.sizeX
  const ridgeHeight = (shortDim / 2) * ROOF_PITCH[room.roofType!]
  return { ridgeAlongX, shortDim, ridgeHeight }
}
