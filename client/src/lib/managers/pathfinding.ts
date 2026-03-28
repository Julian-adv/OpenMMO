import { passability_find_path } from '../wasm/onlinerpg_shared'

export interface PathWaypoint {
  x: number
  z: number
  floor: number
}

export interface PathResult {
  waypoints: PathWaypoint[]
  found: boolean
}

/**
 * Find a smoothed path on a 1m world grid with floor-level awareness.
 * Delegates to the WASM A* implementation in the shared crate.
 */
export function findPath(
  startX: number,
  startZ: number,
  startFloor: number,
  goalX: number,
  goalZ: number,
  goalFloor: number
): PathResult {
  return passability_find_path(
    startX,
    startZ,
    startFloor,
    goalX,
    goalZ,
    goalFloor
  ) as PathResult
}
