import { afterEach, describe, expect, it, vi } from 'vitest'
import { Mesh, Raycaster, Vector3, type Group } from 'three'
import type { DungeonRoom } from '../managers/dungeonManager'
import { buildDungeonFloorGroup, type WallRun } from './dungeon-geo-floor'
import { isoCameraOccludesPlayer } from './iso-occlusion'
import { dungeonCaveTheme } from './dungeon-cave-themes'
import {
  DUNGEON_FLOOR_TEXTURE_IDX,
  DUNGEON_WALL_TEXTURE_IDX,
} from './dungeon-geo-constants'

vi.mock('./dungeon-geo-doors', () => ({ buildInteriorDoor: vi.fn() }))

const ctx = {
  grid: 20,
  wallHeight: 3,
  floorHeight: 4,
  shaftW: 2,
  shaftLen: 8,
}
let group: Group
afterEach(() => {
  group?.traverse((object) => {
    if (object instanceof Mesh) object.geometry.dispose()
  })
})

function buildWalls(corridors: DungeonRoom[], rooms: DungeonRoom[] = []) {
  const carved = Array<boolean>(ctx.grid ** 2).fill(false)
  for (const rect of [...rooms, ...corridors])
    for (let z = rect.z; z < rect.z + rect.d; z++)
      for (let x = rect.x; x < rect.x + rect.w; x++)
        carved[x + z * ctx.grid] = true
  const floor = buildDungeonFloorGroup(
    {
      depth: 1,
      rooms,
      carved,
      upShaft: { x: 16, z: 12, alongZ: true, reversed: false },
      spawns: [],
      props: [],
    },
    ctx,
    []
  )
  group = floor.group
  return floor.wallRuns
}

function wallAt(runs: WallRun[], x: number, z: number) {
  const run = runs.find((r) => r.localAABB.containsPoint(new Vector3(x, 1, z)))
  expect(run).toBeDefined()
  return run!
}

describe('dungeon wall fade groups', () => {
  it('textures corridor ground separately and keeps cave walls out of ground picking', () => {
    const runs = buildWalls(
      [{ x: 4, z: 6, w: 2, d: 4 }],
      [{ x: 2, z: 2, w: 6, d: 4 }]
    )
    const cave = dungeonCaveTheme('', 1)
    const groundAt = (x: number, z: number) =>
      new Raycaster(
        new Vector3(x, 5, z),
        new Vector3(0, -1, 0)
      ).intersectObject(group)[0]
    expect(groundAt(4.02, 8).point.y).toBe(0)
    expect(groundAt(4.02, 8).object.userData.textureIndex).toBe(
      cave.floorTexture
    )
    expect(groundAt(4, 4).object.userData.textureIndex).toBe(
      DUNGEON_FLOOR_TEXTURE_IDX
    )
    expect(wallAt(runs, 3.95, 8).mesh.userData.textureIndex).toBe(
      cave.wallTexture
    )
    expect(wallAt(runs, 1.95, 4).mesh.userData.textureIndex).toBe(
      DUNGEON_WALL_TEXTURE_IDX
    )
  })

  it('pairs an L-shaped corridor corner when only its south wall occludes', () => {
    const runs = buildWalls([
      { x: 2, z: 2, w: 2, d: 8 },
      { x: 2, z: 8, w: 8, d: 2 },
      { x: 12, z: 2, w: 2, d: 4 },
    ])
    const south = wallAt(runs, 6, 10.05)
    const west = wallAt(runs, 1.95, 5)

    expect(isoCameraOccludesPlayer(south.localAABB, 6, 1, 9.5, 0.05)).toBe(true)
    expect(isoCameraOccludesPlayer(west.localAABB, 6, 1, 9.5, 0.05)).toBe(false)
    expect(south.fadeGroup).toBeGreaterThanOrEqual(0)
    expect(west.fadeGroup).toBe(south.fadeGroup)
    expect(wallAt(runs, 13, 6.05).fadeGroup).not.toBe(south.fadeGroup)
    expect(wallAt(runs, 3, 1.95).fadeGroup).toBe(-1)
    expect(wallAt(runs, 4.05, 5).fadeGroup).toBe(-1)
  })

  it('joins both south runs connected by the west wall of an inner bend', () => {
    const runs = buildWalls([
      { x: 2, z: 2, w: 8, d: 2 },
      { x: 8, z: 2, w: 2, d: 8 },
    ])
    const innerSouth = wallAt(runs, 5, 4.05)
    const innerWest = wallAt(runs, 7.95, 7)
    const outerSouth = wallAt(runs, 9, 10.05)

    expect(innerSouth.fadeGroup).toBeGreaterThanOrEqual(0)
    expect(innerWest.fadeGroup).toBe(innerSouth.fadeGroup)
    expect(outerSouth.fadeGroup).toBe(innerSouth.fadeGroup)
  })

  it('preserves room groups across doorway gaps and separates corridor walls', () => {
    const runs = buildWalls(
      [{ x: 4, z: 6, w: 2, d: 4 }],
      [{ x: 2, z: 2, w: 6, d: 4 }]
    )
    const roomSouth = wallAt(runs, 3, 6.05)
    const corridorSouth = wallAt(runs, 5, 10.05)

    expect(roomSouth.fadeGroup).toBe(0)
    expect(wallAt(runs, 7, 6.05).fadeGroup).toBe(roomSouth.fadeGroup)
    expect(wallAt(runs, 1.95, 4).fadeGroup).toBe(roomSouth.fadeGroup)
    expect(corridorSouth.fadeGroup).not.toBe(roomSouth.fadeGroup)
    expect(wallAt(runs, 3.95, 8).fadeGroup).toBe(corridorSouth.fadeGroup)
  })
})
