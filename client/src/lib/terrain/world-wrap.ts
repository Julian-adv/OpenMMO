import { REGION_CELLS } from './terrain-constants'

/** Full baked circumference per axis (the world is a torus): regions -16 through +15. */
export const WORLD_WIDTH_X = 32 * REGION_CELLS
/** Tile -256 extends half a 64 m tile west of its center at -16,384. */
export const WORLD_MIN_X = -16 * REGION_CELLS - 32
/** Exclusive east edge; periodically identical to WORLD_MIN_X. */
export const WORLD_MAX_X = WORLD_MIN_X + WORLD_WIDTH_X
/** North-south circumference and edges — same values as X (square torus). */
export const WORLD_WIDTH_Z = WORLD_WIDTH_X
export const WORLD_MIN_Z = WORLD_MIN_X
export const WORLD_MAX_Z = WORLD_MIN_Z + WORLD_WIDTH_Z

export function wrapWorldX(x: number): number {
  return (
    ((((x - WORLD_MIN_X) % WORLD_WIDTH_X) + WORLD_WIDTH_X) % WORLD_WIDTH_X) +
    WORLD_MIN_X
  )
}

export function wrapWorldZ(z: number): number {
  return (
    ((((z - WORLD_MIN_Z) % WORLD_WIDTH_Z) + WORLD_WIDTH_Z) % WORLD_WIDTH_Z) +
    WORLD_MIN_Z
  )
}

/** Shortest signed X offset from `fromX` to `toX` on the toroidal world. */
export function shortestWrappedDeltaX(fromX: number, toX: number): number {
  const rawDelta = toX - fromX
  if (rawDelta >= -WORLD_WIDTH_X / 2 && rawDelta < WORLD_WIDTH_X / 2) {
    return rawDelta
  }
  return (
    ((((rawDelta + WORLD_WIDTH_X / 2) % WORLD_WIDTH_X) + WORLD_WIDTH_X) %
      WORLD_WIDTH_X) -
    WORLD_WIDTH_X / 2
  )
}

/** Shortest signed Z offset from `fromZ` to `toZ` on the toroidal world. */
export function shortestWrappedDeltaZ(fromZ: number, toZ: number): number {
  const rawDelta = toZ - fromZ
  if (rawDelta >= -WORLD_WIDTH_Z / 2 && rawDelta < WORLD_WIDTH_Z / 2) {
    return rawDelta
  }
  return (
    ((((rawDelta + WORLD_WIDTH_Z / 2) % WORLD_WIDTH_Z) + WORLD_WIDTH_Z) %
      WORLD_WIDTH_Z) -
    WORLD_WIDTH_Z / 2
  )
}

/** Periodic representation of `x` nearest to the given reference position. */
export function unwrapWorldXNear(referenceX: number, x: number): number {
  return referenceX + shortestWrappedDeltaX(referenceX, x)
}

/** Periodic representation of `z` nearest to the given reference position. */
export function unwrapWorldZNear(referenceZ: number, z: number): number {
  return referenceZ + shortestWrappedDeltaZ(referenceZ, z)
}
