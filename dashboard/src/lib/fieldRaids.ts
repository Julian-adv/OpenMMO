export interface FieldRaidCounts {
  boss: string
  name: string
  summons: number
  defeated: number
  moon_set: number
  dawn: number
  wiped: number
  last_defeated_at: number | null
}

export type FieldRaidOutcome = 'defeated' | 'moon_set' | 'dawn' | 'wiped'

export interface FieldRaidRaider {
  character_id: number
  name: string
  dealt: number
  taken: number
  deaths: number
  near_at_summon: boolean
  in_zone: boolean
  in_zone_at_end: boolean
}

export interface RecentFieldRaid {
  boss: string
  name: string
  summoned_at: number
  ended_at: number
  outcome: FieldRaidOutcome
  summoner_id: number | null
  boss_health: number
  boss_max_health: number
  raiders: FieldRaidRaider[]
}

export interface FieldRaids {
  until: number
  collection_started_at: number
  bosses: FieldRaidCounts[]
  recent: RecentFieldRaid[]
}

export const outcomeLabels: Record<FieldRaidOutcome, string> = {
  defeated: '성공', moon_set: '달 짐', dawn: '새벽', wiped: '전멸',
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0
const flag = (value: unknown): value is boolean => typeof value === 'boolean'
const object = (value: unknown) => value !== null && typeof value === 'object'

const validBoss = (boss: FieldRaidCounts) =>
  text(boss.boss) && text(boss.name)
  && [boss.summons, boss.defeated, boss.moon_set, boss.dawn, boss.wiped].every(count)
  && failures(boss) + boss.defeated <= boss.summons
  && (boss.last_defeated_at === null || count(boss.last_defeated_at))

const validRaider = (raider: FieldRaidRaider) =>
  object(raider) && Number.isSafeInteger(raider.character_id) && text(raider.name)
  && [raider.dealt, raider.taken, raider.deaths].every(count)
  && [raider.near_at_summon, raider.in_zone, raider.in_zone_at_end].every(flag)

const validRaid = (raid: RecentFieldRaid) =>
  object(raid) && text(raid.boss) && text(raid.name)
  && count(raid.summoned_at) && count(raid.ended_at) && raid.summoned_at <= raid.ended_at
  && Object.hasOwn(outcomeLabels, raid.outcome)
  && (raid.summoner_id === null || Number.isSafeInteger(raid.summoner_id))
  && count(raid.boss_health) && count(raid.boss_max_health) && raid.boss_health <= raid.boss_max_health
  && Array.isArray(raid.raiders) && raid.raiders.every(validRaider)

export function parseFieldRaids(value: unknown): FieldRaids {
  const data = value as FieldRaids
  if (!object(data) || !count(data.until) || !count(data.collection_started_at)
    || !Array.isArray(data.bosses) || !data.bosses.every((boss) => object(boss) && validBoss(boss))
    || !Array.isArray(data.recent) || !data.recent.every(validRaid)) {
    throw new Error('Invalid field raids response')
  }
  return data
}

export const failures = (boss: FieldRaidCounts) => boss.moon_set + boss.dawn + boss.wiped

export const successRate = (boss: FieldRaidCounts) => {
  const settled = boss.defeated + failures(boss)
  return settled === 0 ? null : boss.defeated / settled * 100
}

export const bossHealthLeft = (raid: RecentFieldRaid) =>
  raid.boss_max_health === 0 ? null : raid.boss_health / raid.boss_max_health * 100

export const raidMinutes = (raid: RecentFieldRaid) => (raid.ended_at - raid.summoned_at) / 60

/** What set the raider apart, shortest first. */
export function raiderTags(raid: RecentFieldRaid, raider: FieldRaidRaider) {
  const tags: string[] = []
  if (raider.character_id === raid.summoner_id) tags.push('소환')
  if (raider.in_zone_at_end) tags.push('끝까지')
  else if (!raider.in_zone) tags.push('근처만')
  return tags
}
