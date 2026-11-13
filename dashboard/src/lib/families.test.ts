import { describe, expect, it } from 'vitest'
import { parseFamilies } from './families'

const day = (day_start: number, founded = 0, merged = 0, dissolved = 0) => ({ day_start, founded, merged, dissolved })
const ironwood = { name: 'Ironwood', created_at: 1_799_990_000, members: 3, estates: 2, head: 'HeadB', elders: ['HeadA'] }
const data = {
  until: 1_800_000_100, collection_started_at: 1_799_000_000, active: 1, members: 3,
  total: { founded: 3, merged: 1, dissolved: 1 },
  daily: [day(1_799_852_400, 1), day(1_799_938_800, 2, 1, 1)],
  top: [ironwood],
}

describe('families', () => {
  it('accepts counts and a family without a head or elders', () => {
    expect(parseFamilies(data)).toEqual(data)
    expect(parseFamilies({ ...data, top: [{ ...ironwood, head: null, elders: [] }] })).toBeTruthy()
  })

  it('rejects malformed counts and families', () => {
    for (const broken of [
      { ...data, active: -1 }, { ...data, total: { founded: 1, merged: 1 } }, { ...data, daily: [day(1.5)] },
      { ...data, top: [{ ...ironwood, name: '' }] }, { ...data, top: [{ ...ironwood, elders: [''] }] },
      { ...data, top: [{ ...ironwood, head: 3 }] }, { ...data, top: null }, null,
    ]) {
      expect(() => parseFamilies(broken)).toThrow()
    }
  })
})
