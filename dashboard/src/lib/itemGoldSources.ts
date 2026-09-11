import type { GoldHours } from './metrics'

export type GoldSourceEntry = {
  name: string
  quantity: number
  gold: number
} & ({ source: 'item_sale', item_def_id: string } |
  { source: 'dungeon_chest' | 'coin_pile' | 'coin_pouch' | 'npc_salary', item_def_id?: never })

export interface ItemGoldSources {
  from: number
  until: number
  collection_started_at: number
  rewards_started_at: number
  total_gold: number
  entries: GoldSourceEntry[]
}

export function goldSourceKey(entry: GoldSourceEntry): string {
  return `${entry.source}:${entry.item_def_id ?? ''}`
}

export function parseItemGoldSources(value: unknown, hours: GoldHours): ItemGoldSources {
  if (!value || typeof value !== 'object') throw new Error('Invalid item gold sources response')
  const data = value as ItemGoldSources
  if (!Number.isSafeInteger(data.until) || data.until < 0 ||
    !Number.isSafeInteger(data.from) || data.until - data.from !== hours * 3600 ||
    data.until % 3600 !== 0 ||
    !Number.isSafeInteger(data.collection_started_at) || data.collection_started_at < 0 || data.collection_started_at >= data.until + 3600 ||
    !Number.isSafeInteger(data.rewards_started_at) || data.rewards_started_at < data.collection_started_at || data.rewards_started_at >= data.until + 3600 ||
    !Number.isSafeInteger(data.total_gold) || data.total_gold < 0 ||
    !Array.isArray(data.entries) ||
    !data.entries.every((entry, index) => entry &&
      (entry.source === 'item_sale'
        ? typeof entry.item_def_id === 'string' && entry.item_def_id.trim().length > 0
        : ['dungeon_chest', 'coin_pile', 'coin_pouch', 'npc_salary'].includes(entry.source) && entry.item_def_id === undefined) &&
      typeof entry.name === 'string' && entry.name.trim().length > 0 &&
      Number.isSafeInteger(entry.quantity) && entry.quantity > 0 &&
      Number.isSafeInteger(entry.gold) && entry.gold >= 0 &&
      (index === 0 || entry.gold <= data.entries[index - 1].gold)) ||
    new Set(data.entries.map(goldSourceKey)).size !== data.entries.length ||
    data.entries.reduce((total, entry) => total + entry.gold, 0) !== data.total_gold) {
    throw new Error('Invalid item gold sources response')
  }
  return data
}
