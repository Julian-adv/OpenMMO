import { describe, expect, it } from 'vitest'
import { angleDelta, horseTurnDuration, steerHorse } from './horseMovement'

describe('mounted steering', () => {
  it('matches server timing and mirrors left and right turns', () => {
    for (const sign of [-1, 1]) {
      const start = steerHorse(0, (sign * Math.PI) / 2, 0.2)
      expect(start.rotation).toBeCloseTo((sign * Math.PI) / 6)
      expect(start.travelTime).toBeCloseTo(0)
      const finish = steerHorse(0, (sign * Math.PI) / 2, 0.6)
      expect(finish.rotation).toBeCloseTo((sign * Math.PI) / 2)
      expect(finish.travelTime).toBeCloseTo(0.1)
    }
    expect(horseTurnDuration(Math.PI)).toBeCloseTo(1)
    expect(angleDelta(Math.PI - 0.1, -Math.PI + 0.1)).toBeCloseTo(0.2)
  })

  it('preserves travel and rotation across different frame rates', () => {
    for (const target of [0.1, Math.PI / 2, Math.PI, -2]) {
      const expected = steerHorse(0, target, 1.2)
      let rotation = 0
      let travel = 0
      for (let i = 0; i < 120; i++) {
        const step = steerHorse(rotation, target, 0.01)
        rotation = step.rotation
        travel += step.travelTime
      }
      expect(angleDelta(rotation, expected.rotation)).toBeCloseTo(0)
      expect(travel).toBeCloseTo(expected.travelTime)
    }
  })
})
