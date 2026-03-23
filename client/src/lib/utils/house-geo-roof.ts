/**
 * house-geo-roof.ts — Flat and gabled roof geometry generation.
 */
import * as THREE from 'three'
import type { RoomData, WallConfig } from '../types/housing'
import {
  WALL_THICKNESS,
  FLOOR_THICKNESS,
  ROOF_OVERHANG,
  HOUSING_TEXTURES,
  bakedGeo,
  cellInFootprint,
  floorYBase,
  gabledRoofDims,
  type GeoEntry,
  type RoomFootprint,
} from './house-geo-utils'

const _roofMatrix = new THREE.Matrix4()

export function shouldSuppressRoof(
  room: RoomData,
  upperFloorFootprints: RoomFootprint[]
): boolean {
  if (upperFloorFootprints.length === 0) return false
  for (let x = room.localX; x < room.localX + room.sizeX; x++) {
    for (let z = room.localZ; z < room.localZ + room.sizeZ; z++) {
      if (!upperFloorFootprints.some((fp) => cellInFootprint(x, z, fp))) {
        return false
      }
    }
  }
  return true
}

/** Generate roof geometry for a room (flat or gabled). */
export function collectRoofGeometry(
  room: RoomData,
  frontTarget: GeoEntry[],
  backTarget?: GeoEntry[]
) {
  if (room.roofType && room.roofType !== 'flat') {
    collectGabledRoof(room, frontTarget, backTarget ?? frontTarget)
  } else {
    collectFlatRoof(room, frontTarget)
  }
}

function collectFlatRoof(room: RoomData, target: GeoEntry[]) {
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

/**
 * Build a gabled (맞배지붕) roof:
 * - 2 sloped rectangular planes
 * - 2 triangular gable walls at each end
 * - ROOF_OVERHANG eaves on all sides
 */
function collectGabledRoof(
  room: RoomData,
  frontTarget: GeoEntry[],
  backTarget: GeoEntry[]
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight } = room
  const yBase = floorYBase(room.floorLevel, wallHeight)
  const wallTopY = yBase + FLOOR_THICKNESS / 2 + wallHeight
  const roofIdx = room.roofTexture % HOUSING_TEXTURES.length
  const { ridgeAlongX, shortDim, ridgeHeight } = gabledRoofDims(room)

  const cx = localX + sizeX / 2
  const cz = localZ + sizeZ / 2
  const oh = ROOF_OVERHANG

  const halfShort = shortDim / 2
  const halfLong = ridgeAlongX ? sizeX / 2 : sizeZ / 2

  const slopeAngle = Math.atan2(ridgeHeight, halfShort)
  const eaveDropY = (oh * ridgeHeight) / halfShort
  const slopeLen =
    ((halfShort + oh) * Math.sqrt(halfShort ** 2 + ridgeHeight ** 2)) /
    halfShort
  const ridgeLen = halfLong * 2 + oh * 2

  const ridgeExt = (WALL_THICKNESS * ridgeHeight) / halfShort
  const totalSlopeLen = slopeLen + ridgeExt

  // Build two slope slabs
  for (const side of [-1, 1] as const) {
    const geo = new THREE.BoxGeometry(ridgeLen, WALL_THICKNESS, totalSlopeLen)

    const uv = geo.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(i, uv.getX(i) * ridgeLen, uv.getY(i) * totalSlopeLen)
    }

    // Miter cut: pull inner (-Y) vertices at ridge end back toward eave
    const pos = geo.getAttribute('position')
    const innerY = -WALL_THICKNESS / 2
    const ridgeEndZ = (-side * totalSlopeLen) / 2
    for (let i = 0; i < pos.count; i++) {
      if (
        Math.abs(pos.getY(i) - innerY) < 0.001 &&
        Math.abs(pos.getZ(i) - ridgeEndZ) < 0.001
      ) {
        pos.setZ(i, ridgeEndZ + side * ridgeExt)
      }
    }

    geo.translate(0, WALL_THICKNESS / 2, (-side * ridgeExt) / 2)

    _roofMatrix.makeRotationX(side * slopeAngle)
    geo.applyMatrix4(_roofMatrix)

    if (!ridgeAlongX) {
      _roofMatrix.makeRotationY(Math.PI / 2)
      geo.applyMatrix4(_roofMatrix)
    }

    const perpCenter = (side * (halfShort + oh)) / 2
    const yCenter = wallTopY + (ridgeHeight - eaveDropY) / 2
    const tx = cx + (ridgeAlongX ? 0 : perpCenter)
    const tz = cz + (ridgeAlongX ? perpCenter : 0)
    _roofMatrix.makeTranslation(tx, yCenter, tz)
    geo.applyMatrix4(_roofMatrix)

    frontTarget.push({ geo, textureIndex: roofIdx })
  }

  // Build two triangular gable walls at each end of the ridge
  for (const endSign of [-1, 1] as const) {
    const geo = new THREE.BufferGeometry()

    let wallSegs: WallConfig[]
    if (ridgeAlongX) {
      wallSegs = endSign === -1 ? room.wallWest : room.wallEast
    } else {
      wallSegs = endSign === -1 ? room.wallNorth : room.wallSouth
    }
    const gableTexIdx =
      (wallSegs.find((s) => s.variant !== 'open')?.texture ??
        room.roofTexture) % HOUSING_TEXTURES.length

    const positions = new Float32Array(3 * 3)
    const normals = new Float32Array(3 * 3)
    const uvs = new Float32Array(3 * 2)

    const endOffset = ridgeAlongX
      ? (endSign * sizeX) / 2
      : (endSign * sizeZ) / 2

    const gnx = ridgeAlongX ? endSign : 0
    const gnz = ridgeAlongX ? 0 : endSign

    for (let i = 0; i < 3; i++) {
      let perpOffset: number, y: number
      if (i === 2) {
        perpOffset = 0
        y = wallTopY + ridgeHeight
      } else {
        perpOffset = (i === 0 ? -1 : 1) * halfShort
        y = wallTopY
      }

      const px = cx + (ridgeAlongX ? endOffset : perpOffset)
      const pz = cz + (ridgeAlongX ? perpOffset : endOffset)

      positions[i * 3] = px
      positions[i * 3 + 1] = y
      positions[i * 3 + 2] = pz

      normals[i * 3] = gnx
      normals[i * 3 + 1] = 0
      normals[i * 3 + 2] = gnz

      if (i === 2) {
        uvs[i * 2] = shortDim / 2
        uvs[i * 2 + 1] = ridgeHeight
      } else {
        uvs[i * 2] = i === 0 ? 0 : shortDim
        uvs[i * 2 + 1] = 0
      }
    }

    const flipWinding = ridgeAlongX ? endSign === 1 : endSign === -1
    geo.setIndex(flipWinding ? [0, 2, 1] : [0, 1, 2])
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3))
    geo.setAttribute('normal', new THREE.BufferAttribute(normals, 3))
    geo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))

    const isFront = ridgeAlongX ? endSign === -1 : endSign === 1
    const target = isFront ? frontTarget : backTarget
    target.push({ geo, textureIndex: gableTexIdx })
  }
}
