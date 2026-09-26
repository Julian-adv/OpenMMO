import { describe, expect, it } from 'vitest'
import { angleDelta } from './horseMovement'
import { MovementRotation } from './movementRotation'

describe('movement rotation display', () => {
  it('starts at the supplied facing and completes each turn within 120ms', () => {
    const rotation = new MovementRotation()
    expect(rotation.update(1, 0, true)).toBe(1)
    expect(rotation.update(2, 0, true)).toBe(1)
    let previous = 1
    for (let frame = 1; frame <= 12; frame++) {
      const next = rotation.update(2, 0.01, true)
      expect(next).toBeGreaterThan(previous)
      expect(next).toBeLessThanOrEqual(2)
      previous = next
    }
    expect(previous).toBeCloseTo(2)
    expect(rotation.update(2, 0.01, true)).toBe(2)
  })

  it.each([-1, 1])(
    'takes the short turn across the angle seam (%i)',
    (sign) => {
      const rotation = new MovementRotation()
      const from = sign * (Math.PI - 0.1)
      const to = sign * (-Math.PI + 0.1)
      rotation.update(from, 0, true)
      const halfway = rotation.update(to, 0.06, true)
      expect(angleDelta(from, halfway) * sign).toBeGreaterThan(0)
      expect(Math.abs(angleDelta(from, halfway))).toBeLessThan(0.2)
      expect(rotation.update(to, 0.06, true)).toBe(to)
    }
  )

  it('retargets from the displayed angle when a drag reverses mid-turn', () => {
    const rotation = new MovementRotation()
    rotation.update(0, 0, true)
    const before = rotation.update(1, 0.04, true)
    expect(rotation.update(-1, 0, true)).toBeCloseTo(before)
    const after = rotation.update(-1, 0.04, true)
    expect(after).toBeLessThan(before)
    expect(after).toBeGreaterThan(-1)
    expect(rotation.update(-1, 0.08, true)).toBe(-1)
  })

  it('produces the same facing across frame rates and settles after a stall', () => {
    const rotations = [30, 60, 120].map((fps) => {
      const rotation = new MovementRotation()
      rotation.update(0, 0, true)
      let displayed = 0
      for (let frame = 0; frame < fps / 10; frame++)
        displayed = rotation.update(1, 1 / fps, true)
      expect(rotation.update(1, 1, true)).toBe(1)
      return displayed
    })
    expect(rotations[0]).toBeCloseTo(rotations[1], 10)
    expect(rotations[1]).toBeCloseTo(rotations[2], 10)
  })

  it('resets immediately when smoothing is disabled and resumes from that facing', () => {
    const rotation = new MovementRotation()
    rotation.update(0, 0, true)
    rotation.update(1, 0.04, true)
    expect(rotation.update(-2, 0, false)).toBe(-2)
    expect(rotation.update(-1, 0, true)).toBe(-2)
    expect(rotation.update(-1, 0.12, true)).toBe(-1)
  })

  it('does not restart a turn for equivalent angles or coordinate rounding', () => {
    const rotation = new MovementRotation()
    rotation.update(0, 0, true)
    rotation.update(1, 0.06, true)
    expect(rotation.update(1 + Math.PI * 2 + 1e-8, 0.06, true)).toBe(1)
  })
})
