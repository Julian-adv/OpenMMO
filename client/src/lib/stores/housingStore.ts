import { writable } from 'svelte/store'

/** Y offset to raise entities above the house floor surface */
export const playerFloorOffset = writable(0)

/** Current floor level the player is on (-1 = outdoors, 0 = 1F, 1 = 2F) */
export const playerFloorLevel = writable(-1)
