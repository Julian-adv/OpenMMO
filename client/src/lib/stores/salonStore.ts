import { get, writable } from 'svelte/store'
import { networkManager } from '../network/socket'
import { MAX_TRADE_DISTANCE_METERS } from '../data/tradeConstants'
import { npcNameById } from '../data/traderDefs'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { gameStore } from './gameStore'
import type { CharacterAppearance } from '../network/networkTypes'

/** The registry NPC `HAIRDRESSER` on the server. */
const HAIRDRESSER = npcNameById('hairdresser')

export type SalonError =
  | 'too_far'
  | 'asleep'
  | 'unsupported'
  | 'unchanged'
  | 'trading'
  | 'not_enough_gold'
  | 'failed'

export interface SalonSession {
  npcPlayerId: number
  pending: boolean
  error: SalonError | null
  /** Copper paid for the last restyle of this visit. */
  paid: number | null
}

export const salonSession = writable<SalonSession | null>(null)

/** Sitting down in a chair beside the hairdresser opens the salon; the server checks the floor. */
export function openSalonFromChair() {
  const { currentPlayer, otherPlayers } = get(gameStore)
  if (!currentPlayer) return
  const npc = [...otherPlayers.values()].find(
    (player) => player.isOfficialNpc && player.name === HAIRDRESSER
  )
  const at = npc && remotePlayerManager.players.get(npc.id)?.position
  if (
    at &&
    Math.hypot(
      at.x - currentPlayer.position.x,
      at.z - currentPlayer.position.z
    ) <= MAX_TRADE_DISTANCE_METERS
  )
    salonSession.set({
      npcPlayerId: npc.id,
      pending: false,
      error: null,
      paid: null,
    })
}

export function requestRestyle(appearance: CharacterAppearance) {
  salonSession.update((session) => {
    if (!session || session.pending) return session
    networkManager.sendChangeAppearance(session.npcPlayerId, appearance)
    return { ...session, pending: true, error: null }
  })
}

export function applySalonResult(data: {
  price: number
  error?: SalonError | null
}) {
  salonSession.update(
    (session) =>
      session && {
        ...session,
        pending: false,
        error: data.error ?? null,
        paid: data.error ? session.paid : data.price,
      }
  )
}
