import { describe, expect, it } from 'vitest'
import { createCharacterColors, sampleAt, stepChangesAt, stepPath } from './leaderboardHistory'
import { leaderboardPeriods, parseArmorEnchantLeaderboard, parseGoldLeaderboard, parseLandLeaderboard, parseLevelLeaderboard, parseWeaponEnchantLeaderboard } from './metrics'

describe('weapon enchant change hit detection', () => {
  const x = (time: number) => time * 2
  const y = (value: number) => 200 - value * 20
  const value = (sample: { timestamp: number, weapon_enchant: number }) => sample.weapon_enchant
  const hero = { name: '샷', samples: [{ timestamp: 50, weapon_enchant: 5 }, { timestamp: 100, weapon_enchant: 8 }] }

  it('identifies the whole vertical segment and snaps nearby pointers to its observation', () => {
    for (const point of [{ x: 200, y: 40 }, { x: 200, y: 70 }, { x: 200, y: 100 }, { x: 195, y: 70 }]) {
      expect(stepChangesAt([hero], point, x, y, value)).toEqual([{ name: '샷', timestamp: 100, before: 5, after: 8 }])
    }
    expect(stepChangesAt([hero], { x: 193, y: 70 }, x, y, value)).toEqual([])
    expect(stepChangesAt([hero], { x: 200, y: 120 }, x, y, value)).toEqual([])
  })

  it('returns all overlapping owners even with more than ten characters', () => {
    const series = Array.from({ length: 12 }, (_, index) => ({ ...hero, name: `Hero${index}` }))
    expect(stepChangesAt(series, { x: 200, y: 70 }, x, y, value).map((change) => change.name)).toEqual(series.map((entry) => entry.name))
    const lower = { name: 'Lower', samples: [{ timestamp: 50, weapon_enchant: 0 }, { timestamp: 100, weapon_enchant: 3 }] }
    expect(stepChangesAt([hero, lower], { x: 200, y: 70 }, x, y, value).map((change) => change.name)).toEqual(['샷'])
  })

  it('selects the nearest change and preserves decreases', () => {
    const falling = { name: '래인저', samples: [{ timestamp: 50, weapon_enchant: 8 }, { timestamp: 104, weapon_enchant: 0 }] }
    for (const series of [[hero, falling], [falling, hero]]) {
      expect(stepChangesAt(series, { x: 207, y: 70 }, x, y, value)).toEqual([{ name: '래인저', timestamp: 104, before: 8, after: 0 }])
    }
  })

  it('does not invent changes at the first sample or on unchanged values', () => {
    expect(stepChangesAt([hero], { x: 100, y: 100 }, x, y, value)).toEqual([])
    const steady = { name: 'Steady', samples: [{ timestamp: 50, weapon_enchant: 8 }, { timestamp: 100, weapon_enchant: 8 }] }
    expect(stepChangesAt([steady], { x: 200, y: 40 }, x, y, value)).toEqual([])
    expect(stepChangesAt([], { x: 200, y: 70 }, x, y, value)).toEqual([])
  })
})

describe('land ownership history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'Hero', land_plots: 4, account_first_rank: 1 }, { name: 'Alt', land_plots: 2, account_first_rank: 1 }]
  const series = entries.map((entry) => ({ name: entry.name, started_at: timestamp - 100,
    samples: [{ timestamp: timestamp - 100, land_plots: 0 }, { timestamp, land_plots: entry.land_plots }] }))
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(leaderboardPeriods)('accepts the $label period and ownership starting from zero', ({ hours, interval }) => {
    const history = { ...data, from: timestamp - hours * 3600, sample_interval_seconds: interval }
    expect(parseLandLeaderboard(history, hours)).toEqual(history)
    expect(parseLandLeaderboard({ ...history, entries: [], series: [] }, hours).entries).toEqual([])
    expect(sampleAt(series[0].samples, timestamp - 1)?.land_plots).toBe(0)
  })

  it('rejects non-owners in the ranking, invalid plot counts and mismatched history', () => {
    for (const land_plots of [0, -1, 1.5, NaN, Infinity]) {
      expect(() => parseLandLeaderboard({ ...data, entries: [entries[0], { ...entries[1], land_plots }] }, 168)).toThrow()
    }
    for (const land_plots of [-1, 1.5, NaN, Infinity]) {
      expect(() => parseLandLeaderboard({ ...data, series: [{ ...series[0], samples: [{ timestamp: timestamp - 100, land_plots }] }, series[1]] }, 168)).toThrow()
    }
    expect(() => parseLandLeaderboard({ ...data, entries: [...entries].reverse() }, 168)).toThrow()
    expect(() => parseLandLeaderboard({ ...data, series: [...series].reverse() }, 168)).toThrow()
    expect(() => parseLandLeaderboard({ ...data, series: [] }, 168)).toThrow()
  })
})

