import { writable } from 'svelte/store'

/** Visual floor: 0 = ground/1F, 1 = 2F. Official movement uses ownPlayerFloor. */
export const playerVisualFloorLevel = writable(0)

/** ID of the house the player is currently inside, or null if outdoors */
export const playerInsideHouseId = writable<string | null>(null)

export function resetHousingStore() {
  playerVisualFloorLevel.set(0)
  playerInsideHouseId.set(null)
}
