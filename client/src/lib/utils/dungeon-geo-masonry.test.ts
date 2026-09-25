import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  Mesh,
  MeshBasicMaterial,
  Raycaster,
  Vector3,
  Group,
  type BufferGeometry,
} from 'three'
import type { DungeonFloorLayout } from '../managers/dungeonManager'
import { buildMasonryWall, buildMasonryWallGhost } from './dungeon-geo-masonry'
import { disposeDungeonGroup } from './dungeon-geometry'
import { buildDungeonFloorGroup } from './dungeon-geo-floor'
import { dungeonCaveTheme } from './dungeon-cave-themes'
import { HOUSING_TEXTURES } from './housing-textures'
import { DUNGEON_FLOOR_TEXTURE_IDX } from './dungeon-geo-constants'

vi.mock('./dungeon-geo-doors', () => ({ buildInteriorDoor: vi.fn() }))

const geometries: BufferGeometry[] = []
const material = new MeshBasicMaterial()
afterEach(() => {
  for (const geo of geometries) geo.dispose()
  geometries.length = 0
  material.dispose()
})

describe('masonry walls', () => {
  it.each([
    [true, 1],
    [true, -1],
    [false, 1],
    [false, -1],
  ] as const)(
    'has flat stepped brick faces and thin tops (alongX=%s, inward=%s)',
    (alongX, inward) => {
      const geo = buildMasonryWall(alongX, 2, 10, 5, inward, 3, 42)
      geometries.push(geo)
      const mesh = new Mesh(geo, material)
      const point = (a: number, y: number, depth: number) =>
        new Vector3(
          alongX ? a : 5 + inward * depth,
          y,
          alongX ? 5 + inward * depth : a
        )
      const direction = point(0, 0, -1).sub(point(0, 0, 0))
      const frontAt = (a: number, y: number) => {
        const hits = new Raycaster(point(a, y, 1), direction).intersectObject(
          mesh
        )
        const hit = hits[0]
        expect(hit).toBeDefined()
        expect(new Set(hits.map((h) => h.distance.toFixed(5))).size).toBe(1)
        expect(hit.face!.normal.dot(direction)).toBeCloseTo(-1, 5)
        return 1 - hit.distance
      }
      const depths: number[] = []
      for (let row = 2; row < 12; row++) {
        for (let column = 7; column < 20; column++) {
          const a = ((column + 0.5 + (row % 2) * 0.5) * 4) / 9
          const y = ((row + 0.5) * 2) / 9
          const depth = frontAt(a, y)
          expect(frontAt(a + 0.05, y + 0.025)).toBeCloseTo(depth, 5)
          depths.push(depth)
        }
      }
      expect(Math.max(...depths)).toBeGreaterThan(0.015)
      expect(Math.max(...depths)).toBeLessThan(0.04001)
      expect(Math.min(...depths)).toBeGreaterThan(-0.01201)
      expect(depths.filter((depth) => depth > 0.001).length).toBeLessThan(
        depths.length * 0.1
      )
      expect(
        depths.filter((depth) => Math.abs(depth) < 0.001).length
      ).toBeGreaterThan(depths.length * 0.9)
      expect(frontAt(2.01, 1)).toBeCloseTo(0, 5)
      expect(frontAt(9.99, 1)).toBeCloseTo(0, 5)
      expect(frontAt(6, 2.98)).toBeCloseTo(0, 5)
      const back = new Raycaster(
        point(6, 1, -1),
        direction.clone().negate()
      ).intersectObject(mesh)[0]
      expect(back.distance).toBeCloseTo(0.9, 5)
      expect(geo.index!.count / 3).toBeLessThan(3000)
    }
  )

  it('keeps brick depths stable across rebuilds', () => {
    const first = buildMasonryWall(true, 2, 10, 5, 1, 3, 42)
    const again = buildMasonryWall(true, 2, 10, 5, 1, 3, 42)
    const other = buildMasonryWall(true, 2, 10, 5, 1, 3, 77)
    geometries.push(first, again, other)
    expect(first.getAttribute('position').array).toEqual(
      again.getAttribute('position').array
    )
    expect(first.getAttribute('position').array).not.toEqual(
      other.getAttribute('position').array
    )
    expect(first.getAttribute('color').array).toEqual(
      again.getAttribute('color').array
    )
    const colors = first.getAttribute('color')
    const tones = new Set<number>()
    for (let i = 0; i < colors.count; i++) tones.add(colors.getX(i))
    expect(tones.size).toBeGreaterThan(100)
  })

  it('keeps visible mortar gaps between staggered block faces', () => {
    const geo = buildMasonryWall(true, 2, 10, 5, 1, 3, 42)
    geometries.push(geo)
    const mesh = new Mesh(geo, material)
    const uvAt = (x: number, y: number) => {
      const hit = new Raycaster(
        new Vector3(x, y, 6),
        new Vector3(0, 0, -1)
      ).intersectObject(mesh)[0]
      expect(hit).toBeDefined()
      return hit.uv!
    }
    const joint = uvAt(4, 1)
    for (const offset of [-0.006, 0.006]) {
      expect(uvAt(4 + offset, 1).distanceTo(joint)).toBeLessThan(0.00001)
      expect(uvAt(4.1, 8 / 9 + offset).distanceTo(joint)).toBeLessThan(0.00001)
    }
    expect(uvAt(4.02, 1).distanceTo(joint)).toBeGreaterThan(0.01)
    expect(uvAt(4, 11 / 9).distanceTo(joint)).toBeGreaterThan(0.01)
  })

  it.each([true, false])(
    'keeps clipped blocks inside very short wall runs (alongX=%s)',
    (alongX) => {
      const geo = buildMasonryWall(alongX, 2.221, 2.223, 5, 1, 3, 42)
      geometries.push(geo)
      const positions = geo.getAttribute('position')
      for (let i = 0; i < positions.count; i++) {
        const along = alongX ? positions.getX(i) : positions.getZ(i)
        expect(along).toBeGreaterThanOrEqual(2.221 - 0.000001)
        expect(along).toBeLessThanOrEqual(2.223 + 0.000001)
      }
    }
  )
})

