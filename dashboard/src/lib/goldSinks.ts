import type { GoldHours } from './metrics'
import { hasValidGoldRanking, type GoldRanking, type GoldRankingEntry } from './goldRanking'

export const itemSinkLabels = { item_purchase: '상인 구매', item_buyback: '상인 재매입', impound_fee: '압수품 반환' } as const

export type GoldSinkEntry = GoldRankingEntry & ({ sink: keyof typeof itemSinkLabels, item_def_id: string, npc_id?: never } |
  { sink: 'npc_outfit', npc_id: string, item_def_id?: never } |
  { sink: 'stall_tax' | 'land_tax' | 'land_recovery' | 'salon', item_def_id?: never, npc_id?: never })

export type GoldSinks = GoldRanking<GoldSinkEntry>

export function goldSinkKey(entry: GoldSinkEntry): string {
  return `${entry.sink}:${entry.item_def_id ?? entry.npc_id ?? ''}`
}

export function parseGoldSinks(value: unknown, hours: GoldHours): GoldSinks {
  if (!value || typeof value !== 'object') throw new Error('Invalid gold sinks response')
  const data = value as GoldSinks
  const named = (id: unknown) => typeof id === 'string' && id.trim().length > 0
  if (!hasValidGoldRanking(data, hours, goldSinkKey, (entry) => entry.sink in itemSinkLabels
    ? named(entry.item_def_id) && entry.npc_id === undefined
    : entry.sink === 'npc_outfit'
      ? named(entry.npc_id) && entry.item_def_id === undefined
      : ['stall_tax', 'land_tax', 'land_recovery', 'salon'].includes(entry.sink)
        && entry.item_def_id === undefined && entry.npc_id === undefined)) {
    throw new Error('Invalid gold sinks response')
  }
  return data
}
