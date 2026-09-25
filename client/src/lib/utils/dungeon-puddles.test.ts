import { describe, expect, it } from 'vitest'
import type { DungeonFloorLayout } from '../managers/dungeonManager'
import {
  dungeonPuddleBounds,
  dungeonPuddleRadius,
  generateDungeonPuddles,
} from './dungeon-puddles'

const ctx = {
  grid: 64,
  wallHeight: 3,
  floorHeight: 4,
  shaftW: 2,
  shaftLen: 8,
}

function fixture(): DungeonFloorLayout {
  const rooms = [
    { x: 2, z: 2, w: 14, d: 14 },
    { x: 40, z: 2, w: 14, d: 14 },
    { x: 2, z: 40, w: 14, d: 14 },
    { x: 40, z: 40, w: 14, d: 14 },
  ]
  const carved = Array<boolean>(ctx.grid ** 2).fill(false)
  for (const r of [
    ...rooms,
    { x: 8, z: 8, w: 39, d: 2 },
    { x: 8, z: 8, w: 2, d: 39 },
    { x: 8, z: 46, w: 39, d: 2 },
    { x: 46, z: 8, w: 2, d: 39 },
  ])
    for (let z = r.z; z < r.z + r.d; z++)
      for (let x = r.x; x < r.x + r.w; x++) carved[x + z * ctx.grid] = true
  return {
    depth: 1,
    rooms,
    carved,
    upShaft: { x: 6, z: 4, alongZ: true, reversed: false },
    downShaft: { x: 42, z: 44, alongZ: false, reversed: true },
    chest: [45, 5],
    spawns: [],
    props: [{ x: 12, z: 12, kind: 'barrel', stack: 1, rotation: 0 }],
  }
}