describe('character level history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'Hero', level: 10, account_first_rank: 1 }]
  const series = [{ name: 'Hero', started_at: timestamp - 400 * 86400, samples: [{ timestamp, level: 10 }] }]
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(leaderboardPeriods)('accepts the $label range with a carried baseline and newly tracked characters', ({ hours, interval }) => {
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
    expect(sampleAt(samples, 99)).toBeNull()
    expect(sampleAt(samples, 199)?.level).toBe(5)
    expect(sampleAt(samples, 200)?.level).toBe(6)
    expect(sampleAt(samples, 400)?.level).toBe(4)
    expect(sampleAt([], 400)).toBeNull()
    expect(stepPath(samples, 400, (time) => time, (level) => level, (sample) => sample.level)).toBe('M100,5 H200V6 H300V4 H400')
  })

  it('keeps ten distinct colors through ranking changes and replaces only departed characters', () => {
    const assign = createCharacterColors()
    const names = Array.from({ length: 10 }, (_, index) => `Hero${index}`)
    const colors = assign(names)
    expect(new Set(Object.values(colors)).size).toBe(10)
    expect(assign([...names].reverse())).toEqual(colors)
    const replacement = assign(['NewHero', ...names.slice(1)])
    for (const name of names.slice(1)) expect(replacement[name]).toBe(colors[name])
    expect(new Set(Object.values(replacement)).size).toBe(10)
    expect(assign(['__proto__', 'constructor', 'toString'])).toHaveProperty('__proto__')
  })

  it('assigns stable colors to every character beyond the first ten', () => {
    const assign = createCharacterColors()
    const names = Array.from({ length: 50 }, (_, index) => `Hero${index}`)
    const colors = assign(names)
    expect(Object.values(colors).every((color) => typeof color === 'string' && color.length > 0)).toBe(true)
    expect(new Set(Object.values(colors)).size).toBe(names.length)
    expect(assign([...names].reverse())).toEqual(colors)
    const replacements = assign([...names.slice(5), 'NewHero', 'AnotherHero'])
    for (const name of names.slice(5)) expect(replacements[name]).toBe(colors[name])
    expect(new Set(Object.values(replacements)).size).toBe(names.length - 3)
  })
})

describe('character gold history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'RichHero', gold: 5000000000, account_first_rank: 1 }, { name: 'NewHero', gold: 0, account_first_rank: 1 }]
  const series = entries.map((entry) => ({ name: entry.name, started_at: timestamp, samples: [{ timestamp, gold: entry.gold }] }))
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(leaderboardPeriods)('accepts $label gold histories, including zero and large balances', ({ hours, interval }) => {
    const history = { ...data, from: timestamp - hours * 3600, sample_interval_seconds: interval }
    expect(parseGoldLeaderboard(history, hours)).toEqual(history)
    expect(parseGoldLeaderboard({ ...history, entries: [], series: [] }, hours).entries).toEqual([])
    expect(() => parseLevelLeaderboard(history, hours)).toThrow()
  })

  it('rejects invalid balances, ranking order, account references and mismatched histories', () => {
    for (const gold of [-1, 0.5, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => parseGoldLeaderboard({ ...data, entries: [{ ...entries[0], gold }, entries[1]] }, 168)).toThrow()
      expect(() => parseGoldLeaderboard({ ...data, series: [{ ...series[0], samples: [{ timestamp, gold }] }, series[1]] }, 168)).toThrow()
    }
    for (const invalid of [
      { entries: [...entries].reverse() },
      { entries: [entries[0], { ...entries[1], account_first_rank: 3 }] },
      { entries: Array.from({ length: 11 }, () => entries[0]) },
      { series: [...series].reverse() },
      { series: [{ ...series[0], samples: [] }, series[1]] },
      { series: [{ ...series[0], samples: [series[0].samples[0], series[0].samples[0]] }, series[1]] },
      { sample_interval_seconds: 60 },
    ]) expect(() => parseGoldLeaderboard({ ...data, ...invalid }, 168)).toThrow()
  })

  it('keeps earned and spent gold until the next recorded change', () => {
    const samples = [{ timestamp: 100, gold: 0 }, { timestamp: 200, gold: 5000000000 }, { timestamp: 300, gold: 50 }]
    expect(sampleAt(samples, 99)).toBeNull()
    expect(sampleAt(samples, 199)?.gold).toBe(0)
    expect(sampleAt(samples, 200)?.gold).toBe(5000000000)
    expect(sampleAt(samples, 400)?.gold).toBe(50)
    expect(stepPath(samples, 400, (time) => time, (gold) => gold, (sample) => sample.gold)).toBe('M100,0 H200V5000000000 H300V50 H400')
  })
})

