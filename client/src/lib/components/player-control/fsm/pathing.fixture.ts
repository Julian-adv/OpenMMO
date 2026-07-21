import { vi } from 'vitest'
import type { Pathing, PathWaypoint } from './movement-substrate'

/** Test `Pathing` on flat ground floor 0. Routes straight to the goal unless
 *  `waypoints` supplies a detour (or `[]` for "no path found"). `findPath` is a
 *  spy so callers can assert routing was skipped. */
export function directPathing(waypoints?: PathWaypoint[]): Pathing {
  return {
    currentFloor: 0,
    getFloorAt: () => 0,
    findPath: vi.fn((_sx, _sz, _sf, gx, gz, gf) => ({
      waypoints: waypoints ?? [{ x: gx, z: gz, floor: gf }],
    })),
    waypointHeight: () => 0,
  }
}
