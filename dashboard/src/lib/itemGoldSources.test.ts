import { describe, expect, it } from 'vitest'
import { goldPeriods } from './metrics'
import { goldSourceKey, parseItemGoldSources } from './itemGoldSources'

const until = 1_800_000_000
const entries = [
  { source: 'item_sale', item_def_id: 'iron_sword', name: '철검', quantity: 2, gold: 8000 },
  { source: 'item_sale', item_def_id: 'healing_potion', name: '회복 물약', quantity: 10, gold: 2400 },
]
const data = { from: until - 86400, until, collection_started_at: until - 3600, rewards_started_at: until - 60, total_gold: 10400, entries }

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
    const response = { ...data, collection_started_at: until + 120, rewards_started_at: until + 120, entries: [], total_gold: 0 }
    expect(parseItemGoldSources(response, 24)).toEqual(response)
    expect(() => parseItemGoldSources({ ...response, from: response.from + 1, until: until + 1 }, 24)).toThrow()
  })

  it('ranks item sales and all direct rewards together with distinct source keys', () => {
    const rewards = [
      { source: 'dungeon_chest', name: '던전 보상 상자', quantity: 2, gold: 15000 },
      { source: 'npc_salary', name: '주민 NPC 급여', quantity: 1, gold: 3000 },
      { source: 'coin_pouch', name: '동전 주머니 개봉', quantity: 1, gold: 12 },
      { source: 'coin_pile', name: '동전 더미 획득', quantity: 2, gold: 7 },
    ]
    const mixed = [...entries, ...rewards].sort((a, b) => b.gold - a.gold)
    const response = { ...data, entries: mixed, total_gold: 28419 }
    const parsed = parseItemGoldSources(response, 24)
    expect(parsed).toEqual(response)
    expect(new Set(parsed.entries.map(goldSourceKey)).size).toBe(6)
    for (const reward of rewards) {
      expect(() => parseItemGoldSources({ ...data, entries: [reward, reward], total_gold: reward.gold * 2 }, 24)).toThrow()
      expect(() => parseItemGoldSources({ ...data, entries: [{ ...reward, item_def_id: 'iron_sword' }], total_gold: reward.gold }, 24)).toThrow()
    }
  })

  it('rejects incorrect totals, duplicate items, broken ordering and invalid counts', () => {
    for (const invalid of [
      { total_gold: 1 }, { from: until }, { collection_started_at: until + 3600 },
      { rewards_started_at: until + 3600 }, { rewards_started_at: data.collection_started_at - 1 },
      { entries: [{ ...entries[0], source: 'unknown' }], total_gold: 8000 },
      { entries: [...entries].reverse() }, { entries: [entries[0], entries[0]], total_gold: 16000 },
      ...[0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1].map((quantity) => ({ entries: [{ ...entries[0], quantity }], total_gold: 8000 })),
      ...[-1, Infinity, Number.MAX_SAFE_INTEGER + 1].map((gold) => ({ entries: [{ ...entries[0], gold }], total_gold: gold })),
    ]) expect(() => parseItemGoldSources({ ...data, ...invalid }, 24)).toThrow()
  })
})
