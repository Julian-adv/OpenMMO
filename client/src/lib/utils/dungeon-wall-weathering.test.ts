import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  BufferGeometry,
  BoxGeometry,
  Mesh,
  MeshBasicMaterial,
  Raycaster,
  Vector3,
} from 'three'
import { buildCaveWall } from './dungeon-geo-cave'
import { buildMasonryWall } from './dungeon-geo-masonry'
import { buildDungeonWallWeathering } from './dungeon-wall-weathering'
import { buildDungeonFloorGroup } from './dungeon-geo-floor'
import { disposeDungeonGroup } from './dungeon-geometry'
import { dungeonCaveTheme } from './dungeon-cave-themes'
import { getHousingMaterial } from './housing-textures'
import {
  DUNGEON_WALL_WEATHERING_TEXTURE_IDX,
  DUNGEON_WALL_DETAILS_TEXTURE_IDX,
  DUNGEON_WALL_TEXTURE_IDX,
} from './dungeon-geo-constants'

vi.mock('./dungeon-geo-doors', () => ({ buildInteriorDoor: vi.fn() }))

const geometries: BufferGeometry[] = []
const material = new MeshBasicMaterial()
const buildRoomWall: typeof buildCaveWall = (
  alongX,
  lo,
  hi,
  boundary,
  inward,
  height
) =>
  new BoxGeometry(
    alongX ? hi - lo : 0.1,
    height,
    alongX ? 0.1 : hi - lo
  ).translate(
    alongX ? (lo + hi) / 2 : boundary - inward * 0.05,
    height / 2 + 0.02,
    alongX ? boundary - inward * 0.05 : (lo + hi) / 2
  )
afterEach(() => {
  for (const geo of geometries) geo.dispose()
  geometries.length = 0
  material.dispose()
})

