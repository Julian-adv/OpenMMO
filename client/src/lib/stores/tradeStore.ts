import { writable } from 'svelte/store'

/** An open shop session with a merchant NPC, driven by ServerMessage::ShopState.
 *  Set to a session to open the trade window, to null to close it. */
export interface ShopSession {
  merchantPlayerId: string
  merchantName: string
  catalog: string[]
  sellRatePercent: number
}

export const shopSession = writable<ShopSession | null>(null)
