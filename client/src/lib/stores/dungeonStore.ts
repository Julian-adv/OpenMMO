import { derived, writable } from 'svelte/store'

/**
 * Dungeon depth the local player is on. 0 = surface, 1..N = floors below
 * the entrance. Kept separate from housing's playerFloorLevel (where -1
 * means outdoors); the wire format `floor_level = -depth` is produced
 * only at the network boundary.
 */
export const currentDungeonDepth = writable(0)

/** Entrance id of the dungeon the player is in (null = none). */
export const currentDungeonId = writable<string | null>(null)

/** True while the player is below the surface (hides overworld layers). */
export const isUnderground = derived(currentDungeonDepth, (d) => d >= 1)

/**
 * Whether the surface entrance double-doors are swung open. Client-only state
 * (the entrance structure is purely cosmetic): toggled by clicking a door leaf,
 * reset to shut whenever the active entrance changes.
 */
export const dungeonDoorOpen = writable(false)