describe('dungeon puddle placement', () => {
  it('keeps positions stable on re-entry and varies them by dungeon and depth', () => {
    const layout = fixture()
    const first = generateDungeonPuddles(layout, ctx, 'crypt')
    expect(first.length).toBeGreaterThan(0)
    expect(generateDungeonPuddles(layout, ctx, 'crypt')).toEqual(first)
    expect(generateDungeonPuddles(layout, ctx, 'cave')).not.toEqual(first)
    expect(
      generateDungeonPuddles({ ...layout, depth: 2 }, ctx, 'crypt')
    ).not.toEqual(first)
  })

  it('keeps the entire surface on its floor region and clear of shafts and props', () => {
    const layout = fixture()
    const roomAt = (x: number, z: number) =>
      layout.rooms.findIndex(
        (r) => x >= r.x && x < r.x + r.w && z >= r.z && z < r.z + r.d
      )
    let corridors = 0
    let rooms = 0
    for (let seed = 0; seed < 100; seed++) {
      const puddles = generateDungeonPuddles(layout, ctx, `crypt-${seed}`)
      for (const p of puddles) {
        const bounds = dungeonPuddleBounds(p)
        const region = roomAt(p.x, p.z)
        if (region < 0) corridors++
        else rooms++
        for (let z = Math.floor(bounds.minZ); z < Math.ceil(bounds.maxZ); z++)
          for (
            let x = Math.floor(bounds.minX);
            x < Math.ceil(bounds.maxX);
            x++
          ) {
            expect(layout.carved[x + z * ctx.grid]).toBe(true)
            expect(roomAt(x, z)).toBe(region)
            expect(x >= 6 && x < 8 && z >= 4 && z < 12).toBe(false)
            expect(x >= 42 && x < 50 && z >= 44 && z < 46).toBe(false)
            expect(x === 45 && z === 5).toBe(false)
            expect(x === 12 && z === 12).toBe(false)
            if (p.x - p.width / 2 < bounds.minX && x === bounds.minX)
              expect(layout.carved[x - 1 + z * ctx.grid]).toBe(false)
            if (p.x + p.width / 2 > bounds.maxX && x + 1 === bounds.maxX)
              expect(layout.carved[x + 1 + z * ctx.grid]).toBe(false)
            if (p.z - p.depth / 2 < bounds.minZ && z === bounds.minZ)
              expect(layout.carved[x + (z - 1) * ctx.grid]).toBe(false)
            if (p.z + p.depth / 2 > bounds.maxZ && z + 1 === bounds.maxZ)
              expect(layout.carved[x + (z + 1) * ctx.grid]).toBe(false)
          }
      }
    }
    expect(corridors).toBeGreaterThan(rooms * 10)
    expect(rooms).toBeGreaterThan(0)
    expect(rooms).toBeLessThan(65)
  })

  it('does not create puddles without carved floor', () => {
    const layout = fixture()
    layout.carved.fill(false)
    expect(generateDungeonPuddles(layout, ctx, 'crypt')).toEqual([])
  })

  it('mixes small and much larger pools with irregular outlines and edge placement', () => {
    const layout = fixture()
    const puddles = Array.from({ length: 100 }, (_, index) =>
      generateDungeonPuddles(layout, ctx, `variety-${index}`)
    ).flat()
    const lengths = puddles.map((p) => Math.max(p.width, p.depth))
    expect(lengths.some((length) => length < 2)).toBe(true)
    expect(lengths.filter((length) => length >= 6).length).toBeGreaterThan(10)
    expect(lengths.some((length) => length >= 8)).toBe(true)
    expect(new Set(puddles.map((p) => p.shape.lobes))).toEqual(
      new Set([2, 3, 4, 5])
    )
    const horizontal = puddles.filter(
      (p) => p.x > 17 && p.x < 39 && p.z > 8 && p.z < 10
    )
    expect(horizontal.some((p) => p.z < 8.65)).toBe(true)
    expect(horizontal.some((p) => p.z > 9.35)).toBe(true)
    expect(horizontal.some((p) => Math.abs(p.z - 9) < 0.1)).toBe(true)
  })

  it('places zero to two distinct drip sources inside each water outline', () => {
    const layout = fixture()
    const puddles = Array.from({ length: 100 }, (_, index) =>
      generateDungeonPuddles(layout, ctx, `drips-${index}`)
    ).flat()
    const counts = [0, 0, 0]
    for (const p of puddles) {
      counts[p.drips.length]++
      for (const drip of p.drips) {
        const bounds = dungeonPuddleBounds(p)
        expect(drip.x).toBeGreaterThanOrEqual(bounds.minX + 0.1)
        expect(drip.x).toBeLessThanOrEqual(bounds.maxX - 0.1)
        expect(drip.z).toBeGreaterThanOrEqual(bounds.minZ + 0.1)
        expect(drip.z).toBeLessThanOrEqual(bounds.maxZ - 0.1)
        const dx = ((drip.x - p.x) * 2) / p.width
        const dz = ((drip.z - p.z) * 2) / p.depth
        const angle = Math.atan2(dz, dx)
        expect(Math.hypot(dx, dz)).toBeLessThan(
          dungeonPuddleRadius(p.shape, angle) - 0.1
        )
        expect(drip.period).toBeGreaterThan(1.8)
        expect(drip.phase).toBeLessThan(drip.period)
      }
      if (p.drips.length === 2) {
        const [a, b] = p.drips
        expect(Math.hypot(a.x - b.x, a.z - b.z)).toBeGreaterThan(0.1)
        expect(a.phase).not.toBe(b.phase)
      }
    }
    for (const count of counts)
      expect(count).toBeGreaterThan(puddles.length * 0.1)
  })

  it('cuts the actual water outline against walls in all four directions', () => {
    const touching = [0, 0, 0, 0]
    for (let index = 0; index < 40; index++) {
      for (const p of generateDungeonPuddles(
        fixture(),
        ctx,
        `edges-${index}`
      )) {
        const b = dungeonPuddleBounds(p)
        const extents = [
          p.x + (dungeonPuddleRadius(p.shape, 0) * p.width) / 2 > b.maxX,
          p.z + (dungeonPuddleRadius(p.shape, Math.PI / 2) * p.depth) / 2 >
            b.maxZ,
          p.x - (dungeonPuddleRadius(p.shape, Math.PI) * p.width) / 2 < b.minX,
          p.z - (dungeonPuddleRadius(p.shape, -Math.PI / 2) * p.depth) / 2 <
            b.minZ,
        ]
        extents.forEach((cut, side) => {
          if (cut) touching[side]++
        })
      }
    }
    expect(touching.every((count) => count > 10)).toBe(true)
  })
})
