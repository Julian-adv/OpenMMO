import { describe, expect, it } from 'vitest'
import { describeChances, formatChance, parseRareDrops, perTenThousandKills, totalHoldings } from './rareDrops'

const holdings = { inventory: 3, npc_inventory: 1, storage: 2, ground: 1 }
const scroll = {
  item_def_id: 'scroll_of_enchant_weapon', name: 'Scroll of Enchant Weapon',
  world_drop: { chance: 0.009, low_level_chance: 0.0045, low_level_max_level: 8 }, monster_drops: [],
  kills: 1000, expected_kill_drops: 7.2, kill_drops: 8, other_drops: 2, holdings,
}
const stethoscope = {
  item_def_id: 'stethoscope', name: 'Stethoscope', world_drop: null,
  monster_drops: [{ monster_type: 'goblin', name: 'Goblin', chance: 0.0004 }, { monster_type: 'hobgoblin', name: 'Hobgoblin', chance: 0.0004 }],
  kills: 500, expected_kill_drops: 0.2, kill_drops: 1, other_drops: 0, holdings,
}
const data = { until: 1_800_000_000, collection_started_at: 1_799_000_000, total_kills: 1000, items: [scroll, stethoscope] }

describe('rare drops', () => {
  it('accepts world and monster drop items', () => {
    expect(parseRareDrops(data)).toEqual(data)
  })

  it('rejects malformed counts, chances and kills beyond the total', () => {
    for (const item of [
      { ...scroll, kills: 1001 }, { ...scroll, kill_drops: -1 }, { ...scroll, expected_kill_drops: NaN },
      { ...scroll, world_drop: { ...scroll.world_drop, chance: 2 } }, { ...stethoscope, monster_drops: [{ monster_type: 'goblin', name: '', chance: 0.1 }] },
      { ...scroll, holdings: { ...holdings, ground: 0.5 } }, null,
    ]) {
      expect(() => parseRareDrops({ ...data, items: [item] })).toThrow()
    }
    expect(() => parseRareDrops(null)).toThrow()
  })

  it('describes chances and per-kill rates', () => {
    expect(formatChance(0.0045)).toBe('0.45%')
    expect(describeChances(scroll)).toEqual(['몬스터 Lv9+ 0.9% · Lv8 이하 0.45%', '상자·통 0.9%'])
    expect(describeChances(stethoscope)).toEqual(['Goblin·Hobgoblin 0.04%'])
    expect(perTenThousandKills(8, 1000)).toBe('80')
    expect(perTenThousandKills(0.2, 500)).toBe('4')
    expect(perTenThousandKills(1, 3)).toBe('3,333.33')
    expect(perTenThousandKills(0, 0)).toBeNull()
    expect(totalHoldings(holdings)).toBe(7)
  })
})
