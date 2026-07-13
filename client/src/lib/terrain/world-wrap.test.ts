import { describe, expect, it } from 'vitest'
import {
  WORLD_MAX_X,
  WORLD_MIN_X,
  shortestWrappedDeltaX,
  unwrapWorldXNear,
  wrapWorldX,
} from './world-wrap'

describe('wrapWorldX', () => {
  it('wraps across the baked east and west terrain edges', () => {
    expect(wrapWorldX(WORLD_MIN_X)).toBe(WORLD_MIN_X)
    expect(wrapWorldX(WORLD_MAX_X)).toBe(WORLD_MIN_X)
    expect(wrapWorldX(WORLD_MAX_X + 0.25)).toBe(WORLD_MIN_X + 0.25)
    expect(wrapWorldX(WORLD_MIN_X - 0.25)).toBe(WORLD_MAX_X - 0.25)
  })
})

describe('shortestWrappedDeltaX', () => {
  it('uses the short path across both world edges', () => {
    expect(shortestWrappedDeltaX(WORLD_MAX_X - 1, WORLD_MIN_X + 1)).toBe(2)
    expect(shortestWrappedDeltaX(WORLD_MIN_X + 1, WORLD_MAX_X - 1)).toBe(-2)
  })

  it('unwraps a canonical coordinate next to a local reference', () => {
    expect(unwrapWorldXNear(WORLD_MAX_X - 1, WORLD_MIN_X + 1)).toBe(
      WORLD_MAX_X + 1
    )
  })
})
