import { describe, expect, it } from 'vitest'
import { foliageSeason, winterDay } from './foliageSeason'

describe('foliageSeason', () => {
  it('is green in summer, turned in autumn and bare in winter', () => {
    expect(foliageSeason(200)).toEqual({
      grassDry: 0,
      leafTurn: 0,
      leafFall: 0,
    })
    const autumn = foliageSeason(305)
    expect(autumn.leafTurn).toBeGreaterThan(0.8)
    expect(autumn.leafFall).toBe(0)
    expect(foliageSeason(45)).toEqual({ grassDry: 1, leafTurn: 0, leafFall: 1 })
  })

  it('stays continuous across the year boundary and through spring', () => {
    const before = foliageSeason(359.99)
    const after = foliageSeason(0)
    expect(before.grassDry).toBeCloseTo(after.grassDry)
    expect(before.leafFall).toBeCloseTo(after.leafFall)
    let prev = foliageSeason(60)
    for (let d = 60; d < 140; d += 0.5) {
      const s = foliageSeason(d)
      expect(Math.abs(s.leafFall - prev.leafFall)).toBeLessThan(0.05)
      expect(s.leafTurn).toBe(0)
      prev = s
    }
  })

  it('counts winter days from 12/30', () => {
    expect(winterDay(-24 * 60)).toBeCloseTo(0)
    expect(winterDay(44 * 24 * 60)).toBeCloseTo(45)
  })
})
