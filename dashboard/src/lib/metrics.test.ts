import { describe, expect, it } from 'vitest'
import { connectionParts, formatAxisTime, formatDateTime, nearestSample, parseHistory, parseUniqueHistory, periods, uniquePeriods, splitSegments, summarize } from './metrics'

describe('concurrent account history', () => {
  const webCount = (accounts: number) => ({ accounts, web_accounts: accounts, agent_accounts: 0, other_accounts: 0 })
  const samples = [
    { timestamp: 60, accounts: 0 },
    { timestamp: 120, accounts: 4 },
    { timestamp: 300, accounts: 2 },
  ].map((sample) => ({ ...sample, ...webCount(sample.accounts), peak_accounts: sample.accounts, peak_timestamp: sample.timestamp, sample_count: 1 }))

  it('keeps a measured zero distinct from missing data', () => {
    expect(summarize([])).toEqual({ peak: null, average: null, peakAt: null, sampleCount: 0 })
    expect(summarize([samples[0]])).toEqual({ peak: 0, average: 0, peakAt: 60, sampleCount: 1 })
    expect(summarize(samples)).toEqual({ peak: 4, average: 2, peakAt: 120, sampleCount: 3 })
    expect(splitSegments(samples, 60)).toEqual([samples.slice(0, 2), [samples[2]]])
  })

  it('weights aggregate averages by observed minutes and preserves the original peak', () => {
    const aggregated = [
      { timestamp: 599, accounts: 4, peak_accounts: 6, peak_timestamp: 599, sample_count: 3 },
      { timestamp: 1200, accounts: 3, peak_accounts: 3, peak_timestamp: 1200, sample_count: 1 },
    ].map((sample) => ({ ...sample, ...webCount(sample.accounts) }))
    expect(summarize(aggregated)).toEqual({ peak: 6, average: 3.75, peakAt: 599, sampleCount: 4 })
    expect(splitSegments(aggregated, 600)).toEqual([[aggregated[0]], [aggregated[1]]])
  })

  it.each(periods)('accepts the $label range with its matching resolution', ({ hours, interval }) => {
    const data = { from: 0, until: hours * 3600, sample_interval_seconds: interval,
      current: { timestamp: hours * 3600, ...webCount(2) }, samples: [samples[0]] }
    expect(parseHistory(data, hours)).toEqual(data)
    expect(() => parseHistory({ ...data, sample_interval_seconds: interval * 2 }, hours)).toThrow()
  })

  it('validates fractional averages and rejects invalid aggregate statistics', () => {
    const sample = { timestamp: 0, ...webCount(1.5), peak_accounts: 3, peak_timestamp: 60, sample_count: 2 }
    const data = { from: 0, until: 168 * 3600, sample_interval_seconds: 600,
      current: { timestamp: 168 * 3600, ...webCount(2) }, samples: [sample] }
    expect(parseHistory(data, 168)).toEqual(data)
    for (const invalid of [{ accounts: NaN }, { peak_accounts: 1 }, { peak_timestamp: 600 }, { sample_count: 0 }, { sample_count: 11 }]) {
      expect(() => parseHistory({ ...data, samples: [{ ...sample, ...invalid }] }, 168)).toThrow()
    }
  })

  it('shows Korean weekdays with dates and years on longer ranges', () => {
    const timestamp = Date.UTC(2026, 0, 1, 15) / 1000
    expect(formatAxisTime(timestamp, 24)).toBe('00:00')
    expect(formatAxisTime(timestamp, 168)).toBe('1. 2. (금)')
    expect(formatAxisTime(timestamp, 4320)).toBe('1. 2. (금)')
    expect(formatAxisTime(timestamp, 8760)).toBe('26. 1.')
    expect(formatDateTime(timestamp)).toContain('2026년')
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
      current: { timestamp: 3600, ...webCount(2) }, samples }
    expect(parseHistory(data, 1)).toEqual(data)
    expect(() => parseHistory(data, 24)).toThrow()
    expect(() => parseHistory({ ...data, samples: [...samples, samples[2]] }, 1)).toThrow()
    expect(() => parseHistory({ ...data, samples: [{ timestamp: 3660, accounts: 2 }] }, 1)).toThrow()
    expect(() => parseHistory({ ...data, current: { timestamp: 3600, accounts: -1 } }, 1)).toThrow()
  })

  it('shows component shares of the unchanged total, including unclassified history and zero', () => {
    const sample = { timestamp: 60, accounts: 10, web_accounts: 6, agent_accounts: 3, other_accounts: 1 }
    expect(connectionParts(sample).map((part) => part.percent)).toEqual([60, 30, 10])
    expect(connectionParts({ timestamp: 60, ...webCount(0) }).map((part) => part.percent)).toEqual([0, 0, 0])
    expect(connectionParts({ ...sample, web_accounts: 0, agent_accounts: 0, other_accounts: 10 }).map((part) => part.percent)).toEqual([0, 0, 100])
  })

  it('rejects component counts that disagree with the total or have invalid values', () => {
    const current = { timestamp: 3600, accounts: 3, web_accounts: 1, agent_accounts: 2, other_accounts: 0 }
    const data = { from: 0, until: 3600, sample_interval_seconds: 60, current, samples }
    expect(parseHistory(data, 1)).toEqual(data)
    for (const invalid of [{ web_accounts: -1 }, { agent_accounts: NaN }, { other_accounts: undefined }, { web_accounts: 2 }, { web_accounts: 0.5, agent_accounts: 2.5 }]) {
      expect(() => parseHistory({ ...data, current: { ...current, ...invalid } }, 1)).toThrow()
    }
    expect(() => parseHistory({ ...data, samples: [{ ...samples[1], agent_accounts: 1 }] }, 1)).toThrow()
  })

  it('accepts fractional aggregate components without changing the total or weighting', () => {
    const sample = { timestamp: 0, accounts: 6, web_accounts: 2 / 3, agent_accounts: 4 / 3, other_accounts: 4,
      peak_accounts: 8, peak_timestamp: 60, sample_count: 3 }
    const data = { from: 0, until: 168 * 3600, sample_interval_seconds: 600,
      current: { timestamp: 168 * 3600, ...webCount(0) }, samples: [sample] }
    expect(parseHistory(data, 168)).toEqual(data)
    expect(summarize(data.samples)).toEqual({ peak: 8, peakAt: 60, average: 6, sampleCount: 3 })
    expect(connectionParts(sample).reduce((total, part) => total + part.percent, 0)).toBeCloseTo(100)
  })
})

