import * as THREE from 'three'
import type { DungeonFloorLayout } from '../managers/dungeonManager'
import { addMergedMeshes, type GeoEntry } from './house-geo-utils'
import { dungeonCaveTheme } from './dungeon-cave-themes'
import {
  DUNGEON_FLOOR_UV_SCALE,
  type DungeonGeoCtx,
} from './dungeon-geo-constants'
import { generateDungeonFloorStones } from './dungeon-floor-stones'

export function buildDungeonFloorRubble(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx,
  dungeonId: string
): THREE.Group {
  const theme = dungeonCaveTheme(dungeonId, layout.depth)
  const masonry = theme.id === 'masonry'
  const entries: GeoEntry[] = generateDungeonFloorStones(
    layout,
    ctx,
    dungeonId
  ).map((stone) => {
    const geo = masonry
      ? new THREE.CylinderGeometry(1, 0.92, 2, 5)
      : new THREE.IcosahedronGeometry(1, 0)
    const height = masonry ? Math.min(stone.height, 0.035) : stone.height
    geo.scale(stone.width / 2, height * 0.6, stone.depth / 2)
    geo.rotateY(stone.rotation)
    geo.translate(stone.x, height * 0.4, stone.z)
    const positions = geo.getAttribute('position')
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < positions.count; i++)
      uv.setXY(
        i,
        positions.getX(i) * DUNGEON_FLOOR_UV_SCALE,
        positions.getZ(i) * DUNGEON_FLOOR_UV_SCALE
      )
    return { geo, textureIndex: theme.floorTexture }
  })
  const group = new THREE.Group()
  group.name = 'floorRubble'
  addMergedMeshes(group, entries)
  for (const child of group.children) {
    const mesh = child as THREE.Mesh
    mesh.castShadow = false
    mesh.raycast = () => {}
  }
  return group
}
