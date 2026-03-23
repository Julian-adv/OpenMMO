/**
 * house-geo-stairwell.ts — Stairwell geometry and Y offset calculation.
 */
import * as THREE from 'three'
import type { RoomData, WallConfig } from '../types/housing'
import {
  WALL_THICKNESS,
  FLOOR_THICKNESS,
  LANDING_DEPTH,
  HOUSING_TEXTURES,
  bakedGeo,
  floorYBase,
  type GeoEntry,
} from './house-geo-utils'

/**
 * Generate stairwell geometry: steps ascending along the longer axis,
 * within 1 floor height. No walls, no roof. Includes landings at top/bottom.
 * Placed inside an existing room.
 */
export function collectStairwellGeometries(
  room: RoomData,
  backEntries: GeoEntry[],
  allRooms: RoomData[]
) {
  const { localX, localZ, sizeX, sizeZ, wallHeight, floorLevel } = room
  const yBase = floorYBase(floorLevel, wallHeight) + FLOOR_THICKNESS / 2
  const totalRise = wallHeight + FLOOR_THICKNESS
  const floorIdx = room.floorTexture % HOUSING_TEXTURES.length

  const alongZ = sizeZ >= sizeX
  const stairLen = alongZ ? sizeZ : sizeX
  const stairWidth = alongZ ? sizeX : sizeZ

  // Detect solid walls on each side of the stairwell to inset geometry
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
  const { localX, localZ, sizeX, sizeZ, wallHeight, floorLevel } = room
  const alongZ = sizeZ >= sizeX
  const stairLen = alongZ ? sizeZ : sizeX
  const totalRise = wallHeight + FLOOR_THICKNESS
  const baseY = floorYBase(floorLevel, wallHeight)

  const roomStartX = houseOriginX + localX
  const roomStartZ = houseOriginZ + localZ
  const posAlongStair = alongZ ? wz - roomStartZ : wx - roomStartX

  const t = Math.max(0, Math.min(stairLen, posAlongStair))

  if (t <= LANDING_DEPTH) return baseY + FLOOR_THICKNESS / 2
  if (t >= stairLen - LANDING_DEPTH)
    return baseY + totalRise + FLOOR_THICKNESS / 2

  const stairT = (t - LANDING_DEPTH) / (stairLen - LANDING_DEPTH * 2)
  return baseY + stairT * totalRise + FLOOR_THICKNESS / 2
}
