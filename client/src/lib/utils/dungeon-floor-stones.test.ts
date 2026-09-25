import { describe, expect, it } from 'vitest'
import { Mesh, Raycaster, Vector3 } from 'three'
import type { DungeonFloorLayout } from '../managers/dungeonManager'
import { generateDungeonFloorStones } from './dungeon-floor-stones'
import { buildDungeonFloorRubble } from './dungeon-geo-rubble'

const ctx = { grid: 32, wallHeight: 3, floorHeight: 4, shaftW: 2, shaftLen: 8 }
const carved = Array<boolean>(ctx.grid ** 2).fill(false)
for (let z = 2; z < 30; z++)
  for (let x = 8; x < 12; x++) carved[x + z * ctx.grid] = true
const layout: DungeonFloorLayout = {
  depth: 1,
  carved,
  rooms: [{ x: 8, z: 26, w: 4, d: 4 }],
  upShaft: { x: 8, z: 2, alongZ: true, reversed: false },
  downShaft: { x: 10, z: 2, alongZ: true, reversed: false },
  props: [{ x: 8, z: 17, kind: 'barrel', rotation: 0, stack: 1 }],
  chest: [11, 20],
  spawns: [],
}

describe('dungeon floor stones', () => {
  it('adds stable shallow stones with clearance at rooms, shafts, and props', () => {
    const first = generateDungeonFloorStones(layout, ctx, 'crypt')
    expect(first.length).toBeGreaterThan(0)
    expect(generateDungeonFloorStones(layout, ctx, 'crypt')).toEqual(first)
    expect(generateDungeonFloorStones(layout, ctx, 'other')).not.toEqual(first)
    for (let i = 0; i < 30; i++) {
      for (const stone of generateDungeonFloorStones(
        layout,
        ctx,
        `crypt-${i}`
      )) {
        expect(stone.height).toBeLessThanOrEqual(0.1)
        expect(stone.x - stone.width / 2).toBeGreaterThan(8)
        expect(stone.x + stone.width / 2).toBeLessThan(12)
        expect(stone.z - stone.width / 2).toBeGreaterThanOrEqual(10.5)
        expect(stone.z + stone.width / 2).toBeLessThan(25.5)
        expect(Math.hypot(stone.x - 8.5, stone.z - 17.5)).toBeGreaterThan(1)
        expect(Math.hypot(stone.x - 11.5, stone.z - 20.5)).toBeGreaterThan(1)
      }
    }
  })

  it('merges stones into one non-pickable mesh', () => {
    const group = buildDungeonFloorRubble(layout, ctx, 'crypt')
    expect(group.children).toHaveLength(1)
    const mesh = group.children[0] as Mesh
    mesh.geometry.computeBoundingBox()
    expect(mesh.geometry.boundingBox!.max.y).toBeLessThan(0.101)
    const stone = generateDungeonFloorStones(layout, ctx, 'crypt')[0]
    const ray = new Raycaster(
      new Vector3(stone.x, 1, stone.z),
      new Vector3(0, -1, 0)
    )
    expect(ray.intersectObject(group)).toEqual([])
    mesh.geometry.dispose()
  })
})
