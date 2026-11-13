export interface FamilyEventCounts {
  founded: number
  merged: number
  dissolved: number
}

export interface FamilyDayCounts extends FamilyEventCounts {
  day_start: number
}

export interface TopFamily {
  name: string
  created_at: number
  members: number
  estates: number
  head: string | null
  elders: string[]
}

export interface Families {
  until: number
  collection_started_at: number
  active: number
  members: number
  total: FamilyEventCounts
  daily: FamilyDayCounts[]
  top: TopFamily[]
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0
const object = (value: unknown) => value !== null && typeof value === 'object'

const validCounts = (counts: FamilyEventCounts) =>
  object(counts) && [counts.founded, counts.merged, counts.dissolved].every(count)

const validFamily = (family: TopFamily) =>
  object(family) && text(family.name) && [family.created_at, family.members, family.estates].every(count)
  && (family.head === null || text(family.head))
  && Array.isArray(family.elders) && family.elders.every(text)

export function parseFamilies(value: unknown): Families {
  const data = value as Families
  if (!object(data) || ![data.until, data.collection_started_at, data.active, data.members].every(count)
    || !validCounts(data.total)
    || !Array.isArray(data.daily) || !data.daily.every((day) => validCounts(day) && count(day.day_start))
    || !Array.isArray(data.top) || !data.top.every(validFamily)) {
    throw new Error('Invalid families response')
  }
  return data
}

export const dailySeries = [
  { key: 'founded', label: '창설', color: '#0f9d85' },
  { key: 'merged', label: '통합', color: '#5276d1' },
  { key: 'dissolved', label: '해산', color: '#c66a2b' },
] as const
