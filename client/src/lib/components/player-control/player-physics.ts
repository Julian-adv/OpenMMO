import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
import { housingManager } from '../../managers/housingManager'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'
import {
  isSlopeTooSteepUphill,
  SLOPE_LOOKAHEAD_DISTANCE,
} from '../../utils/movementUtils'
import { wrapWorldX } from '../../terrain/world-wrap'

export interface PlayerPhysicsDeps {
  /** Live read — heightManager is a Svelte prop and may change identity. */
  getHeightManager: () => TerrainHeightManager
  /** Live read — bridgeManager uses current Y to disambiguate stacked decks. */
  getCurrentPlayerY: () => number | null
  /** Live read — housing floor offset above ground. */
  getFloorOffset: () => number
  /** Live read — passability floor index the player is keyed to. Housing and
   *  dungeon collision select their grid by this, not by Y. */
  getPassabilityFloor: () => number
}

export interface PlayerPhysics {
  sampleHeight(x: number, z: number): number
  /**
   * Ground Y at a point the player has not reached yet. `sampleHeight` can't
   * answer this — its house term is the scalar offset for where the player
   * stands *now*, so on a stairwell ramp it is off by up to a storey, and the
   * server trusts waypoint Y as the authoritative collision height.
   */
  waypointHeight(floor: number, x: number, z: number): number
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
    x = wrapWorldX(x)
    // Dungeon floors and stair-shaft ramps replace terrain entirely while
    // underground (and on the surface entrance ramp).
    const dungeonY = dungeonManager.sampleHeightAt(x, z)
    if (dungeonY !== null) return dungeonY
    // Houses ride the scalar floor offset below, not a per-(x, z) lookup — a
    // stairwell footprint sits inside the upper room, so position alone can't
    // say whether a point is the room or the ramp.
    const deckY = bridgeManager.findDeckYAt(x, z, deps.getCurrentPlayerY())
    if (deckY !== null) return deckY
    return (
      deps.getHeightManager().getHeightAtWorldPosition(x, z) +
      deps.getFloorOffset()
    )
  }

  function waypointHeight(floor: number, x: number, z: number): number {
    x = wrapWorldX(x)
    const dungeonY = dungeonManager.sampleHeightAt(x, z)
    if (dungeonY !== null) return dungeonY
    // Floor-keyed, so the stairwell ramp resolves per position instead of
    // riding the player's current offset.
    const houseY = housingManager.floorHeightAt(floor, x, z)
    if (houseY !== null) return houseY
    const deckY = bridgeManager.findDeckYAt(x, z, deps.getCurrentPlayerY())
    if (deckY !== null) return deckY
    // No floor offset here: a point outside every house sits on the terrain,
    // whatever storey the player happens to be standing on.
    return deps.getHeightManager().getHeightAtWorldPosition(x, z)
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
    // Bridges still key off Y — they are decks in open air, not floors of a
    // structure with a grid per level.
    const floor = deps.getPassabilityFloor()
    if (housingManager.isMovementBlocked(fromX, fromZ, toX, toZ, floor, y))
      return true
    if (bridgeManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    // Surface dungeon entrance walls (and the shut door) seal the stair hole.
    // Shut interior doors need no check here — they're sealed into the wasm
    // passability cells like walls.
    if (dungeonManager.entranceBlocksMovement(fromX, fromZ, toX, toZ))
      return true
    if (housingManager.isCircleBlocked(toX, toZ, PLAYER_RADIUS, floor, y)) {
      // Allow movement when the source is already overlapping a wall (e.g.
      // spawn next to a freshly placed editor wall) so the player can escape.
      if (
        !housingManager.isCircleBlocked(fromX, fromZ, PLAYER_RADIUS, floor, y)
      ) {
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
    if (!isSlopeTooSteepUphill(fromY, aheadY, SLOPE_LOOKAHEAD_DISTANCE)) {
      return false
    }
    // Indoors both samples carry the same floor offset, so the test above
    // reduces to the raw terrain slope under the house and walls off a whole
    // contour line across the rooms. House floors are flat and walkable.
    // Checked only on rejection — the house scan is the expensive half.
    return !housingManager.isPointUnderHouseXZ(wrapWorldX(fromX), fromZ)
  }

  return { sampleHeight, waypointHeight, isMovementBlocked, isUphillTooSteep }
}