describe('dungeon wall weathering', () => {
  it.each([
    [true, 1],
    [true, -1],
    [false, 1],
    [false, -1],
  ] as const)(
    'conforms to both sides of curved, brick, and room walls (%s, %s)',
    (alongX, inward) => {
      for (const build of [buildCaveWall, buildMasonryWall, buildRoomWall]) {
        const wall = build(alongX, 2, 14, 5, inward, 3, 42)
        const decal = buildDungeonWallWeathering(wall, {
          alongX,
          inward,
          lo: 2,
          hi: 14,
          boundary: 5,
          height: 3,
          seed: 42,
        })!
        geometries.push(wall, decal)
        const mesh = new Mesh(wall, material)
        const positions = decal.getAttribute('position')
        const normals = decal.getAttribute('normal')
        const sides = new Set<number>()
        for (let i = 0; i < positions.count; i += 3) {
          const point = new Vector3()
            .fromBufferAttribute(positions, i)
            .add(new Vector3().fromBufferAttribute(positions, i + 1))
            .add(new Vector3().fromBufferAttribute(positions, i + 2))
            .divideScalar(3)
          const normal = new Vector3()
            .fromBufferAttribute(normals, i)
            .normalize()
          const along = alongX ? point.x : point.z
          expect(along).toBeGreaterThanOrEqual(2 - 0.00001)
          expect(along).toBeLessThanOrEqual(14 + 0.00001)
          expect(point.y).toBeGreaterThanOrEqual(-0.00001)
          sides.add(Math.sign(alongX ? normal.z : normal.x))
          if (i % 21 !== 0) continue
          const hits = new Raycaster(
            point.clone().addScaledVector(normal, 0.5),
            normal.negate(),
            0,
            0.51
          ).intersectObject(mesh)
          expect(hits[0]?.distance).toBeCloseTo(0.5, 4)
        }
        expect(sides).toEqual(new Set([-1, 1]))
        expect(positions.count).toBeLessThan(30000)
      }
    }
  )

  it('keeps a floor stable but varies marks between seeds', () => {
    const wall = buildMasonryWall(true, 2, 14, 5, 1, 3, 42)
    const spec = {
      alongX: true,
      inward: 1,
      lo: 2,
      hi: 14,
      boundary: 5,
      height: 3,
      seed: 42,
    }
    const first = buildDungeonWallWeathering(wall, spec)!
    const again = buildDungeonWallWeathering(wall, spec)!
    const other = buildDungeonWallWeathering(wall, { ...spec, seed: 77 })!
    geometries.push(wall, first, again, other)
    expect(first.getAttribute('position').array).toEqual(
      again.getAttribute('position').array
    )
    expect(first.getAttribute('uv').array).toEqual(
      again.getAttribute('uv').array
    )
    expect(first.getAttribute('position').array).not.toEqual(
      other.getAttribute('position').array
    )
    const uv = first.getAttribute('uv')
    const variants = new Set<string>()
    for (let i = 0; i < uv.count; i++) {
      expect(uv.getX(i)).toBeGreaterThanOrEqual(0)
      expect(uv.getX(i)).toBeLessThanOrEqual(1)
      expect(uv.getY(i)).toBeGreaterThanOrEqual(0)
      expect(uv.getY(i)).toBeLessThanOrEqual(1)
    }
    for (const group of first.groups) {
      for (let i = group.start; i < group.start + group.count; i++) {
        const columns = group.materialIndex === 1 ? 2 : 3
        variants.add(
          `${group.materialIndex}:${Math.floor(uv.getX(i) * columns)}:${Math.floor(uv.getY(i) * 2)}`
        )
      }
    }
    expect(variants.size).toBeGreaterThanOrEqual(3)
    expect(buildDungeonWallWeathering(wall, { ...spec, hi: 2.5 })).toBeNull()
  })

  it('places cobwebs near the upper ends and keeps crack centers opaque', () => {
    const wall = buildRoomWall(true, 2, 14, 5, 1, 3, 42)
    wall.translate(0, 2, 0)
    geometries.push(wall)
    let webVertices = 0
    let crackVertices = 0
    for (const seed of [42, 77, 17]) {
      const decal = buildDungeonWallWeathering(wall, {
        alongX: true,
        lo: 2,
        hi: 14,
        boundary: 5,
        inward: 1,
        height: 3,
        seed,
      })!
      geometries.push(decal)
      const detail = decal.groups.find((g) => g.materialIndex === 1)!
      const uv = decal.getAttribute('uv')
      const positions = decal.getAttribute('position')
      const colors = decal.getAttribute('color')
      for (let i = detail.start; i < detail.start + detail.count; i++) {
        if (uv.getX(i) > 0.5) {
          expect(positions.getY(i)).toBeGreaterThan(3.5)
          expect(
            Math.min(positions.getX(i) - 2, 14 - positions.getX(i))
          ).toBeLessThan(1.4)
          webVertices++
        } else {
          expect(colors.getW(i)).toBeGreaterThanOrEqual(0.939)
          crackVertices++
        }
      }
      expect(decal.boundingBox!.min.y).toBeGreaterThanOrEqual(2.019)
    }
    expect(webVertices).toBeGreaterThan(0)
    expect(crackVertices).toBeGreaterThan(0)
  })

  it('adds unpickable overlays to all corridor sets and rooms and disposes them with the floor', () => {
    const ctx = {
      grid: 24,
      wallHeight: 3,
      floorHeight: 4,
      shaftW: 2,
      shaftLen: 8,
    }
    const carved = Array<boolean>(ctx.grid ** 2).fill(false)
    for (let z = 2; z < 22; z++)
      for (let x = 4; x < 8; x++) carved[x + z * ctx.grid] = true
    const themes = new Set<string>()
    for (const depth of [1, 2, 3]) {
      const theme = dungeonCaveTheme('weathering-test', depth)
      themes.add(theme.id)
      const floor = buildDungeonFloorGroup(
        {
          depth,
          carved,
          rooms: [{ x: 4, z: 18, w: 4, d: 4 }],
          upShaft: { x: 4, z: 2, alongZ: true, reversed: false },
          props: [],
          spawns: [],
        },
        ctx,
        [],
        'weathering-test'
      )
      const overlays = floor.wallRuns.filter((w) => w.weathering)
      expect(overlays.length).toBeGreaterThan(0)
      expect(
        overlays.some(
          (w) => w.mesh.userData.textureIndex === DUNGEON_WALL_TEXTURE_IDX
        )
      ).toBe(true)
      expect(
        overlays.some((w) => w.mesh.userData.textureIndex === theme.wallTexture)
      ).toBe(true)
      const disposals = overlays.map(({ mesh, weathering }) => {
        expect(weathering!.parent).toBe(mesh)
        expect(weathering!.material).toEqual([
          getHousingMaterial(DUNGEON_WALL_WEATHERING_TEXTURE_IDX),
          getHousingMaterial(DUNGEON_WALL_DETAILS_TEXTURE_IDX),
        ])
        expect(new Raycaster().intersectObject(weathering!)).toEqual([])
        expect(weathering!.castShadow).toBe(false)
        return vi.spyOn(weathering!.geometry, 'dispose')
      })
      disposeDungeonGroup(floor.group)
      for (const dispose of disposals) expect(dispose).toHaveBeenCalledOnce()
    }
    expect(themes.size).toBe(3)
    const decalMaterial = getHousingMaterial(
      DUNGEON_WALL_WEATHERING_TEXTURE_IDX
    )
    expect(decalMaterial.depthWrite).toBe(false)
    expect(decalMaterial.polygonOffset).toBe(true)
  })
})
