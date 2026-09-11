import { describe, expect, it } from 'vitest'
import { createLevelColors, levelAt, levelPath } from './levelHistory'
import { levelPeriods, parseLevelLeaderboard } from './metrics'

describe('character level history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'Hero', level: 10, account_first_rank: 1 }]
  const series = [{ name: 'Hero', started_at: timestamp - 400 * 86400, samples: [{ timestamp, level: 10 }] }]
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(levelPeriods)('accepts the $label range with a carried baseline and newly tracked characters', ({ hours, interval }) => {
    const from = timestamp - hours * 3600
    const history = { ...data, from, sample_interval_seconds: interval,
      series: [{ ...series[0], samples: [{ timestamp: from, level: 5 }, { timestamp, level: 10 }] }] }
    expect(parseLevelLeaderboard(history, hours)).toEqual(history)
    const newCharacter = { ...history, series: [{ name: 'Hero', started_at: timestamp, samples: [{ timestamp, level: 10 }] }] }
    expect(parseLevelLeaderboard(newCharacter, hours)).toEqual(newCharacter)
    expect(parseLevelLeaderboard({ ...history, entries: [], series: [] }, hours).entries).toEqual([])
  })

  it('rejects mismatched characters, invalid levels, duplicate times and invented baselines', () => {
    const validSeries = { ...series[0], started_at: timestamp }
    const valid = { ...data, series: [validSeries] }
    for (const invalid of [{ name: 'SomeoneElse' }, { started_at: timestamp + 1 }, { samples: [] },
      { samples: [{ timestamp, level: 0 }] }, { samples: [{ timestamp, level: 1.5 }] },
      { samples: [{ timestamp: timestamp + 1, level: 10 }] }, { samples: [series[0].samples[0], series[0].samples[0]] },
      { samples: [{ timestamp: data.from, level: 1 }, ...series[0].samples] }]) {
      expect(() => parseLevelLeaderboard({ ...valid, series: [{ ...validSeries, ...invalid }] }, 168)).toThrow()
    }
    expect(() => parseLevelLeaderboard({ ...valid, series: [] }, 168)).toThrow()
    expect(() => parseLevelLeaderboard(valid, 720)).toThrow()
    expect(() => parseLevelLeaderboard({ ...valid, sample_interval_seconds: 60 }, 168)).toThrow()
  })

  it('uses the last known level without anticipating a level-up or extending history backwards', () => {
    const samples = [{ timestamp: 100, level: 5 }, { timestamp: 200, level: 6 }, { timestamp: 300, level: 4 }]
    expect(levelAt(samples, 99)).toBeNull()
    expect(levelAt(samples, 199)?.level).toBe(5)
    expect(levelAt(samples, 200)?.level).toBe(6)
    expect(levelAt(samples, 400)?.level).toBe(4)
    expect(levelAt([], 400)).toBeNull()
    expect(levelPath(samples, 400, (time) => time, (level) => level)).toBe('M100,5 H200V6 H300V4 H400')
  })

  it('keeps ten distinct colors through ranking changes and replaces only departed characters', () => {
    const assign = createLevelColors()
    const names = Array.from({ length: 10 }, (_, index) => `Hero${index}`)
    const colors = assign(names)
    expect(new Set(Object.values(colors)).size).toBe(10)
    expect(assign([...names].reverse())).toEqual(colors)
    const replacement = assign(['NewHero', ...names.slice(1)])
    for (const name of names.slice(1)) expect(replacement[name]).toBe(colors[name])
    expect(new Set(Object.values(replacement)).size).toBe(10)
    expect(assign(['__proto__', 'constructor', 'toString'])).toHaveProperty('__proto__')
  })
})
