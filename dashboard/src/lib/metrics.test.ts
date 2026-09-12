import { describe, expect, it } from 'vitest'
import { connectionParts, formatAxisTime, formatDateTime, formatGold, goldSegments, goldPeriods, kstDayStart, nearestSample, parseGoldHistory, parsePerAccountGoldHistory, parseHistory, parsePriceIndexHistory, parseUniqueHistory, periods, uniquePeriods, splitSegments, summarize } from './metrics'

describe('gold display units', () => {
  it.each([
    [0, '0c'], [1, '1c'], [100, '1s'], [9999, '99s 99c'],
    [10000, '1g'], [86134, '8g 61s 34c'], [123456789, '12,345g 67s 89c'],
  ])('displays %i copper as %s gold', (copper, expected) => {
    expect(formatGold(copper)).toBe(expected)
  })

  it('preserves denomination colors and rounds averages before splitting units', () => {
    expect(goldSegments(86134)).toEqual([
      { unit: 'gold', text: '8g' }, { unit: 'silver', text: '61s' }, { unit: 'copper', text: '34c' },
    ])
    expect(formatGold(10000 / 3)).toBe('33s 33c')
    expect(formatGold(99.9)).toBe('1s')
    expect(formatGold(9999.9)).toBe('1g')
  })
})

describe('gold per active account history', () => {
  const midnight = 1000 * 86400 - 9 * 3600
  const latest = { timestamp: midnight, total_gold: 100, accounts: 3, gold_per_account: 100 / 3 }
  const sample = { timestamp: midnight, gold_per_account: 100 / 3, peak_gold_per_account: 100 / 3, sample_count: 1 }
  const data = { from: midnight - 86400, until: midnight, sample_interval_seconds: 3600,
    window_seconds: 86400, collection_started_at: 0, latest, samples: [sample] }

  it.each(goldPeriods)('accepts each active window independently of the $label chart range', ({ hours, interval }) => {
    for (const activePeriod of uniquePeriods) {
      const history = { ...data, from: midnight - hours * 3600, sample_interval_seconds: interval,
        window_seconds: activePeriod.hours * 3600,
        samples: [{ ...sample, timestamp: Math.floor(midnight / interval) * interval }] }
      expect(parsePerAccountGoldHistory(history, hours, activePeriod.hours)).toEqual(history)
    }
  })

  it('keeps valid zero gold, missing denominators, gaps and stale latest observations distinct', () => {
    const zero = { ...data, latest: { ...latest, total_gold: 0, gold_per_account: 0 },
      samples: [{ ...sample, gold_per_account: 0, peak_gold_per_account: 0 }] }
    expect(parsePerAccountGoldHistory(zero, 24, 24).latest?.gold_per_account).toBe(0)
    expect(parsePerAccountGoldHistory({ ...data, latest: null }, 24, 24).samples).toEqual([sample])
    expect(parsePerAccountGoldHistory({ ...data, latest: null, samples: [] }, 24, 24).latest).toBeNull()
    const stale = { ...data, from: midnight + 3600, until: midnight + 3600 + 86400, samples: [] }
    expect(parsePerAccountGoldHistory(stale, 24, 24).latest).toEqual(latest)
    const samples = [{ ...sample, timestamp: midnight - 7200 }, sample]
    expect(splitSegments(parsePerAccountGoldHistory({ ...data, samples }, 24, 24).samples, 3600)).toHaveLength(2)
  })

  it('rejects zero denominators, mismatched windows and malformed ratios', () => {
    for (const invalid of [{ accounts: 0 }, { accounts: -1 }, { accounts: 1.5 }, { gold_per_account: Infinity }, { gold_per_account: 99 }, { timestamp: midnight + 3600 }]) {
      expect(() => parsePerAccountGoldHistory({ ...data, latest: { ...latest, ...invalid } }, 24, 24)).toThrow()
    }
    for (const invalid of [{ gold_per_account: NaN }, { gold_per_account: -1 }, { peak_gold_per_account: 0 }, { sample_count: 0 }, { sample_count: 2 }]) {
      expect(() => parsePerAccountGoldHistory({ ...data, samples: [{ ...sample, ...invalid }] }, 24, 24)).toThrow()
    }
    expect(() => parsePerAccountGoldHistory(data, 24, 168)).toThrow()
    expect(() => parsePerAccountGoldHistory({ ...data, samples: [] }, 24, 24)).toThrow()
    expect(() => parsePerAccountGoldHistory({ ...data, samples: [sample, sample] }, 24, 24)).toThrow()
    expect(() => parsePerAccountGoldHistory({ ...data, collection_started_at: midnight }, 24, 24)).toThrow()
    expect(kstDayStart(midnight - 1)).toBe(midnight - 86400)
    expect(kstDayStart(midnight)).toBe(midnight)
    expect(kstDayStart(midnight + 86399)).toBe(midnight)
  })

  it('allows floating point rounding in bucket averages', () => {
    const history = { ...data, from: midnight - 4320 * 3600, sample_interval_seconds: 21600,
      samples: [{ timestamp: Math.floor(midnight / 21600) * 21600,
        gold_per_account: 0.10000000000000002, peak_gold_per_account: 0.1, sample_count: 6 }] }
    expect(parsePerAccountGoldHistory(history, 4320, 24)).toEqual(history)
  })
})

