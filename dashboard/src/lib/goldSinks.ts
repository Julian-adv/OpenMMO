import type { GoldHours } from './metrics'
import { hasValidGoldRanking, type GoldRanking, type GoldRankingEntry } from './goldRanking'

export type GoldSinkEntry = GoldRankingEntry & ({ sink: 'item_purchase' | 'item_buyback', item_def_id: string } |
  { sink: 'stall_tax' | 'land_tax' | 'land_recovery', item_def_id?: never })

export type GoldSinks = GoldRanking<GoldSinkEntry>

export function goldSinkKey(entry: GoldSinkEntry): string {
  return `${entry.sink}:${entry.item_def_id ?? ''}`
}

export function parseGoldSinks(value: unknown, hours: GoldHours): GoldSinks {
  if (!value || typeof value !== 'object') throw new Error('Invalid gold sinks response')
  const data = value as GoldSinks
  if (!hasValidGoldRanking(data, hours, goldSinkKey, (entry) => entry.sink === 'item_purchase' || entry.sink === 'item_buyback'
    ? typeof entry.item_def_id === 'string' && entry.item_def_id.trim().length > 0
    : ['stall_tax', 'land_tax', 'land_recovery'].includes(entry.sink) && entry.item_def_id === undefined)) {
    throw new Error('Invalid gold sinks response')
  }
  return data
}
