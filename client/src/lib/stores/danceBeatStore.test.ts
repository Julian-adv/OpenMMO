import { afterEach, describe, expect, it } from 'vitest'
import {
  clearDanceBeat,
  danceAnchors,
  danceBeatTime,
  setDanceBeat,
} from './danceBeatStore'

afterEach(() => danceAnchors.clear())

describe('dance beat', () => {
  it('anchors the beat elapsedMs before now', () => {
    setDanceBeat(7, 1_500, 10_000)
    expect(danceAnchors.get(7)).toBe(8_500)
    clearDanceBeat(7)
    expect(danceAnchors.has(7)).toBe(false)
  })

  it('puts two viewers of one beat on the same frame of the loop', () => {
    // One viewer heard the beat at 0 ms in, the other joined 3.2 s later.
    setDanceBeat(1, 0, 1_000)
    setDanceBeat(2, 3_200, 4_200)
    const at = 9_000
    expect(danceBeatTime(danceAnchors.get(1)!, 2.5, at)).toBeCloseTo(
      danceBeatTime(danceAnchors.get(2)!, 2.5, at)
    )
  })

  it('wraps the elapsed time into the clip', () => {
    expect(danceBeatTime(0, 2, 5_000)).toBeCloseTo(1)
    expect(danceBeatTime(1_000, 2, 1_000)).toBe(0)
  })

  it('stays inside the clip when the anchor is ahead of the clock', () => {
    const t = danceBeatTime(2_000, 2, 1_500)
    expect(t).toBeGreaterThanOrEqual(0)
    expect(t).toBeCloseTo(1.5)
  })

  it('treats a clip with no length as frame zero', () => {
    expect(danceBeatTime(0, 0, 5_000)).toBe(0)
  })
})
