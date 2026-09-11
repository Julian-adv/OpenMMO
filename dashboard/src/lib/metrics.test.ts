import { describe, expect, it } from 'vitest'
import { connectionParts, formatAxisTime, formatBreakdown, formatDateTime, nearestSample, parseHistory, periods, splitSegments, summarize } from './metrics'

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

  it('shows Korean dates and years on longer ranges', () => {
    const timestamp = Date.UTC(2026, 0, 1, 15) / 1000
    expect(formatAxisTime(timestamp, 24)).toBe('00:00')
    expect(formatAxisTime(timestamp, 168)).toBe('1. 2.')
    expect(formatAxisTime(timestamp, 4320)).toBe('1. 2.')
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
    expect(formatBreakdown(sample)).toBe('웹 접속 6계정 (60%), 외부 에이전트 3계정 (30%), 기타·미분류 1계정 (10%)')
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
