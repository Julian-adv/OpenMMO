import { describe, expect, it } from 'vitest'
import { goldPeriods } from './metrics'
import { parseItemGoldSources } from './itemGoldSources'

const until = 1_800_000_000
const entries = [
  { item_def_id: 'iron_sword', name: '철검', quantity: 2, gold: 8000 },
  { item_def_id: 'healing_potion', name: '회복 물약', quantity: 10, gold: 2400 },
]
const data = { from: until - 86400, until, collection_started_at: until - 3600, total_gold: 10400, entries }

describe('item gold production ranking', () => {
  it.each(goldPeriods)('accepts $label with a partial collection period', ({ hours }) => {
    const response = { ...data, from: until - hours * 3600 }
    expect(parseItemGoldSources(response, hours)).toEqual(response)
  })

  it('distinguishes an empty period from zero-value sales', () => {
    expect(parseItemGoldSources({ ...data, total_gold: 0, entries: [] }, 24).entries).toEqual([])
    expect(parseItemGoldSources({ ...data, total_gold: 0, entries: [{ ...entries[0], gold: 0 }] }, 24).entries).toHaveLength(1)
  })

  it('allows collection to start during the first unfinished hour', () => {
    const response = { ...data, collection_started_at: until + 120, entries: [], total_gold: 0 }
    expect(parseItemGoldSources(response, 24)).toEqual(response)
    expect(() => parseItemGoldSources({ ...response, from: response.from + 1, until: until + 1 }, 24)).toThrow()
  })

  it('rejects incorrect totals, duplicate items, broken ordering and invalid counts', () => {
    for (const invalid of [
      { total_gold: 1 }, { from: until }, { collection_started_at: until + 3600 },
      { entries: [...entries].reverse() }, { entries: [entries[0], entries[0]], total_gold: 16000 },
      ...[0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1].map((quantity) => ({ entries: [{ ...entries[0], quantity }], total_gold: 8000 })),
      ...[-1, Infinity, Number.MAX_SAFE_INTEGER + 1].map((gold) => ({ entries: [{ ...entries[0], gold }], total_gold: gold })),
    ]) expect(() => parseItemGoldSources({ ...data, ...invalid }, 24)).toThrow()
  })
})