describe('masonry wall ghosts', () => {
  it.each([true, false])(
    'draws only one flat layer from either side (alongX=%s)',
    (alongX) => {
      const geo = buildMasonryWallGhost(alongX, 2, 10, 5, 3)
      geometries.push(geo)
      const mesh = new Mesh(geo, material)
      const positions = geo.getAttribute('position')
      for (let i = 0; i < positions.count; i++) {
        expect(alongX ? positions.getZ(i) : positions.getX(i)).toBe(5)
      }
      for (const side of [-1, 1]) {
        for (const y of [0.25, 1, 2.5]) {
          const target = new Vector3(alongX ? 6.1 : 5, y, alongX ? 5 : 6.1)
          const origin = target
            .clone()
            .add(new Vector3(alongX ? 0.8 : side, 0.7, alongX ? side : 0.8))
          const hits = new Raycaster(
            origin,
            target.clone().sub(origin).normalize()
          ).intersectObject(mesh)
          expect(hits).toHaveLength(1)
          expect(hits[0].point.distanceTo(target)).toBeLessThan(0.00001)
        }
      }
      expect(geo.index!.count / 3).toBe(4)
    }
  )

  it('keeps ghost planes unpickable and releases them with the floor', () => {
    const floor = buildDungeonFloorGroup(layout, ctx, [], dungeonId)
    const masonryWalls = floor.wallRuns.filter((wall) => wall.ghostMesh)
    expect(masonryWalls.length).toBeGreaterThan(0)
    const disposals = []
    for (const { mesh, ghostMesh } of masonryWalls) {
      expect(mesh.visible).toBe(true)
      expect(ghostMesh!.visible).toBe(false)
      expect(ghostMesh!.parent).toBe(mesh.parent)
      expect(new Raycaster().intersectObject(ghostMesh!)).toEqual([])
      const ghostMaterial = ghostMesh!.material as MeshBasicMaterial
      expect(ghostMaterial.opacity).toBeLessThanOrEqual(0.25)
      expect(ghostMaterial.depthWrite).toBe(false)
      disposals.push(vi.spyOn(ghostMesh!.geometry, 'dispose'))
    }
    disposeDungeonGroup(floor.group)
    for (const dispose of disposals) expect(dispose).toHaveBeenCalledOnce()
  })
})

const ctx = { grid: 32, wallHeight: 3, floorHeight: 4, shaftW: 2, shaftLen: 8 }
const carved = Array<boolean>(ctx.grid ** 2).fill(false)
for (let z = 2; z < 28; z++)
  for (let x = 4; x < 8; x++) carved[x + z * ctx.grid] = true
