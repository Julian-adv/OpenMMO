import { describe, expect, it, vi } from 'vitest'
import {
  advanceRainWetness,
  PUDDLE_DRY_SECONDS,
  PUDDLE_FILL_SECONDS,
  PUDDLE_SAMPLE_BUDGET,
  RainPuddleTracker,
} from './rainPuddles'

describe('rain puddles', () => {
  it('fills with sustained rain and dries completely after rain stops', () => {
    const halfway = advanceRainWetness(0, 1, PUDDLE_FILL_SECONDS / 2)
    expect(halfway).toBeCloseTo(0.5)
    const full = advanceRainWetness(halfway, 1, PUDDLE_FILL_SECONDS)
    expect(full).toBe(1)
    expect(advanceRainWetness(full, 0, PUDDLE_DRY_SECONDS / 2)).toBeCloseTo(0.5)
    expect(advanceRainWetness(full, 0, PUDDLE_DRY_SECONDS)).toBe(0)
  })

  it('accumulates light rain more slowly and refills remaining puddles', () => {
    expect(advanceRainWetness(0, 0.25, PUDDLE_FILL_SECONDS)).toBeCloseTo(0.25)
    const partlyDry = advanceRainWetness(1, 0, 120)
    expect(advanceRainWetness(partlyDry, 1, 30)).toBeGreaterThan(partlyDry)
  })

  it('is independent of frame rate and clamps long pauses and invalid ranges', () => {
    let wet = 0
    for (let i = 0; i < 1800; i++) wet = advanceRainWetness(wet, 0.6, 1 / 30)
    expect(wet).toBeCloseTo(advanceRainWetness(0, 0.6, 60))
    expect(advanceRainWetness(0, 2, 900)).toBe(1)
    expect(advanceRainWetness(1, -1, 900)).toBe(0)
    expect(advanceRainWetness(0.5, 1, -1)).toBe(0.5)
  })

  it('restores recent rain over multiple bounded updates', () => {
    const tracker = new RainPuddleTracker()
    const recentStorm = (_x: number, _z: number, ago: number) =>
      ago >= 120 ? 1 : 0
    const point = tracker.sample(0, 0, true)
    expect(point.wetness).toBe(0)
    for (let i = 0; i < 10; i++) {
      tracker.sample(0, 0, true)
      tracker.update(0, recentStorm)
    }
    expect(point.wetness).toBeCloseTo(0.5)
  })

  it('shares boundary samples and keeps wetness attached to its world location', () => {
    const tracker = new RainPuddleTracker()
    const rain = (x: number) => (x < 0 ? 1 : 0)
    const wet = tracker.sample(-32, 32, false)
    const dry = tracker.sample(32, 32, false)
    tracker.update(45, rain)
    expect(tracker.sample(-32, 32, false)).toBe(wet)
    expect(wet.wetness).toBeCloseTo(0.5)
    expect(dry.wetness).toBe(0)
  })

  it('does not invent a history for a new weather override', () => {
    const tracker = new RainPuddleTracker()
    const point = tracker.sample(0, 0, false)
    expect(point.wetness).toBe(0)
    tracker.update(30, () => 1)
    tracker.sample(0, 0, false)
    tracker.update(30, () => 0)
    expect(point.wetness).toBeCloseTo(30 / 90 - 30 / 240)
  })

  it('bounds weather queries when many newly visible corners need history', () => {
    const tracker = new RainPuddleTracker()
    const rain = vi.fn(() => 1)
    for (let frame = 0; frame < 120; frame++) {
      for (let i = 0; i < 16; i++) tracker.sample(i * 64, 0, true)
      rain.mockClear()
      tracker.update(1 / 60, rain)
      expect(rain.mock.calls.length).toBeLessThanOrEqual(PUDDLE_SAMPLE_BUDGET)
    }
    expect(tracker.sample(0, 0, true).wetness).toBe(1)
    expect(tracker.sample(15 * 64, 0, true).wetness).toBe(1)
  })

  it('cancels pending history when a weather override arrives', () => {
    const tracker = new RainPuddleTracker()
    const rain = vi.fn((_x: number, _z: number, _ago: number) => 1)
    const point = tracker.sample(0, 0, true)
    tracker.update(1, rain, false)
    expect(rain).toHaveBeenCalledExactlyOnceWith(0, 0, 0)
    expect(point.wetness).toBeCloseTo(1 / PUDDLE_FILL_SECONDS)
  })

  it('accumulates continuously between weather samples while standing still', () => {
    const tracker = new RainPuddleTracker()
    const rain = vi.fn(() => 1)
    const point = tracker.sample(0, 0, false)
    tracker.update(0, rain)
    for (let i = 0; i < 60; i++) {
      const before = point.wetness
      tracker.sample(0, 0, false)
      tracker.update(1 / 60, rain)
      expect(point.wetness - before).toBeCloseTo(1 / (60 * PUDDLE_FILL_SECONDS))
    }
    expect(rain.mock.calls.length).toBeLessThanOrEqual(2)
  })

  it('does not keep sampling previously visited, offscreen ground', () => {
    const tracker = new RainPuddleTracker()
    const rain = vi.fn(() => 1)
    for (let i = 0; i < 100; i++) tracker.sample(i * 64, 0, false)
    tracker.update(0, rain)
    rain.mockClear()
    tracker.update(120, rain)
    expect(rain).not.toHaveBeenCalled()
    tracker.sample(0, 0, false)
    tracker.update(1, rain)
    expect(rain).toHaveBeenCalledOnce()
  })

  it('resumes interrupted history when a tile briefly leaves the view', () => {
    const tracker = new RainPuddleTracker()
    const point = tracker.sample(0, 0, true)
    tracker.update(0, () => 1)
    tracker.update(0, () => 1)
    for (let frame = 0; frame < 10; frame++) {
      tracker.sample(0, 0, true)
      tracker.update(0, () => 1)
    }
    expect(point.wetness).toBe(1)
  })

  it('rebuilds stale samples from current weather history on return', () => {
    const tracker = new RainPuddleTracker()
    const point = tracker.sample(0, 0, false)
    tracker.update(PUDDLE_FILL_SECONDS, () => 1)
    expect(point.wetness).toBe(1)
    tracker.update(PUDDLE_DRY_SECONDS * 2 + 1, () => 0)
    const revisited = tracker.sample(0, 0, true)
    for (let frame = 0; frame < 10; frame++) {
      tracker.sample(0, 0, true)
      tracker.update(0, () => 0)
    }
    expect(revisited).not.toBe(point)
    expect(revisited.wetness).toBe(0)
  })

  it('restores current weather after puddles have been disabled', () => {
    const tracker = new RainPuddleTracker()
    const point = tracker.sample(0, 0, false)
    tracker.update(PUDDLE_FILL_SECONDS, () => 1, false)
    expect(point.wetness).toBe(1)
    tracker.sample(0, 0, false)
    tracker.pause(PUDDLE_DRY_SECONDS + 1)
    const dry = vi.fn(() => 0)
    tracker.update(0, dry)
    expect(dry).not.toHaveBeenCalled()
    for (let frame = 0; frame < 10; frame++) {
      tracker.sample(0, 0, true)
      tracker.update(0, dry)
    }
    expect(point.wetness).toBe(0)
  })

  it('preserves wetness across a brief pause without inventing override history', () => {
    const tracker = new RainPuddleTracker()
    const point = tracker.sample(0, 0, false)
    tracker.update(PUDDLE_FILL_SECONDS, () => 1, false)
    tracker.pause(3)
    expect(tracker.sample(0, 0, false)).toBe(point)
    expect(point.wetness).toBeCloseTo(1 - 3 / PUDDLE_DRY_SECONDS)
  })
})
