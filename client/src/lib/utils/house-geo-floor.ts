/**
 * house-geo-floor.ts — Floor geometry generation with stairwell hole punching.
 */
import * as THREE from 'three'
import type { RoomData } from '../types/housing'
import {
  FLOOR_THICKNESS,
  HOUSING_TEXTURES,
  bakedGeo,
  floorYBase,
  cellInFootprint,
  type GeoEntry,
  type RoomFootprint,
} from './house-geo-utils'

/** Generate floor geometry for a room, punching stairwell holes on 2F+. */
export function collectFloorGeometry(
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
