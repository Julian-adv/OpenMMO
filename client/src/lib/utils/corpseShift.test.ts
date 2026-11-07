import { describe, expect, it } from 'vitest'
import { findCorpseShift } from './corpseShift'

// A wall along x = 1: any line from one side of it to the other is blocked.
const wallAtX1 = (x0: number, _z0: number, x1: number) => x0 < 1 !== x1 < 1

describe('findCorpseShift', () => {
  it('leaves a corpse that already fits where it fell', () => {
    const outline = [
      { x: -1, z: 0 },
      { x: 0.5, z: 0 },
    ]
    // 0.5 + the 0.3 m margin still stays short of the wall at 1.
    expect(findCorpseShift({ x: 0, z: 0 }, outline, wallAtX1)).toBeNull()
  })

  it('moves a corpse lying through the wall back inside it', () => {
    const outline = [
      { x: -0.2, z: 0 },
      { x: 1.6, z: 0 },
    ]
    const shift = findCorpseShift({ x: 0, z: 0 }, outline, wallAtX1)!
    // Clear of the wall by the 0.3 m margin too.
    expect(1.6 + shift.x).toBeLessThan(0.7)
    expect(Math.hypot(shift.x, shift.z)).toBeLessThanOrEqual(1 + 1e-6)
  })

  it('gives up rather than move it further than 2 m', () => {
    const outline = [{ x: 5, z: 0 }]
    expect(findCorpseShift({ x: 0, z: 0 }, outline, wallAtX1)).toBeNull()
  })
})
