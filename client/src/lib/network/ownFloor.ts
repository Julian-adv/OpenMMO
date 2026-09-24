import { writable } from 'svelte/store'
import { dungeonManager } from '../managers/dungeonManager'
import { playerVisualFloorLevel } from '../stores/housingStore'

export const ownPlayerFloor = writable(0)

export function syncOwnFloor(
  floorLevel: number | undefined,
  x: number,
  z: number
) {
  const floor = floorLevel ?? 0
  ownPlayerFloor.set(floor)
  // Clearing dungeon depth first would briefly report the old surface floor.
  if (floor >= 0) playerVisualFloorLevel.set(floor)
  dungeonManager.syncFromFloorLevel(floor, x, z)
  if (floor < 0) playerVisualFloorLevel.set(0)
}
