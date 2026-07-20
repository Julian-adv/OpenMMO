import {
  calculateMovementStep,
  initMovementState,
  type MovementConfig,
  type MovementState,
  type Position,
} from '../../../utils/movementUtils'
import { shortestWrappedDeltaX } from '../../../terrain/world-wrap'

export interface PathWaypoint {
  x: number
  z: number
  floor: number
}

/** Network move sender: omitted/false `append` replaces the server's waypoint
 *  queue, true extends it (see PlayerMove in shared messages). */
export type SendPlayerMove = (
  position: Position,
  rotation: number,
  append?: boolean
) => void

interface MovementSubstrateInput {
  currentPos: Position
  movementTarget: Position
  movementState: MovementState
  pathWaypoints: PathWaypoint[]
  currentWaypointIndex: number
  config: MovementConfig
  deltaTimeSeconds: number
  sampleHeight: (x: number, z: number) => number
  waypointHeight: (floor: number, x: number, z: number) => number
  isMovementBlocked: (
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ) => boolean
  isUphillTooSteep: (
    x: number,
    z: number,
    y: number,
    dirX: number,
    dirZ: number
  ) => boolean
  getFloorLevel: () => number
  setFloorLevel: (floor: number) => void
  writePlayerPosition: (position: Position, rotation: number) => void
  sendPlayerMove: SendPlayerMove
}

/**
 * When a diagonal step is blocked (typically grazing a convex wall corner the
 * 0.3m body radius can't clear), try to keep moving along whichever single axis
 * is still clear. This lets the player slide around corners instead of getting
 * permanently stuck — the pathfinder smooths paths using cell-edge walls only
 * (no radius buffer), so smoothed diagonals can clip corners that continuous
 * collision refuses to cross. Returns the slid position, or null if both axes
 * are blocked (a genuine dead-end).
 */
function resolveWallSlide(
  from: Position,
  to: Position,
  isMovementBlocked: MovementSubstrateInput['isMovementBlocked']
): Position | null {
  const dx = to.x - from.x
  const dz = to.z - from.z
  const EPS = 1e-6

  const xOnlyOk =
    Math.abs(dx) > EPS &&
    !isMovementBlocked(from.x, from.z, from.x + dx, from.z, from.y)
  const zOnlyOk =
    Math.abs(dz) > EPS &&
    !isMovementBlocked(from.x, from.z, from.x, from.z + dz, from.y)

  // When both axes are individually clear (a corner tip blocks only the exact
  // diagonal), keep the axis with the greater progress toward the target.
  const preferX = xOnlyOk && (!zOnlyOk || Math.abs(dx) >= Math.abs(dz))
  if (preferX) return { x: from.x + dx, y: from.y, z: from.z }
  if (zOnlyOk) return { x: from.x, y: from.y, z: from.z + dz }
  return null
}

export type MovementSubstrateOutcome =
  | { kind: 'blocked' }
  | { kind: 'slope_blocked' }
  | {
      kind: 'continued'
      currentSpeed: number
      playerRotation: number
      totalDistance: number
    }
  | {
      kind: 'next_waypoint'
      currentSpeed: number
      playerRotation: number
      movementTarget: Position
      movementState: MovementState
      currentWaypointIndex: number
    }
  | {
      kind: 'arrived'
      currentSpeed: number
      playerRotation: number
    }

export function stepMovementSubstrate({
  currentPos,
  movementTarget,
  movementState,
  pathWaypoints,
  currentWaypointIndex,
  config,
  deltaTimeSeconds,
  sampleHeight,
  waypointHeight,
  isMovementBlocked,
  isUphillTooSteep,
  getFloorLevel,
  setFloorLevel,
  writePlayerPosition,
  sendPlayerMove,
}: MovementSubstrateInput): MovementSubstrateOutcome {
  const result = calculateMovementStep(
    currentPos,
    movementState,
    config,
    deltaTimeSeconds
  )

  movementState.currentSpeed = result.newSpeed
  const currentSpeed = result.newSpeed
  const playerRotation = result.rotation

  if (result.arrived) {
    if (
      isMovementBlocked(
        currentPos.x,
        currentPos.z,
        movementTarget.x,
        movementTarget.z,
        currentPos.y
      )
    ) {
      // Blocked stops replace the server's queue with the stop point so it
      // doesn't keep walking to an already-sent waypoint.
      sendPlayerMove(currentPos, playerRotation)
      return { kind: 'blocked' }
    }

    const arrivedWp = pathWaypoints[currentWaypointIndex]
    if (arrivedWp && arrivedWp.floor !== getFloorLevel()) {
      setFloorLevel(arrivedWp.floor)
    }

    writePlayerPosition(
      {
        x: movementTarget.x,
        y: sampleHeight(movementTarget.x, movementTarget.z),
        z: movementTarget.z,
      },
      playerRotation
    )

    const nextWaypointIndex = currentWaypointIndex + 1
    if (nextWaypointIndex < pathWaypoints.length) {
      const nextWp = pathWaypoints[nextWaypointIndex]

      if (nextWp.floor !== getFloorLevel()) {
        setFloorLevel(nextWp.floor)
      }

      const wpPos: Position = {
        x: nextWp.x,
        y: waypointHeight(nextWp.floor, nextWp.x, nextWp.z),
        z: nextWp.z,
      }

      const ndx = shortestWrappedDeltaX(movementTarget.x, wpPos.x)
      const ndz = wpPos.z - movementTarget.z
      const nextRotation = Math.atan2(ndx, ndz)
      const nextMovementState = initMovementState(
        movementTarget,
        wpPos,
        movementState.currentSpeed
      )

      sendPlayerMove(wpPos, nextRotation, true)

      return {
        kind: 'next_waypoint',
        currentSpeed: nextMovementState.currentSpeed,
        playerRotation: nextRotation,
        movementTarget: wpPos,
        movementState: nextMovementState,
        currentWaypointIndex: nextWaypointIndex,
      }
    }

    sendPlayerMove(movementTarget, playerRotation, true)
    return { kind: 'arrived', currentSpeed, playerRotation }
  }

  let stepPos = result.newPos
  if (
    isMovementBlocked(
      currentPos.x,
      currentPos.z,
      stepPos.x,
      stepPos.z,
      currentPos.y
    )
  ) {
    const slid = resolveWallSlide(currentPos, stepPos, isMovementBlocked)
    if (!slid) {
      sendPlayerMove(currentPos, playerRotation)
      return { kind: 'blocked' }
    }
    stepPos = slid
  }

  const dirX = Math.sin(result.rotation)
  const dirZ = Math.cos(result.rotation)
  if (isUphillTooSteep(currentPos.x, currentPos.z, currentPos.y, dirX, dirZ)) {
    sendPlayerMove(currentPos, playerRotation)
    return { kind: 'slope_blocked' }
  }

  writePlayerPosition(
    {
      x: stepPos.x,
      y: sampleHeight(stepPos.x, stepPos.z),
      z: stepPos.z,
    },
    playerRotation
  )

  return {
    kind: 'continued',
    currentSpeed,
    playerRotation,
    totalDistance: movementState.totalDistance,
  }
}