describe('character weapon enchant history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'Hero', weapon_enchant: 8, account_first_rank: 1 }, { name: 'NewHero', weapon_enchant: 7, account_first_rank: 1 }]
  const series = entries.map((entry) => ({ name: entry.name, started_at: timestamp - 100, samples: [
    { timestamp: timestamp - 100, weapon_enchant: 0 }, { timestamp, weapon_enchant: entry.weapon_enchant },
  ] }))
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(leaderboardPeriods)('accepts $label weapon enchant histories including zero', ({ hours, interval }) => {
    const history = { ...data, from: timestamp - hours * 3600, sample_interval_seconds: interval }
    expect(parseWeaponEnchantLeaderboard(history, hours)).toEqual(history)
    expect(parseWeaponEnchantLeaderboard({ ...history, entries: [], series: [] }, hours).entries).toEqual([])
    expect(() => parseLevelLeaderboard(history, hours)).toThrow()
    expect(() => parseGoldLeaderboard(history, hours)).toThrow()
  })

  it('accepts more than ten qualifying characters, including account references beyond tenth place', () => {
    const entries = Array.from({ length: 15 }, (_, index) => ({ name: `Hero${index}`, weapon_enchant: index < 3 ? 8 : 7, account_first_rank: index < 11 ? 1 : 12 }))
    const series = entries.map((entry) => ({ name: entry.name, started_at: timestamp, samples: [{ timestamp, weapon_enchant: entry.weapon_enchant }] }))
    const history = { ...data, entries, series }
    expect(parseWeaponEnchantLeaderboard(history, 168)).toEqual(history)
  })

  it('excludes current values below +7 even when earlier history was higher', () => {
    for (const weapon_enchant of [0, 6]) {
      expect(() => parseWeaponEnchantLeaderboard({ ...data, entries: [entries[0], { ...entries[1], weapon_enchant }] }, 168)).toThrow()
    }
  })

  it('rejects invalid enchant values, mismatched histories and ranking order', () => {
    for (const weapon_enchant of [-1, 0.5, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => parseWeaponEnchantLeaderboard({ ...data, entries: [{ ...entries[0], weapon_enchant }, entries[1]] }, 168)).toThrow()
      expect(() => parseWeaponEnchantLeaderboard({ ...data, series: [{ ...series[0], samples: [{ timestamp: timestamp - 100, weapon_enchant }] }, series[1]] }, 168)).toThrow()
    }
    expect(() => parseWeaponEnchantLeaderboard({ ...data, entries: [...entries].reverse() }, 168)).toThrow()
    expect(() => parseWeaponEnchantLeaderboard({ ...data, series: [...series].reverse() }, 168)).toThrow()
    expect(() => parseWeaponEnchantLeaderboard({ ...data, series: [] }, 168)).toThrow()
  })

  it('preserves decreases when the strongest owned weapon is lost', () => {
    const samples = [{ timestamp: 100, weapon_enchant: 3 }, { timestamp: 200, weapon_enchant: 7 }, { timestamp: 300, weapon_enchant: 0 }]
    expect(sampleAt(samples, 99)).toBeNull()
    expect(sampleAt(samples, 199)?.weapon_enchant).toBe(3)
    expect(sampleAt(samples, 200)?.weapon_enchant).toBe(7)
    expect(sampleAt(samples, 400)?.weapon_enchant).toBe(0)
    expect(stepPath(samples, 400, (time) => time, (enchant) => enchant, (sample) => sample.weapon_enchant)).toBe('M100,3 H200V7 H300V0 H400')
  })
})

describe('character armor enchant history', () => {
  const timestamp = 1800000000
  const entries = [{ name: 'Hero', armor_enchant: 27, account_first_rank: 1 }, { name: 'NewHero', armor_enchant: 0, account_first_rank: 1 }]
  const series = entries.map((entry) => ({ name: entry.name, started_at: timestamp - 100, samples: [
    { timestamp: timestamp - 100, armor_enchant: entry.armor_enchant + 5 },
    { timestamp, armor_enchant: entry.armor_enchant },
  ] }))
  const data = { timestamp, from: timestamp - 168 * 3600, sample_interval_seconds: 3600, entries, series }

  it.each(leaderboardPeriods)('accepts $label slot totals, decreases and zero', ({ hours, interval }) => {
    const history = { ...data, from: timestamp - hours * 3600, sample_interval_seconds: interval }
    expect(parseArmorEnchantLeaderboard(history, hours)).toEqual(history)
    expect(parseArmorEnchantLeaderboard({ ...history, entries: [], series: [] }, hours).entries).toEqual([])
    expect(() => parseWeaponEnchantLeaderboard(history, hours)).toThrow()
  })

  it('rejects invalid totals, ranking order and mismatched histories', () => {
    for (const armor_enchant of [-1, 0.5, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => parseArmorEnchantLeaderboard({ ...data, entries: [{ ...entries[0], armor_enchant }, entries[1]] }, 168)).toThrow()
      expect(() => parseArmorEnchantLeaderboard({ ...data, series: [{ ...series[0], samples: [{ timestamp: timestamp - 100, armor_enchant }] }, series[1]] }, 168)).toThrow()
    }
    expect(() => parseArmorEnchantLeaderboard({ ...data, entries: [...entries].reverse() }, 168)).toThrow()
    expect(() => parseArmorEnchantLeaderboard({ ...data, series: [...series].reverse() }, 168)).toThrow()
    expect(() => parseArmorEnchantLeaderboard({ ...data, series: [] }, 168)).toThrow()
  })
})
