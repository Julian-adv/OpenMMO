import { describe, expect, it } from 'vitest'
import { countryName, parseCountryStats, share } from './countries'

const entry = { country: 'KR', accounts: 3, sessions: 5, current_accounts: 2 }
const stats = { from: 1_800_000_000, until: 1_800_086_400, collection_started_at: 1_800_000_100, last_aggregated_at: 1_800_086_400, accounts: 3, current_accounts: 2, countries: [entry, { ...entry, country: 'ZZ' }] }

describe('country stats', () => {
  it('accepts classified and unclassified countries and an empty collection', () => {
    expect(parseCountryStats(stats)).toEqual(stats)
    expect(parseCountryStats({ ...stats, last_aggregated_at: null, countries: [] }).countries).toEqual([])
    expect(parseCountryStats({ ...stats, countries: [{ ...entry, accounts: 0, sessions: 0 }] }).countries[0].current_accounts).toBe(2)
  })

  it('rejects malformed data instead of showing misleading shares', () => {
    for (const countries of [null, [null], [entry, entry], [{ ...entry, country: 'kr' }], [{ ...entry, country: 'KOR' }],
      [{ ...entry, accounts: -1 }], [{ ...entry, sessions: 2 }], [{ ...entry, current_accounts: 1.5 }]]) {
      expect(() => parseCountryStats({ ...stats, countries })).toThrow()
    }
    expect(() => parseCountryStats(null)).toThrow()
    expect(() => parseCountryStats({ ...stats, from: stats.until + 1 })).toThrow()
    expect(() => parseCountryStats({ ...stats, collection_started_at: null })).toThrow()
    expect(() => parseCountryStats({ ...stats, last_aggregated_at: undefined })).toThrow()
    expect(() => parseCountryStats({ ...stats, last_aggregated_at: stats.until + 1 })).toThrow()
  })

  it('names countries in Korean and keeps unknown codes readable', () => {
    expect(countryName('KR')).toBe('대한민국')
    expect(countryName('ZZ')).toBe('미분류')
    expect(share(1, 4)).toBe(25)
    expect(share(1, 0)).toBe(0)
  })
})
