import { describe, expect, it } from 'vitest'
import { Mesh, MeshBasicMaterial, Raycaster, Vector3 } from 'three'
import { buildCaveWall } from './dungeon-geo-cave'
import { DUNGEON_CAVE_THEMES, dungeonCaveTheme } from './dungeon-cave-themes'

describe('cave corridor walls', () => {
  it.each([
    [true, 1],
    [true, -1],
    [false, 1],
    [false, -1],
  ] as const)(
    'faces into the corridor (alongX=%s, inward=%s)',
    (alongX, inward) => {
      const geo = buildCaveWall(alongX, 2, 10, 5, inward, 3, 42)
      const material = new MeshBasicMaterial()
      const mesh = new Mesh(geo, material)
      const start = new Vector3(
        alongX ? 6 : 5 + inward,
        1,
        alongX ? 5 + inward : 6
      )
      const direction = new Vector3(
        alongX ? 0 : -inward,
        0,
        alongX ? -inward : 0
      )
      const hits = new Raycaster(start, direction, 0, 1.1).intersectObject(mesh)
      expect(hits.length).toBeGreaterThan(0)
      expect(hits[0].face!.normal.dot(direction)).toBeLessThan(-0.9)
      const positions = geo.getAttribute('position')
      const upperSections = new Map<string, number[]>()
      let baseInset = 0
      for (let i = 0; i < positions.count; i++) {
        const along = alongX ? positions.getX(i) : positions.getZ(i)
        const across = alongX ? positions.getZ(i) : positions.getX(i)
        const offset = (across - 5) * inward
        expect(positions.getY(i)).toBeGreaterThanOrEqual(0)
        expect(offset).toBeLessThan(0.2)
        if (positions.getY(i) === 0) baseInset = Math.max(baseInset, offset)
        if (positions.getY(i) > 2.95) {
          expect(offset).toBeLessThanOrEqual(0.013)
          expect(offset).toBeGreaterThanOrEqual(-0.113)
        }
        if (positions.getY(i) > 0.25) {
          const key = `${along}:${positions.getY(i)}`
          const section = upperSections.get(key) ?? []
          section.push(across)
          upperSections.set(key, section)
        }
        if (along === 2 || along === 10) {
          expect(offset).toBeLessThanOrEqual(0.00001)
          expect(offset).toBeGreaterThanOrEqual(-0.10001)
        }
      }
      expect(baseInset).toBeGreaterThan(0.13)
      for (const section of upperSections.values())
        expect(Math.max(...section) - Math.min(...section)).toBeCloseTo(0.1, 5)
      const relief: number[] = []
      for (const y of [0.75, 1.25, 1.75, 2.25]) {
        for (let along = 3; along <= 9; along += 0.5) {
          const origin = new Vector3(
            alongX ? along : 5 + inward,
            y,
            alongX ? 5 + inward : along
          )
          const hit = new Raycaster(origin, direction, 0, 1.3).intersectObject(
            mesh
          )[0]
          expect(hit).toBeDefined()
          relief.push(1 - hit.distance)
        }
      }
      expect(Math.max(...relief)).toBeGreaterThan(0.045)
      expect(Math.min(...relief)).toBeLessThan(-0.03)
      expect(geo.boundingBox!.max.y).toBeLessThan(3.2)
      expect(geo.index!.count / 3).toBeLessThan(900)
      geo.dispose()
      material.dispose()
    }
  )

  it('rebuilds the same contour and changes it for another seed', () => {
    const a = buildCaveWall(true, 2, 10, 5, 1, 3, 42)
    const b = buildCaveWall(true, 2, 10, 5, 1, 3, 42)
    const c = buildCaveWall(true, 2, 10, 5, 1, 3, 77)
    expect(a.getAttribute('position').array).toEqual(
      b.getAttribute('position').array
    )
    expect(a.getAttribute('position').array).not.toEqual(
      c.getAttribute('position').array
    )
    a.dispose()
    b.dispose()
    c.dispose()
  })
})

describe('cave material sets', () => {
  it('uses all three registered wall/floor pairs across consecutive floors', () => {
    const themes = [1, 2, 3].map((depth) => dungeonCaveTheme('ruins', depth))
    expect(new Set(themes.map((theme) => theme.id)).size).toBe(3)
    expect(dungeonCaveTheme('ruins', 2)).toBe(themes[1])
    for (const theme of DUNGEON_CAVE_THEMES) {
      expect(theme.wallTexture).toBeGreaterThanOrEqual(0)
      expect(theme.floorTexture).toBeGreaterThanOrEqual(0)
      expect(theme.wallTexture).not.toBe(theme.floorTexture)
    }
  })
})
