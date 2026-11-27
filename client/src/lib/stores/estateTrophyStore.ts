import { get, writable } from 'svelte/store'
import estateDefenseJson from '../../../../data/estate_defense.json'
import type { EstateTrophyRecord } from '../network/networkTypes'

const TROPHIES = new Set(
  Object.values(estateDefenseJson as Record<string, { trophy: string }>).map(
    (grade) => grade.trophy
  )
)

/** Wall trophies, the only items with a defense on record. */
export const isEstateTrophy = (itemDefId: string) => TROPHIES.has(itemDefId)

/** Records by lasting item id; `null` means the item has none. */
export const trophyRecords = writable<Map<number, EstateTrophyRecord | null>>(
  new Map()
)
/** The wall trophy whose record window is open. */
export const shownTrophy = writable<{
  itemId: number
  itemDefId: string
} | null>(null)

const inFlight = new Set<number>()
let pendingWindow: { itemId: number; itemDefId: string } | null = null

/** Asks once per item; records never change. */
export function requestTrophyRecord(
  itemId: number,
  send: (itemId: number) => void
) {
  if (inFlight.has(itemId) || get(trophyRecords).has(itemId)) return
  inFlight.add(itemId)
  send(itemId)
}

/** Opens the window once the item's record is known to exist. */
export function showTrophyRecord(
  itemId: number,
  itemDefId: string,
  send: (itemId: number) => void
) {
  const known = get(trophyRecords)
  if (known.has(itemId)) {
    if (known.get(itemId)) shownTrophy.set({ itemId, itemDefId })
    return
  }
  pendingWindow = { itemId, itemDefId }
  requestTrophyRecord(itemId, send)
}

export function receiveTrophyRecord(
  itemId: number,
  record: EstateTrophyRecord | null
) {
  inFlight.delete(itemId)
  trophyRecords.update((records) => records.set(itemId, record))
  if (pendingWindow?.itemId === itemId) {
    if (record) shownTrophy.set(pendingWindow)
    pendingWindow = null
  }
}

/** A reply lost to a disconnect is asked again after reconnecting. */
export function resetTrophyRequests() {
  inFlight.clear()
  pendingWindow = null
}
