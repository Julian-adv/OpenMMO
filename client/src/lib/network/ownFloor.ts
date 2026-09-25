import { dungeonManager } from '../managers/dungeonManager'
import { playerVisualFloorLevel } from '../stores/housingStore'

export function syncOwnFloor(
  floorLevel: number | undefined,
  x: number,
  z: number
) {
  const floor = floorLevel ?? 0
  // Clearing dungeon depth first would briefly report the old surface floor.
  if (floor >= 0) playerVisualFloorLevel.set(floor)
  dungeonManager.syncFromFloorLevel(floor, x, z)
  if (floor < 0) playerVisualFloorLevel.set(0)
}
