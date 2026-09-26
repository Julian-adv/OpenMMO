import { describe, expect, it } from 'vitest'
import { SnowCoverTracker, SNOW_SAMPLE_SECONDS } from './snowCover'

describe('SnowCoverTracker', () => {
  it('snaps to the first sample, then eases toward later ones', () => {
    const tracker = new SnowCoverTracker()
    expect(tracker.update(0.016, () => 0.8, null)).toBeCloseTo(0.8)
    tracker.update(SNOW_SAMPLE_SECONDS, () => 0.2, null)
    expect(tracker.value).toBeGreaterThan(0.2)
    expect(tracker.value).toBeLessThan(0.8)
    for (let i = 0; i < 100; i++) tracker.update(1, () => 0.2, null)
    expect(tracker.value).toBeCloseTo(0.2, 3)
  })

  it('keeps the last value while the model is unavailable', () => {
    const tracker = new SnowCoverTracker()
    tracker.update(0.016, () => 0.5, null)
    for (let i = 0; i < 20; i++) tracker.update(1, () => null, null)
    expect(tracker.value).toBeCloseTo(0.5)
  })

  it('builds cover under forced snow and melts it slowly afterwards', () => {
    const tracker = new SnowCoverTracker()
    tracker.update(0.016, () => 0, null)
    for (let i = 0; i < 700; i++) tracker.update(1, () => 0, 1)
    expect(tracker.value).toBeGreaterThan(0.95)
    for (let i = 0; i < 600; i++) tracker.update(1, () => 0, null)
    expect(tracker.value).toBeGreaterThan(0.85)
    expect(tracker.value).toBeLessThan(0.95)
  })
})
