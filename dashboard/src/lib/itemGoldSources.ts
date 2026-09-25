import type { GoldHours } from './metrics'
import { hasValidGoldRanking, type GoldRanking, type GoldRankingEntry } from './goldRanking'

export type GoldSourceEntry = GoldRankingEntry & ({ source: 'item_sale', item_def_id: string } |
  { source: 'dungeon_chest' | 'coin_pile' | 'coin_pouch' | 'npc_salary', item_def_id?: never })

export interface ItemGoldSources extends GoldRanking<GoldSourceEntry> {
  rewards_started_at: number
}

export function goldSourceKey(entry: GoldSourceEntry): string {
  return `${entry.source}:${entry.item_def_id ?? ''}`
}

export function parseItemGoldSources(value: unknown, hours: GoldHours): ItemGoldSources {
  if (!value || typeof value !== 'object') throw new Error('Invalid item gold sources response')
  const data = value as ItemGoldSources
  if (!hasValidGoldRanking(data, hours, goldSourceKey, (entry) => entry.source === 'item_sale'
    ? typeof entry.item_def_id === 'string' && entry.item_def_id.trim().length > 0
    : ['dungeon_chest', 'coin_pile', 'coin_pouch', 'npc_salary'].includes(entry.source) && entry.item_def_id === undefined) ||
    !Number.isSafeInteger(data.rewards_started_at) || data.rewards_started_at < data.collection_started_at || data.rewards_started_at >= data.until + 3600) {
    throw new Error('Invalid item gold sources response')
  }
  return data
}
