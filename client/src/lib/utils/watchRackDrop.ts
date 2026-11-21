import { get } from 'svelte/store'
import { networkManager } from '../network/socket'
import { pointInRect } from '../stores/dragStore'
import { watchRack } from '../stores/watchDutyStore'

/** With the rack window open, a lent piece goes back on the rack. */
export function putOnWatchRack(itemDefId: string): boolean {
  const rack = get(watchRack)
  if (!rack?.owed.includes(itemDefId)) return false
  networkManager.sendWatchRackMove(rack.objectId, itemDefId, false, false)
  return true
}

/** A lent piece dropped on the open rack window goes back on the rack. */
export function returnToWatchRack(
  itemDefId: string,
  x: number,
  y: number
): boolean {
  const panel = document.querySelector('[data-watch-rack]')
  if (!panel || !pointInRect(x, y, panel.getBoundingClientRect())) return false
  return putOnWatchRack(itemDefId)
}
