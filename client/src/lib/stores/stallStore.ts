import { get, writable } from 'svelte/store'
import { networkManager } from '../network/socket'
import type { StallState } from '../network/networkTypes'

/** The stall panel's whole state, replaced outright on every server push. */
export const openStall = writable<StallState | null>(null)

/** Close the panel and stop the server pushing listing changes at us. */
export function closeStallPanel() {
  if (get(openStall) === null) return
  openStall.set(null)
  networkManager.sendCloseStall()
}
