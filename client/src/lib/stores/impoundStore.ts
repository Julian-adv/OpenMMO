import { writable } from 'svelte/store'
import { networkManager } from '../network/socket'

export interface ImpoundedItem {
  item_def_id: string
  quantity: number
  fee: number
}

/** Holds seized goods for buy-back; mirrors `GUARD_CAPTAIN` on the server. */
export const GUARD_CAPTAIN = 'Karl'

export interface ImpoundSession {
  npcPlayerId: number
  items: ImpoundedItem[] | null
  error: string | null
  pending: boolean
  reclaimed: boolean
}

export const impoundSession = writable<ImpoundSession | null>(null)

export function openImpound(npcPlayerId: number) {
  networkManager.sendImpoundQuery(npcPlayerId)
  impoundSession.set({
    npcPlayerId,
    items: null,
    error: null,
    pending: true,
    reclaimed: false,
  })
}

export function applyImpoundState(data: {
  npc_player_id: number
  items: ImpoundedItem[]
  reclaimed: boolean
  error?: string | null
}) {
  impoundSession.update((session) =>
    session?.npcPlayerId === data.npc_player_id
      ? {
          ...session,
          pending: false,
          error: data.error ?? null,
          reclaimed: session.reclaimed || data.reclaimed,
          items: data.error ? session.items : data.items,
        }
      : session
  )
}
