import { housingManager } from '../../managers/housingManager'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'

export interface PlayerPhysicsDeps {
  getPassabilityFloor: () => number
}

export interface PlayerPhysics {
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean
}

export function createPlayerPhysics(deps: PlayerPhysicsDeps): PlayerPhysics {
  const PLAYER_RADIUS = 0.3

  function isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    const floor = deps.getPassabilityFloor()
    if (housingManager.isMovementBlocked(fromX, fromZ, toX, toZ, floor, y))
      return true
    if (bridgeManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    if (dungeonManager.entranceBlocksMovement(fromX, fromZ, toX, toZ))
      return true
    if (housingManager.isCircleBlocked(toX, toZ, PLAYER_RADIUS, floor, y)) {
      // Allow escaping an existing overlap.
      if (
        !housingManager.isCircleBlocked(fromX, fromZ, PLAYER_RADIUS, floor, y)
      ) {
        return true
      }
    }
    return false
  }

  return { isMovementBlocked }
}
