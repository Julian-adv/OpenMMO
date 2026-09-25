// Dungeon geometry uses floor-local coordinates, with walkable ground at y=0.
// Open ceilings and individually faded walls preserve the isometric view.
import * as THREE from 'three'

export {
  DUNGEON_WALL_TEXTURE_IDX,
  DUNGEON_ENTRANCE_WALL_TEXTURE_IDX,
  DUNGEON_FLOOR_TEXTURE_IDX,
  DUNGEON_VOID_TEXTURE_IDX,
  DUNGEON_CEILING_TEXTURE_IDX,
  DUNGEON_PILLAR_TEXTURE_IDX,
  UP_SHAFT_GROUP_NAME,
} from './dungeon-geo-constants'
export type { DungeonGeoCtx } from './dungeon-geo-constants'

export { shaftStepCell } from './dungeon-geo-shaft'

export type { DoorLeaf, InteriorDoor } from './dungeon-geo-doors'

export { buildDungeonFloorGroup } from './dungeon-geo-floor'
export type { WallRun, DungeonFloorGroup } from './dungeon-geo-floor'

export { buildDungeonEntranceGroup } from './dungeon-geo-entrance'
export type { DungeonEntranceGroup } from './dungeon-geo-entrance'

/** Dispose merged geometries (materials are shared — never disposed). */
export function disposeDungeonGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) obj.geometry.dispose()
  })
}
