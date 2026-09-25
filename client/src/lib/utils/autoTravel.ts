import {
  shortestWrappedDeltaX,
  unwrapWorldXNear,
  wrapWorldX,
  WORLD_MIN_REGION_Z,
  WORLD_MAX_REGION_Z,
} from '../terrain/world-wrap'
import { REGION_CELLS, TILE_DIM } from '../terrain/terrain-constants'
import type { PathResult, PathWaypoint } from '../managers/pathfinding'

export interface TravelDestination {
  x: number
  z: number
}

export const TRAVEL_LEG_DISTANCE = 48
export const TRAVEL_ARRIVAL_DISTANCE = 1

export function isTravelDestinationValid(point: TravelDestination): boolean {
  return (
    Number.isFinite(point.x) &&
    Number.isFinite(point.z) &&
    point.z >= WORLD_MIN_REGION_Z * REGION_CELLS - TILE_DIM / 2 &&
    point.z < (WORLD_MAX_REGION_Z + 1) * REGION_CELLS - TILE_DIM / 2
  )
}

export function travelDistance(
  from: TravelDestination,
  to: TravelDestination
): number {
  return Math.hypot(shortestWrappedDeltaX(from.x, to.x), to.z - from.z)
}

type TravelLeg =
  | { kind: 'arrived' }
  | { kind: 'waiting' }
  | { kind: 'blocked' }
  | { kind: 'move'; target: TravelDestination; waypoints: PathWaypoint[] }

export function planTravelLeg(
  from: TravelDestination,
  destination: TravelDestination,
  hasHeightData: (x: number, z: number) => boolean,
  findPath: (target: TravelDestination) => PathResult
): TravelLeg {
  if (!isTravelDestinationValid(destination)) return { kind: 'blocked' }
  const distance = travelDistance(from, destination)
  if (distance <= TRAVEL_ARRIVAL_DISTANCE) return { kind: 'arrived' }
  const fraction = Math.min(1, TRAVEL_LEG_DISTANCE / distance)
  const target = {
    x: wrapWorldX(
      from.x + shortestWrappedDeltaX(from.x, destination.x) * fraction
    ),
    z: from.z + (destination.z - from.z) * fraction,
  }
  if (!hasHeightData(target.x, target.z)) return { kind: 'waiting' }

  const result = findPath({
    ...target,
    x: unwrapWorldXNear(from.x, target.x),
  })
  const last = result.waypoints.at(-1)
  if (
    !last ||
    travelDistance(from, last) < 0.1 ||
    result.waypoints.some((waypoint) => waypoint.floor !== 0) ||
    (!result.found && travelDistance(last, destination) >= distance - 1)
  ) {
    return { kind: 'blocked' }
  }
  let previous = from
  for (const waypoint of result.waypoints) {
    const length = travelDistance(previous, waypoint)
    const steps = Math.max(1, Math.ceil(length / 8))
    for (let step = 0; step <= steps; step++) {
      const x = wrapWorldX(
        previous.x +
          (shortestWrappedDeltaX(previous.x, waypoint.x) * step) / steps
      )
      const z = previous.z + ((waypoint.z - previous.z) * step) / steps
      if (!hasHeightData(x, z)) return { kind: 'waiting' }
    }
    previous = waypoint
  }
  return {
    kind: 'move',
    target,
    waypoints: result.waypoints.map((waypoint) => ({
      ...waypoint,
      x: wrapWorldX(waypoint.x),
    })),
  }
}