describe('hourly gold history', () => {
  it.each(goldPeriods)('accepts hourly gold with the $label resolution', ({ hours, interval }) => {
    const until = hours * 3600
    const data = { from: 0, until, sample_interval_seconds: interval,
      latest: { timestamp: until, total_gold: 123456789 },
      samples: [{ timestamp: until, total_gold: 123456789, peak_gold: 123456789, sample_count: 1 }] }
    expect(parseGoldHistory(data, hours)).toEqual(data)
    expect(() => parseGoldHistory({ ...data, sample_interval_seconds: 60 }, hours)).toThrow()
  })

  it('preserves missing hours, saved zeroes and the latest total outside the selected range', () => {
    const data = { from: 0, until: 86400, sample_interval_seconds: 3600, latest: { timestamp: 10800, total_gold: 25 },
      samples: [{ timestamp: 3600, total_gold: 0, peak_gold: 0, sample_count: 1 },
        { timestamp: 10800, total_gold: 25, peak_gold: 25, sample_count: 1 }] }
    expect(parseGoldHistory(data, 24)).toEqual(data)
    expect(splitSegments(data.samples, 3600)).toEqual(data.samples.map(sample => [sample]))
    expect(parseGoldHistory({ ...data, from: 86400, until: 172800, samples: [] }, 24).latest).toEqual(data.latest)
    expect(parseGoldHistory({ ...data, latest: null, samples: [] }, 24).latest).toBeNull()
    expect(() => parseGoldHistory({ ...data, latest: null }, 24)).toThrow()
    expect(() => parseGoldHistory({ ...data, samples: [] }, 24)).toThrow()
  })

  it('accepts partial buckets and fractional averages and rejects invalid observations', () => {
    const sample = { timestamp: 300, total_gold: 50.5, peak_gold: 101, sample_count: 2 }
    const data = { from: 300, until: 4320 * 3600 + 300, sample_interval_seconds: 21600,
      latest: { timestamp: 7200, total_gold: 101 }, samples: [sample] }
    expect(parseGoldHistory(data, 4320)).toEqual(data)
    for (const invalid of [{ total_gold: -1 }, { total_gold: NaN }, { peak_gold: 50 }, { sample_count: 0 }, { sample_count: 7 }, { timestamp: 301 }]) {
      expect(() => parseGoldHistory({ ...data, samples: [{ ...sample, ...invalid }] }, 4320)).toThrow()
    }
    expect(() => parseGoldHistory({ ...data, samples: [sample, sample] }, 4320)).toThrow()
    expect(() => parseGoldHistory({ ...data, latest: { timestamp: 7201, total_gold: 101 } }, 4320)).toThrow()
  })
})

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
    const data = { from: 0, until: 168 * 3600, sample_interval_seconds: 3600,
      current: { timestamp: 168 * 3600, ...webCount(2) }, samples: [sample] }
    expect(parseHistory(data, 168)).toEqual(data)
    for (const invalid of [{ accounts: NaN }, { peak_accounts: 1 }, { peak_timestamp: 3600 }, { sample_count: 0 }, { sample_count: 61 }]) {
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
    const data = { from: 0, until: 3600, sample_interval_seconds: 3600,
      current: { timestamp: 3600, ...webCount(2) }, samples: samples.slice(0, 2) }
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
    const data = { from: 0, until: 3600, sample_interval_seconds: 3600, current, samples: samples.slice(0, 2) }
    expect(parseHistory(data, 1)).toEqual(data)
    for (const invalid of [{ web_accounts: -1 }, { agent_accounts: NaN }, { other_accounts: undefined }, { web_accounts: 2 }, { web_accounts: 0.5, agent_accounts: 2.5 }]) {
      expect(() => parseHistory({ ...data, current: { ...current, ...invalid } }, 1)).toThrow()
    }
    expect(() => parseHistory({ ...data, samples: [{ ...samples[1], agent_accounts: 1 }] }, 1)).toThrow()
  })

  it('accepts fractional aggregate components without changing the total or weighting', () => {
    const sample = { timestamp: 0, accounts: 6, web_accounts: 2 / 3, agent_accounts: 4 / 3, other_accounts: 4,
      peak_accounts: 8, peak_timestamp: 60, sample_count: 3 }
    const data = { from: 0, until: 168 * 3600, sample_interval_seconds: 3600,
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

describe('price index history', () => {
  const meeting = (timestamp: number, index_before: number, index_after: number) =>
    ({ timestamp, game_day: 1600, m_prev: 200, m_now: 150, growth: -0.25, index_before, index_after })
  const data = { from: 0, until: 168 * 3600, current_index_percent: 75, baseline_index_percent: 90,
    meetings: [meeting(3600, 90, 83), meeting(400000, 83, 75)] }

  it('accepts a chained meeting list starting from the baseline', () => {
    expect(parsePriceIndexHistory(data, 168)).toEqual(data)
    expect(parsePriceIndexHistory({ ...data, meetings: [] }, 168).meetings).toEqual([])
  })

  it('rejects windows, ordering and chains that do not match', () => {
    expect(() => parsePriceIndexHistory(data, 720)).toThrow()
    expect(() => parsePriceIndexHistory({ ...data, meetings: [meeting(0, 90, 83)] }, 168)).toThrow()
    expect(() => parsePriceIndexHistory({ ...data, meetings: [meeting(400000, 90, 83), meeting(3600, 83, 75)] }, 168)).toThrow()
    expect(() => parsePriceIndexHistory({ ...data, meetings: [meeting(3600, 100, 83)] }, 168)).toThrow()
    expect(() => parsePriceIndexHistory({ ...data, meetings: [meeting(3600, 90, 83), meeting(400000, 90, 75)] }, 168)).toThrow()
    expect(() => parsePriceIndexHistory({ ...data, current_index_percent: 0 }, 168)).toThrow()
  })
})
