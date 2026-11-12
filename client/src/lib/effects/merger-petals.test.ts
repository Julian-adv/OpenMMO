import { describe, expect, it } from 'vitest'
import { MERGER_PETALS_DURATION, petalHeight } from './merger-petals'

describe('petalHeight', () => {
  it('releases every petal and lands it before the effect ends', () => {
    for (let i = 0; i < 800; i++) {
      expect(petalHeight(i, 0)).toBeLessThanOrEqual(7)
      const airborne = [1, 2, 3, 4, 5, 6, 7, 8, 9].some(
        (t) => petalHeight(i, t) > 0
      )
      expect(airborne).toBe(true)
      expect(petalHeight(i, MERGER_PETALS_DURATION)).toBe(-1)
    }
  })
})
