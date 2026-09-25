import { resolveHorseSteps } from '../../../utils/horseMovement'
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

/** Sends a move with the waypoint's passability floor. */
export type SendPlayerMove = (
  position: Position,
  rotation: number,
  passabilityFloor?: number,
  append?: boolean
) => void

/** Everything routing a leg needs, shared by click-to-move and combat chase. */
export interface Pathing {
  currentFloor: number
  getFloorAt: (x: number, z: number, y: number) => number
  findPath: (
    startX: number,
    startZ: number,
    startFloor: number,
    goalX: number,
    goalZ: number,
    goalFloor: number
  ) => { waypoints: PathWaypoint[] }
  waypointHeight: (floor: number, x: number, z: number) => number
}

export interface RoutedLeg {
  pathWaypoints: PathWaypoint[]
  movementTarget: Position
  playerRotation: number
}

/** Route to the goal and replace the server queue with the first leg. */
export function routeFirstLeg(
  currentPos: Position,
  goal: Position,
  pathing: Pathing,
  sendPlayerMove: SendPlayerMove
): RoutedLeg | null {
  const goalFloor = pathing.getFloorAt(goal.x, goal.z, goal.y)
  const result = pathing.findPath(
    currentPos.x,
    currentPos.z,
    pathing.currentFloor,
    goal.x,
    goal.z,
    goalFloor
  )
  const pathWaypoints = result.waypoints
  if (pathWaypoints.length === 0) return null

  const firstWp = pathWaypoints[0]
  const movementTarget: Position = {
    x: firstWp.x,
    y: pathing.waypointHeight(firstWp.floor, firstWp.x, firstWp.z),
    z: firstWp.z,
  }
  const playerRotation = Math.atan2(
    shortestWrappedDeltaX(currentPos.x, movementTarget.x),
    movementTarget.z - currentPos.z
  )
  sendPlayerMove(movementTarget, playerRotation, firstWp.floor)

  return { pathWaypoints, movementTarget, playerRotation }
}

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
  setFloorLevel: (floor: number) => void
  writePlayerPosition: (position: Position, rotation: number) => void
  sendPlayerMove: SendPlayerMove
}

/** Slide along a clear axis when a smoothed path clips a wall corner. */
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
  setFloorLevel,
  writePlayerPosition,
  sendPlayerMove,
}: MovementSubstrateInput): MovementSubstrateOutcome {
  const currentWaypointFloor = pathWaypoints[currentWaypointIndex]?.floor
  const result = calculateMovementStep(
    currentPos,
    movementState,
    config,
    deltaTimeSeconds
  )

  if (result.mountSteps) {
    const path = resolveHorseSteps(
      result.mountSteps,
      currentPos,
      config.mountRotation ?? result.rotation,
      { sampleHeight, isMovementBlocked, isUphillTooSteep }
    )
    if (path.blocked) {
      writePlayerPosition(path.position, path.rotation)
      sendPlayerMove(path.position, path.rotation, currentWaypointFloor)
      return { kind: path.blocked }
    }
  }

  movementState.currentSpeed = result.newSpeed
  const currentSpeed = result.newSpeed
  const playerRotation = result.rotation

  if (result.arrived) {
    if (
      !result.mountSteps &&
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
      sendPlayerMove(currentPos, playerRotation, currentWaypointFloor)
      return { kind: 'blocked' }
    }

    const stopPos = result.mountSteps ? result.newPos : movementTarget
    const arrivedPos: Position = {
      x: stopPos.x,
      y: sampleHeight(stopPos.x, stopPos.z),
      z: stopPos.z,
    }
    writePlayerPosition(arrivedPos, playerRotation)

    const nextWaypointIndex = currentWaypointIndex + 1
    if (nextWaypointIndex < pathWaypoints.length) {
      const nextWp = pathWaypoints[nextWaypointIndex]
      // Pre-set before the leg starts — see MovingStateData.floor.
      setFloorLevel(nextWp.floor)

      const wpPos: Position = {
        x: nextWp.x,
        y: waypointHeight(nextWp.floor, nextWp.x, nextWp.z),
        z: nextWp.z,
      }

      const ndx = shortestWrappedDeltaX(arrivedPos.x, wpPos.x)
      const ndz = wpPos.z - arrivedPos.z
      const nextRotation = Math.atan2(ndx, ndz)
      const nextMovementState = initMovementState(
        arrivedPos,
        wpPos,
        movementState.currentSpeed
      )

      sendPlayerMove(wpPos, nextRotation, nextWp.floor, true)

      return {
        kind: 'next_waypoint',
        currentSpeed: nextMovementState.currentSpeed,
        playerRotation:
          config.mountRotation === undefined ? nextRotation : playerRotation,
        movementTarget: wpPos,
        movementState: nextMovementState,
        currentWaypointIndex: nextWaypointIndex,
      }
    }

    if (!result.mountSteps) {
      sendPlayerMove(arrivedPos, playerRotation, currentWaypointFloor, true)
    }
    return { kind: 'arrived', currentSpeed, playerRotation }
  }

  let stepPos = result.newPos
  if (
    !result.mountSteps &&
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
      sendPlayerMove(currentPos, playerRotation, currentWaypointFloor)
      return { kind: 'blocked' }
    }
    stepPos = slid
  }

  const dirX = Math.sin(result.rotation)
  const dirZ = Math.cos(result.rotation)
  if (
    !result.mountSteps &&
    isUphillTooSteep(currentPos.x, currentPos.z, currentPos.y, dirX, dirZ)
  ) {
    sendPlayerMove(currentPos, playerRotation, currentWaypointFloor)
    return { kind: 'slope_blocked' }
  }

  writePlayerPosition(
    {
      x: stepPos.x,
      y: result.mountSteps?.length
        ? stepPos.y
        : sampleHeight(stepPos.x, stepPos.z),
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
