/**
 * house-geo-floor.ts — Floor geometry generation with stairwell hole punching.
 */
import * as THREE from 'three'
import type { RoomData } from '../types/housing'
import {
  FLOOR_THICKNESS,
  WALL_THICKNESS,
  HOUSING_TEXTURES,
  WOOD_TEXTURE_IDX,
  bakedGeo,
  floorYBase,
  floorOverhang,
  cellInFootprint,
  type GeoEntry,
  type RoomFootprint,
} from './house-geo-utils'

/** Add side panels on the camera-facing (south + west) floor slab edges, using that wall's texture. */
function addFloorSidePanels(
  target: GeoEntry[],
  localX: number,
  yBase: number,
  localZ: number,
  sizeX: number,
  sizeZ: number,
  oh: number,
  southTexIdx: number,
  westTexIdx: number
) {
  const totalW = sizeX + oh * 2
  const totalD = sizeZ + oh * 2
  const x0 = localX - oh
  const z0 = localZ - oh
  const cx = localX + sizeX / 2
  const cz = localZ + sizeZ / 2
  const EPS = 0.001

  // South side (+Z)
  target.push({
    geo: bakedGeo(
      new THREE.PlaneGeometry(totalW, FLOOR_THICKNESS),
      cx,
      yBase,
      z0 + totalD + EPS,
      0,
      totalW,
      FLOOR_THICKNESS
    ),
    textureIndex: southTexIdx,
  })
  // West side (-X)
  target.push({
    geo: bakedGeo(
      new THREE.PlaneGeometry(totalD, FLOOR_THICKNESS),
      x0 - EPS,
      yBase,
      cz,
      -Math.PI / 2,
      totalD,
      FLOOR_THICKNESS
    ),
    textureIndex: westTexIdx,
  })
}

function addFloorStrip(
  target: GeoEntry[],
  textureIndex: number,
  x: number,
  y: number,
  z: number,
  w: number,
  d: number
) {
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(w, FLOOR_THICKNESS, d),
      x + w / 2,
      y,
      z + d / 2,
      0,
      w,
      d
    ),
    textureIndex,
  })
}

/** Generate floor geometry for a room, punching stairwell holes on 2F+. */
export function collectFloorGeometry(
  room: RoomData,
  target: GeoEntry[],
  stairwellFootprints: RoomFootprint[]
) {
  const { localX, localZ, sizeX, sizeZ, floorLevel } = room
  const yBase = floorYBase(floorLevel, room.wallHeight)
  const floorIdx = room.floorTexture % HOUSING_TEXTURES.length
  const oh = floorOverhang(floorLevel)

  // Only punch holes for stairwells on the floor below (fl === floorLevel - 1)
  const relevantFootprints = stairwellFootprints.filter(
    (fp) => fp.fl === floorLevel - 1
  )
  const hasStairwellOverlap =
    floorLevel >= 1 &&
    relevantFootprints.some(
      (fp) =>
        localX < fp.x + fp.sx &&
        localX + sizeX > fp.x &&
        localZ < fp.z + fp.sz &&
        localZ + sizeZ > fp.z
    )

  if (hasStairwellOverlap) {
    const isHole = (cx: number, cz: number) =>
      relevantFootprints.some((fp) => cellInFootprint(cx, cz, fp))

    for (let cx = localX; cx < localX + sizeX; cx++) {
      for (let cz = localZ; cz < localZ + sizeZ; cz++) {
        if (isHole(cx, cz)) {
          // Stairwell hole at room edge: add narrow overhang strips
          if (oh > 0) {
            const strip = oh + WALL_THICKNESS // overhang + wall overlap
            const atN = cz === localZ
            const atS = cz === localZ + sizeZ - 1
            // East/west strips: extend depth at corners to fill overhang gap
            const z0 = atN ? cz - oh : cz
            const d = 1 + (atN ? oh : 0) + (atS ? oh : 0)
            if (cx === localX) {
              addFloorStrip(target, floorIdx, localX - oh, yBase, z0, strip, d)
            }
            if (cx === localX + sizeX - 1) {
              addFloorStrip(
                target,
                floorIdx,
                localX + sizeX - WALL_THICKNESS,
                yBase,
                z0,
                strip,
                d
              )
            }
            if (atN) {
              addFloorStrip(target, floorIdx, cx, yBase, localZ - oh, 1, strip)
            }
            if (atS) {
              addFloorStrip(
                target,
                floorIdx,
                cx,
                yBase,
                localZ + sizeZ - WALL_THICKNESS,
                1,
                strip
              )
            }
          }
          continue
        }
        let cellX = cx
        let cellZ = cz
        let cellW = 1
        let cellD = 1
        if (oh > 0) {
          if (cx === localX) {
            cellX -= oh
            cellW += oh
          }
          if (cx === localX + sizeX - 1) {
            cellW += oh
          }
          if (cz === localZ) {
            cellZ -= oh
            cellD += oh
          }
          if (cz === localZ + sizeZ - 1) {
            cellD += oh
          }
        }
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(cellW, FLOOR_THICKNESS, cellD),
            cellX + cellW / 2,
            yBase,
            cellZ + cellD / 2,
            0,
            cellW,
            cellD,
            cx - localX,
            cz - localZ
          ),
          textureIndex: floorIdx,
        })
      }
    }
  } else {
    const totalW = sizeX + oh * 2
    const totalD = sizeZ + oh * 2
    target.push({
      geo: bakedGeo(
        new THREE.BoxGeometry(totalW, FLOOR_THICKNESS, totalD),
        localX + sizeX / 2,
        yBase,
        localZ + sizeZ / 2,
        0,
        totalW,
        totalD
      ),
      textureIndex: floorIdx,
    })
  }

  // Side panels on floor slab edges (visible from outside), using wall texture
  // fitSegment textures (e.g. Plaster & Wood) don't tile on thin edges — use wood instead
  const resolveEdgeTex = (wallTex: number | undefined) => {
    if (wallTex == null) return floorIdx
    const idx = wallTex % HOUSING_TEXTURES.length
    return HOUSING_TEXTURES[idx].fitSegment ? WOOD_TEXTURE_IDX : idx
  }
  const southTexIdx = resolveEdgeTex(room.wallSouth[0]?.texture)
  const westTexIdx = resolveEdgeTex(room.wallWest[0]?.texture)
  addFloorSidePanels(
    target,
    localX,
    yBase,
    localZ,
    sizeX,
    sizeZ,
    oh,
    southTexIdx,
    westTexIdx
  )
}
