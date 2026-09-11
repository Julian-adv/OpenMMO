import { describe, expect, it } from 'vitest'
import { nearestSample, parseHistory, splitSegments, summarize } from './metrics'

describe('concurrent account history', () => {
  const samples = [
    { timestamp: 60, accounts: 0 },
    { timestamp: 120, accounts: 4 },
    { timestamp: 300, accounts: 2 },
  ]

  it('keeps a measured zero distinct from missing data', () => {
    expect(summarize([])).toEqual({ peak: null, average: null, peakAt: null })
    expect(summarize([samples[0]])).toEqual({ peak: 0, average: 0, peakAt: 60 })
    expect(summarize(samples)).toEqual({ peak: 4, average: 2, peakAt: 120 })
    expect(splitSegments(samples, 60)).toEqual([samples.slice(0, 2), [samples[2]]])
  })

  it('finds observations by time instead of stretching missing minutes', () => {
    expect(nearestSample([], 90)).toBeNull()
    expect(nearestSample(samples, 0)).toBe(0)
    expect(nearestSample(samples, 130)).toBe(1)
    expect(nearestSample(samples, 270)).toBe(2)
    expect(nearestSample(samples, 900)).toBe(2)
  })

  it('rejects malformed, duplicate and out-of-window data', () => {
    const data = { from: 0, until: 3600, sample_interval_seconds: 60,
      current: { timestamp: 3600, accounts: 2 }, samples }
    expect(parseHistory(data, 1)).toEqual(data)
    expect(() => parseHistory(data, 24)).toThrow()
    expect(() => parseHistory({ ...data, samples: [...samples, samples[2]] }, 1)).toThrow()
    expect(() => parseHistory({ ...data, samples: [{ timestamp: 3660, accounts: 2 }] }, 1)).toThrow()
    expect(() => parseHistory({ ...data, current: { timestamp: 3600, accounts: -1 } }, 1)).toThrow()
  })
})
