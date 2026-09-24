import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
import { housingManager } from '../../managers/housingManager'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'
import { wrapWorldX } from '../../terrain/world-wrap'

export interface PlayerPhysicsDeps {
  getHeightManager: () => TerrainHeightManager
  /** Disambiguates stacked bridge decks. */
  getCurrentPlayerY: () => number | null
  getPassabilityFloor: () => number
  /** Surface Y while afloat, retaining the previous Y while tiles load. */
  getFloatSurfaceY?: (x: number, z: number) => number | null
}

export interface PlayerPhysics {
  sampleHeight(x: number, z: number): number
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean
}

export function createPlayerPhysics(deps: PlayerPhysicsDeps): PlayerPhysics {
  function sampleHeight(x: number, z: number): number {
    x = wrapWorldX(x)
    const floor = deps.getPassabilityFloor()
    if (floor === 0) {
      const floatY = deps.getFloatSurfaceY?.(x, z)
      if (floatY != null) return floatY
    }
    const dungeonY = dungeonManager.sampleHeightAt(x, z)
    if (dungeonY !== null) return dungeonY
    const houseY = housingManager.floorHeightAt(floor, x, z)
    if (houseY !== null) return houseY
    const deckY = bridgeManager.findDeckYAt(x, z, deps.getCurrentPlayerY())
    if (deckY !== null) return deckY
    return deps.getHeightManager().getHeightAtWorldPosition(x, z)
  }

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

  return { sampleHeight, isMovementBlocked }
}
