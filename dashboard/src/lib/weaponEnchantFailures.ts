export interface WeaponEnchantFailure {
  id: string
  timestamp: number
  character_id: number
  name: string
  item_def_id: string
  item_name: string
  enchant: number
  failure_count: number
}

export interface WeaponEnchantFailures {
  until: number
  collection_started_at: number
  entries: WeaponEnchantFailure[]
}

const nonnegativeInteger = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0
const nonemptyString = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0

export const weaponEnchantFailureKey = (entry: WeaponEnchantFailure): string =>
  JSON.stringify([entry.character_id, entry.item_def_id, entry.enchant])

export function parseWeaponEnchantFailures(value: unknown): WeaponEnchantFailures {
  if (!value || typeof value !== 'object') throw new Error('Invalid weapon enchant failures response')
  const data = value as WeaponEnchantFailures
  if (!nonnegativeInteger(data.until) || !nonnegativeInteger(data.collection_started_at)
    || data.collection_started_at > data.until || !Array.isArray(data.entries) || data.entries.length > 10) {
    throw new Error('Invalid weapon enchant failures response')
  }
  const ids = new Set<string>()
  const groups = new Set<string>()
  let previousTimestamp = data.until
  for (const entry of data.entries) {
    if (!entry || typeof entry !== 'object' || !nonemptyString(entry.id) || ids.has(entry.id)
      || !nonnegativeInteger(entry.timestamp) || entry.timestamp > previousTimestamp
      || !nonnegativeInteger(entry.character_id) || entry.character_id === 0
      || !nonemptyString(entry.name) || !nonemptyString(entry.item_def_id) || !nonemptyString(entry.item_name)
      || !nonnegativeInteger(entry.enchant) || entry.enchant > 2_147_483_647
      || !nonnegativeInteger(entry.failure_count) || entry.failure_count === 0
      || groups.has(weaponEnchantFailureKey(entry))) {
      throw new Error('Invalid weapon enchant failure entry')
    }
    ids.add(entry.id)
    groups.add(weaponEnchantFailureKey(entry))
    previousTimestamp = entry.timestamp
  }
  return data
}