const layout: DungeonFloorLayout = {
  depth: 1,
  carved,
  rooms: [{ x: 4, z: 24, w: 4, d: 4 }],
  upShaft: { x: 4, z: 2, alongZ: true, reversed: false },
  downShaft: { x: 6, z: 2, alongZ: true, reversed: false },
  props: [{ x: 4, z: 12, kind: 'barrel', rotation: 0, stack: 1 }],
  chest: [7, 18],
  spawns: [],
}
const dungeonId = 'floor-review-111'
const soilTexture = HOUSING_TEXTURES.findIndex(
  (entry) => entry.glb === 'red_laterite_soil_stones_1k'
)
function buildFloor() {
  const { group } = buildDungeonFloorGroup(layout, ctx, [], dungeonId)
  group.traverse((obj) => {
    if (obj instanceof Mesh) geometries.push(obj.geometry)
  })
  return group
}
const groundAt = (group: Group, x: number, z: number) =>
  new Raycaster(
    new Vector3(x, 0.5, z),
    new Vector3(0, -1, 0),
    0,
    1
  ).intersectObject(group)[0]

function damageSurface(group: Group) {
  const surface = new Group()
  for (const child of group.getObjectByName('masonryFloorDamage')!.children) {
    const mesh = new Mesh((child as Mesh).geometry, material)
    mesh.userData.textureIndex = child.userData.textureIndex
    surface.add(mesh)
  }
  return surface
}

describe('damaged masonry floors', () => {
  it('keeps a continuous flat picking surface beneath damaged tiles', () => {
    expect(dungeonCaveTheme(dungeonId, 1).id).toBe('masonry')
    const group = buildFloor()
    const floor = groundAt(group, 5, 11).object as Mesh
    expect(floor.userData.textureIndex).toBe(
      dungeonCaveTheme(dungeonId, 1).floorTexture
    )
    expect(floor.geometry.index!.count / 3).toBeLessThan(200)
    expect(HOUSING_TEXTURES[floor.userData.textureIndex].bumpScale).toBe(0)
    for (let z = 10.625; z < 23.5; z += 0.25) {
      for (let x = 4.125; x < 8; x += 0.25) {
        const hit = groundAt(group, x, z)
        expect(hit).toBeDefined()
        expect(hit.point.y).toBe(0)
        expect(hit.object).toBe(floor)
        expect(hit.face!.normal.y).toBe(1)
      }
    }
    expect(groundAt(group, 3.9, 16)).toBeUndefined()
  })

  it('draws soil and broken pieces above the floor without casting seam shadows', () => {
    const group = buildFloor()
    const damage = group.getObjectByName('masonryFloorDamage')!
    expect(damage.children).toHaveLength(2)
    for (const child of damage.children) {
      const mesh = child as Mesh
      expect(mesh.castShadow).toBe(false)
      expect(mesh.receiveShadow).toBe(true)
      mesh.geometry.computeBoundingBox()
      expect(mesh.geometry.boundingBox!.min.y).toBeGreaterThan(0)
      expect(mesh.geometry.boundingBox!.max.y).toBeLessThan(0.035)
    }
    const surface = damageSurface(group)
    let soilHits = 0
    let fragmentHits = 0
    let intactHits = 0
    for (let z = 10.625; z < 23.5; z += 0.25) {
      for (let x = 4.125; x < 8; x += 0.25) {
        const hit = groundAt(surface, x, z)
        if (!hit) {
          intactHits++
        } else if (hit.object.userData.textureIndex === soilTexture) {
          soilHits++
          expect(hit.point.y).toBeCloseTo(0.006, 5)
        } else {
          fragmentHits++
          expect(hit.point.y).toBeGreaterThan(0.006)
        }
      }
    }
    expect(soilHits).toBeGreaterThan(20)
    expect(fragmentHits).toBeGreaterThan(5)
    expect(intactHits).toBeGreaterThan(soilHits * 3)
  })

  it('preserves rooms, shaft openings, and clearance around props and chests', () => {
    const group = buildFloor()
    const damage = damageSurface(group)
    const floorTexture = dungeonCaveTheme(dungeonId, 1).floorTexture
    for (const [x, z] of [
      [4.25, 12.25],
      [5.25, 12.75],
      [7.25, 18.25],
      [6.75, 19.25],
      [4.25, 10.25],
      [6.25, 23.75],
    ]) {
      const hit = groundAt(group, x, z)
      expect(hit.object.userData.textureIndex).toBe(floorTexture)
      expect(hit.point.y).toBeCloseTo(0, 5)
      expect(groundAt(damage, x, z)).toBeUndefined()
    }
    expect(groundAt(group, 5.25, 25.25).object.userData.textureIndex).toBe(
      DUNGEON_FLOOR_TEXTURE_IDX
    )
    expect(groundAt(group, 6.5, 7.5)).toBeUndefined()
  })
})
