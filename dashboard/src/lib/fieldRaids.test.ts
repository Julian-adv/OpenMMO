import { describe, expect, it } from 'vitest'
import {
  bossHealthLeft, failures, parseFieldRaids, raiderTags, raidMinutes, successRate, type FieldRaidRaider, type RecentFieldRaid,
} from './fieldRaids'

const terraclash = {
  boss: 'terraclash', name: 'Terraclash', summons: 5, defeated: 3, moon_set: 0, dawn: 0, wiped: 1, last_defeated_at: 1_799_990_000,
}
const raider: FieldRaidRaider = {
  character_id: 8575, name: '해적', dealt: 0, taken: 900, deaths: 0, near_at_summon: true, in_zone: true, in_zone_at_end: true,
}
const raid: RecentFieldRaid = {
  boss: 'terraclash', name: 'Terraclash', summoned_at: 1_799_999_000, ended_at: 1_799_999_720, outcome: 'dawn',
  summoner_id: 8575, boss_health: 60_000, boss_max_health: 80_000, raiders: [raider],
}
const data = { until: 1_800_000_100, collection_started_at: 1_799_000_000, bosses: [terraclash], recent: [raid] }

describe('field raids', () => {
  it('accepts per-boss counts', () => {
    expect(parseFieldRaids(data)).toEqual(data)
    expect(parseFieldRaids({ ...data, bosses: [{ ...terraclash, summons: 0, defeated: 0, wiped: 0, last_defeated_at: null }] })).toBeTruthy()
  })

  it('rejects malformed counts and more outcomes than summons', () => {
    for (const boss of [
      { ...terraclash, summons: -1 }, { ...terraclash, defeated: 1.5 }, { ...terraclash, wiped: 3 },
      { ...terraclash, name: '' }, { ...terraclash, last_defeated_at: 'x' }, null,
    ]) {
      expect(() => parseFieldRaids({ ...data, bosses: [boss] })).toThrow()
    }
    expect(() => parseFieldRaids(null)).toThrow()
  })

  it('rejects malformed recent raids', () => {
    for (const bad of [
      { ...raid, outcome: 'fled' }, { ...raid, boss_health: 90_000 }, { ...raid, ended_at: 1 },
      { ...raid, summoner_id: 'x' }, { ...raid, raiders: [{ ...raider, taken: -1 }] },
      { ...raid, raiders: [{ ...raider, in_zone: 1 }] }, { ...raid, raiders: [null] }, null,
    ]) {
      expect(() => parseFieldRaids({ ...data, recent: [bad] })).toThrow()
    }
    expect(() => parseFieldRaids({ ...data, recent: undefined })).toThrow()
  })

  it('summarises a raid and tags its raiders', () => {
    expect(raidMinutes(raid)).toBe(12)
    expect(bossHealthLeft(raid)).toBe(75)
    expect(raiderTags(raid, raider)).toEqual(['소환', '끝까지'])
    expect(raiderTags(raid, { ...raider, character_id: 1, in_zone_at_end: false, in_zone: false })).toEqual(['근처만'])
    expect(raiderTags(raid, { ...raider, character_id: 1, in_zone_at_end: false })).toEqual([])
  })

  it('rates success over settled raids only', () => {
    expect(failures(terraclash)).toBe(1)
    expect(successRate(terraclash)).toBe(75)
    expect(successRate({ ...terraclash, defeated: 0, wiped: 0 })).toBeNull()
  })
})
