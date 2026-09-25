import { afterEach, describe, expect, it } from 'vitest'
import { Matrix4, Vector3, type InstancedMesh } from 'three'
import { DungeonPuddles } from './dungeon-puddles'
import type { DungeonDrip, DungeonPuddle } from '../utils/dungeon-puddles'

const drip: DungeonDrip = {
  x: 20.2,
  z: 10.1,
  seed: 12.5,
  period: 2,
  phase: 0,
}
const puddle: DungeonPuddle = {
  x: 20,
  z: 10,
  width: 2,
  depth: 1,
  seed: 12.5,
  shape: { lobes: 3, irregularity: 0.12, phase: 1.2, skew: 0.05 },
  drips: [drip],
}
let effect: DungeonPuddles
afterEach(() => effect?.dispose())

function advance(seconds: number, animate = true) {
  const impacts: number[] = []
  for (let i = 0; i < Math.round(seconds * 100); i++) {
    if (effect.update(0.01, animate).length) impacts.push((i + 1) / 100)
  }
  return impacts
}

describe('dungeon drip timing', () => {
  it('clips the mesh at a wall without stretching the outline or moving ripples', () => {
    effect = new DungeonPuddles(
      [{ ...puddle, clip: { minX: 19.8, minZ: 9.5, maxX: 21, maxZ: 10.2 } }],
      3
    )
    const surface = effect.group.children[1] as InstancedMesh
    const matrix = new Matrix4()
    surface.getMatrixAt(0, matrix)
    const low = new Vector3(-0.5, 0, -0.5).applyMatrix4(matrix)
    const high = new Vector3(0.5, 0, 0.5).applyMatrix4(matrix)
    expect(low.x).toBeCloseTo(19.8)
    expect(low.z).toBeCloseTo(9.5)
    expect(high.x).toBeCloseTo(21)
    expect(high.z).toBeCloseTo(10.2)
    const transform = surface.geometry.getAttribute('aPuddleUv')
    const dripUV = (drip.x - low.x) / (high.x - low.x)
    const remappedU = dripUV * transform.getZ(0) + transform.getX(0)
    expect(puddle.x + (remappedU - 0.5) * puddle.width).toBeCloseTo(drip.x)
    const dripV = (high.z - drip.z) / (high.z - low.z)
    const remappedV = dripV * transform.getW(0) + transform.getY(0)
    expect(puddle.z - (remappedV - 0.5) * puddle.depth).toBeCloseTo(drip.z)
  })

  it('varies each interval up to six times the original without speeding up', () => {
    effect = new DungeonPuddles([puddle], 3)
    const impacts = advance(180)
    const fallTime = Math.sqrt(2.8 / 5)
    expect(impacts.length).toBeGreaterThan(10)
    expect(impacts[0]).toBeGreaterThanOrEqual(fallTime)
    expect(impacts[0]).toBeLessThan(fallTime + 0.01)
    const intervals = impacts
      .slice(1)
      .map((time, index) => time - impacts[index])
    for (const interval of intervals) {
      expect(interval).toBeGreaterThanOrEqual(drip.period - 0.01)
      expect(interval).toBeLessThanOrEqual(drip.period * 6 + 0.01)
    }
    expect(intervals.some((interval) => interval >= drip.period * 4)).toBe(true)
    expect(
      new Set(intervals.map((interval) => interval.toFixed(2))).size
    ).toBeGreaterThan(5)
  })

  it('respects each puddle phase without replaying impacts from before entry', () => {
    effect = new DungeonPuddles(
      [{ ...puddle, drips: [{ ...drip, phase: 1 }] }],
      3
    )
    const drops = effect.group.children[2] as InstancedMesh
    const firstImpact =
      drops.geometry.getAttribute('aDripStart').getX(0) + Math.sqrt(2.8 / 5)
    expect(firstImpact).toBeGreaterThan(1)
    expect(effect.update(0)).toEqual([])
    const impacts = advance(20)
    expect(impacts[0]).toBeGreaterThanOrEqual(firstImpact - 0.000001)
    expect(impacts[0]).toBeLessThan(firstImpact + 0.01)
  })

  it('keeps ambient drip timing when graphics settings hide the particles', () => {
    effect = new DungeonPuddles([puddle], 3)
    expect(advance(1, false)).toEqual([0.75])
  })

  it('does not burst with missed sounds after a stalled or paused frame', () => {
    effect = new DungeonPuddles([puddle], 3)
    expect(advance(0.7)).toEqual([])
    expect(effect.update(30)).toEqual([drip])
    expect(effect.update(0)).toEqual([])
    expect(effect.update(0.1)).toEqual([])
  })

  it('leaves puddles with no drips still and silent', () => {
    effect = new DungeonPuddles([{ ...puddle, drips: [] }], 3)
    expect(advance(10)).toEqual([])
  })

  it('reports both independent impacts at their own positions', () => {
    const second = { ...drip, x: 19.7, z: 9.9, phase: 0.4, period: 3 }
    effect = new DungeonPuddles([{ ...puddle, drips: [drip, second] }], 3)
    const impacts: { time: number; drip: DungeonDrip }[] = []
    for (let i = 0; i < 9000; i++)
      for (const hit of effect.update(0.01))
        impacts.push({ time: (i + 1) / 100, drip: hit })
    const firstTimes = impacts
      .filter((hit) => hit.drip === drip)
      .map((hit) => hit.time)
    const secondTimes = impacts
      .filter((hit) => hit.drip === second)
      .map((hit) => hit.time)
    expect(firstTimes.length).toBeGreaterThan(5)
    expect(secondTimes.length).toBeGreaterThan(5)
    expect(firstTimes).not.toEqual(secondTimes)
    for (let i = 1; i < secondTimes.length; i++) {
      expect(secondTimes[i] - secondTimes[i - 1]).toBeGreaterThanOrEqual(
        second.period - 0.01
      )
      expect(secondTimes[i] - secondTimes[i - 1]).toBeLessThanOrEqual(
        second.period * 6 + 0.01
      )
    }
  })

  it('keeps rendered drops and ripples aligned with the impact sounds', () => {
    const second = {
      ...drip,
      x: 19.7,
      z: 9.9,
      seed: 71.4,
      phase: 0.4,
      period: 3,
    }
    const third = { ...drip, x: 30, seed: 28.7, phase: 1.2 }
    const sources = [drip, second, third]
    effect = new DungeonPuddles(
      [
        { ...puddle, drips: [drip, second] },
        { ...puddle, x: 30, drips: [third] },
      ],
      3
    )
    const surface = effect.group.children[1] as InstancedMesh
    const drops = effect.group.children[2] as InstancedMesh
    const starts = drops.geometry.getAttribute('aDripStart')
    const fallTime = Math.sqrt(2.8 / 5)
    const counts = [0, 0, 0]
    for (let frame = 0; frame < 9000; frame++) {
      const previousStarts = sources.map((_, index) => starts.getX(index))
      for (const hit of effect.update(0.01)) {
        const index = sources.indexOf(hit)
        const puddleIndex = index === 2 ? 1 : 0
        const slot = index === 1 ? 1 : 0
        const impact = surface.geometry
          .getAttribute(`aPuddleDrip${slot}`)
          .getZ(puddleIndex)
        expect(impact).toBeCloseTo(previousStarts[index] + fallTime, 4)
        expect(impact).toBeCloseTo((frame + 1) / 100, 1)
        expect(starts.getX(index)).toBeGreaterThan((frame + 1) / 100)
        counts[index]++
      }
    }
    expect(counts.every((count) => count > 5)).toBe(true)
    expect(surface.geometry.getAttribute('aPuddleDrip1').getW(1)).toBe(0)
  })
})
