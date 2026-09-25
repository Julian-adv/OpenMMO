import type { GoldHours } from './metrics'

export interface GoldRankingEntry {
  name: string
  quantity: number
  gold: number
}

export interface GoldRanking<Entry extends GoldRankingEntry> {
  from: number
  until: number
  collection_started_at: number
  total_gold: number
  entries: Entry[]
}

export function hasValidGoldRanking<Entry extends GoldRankingEntry>(
  data: GoldRanking<Entry>,
  hours: GoldHours,
  entryKey: (entry: Entry) => string,
  validKind: (entry: Entry) => boolean,
): boolean {
  return Number.isSafeInteger(data.until) && data.until >= 0 &&
    Number.isSafeInteger(data.from) && data.until - data.from === hours * 3600 &&
    data.until % 3600 === 0 &&
    Number.isSafeInteger(data.collection_started_at) && data.collection_started_at >= 0 && data.collection_started_at < data.until + 3600 &&
    Number.isSafeInteger(data.total_gold) && data.total_gold >= 0 &&
    Array.isArray(data.entries) &&
    data.entries.every((entry, index) => entry && validKind(entry) &&
      typeof entry.name === 'string' && entry.name.trim().length > 0 &&
      Number.isSafeInteger(entry.quantity) && entry.quantity > 0 &&
      Number.isSafeInteger(entry.gold) && entry.gold >= 0 &&
      (index === 0 || entry.gold <= data.entries[index - 1].gold)) &&
    new Set(data.entries.map(entryKey)).size === data.entries.length &&
    data.entries.reduce((total, entry) => total + entry.gold, 0) === data.total_gold
}
