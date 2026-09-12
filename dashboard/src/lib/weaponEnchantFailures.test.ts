import { describe, expect, it } from 'vitest'
import { parseWeaponEnchantFailures, weaponEnchantFailureKey } from './weaponEnchantFailures'

const until = 1_800_000_000
const entry = { id: 'event-1', timestamp: until - 1, character_id: 1, name: 'Reader', item_def_id: 'iron_sword', item_name: 'Iron Sword', enchant: 8, failure_count: 12 }
const data = { until, collection_started_at: until - 3600, entries: [entry] }

describe('weapon enchant failures', () => {
  it('accepts accumulated counts and separate characters, weapons and enchant levels', () => {
    const response = { ...data, entries: [entry, { ...entry, id: 'event-2', enchant: 7 }, { ...entry, id: 'event-3', item_def_id: 'dagger' }, { ...entry, id: 'event-4', character_id: 2 }] }
    expect(parseWeaponEnchantFailures(response)).toEqual(response)
    expect(parseWeaponEnchantFailures({ ...data, entries: [] }).entries).toEqual([])
  })

  it('accepts exactly ten groups even when their counts exceed ten', () => {
    const entries = Array.from({ length: 10 }, (_, index) => ({ ...entry, id: `event-${index}`, character_id: index + 1 }))
    expect(parseWeaponEnchantFailures({ ...data, entries }).entries).toHaveLength(10)
    expect(() => parseWeaponEnchantFailures({ ...data, entries: [...entries, { ...entry, id: 'extra', character_id: 11 }] })).toThrow()
  })

  it('keeps group identity stable when another failure or a name change arrives', () => {
    expect(weaponEnchantFailureKey({ ...entry, id: 'new-event', name: 'Renamed Reader', timestamp: until, failure_count: 13 }))
      .toBe(weaponEnchantFailureKey(entry))
  })

  it('rejects duplicate groups, invalid counts and timestamps out of order', () => {
    for (const entries of [
      [entry, entry], [null], [{ ...entry, id: '' }], [{ ...entry, character_id: 0 }],
      [{ ...entry, name: '' }], [{ ...entry, item_def_id: '' }], [{ ...entry, item_name: '' }],
      [{ ...entry, enchant: -1 }], [{ ...entry, enchant: 1.5 }], [{ ...entry, enchant: Infinity }],
      [{ ...entry, failure_count: 0 }], [{ ...entry, failure_count: -1 }], [{ ...entry, failure_count: 1.5 }],
      [{ ...entry, failure_count: undefined }], [{ ...entry, failure_count: Number.MAX_SAFE_INTEGER + 1 }],
      [entry, { ...entry, id: 'same-group' }], [entry, { ...entry, character_id: 2 }],
      [{ ...entry, timestamp: until + 1 }], [{ ...entry, timestamp: -1 }],
      [entry, { ...entry, id: 'later', timestamp: until, enchant: 7 }],
    ]) expect(() => parseWeaponEnchantFailures({ ...data, entries })).toThrow()
    for (const value of [null, {}, { ...data, until: NaN }, { ...data, collection_started_at: -1 }, { ...data, collection_started_at: until + 1 }]) {
      expect(() => parseWeaponEnchantFailures(value)).toThrow()
    }
  })
})
