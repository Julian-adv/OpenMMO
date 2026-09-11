import { describe, expect, it } from 'vitest'
import { goldPeriods } from './metrics'
import { goldSinkKey, parseGoldSinks } from './goldSinks'

const until = 1_800_000_000
const entries = [
  { sink: 'item_purchase', item_def_id: 'iron_sword', name: '철검', quantity: 2, gold: 8000 },
  { sink: 'land_recovery', name: '토지 체납 복구 비용', quantity: 1, gold: 6000 },
  { sink: 'item_buyback', item_def_id: 'iron_sword', name: '철검', quantity: 1, gold: 4000 },
  { sink: 'land_tax', name: '토지세', quantity: 1, gold: 2000 },
  { sink: 'stall_tax', name: '가판 판매 수수료', quantity: 2, gold: 15 },
]
const data = { from: until - 86400, until, collection_started_at: until - 3600, total_gold: 20015, entries }

describe('gold consumption ranking', () => {
  it.each(goldPeriods)('accepts $label and distinguishes purchases from buybacks of the same item', ({ hours }) => {
    const response = { ...data, from: until - hours * 3600 }
    const parsed = parseGoldSinks(response, hours)
    expect(parsed).toEqual(response)
    expect(new Set(parsed.entries.map(goldSinkKey)).size).toBe(5)
  })

  it('accepts empty and zero-value periods during the first collection hour', () => {
    const response = { ...data, collection_started_at: until + 120, entries: [], total_gold: 0 }
    expect(parseGoldSinks(response, 24)).toEqual(response)
    expect(parseGoldSinks({ ...response, entries: [{ ...entries[0], gold: 0 }] }, 24).entries).toHaveLength(1)
  })

  it('rejects invalid totals, periods, categories, quantities, duplicate keys and ordering', () => {
    for (const invalid of [
      { total_gold: 1 }, { from: until }, { collection_started_at: -1 }, { collection_started_at: until + 3600 },
      { from: data.from + 1, until: until + 1 }, { entries: [...entries].reverse() },
      { entries: [entries[0], entries[0]], total_gold: 16000 },
      { entries: [{ ...entries[0], sink: 'unknown' }], total_gold: 8000 },
      { entries: [{ ...entries[0], item_def_id: '' }], total_gold: 8000 },
      { entries: [{ ...entries[0], item_def_id: undefined }], total_gold: 8000 },
      { entries: [{ ...entries[1], item_def_id: 'iron_sword' }], total_gold: 6000 },
      ...[0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1].map((quantity) => ({ entries: [{ ...entries[0], quantity }], total_gold: 8000 })),
      ...[-1, Infinity, Number.MAX_SAFE_INTEGER + 1].map((gold) => ({ entries: [{ ...entries[0], gold }], total_gold: gold })),
    ]) expect(() => parseGoldSinks({ ...data, ...invalid }, 24)).toThrow()
  })
})