describe('daily unique account history', () => {
  const day = 86400
  const midnight = 1000 * day - 9 * 3600

  it.each(uniquePeriods)('accepts cached daily counts for the $label window', ({ hours }) => {
    const data = { from: midnight - hours * 3600, until: midnight, window_seconds: hours * 3600,
      sample_interval_seconds: day, collection_started_at: 0, last_aggregated_at: midnight,
      samples: [{ timestamp: midnight - day, accounts: 3 }, { timestamp: midnight, accounts: 4 }] }
    expect(parseUniqueHistory(data, hours)).toEqual(data)
    expect(() => parseUniqueHistory({ ...data, window_seconds: 1 }, hours)).toThrow()
    expect(() => parseUniqueHistory({ ...data, sample_interval_seconds: 60 }, hours)).toThrow()
  })

  it('keeps pending aggregation distinct from a saved zero count', () => {
    const data = { from: midnight - day, until: midnight, window_seconds: day, sample_interval_seconds: day,
      collection_started_at: midnight - 3600, last_aggregated_at: null, samples: [] }
    expect(parseUniqueHistory(data, 24)).toEqual(data)
    const collected = { ...data, last_aggregated_at: midnight, samples: [{ timestamp: midnight, accounts: 0 }] }
    expect(parseUniqueHistory(collected, 24)).toEqual(collected)
    expect(() => parseUniqueHistory({ ...collected, samples: [] }, 24)).toThrow()
    expect(() => parseUniqueHistory({ ...collected, last_aggregated_at: null }, 24)).toThrow()
  })

  it('accepts missing days and an old aggregation outside the selected range', () => {
    const data = { from: midnight - 7 * day, until: midnight, window_seconds: 7 * day, sample_interval_seconds: day,
      collection_started_at: 0, last_aggregated_at: midnight - day,
      samples: [{ timestamp: midnight - 5 * day, accounts: 2 }, { timestamp: midnight - day, accounts: 4 }] }
    expect(parseUniqueHistory(data, 168)).toEqual(data)
    expect(splitSegments(data.samples, day)).toEqual(data.samples.map(sample => [sample]))
    const stale = { ...data, from: midnight - day, window_seconds: day, last_aggregated_at: midnight - 2 * day, samples: [] }
    expect(parseUniqueHistory(stale, 24)).toEqual(stale)
  })

  it('rejects non-daily, duplicate, unordered and invalid daily observations', () => {
    const data = { from: midnight - day, until: midnight, window_seconds: day, sample_interval_seconds: day,
      collection_started_at: 0, last_aggregated_at: midnight,
      samples: [{ timestamp: midnight - day, accounts: 2 }, { timestamp: midnight, accounts: 3 }] }
    for (const accounts of [-1, 0.5, NaN, Infinity]) {
      expect(() => parseUniqueHistory({ ...data, samples: [data.samples[0], { timestamp: midnight, accounts }] }, 24)).toThrow()
    }
    expect(() => parseUniqueHistory({ ...data, samples: [data.samples[0], ...data.samples] }, 24)).toThrow()
    expect(() => parseUniqueHistory({ ...data, samples: [...data.samples].reverse() }, 24)).toThrow()
    expect(() => parseUniqueHistory({ ...data, samples: [{ timestamp: midnight - 3600, accounts: 2 }, data.samples[1]] }, 24)).toThrow()
    expect(() => parseUniqueHistory({ ...data, last_aggregated_at: midnight + day }, 24)).toThrow()
    expect(() => parseUniqueHistory({ ...data, collection_started_at: midnight }, 24)).toThrow()
    expect(formatAxisTime(midnight, 24, true)).toMatch(/\([월화수목금토일]\)/)
  })
})
