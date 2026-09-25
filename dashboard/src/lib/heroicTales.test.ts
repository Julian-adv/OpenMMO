import { describe, expect, it } from 'vitest'
import { parseHeroicTales } from './heroicTales'

const entry = { line: 3, date: '2026-09-13', hero: '용사A', brief: '오거를 쓰러뜨렸다. | 영웅적으로 노래한다.' }
const data = { until: 1_800_000_000, available: true, skipped_lines: 0, entries: [entry] }

describe('heroic tales', () => {
  it('preserves ledger text and accepts repeated deeds on different lines', () => {
    const response = { ...data, skipped_lines: 1, entries: [entry, { ...entry, line: 1 }] }
    expect(parseHeroicTales(response)).toEqual(response)
  })

  it('distinguishes a missing ledger from an empty one', () => {
    for (const available of [true, false]) {
      expect(parseHeroicTales({ ...data, available, entries: [] }).available).toBe(available)
    }
    expect(() => parseHeroicTales({ ...data, available: false })).toThrow()
    expect(() => parseHeroicTales({ ...data, available: false, entries: [], skipped_lines: 1 })).toThrow()
  })

  it('rejects broken responses, duplicate lines and entries outside reverse ledger order', () => {
    for (const value of [null, {}, { ...data, until: -1 }, { ...data, available: 'true' },
      { ...data, skipped_lines: -1 }, { ...data, entries: null }]) {
      expect(() => parseHeroicTales(value)).toThrow()
    }
    for (const entries of [[null], [entry, entry], [entry, { ...entry, line: 4 }],
      [{ ...entry, line: 0 }], [{ ...entry, line: 1.5 }], [{ ...entry, date: '' }],
      [{ ...entry, hero: ' ' }], [{ ...entry, brief: null }]]) {
      expect(() => parseHeroicTales({ ...data, entries })).toThrow()
    }
  })
})
