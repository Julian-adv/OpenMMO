import { describe, expect, it } from 'vitest'
import { attackLog, daggerSkippedLog } from './combatLog'

describe('combat logs', () => {
  it('shows the individual roll for each strike, including a real miss', () => {
    expect(attackLog(17, true, 23, 1)).toBe(
      'Double Slash 1/2 — rolled 17: HIT for 23 damage!'
    )
    expect(attackLog(3, false, 0, 2)).toBe(
      'Double Slash 2/2 — rolled 3: MISSED!'
    )
  })

  it('does not invent a roll or an accuracy miss for a defeated target', () => {
    expect(daggerSkippedLog(2, 'target_defeated')).toBe(
      'Double Slash 2/2: SKIPPED — target defeated.'
    )
  })

  it('distinguishes skipped out-of-range and interrupted strikes', () => {
    expect(daggerSkippedLog(1, 'out_of_range')).toContain(
      'SKIPPED — target out of reach'
    )
    expect(daggerSkippedLog(2, 'weapon_changed')).toContain(
      'SKIPPED — weapon changed'
    )
    expect(daggerSkippedLog(2, 'interrupted')).toContain(
      'SKIPPED — attack interrupted'
    )
  })

  it('preserves normal attack logs', () => {
    expect(attackLog(16, true, 12, null)).toBe('rolled 16: HIT for 12 damage!')
    expect(attackLog(2, false, 0)).toBe('rolled 2: MISSED!')
  })
})
