import {
  initMovementState,
  type MovementState,
  type Position,
} from '../../../utils/movementUtils'
import type { InteractionExitKind } from './interaction'
import { shortestWrappedDeltaX } from '../../../terrain/world-wrap'
import type { PathWaypoint, SendPlayerMove } from './movement-substrate'

// ───────────────────────────────────────────────────────────────────────────
// Move-request decision (click → start / exit-interaction / ignore)
// ───────────────────────────────────────────────────────────────────────────

export type MoveRequestDecision =
  | {
      kind: 'ignored'
      clearPendingPickupAfterMove: boolean
    }
  | {
      kind: 'exit_pickup_and_retry'
      clearPendingPickupAfterMove: boolean
    }
  | {
      kind: 'exit_object_and_delay'
      clearPendingPickupAfterMove: boolean
    }
  | {
      kind: 'start'
      clearPendingPickupAfterMove: boolean
    }

interface DecideMoveRequestInput {
  pickupAfterArrival: number | null
  currentPlayerHealth: number | null
  interactionExit: InteractionExitKind
  hasCurrentPlayer: boolean
  isMoving: boolean
  hasKeyboardInput: boolean
}

export function decideMoveRequest({
  pickupAfterArrival,
  currentPlayerHealth,
  interactionExit,
  hasCurrentPlayer,
  isMoving,
  hasKeyboardInput,
}: DecideMoveRequestInput): MoveRequestDecision {
  const clearPendingPickupAfterMove = pickupAfterArrival === null

  if (currentPlayerHealth !== null && currentPlayerHealth <= 0) {
    return { kind: 'ignored', clearPendingPickupAfterMove }
  }

  if (interactionExit === 'pickup') {
    return { kind: 'exit_pickup_and_retry', clearPendingPickupAfterMove }
  }

  if (interactionExit === 'object') {
    return { kind: 'exit_object_and_delay', clearPendingPickupAfterMove }
  }

  if (!hasCurrentPlayer || isMoving || hasKeyboardInput) {
    if (hasCurrentPlayer && isMoving && !hasKeyboardInput) {
      return { kind: 'start', clearPendingPickupAfterMove }
    }
    return { kind: 'ignored', clearPendingPickupAfterMove }
  }

  return { kind: 'start', clearPendingPickupAfterMove }
}

// ───────────────────────────────────────────────────────────────────────────
// Path-based click movement initialization
// ───────────────────────────────────────────────────────────────────────────

interface StartClickMovementInput {
  currentPos: Position
  clickPosition: Position
  pickupAfterArrival: number | null
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
  sendPlayerMove: SendPlayerMove
}

export interface StartedClickMovement {
  pathWaypoints: PathWaypoint[]
  currentWaypointIndex: number
  movementState: MovementState
  movementTarget: Position
  playerRotation: number
  pendingPickupAfterMoveInstanceId: number | null
}

export function startClickMovement({
  currentPos,
  clickPosition,
  pickupAfterArrival,
  currentFloor,
  getFloorAt,
  findPath,
  waypointHeight,
  sendPlayerMove,
}: StartClickMovementInput): StartedClickMovement {
  const goalFloor = getFloorAt(
    clickPosition.x,
    clickPosition.z,
    clickPosition.y
  )
  const result = findPath(
    currentPos.x,
    currentPos.z,
    currentFloor,
    clickPosition.x,
    clickPosition.z,
    goalFloor
  )
  const pathWaypoints =
    result.waypoints.length > 0
      ? result.waypoints
      : [{ x: clickPosition.x, z: clickPosition.z, floor: goalFloor }]

  const firstWp = pathWaypoints[0]
  const wpPos: Position = {
    x: firstWp.x,
    y: waypointHeight(firstWp.floor, firstWp.x, firstWp.z),
    z: firstWp.z,
  }

  const dx = shortestWrappedDeltaX(currentPos.x, wpPos.x)
  const dz = wpPos.z - currentPos.z
  const playerRotation = Math.atan2(dx, dz)
  const movementState = initMovementState(currentPos, wpPos, 0)

  // A fresh path replaces the server's waypoint queue; the substrate then
  // appends the following legs.
  sendPlayerMove(wpPos, playerRotation, false)

  return {
    pathWaypoints,
    currentWaypointIndex: 0,
    movementState,
    movementTarget: wpPos,
    playerRotation,
    pendingPickupAfterMoveInstanceId: pickupAfterArrival,
  }
}

// ───────────────────────────────────────────────────────────────────────────
// Full move-request flow (decision + click movement start)
// ───────────────────────────────────────────────────────────────────────────

interface MoveRequestPlayer {
  health: number
  position: Position
}

export interface MoveRequestActions {
  clearPendingPickupAfterMove: () => void
  exitPickupAndRetry: () => void
  exitObjectAndDelay: () => void
  applyStartedMovement: (started: StartedClickMovement) => void
}

interface RunMoveRequestInput {
  clickPosition: Position
  pickupAfterArrival: number | null
  currentPlayer: MoveRequestPlayer | null
  interactionExit: InteractionExitKind
  isMoving: boolean
  hasKeyboardInput: boolean
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
  sendPlayerMove: SendPlayerMove
  actions: MoveRequestActions
}

export function runMoveRequest({
  clickPosition,
  pickupAfterArrival,
  currentPlayer,
  interactionExit,
  isMoving,
  hasKeyboardInput,
  currentFloor,
  getFloorAt,
  findPath,
  waypointHeight,
  sendPlayerMove,
  actions,
}: RunMoveRequestInput) {
  const decision = decideMoveRequest({
    pickupAfterArrival,
    currentPlayerHealth: currentPlayer?.health ?? null,
    interactionExit,
    hasCurrentPlayer: currentPlayer !== null,
    isMoving,
    hasKeyboardInput,
  })

  if (decision.clearPendingPickupAfterMove) {
    actions.clearPendingPickupAfterMove()
  }

  switch (decision.kind) {
    case 'ignored':
      return
    case 'exit_pickup_and_retry':
      actions.exitPickupAndRetry()
      return
    case 'exit_object_and_delay':
      actions.exitObjectAndDelay()
      return
    case 'start':
      break
  }

  if (!currentPlayer) return

  actions.applyStartedMovement(
    startClickMovement({
      currentPos: {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      },
      clickPosition,
      pickupAfterArrival,
      currentFloor,
      getFloorAt,
      findPath,
      waypointHeight,
      sendPlayerMove,
    })
  )
}
