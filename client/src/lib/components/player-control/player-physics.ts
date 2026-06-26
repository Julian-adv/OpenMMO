import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
import { housingManager } from '../../managers/housingManager'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'
import {
  isSlopeTooSteepUphill,
  SLOPE_LOOKAHEAD_DISTANCE,
} from '../../utils/movementUtils'

export interface PlayerPhysicsDeps {
  /** Live read — heightManager is a Svelte prop and may change identity. */
  getHeightManager: () => TerrainHeightManager
  /** Live read — bridgeManager uses current Y to disambiguate stacked decks. */
  getCurrentPlayerY: () => number | null
  /** Live read — housing floor offset above ground. */
  getFloorOffset: () => number
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
  /** Sample terrain ahead and report whether the climb would exceed MAX_TRAVERSABLE_SLOPE_DEG. */
  isUphillTooSteep(
    fromX: number,
    fromZ: number,
    fromY: number,
    dirX: number,
    dirZ: number
  ): boolean
}

export function createPlayerPhysics(deps: PlayerPhysicsDeps): PlayerPhysics {
  function sampleHeight(x: number, z: number): number {
    // Dungeon floors and stair-shaft ramps replace terrain entirely while
    // underground (and on the surface entrance ramp).
    const dungeonY = dungeonManager.sampleHeightAt(x, z)
    if (dungeonY !== null) return dungeonY
    const deckY = bridgeManager.findDeckYAt(x, z, deps.getCurrentPlayerY())
    if (deckY !== null) return deckY
    return (
      deps.getHeightManager().getHeightAtWorldPosition(x, z) +
      deps.getFloorOffset()
    )
  }

  // Half-width of the player's collision footprint. Cylinder-vs-wall check
  // at the destination keeps the player from embedding into walls.
  const PLAYER_RADIUS = 0.3

  function isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    if (housingManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    if (bridgeManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    // Surface dungeon entrance walls (and the shut door) seal the stair hole.
    if (dungeonManager.entranceBlocksMovement(fromX, fromZ, toX, toZ))
      return true
    // Shut interior room doors block their corridor mouth underground.
    if (dungeonManager.interiorDoorBlocksMovement(fromX, fromZ, toX, toZ))
      return true
    if (housingManager.isCircleBlocked(toX, toZ, PLAYER_RADIUS, y)) {
      // Allow movement when the source is already overlapping a wall (e.g.
      // spawn next to a freshly placed editor wall) so the player can escape.
      if (!housingManager.isCircleBlocked(fromX, fromZ, PLAYER_RADIUS, y)) {
        return true
      }
    }
    return false
  }

  function isUphillTooSteep(
    fromX: number,
    fromZ: number,
    fromY: number,
    dirX: number,
    dirZ: number
  ): boolean {
    const aheadX = fromX + dirX * SLOPE_LOOKAHEAD_DISTANCE
    const aheadZ = fromZ + dirZ * SLOPE_LOOKAHEAD_DISTANCE
    const aheadY = sampleHeight(aheadX, aheadZ)
    return isSlopeTooSteepUphill(fromY, aheadY, SLOPE_LOOKAHEAD_DISTANCE)
  }

  return { sampleHeight, isMovementBlocked, isUphillTooSteep }
}
