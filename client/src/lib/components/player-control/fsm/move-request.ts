import {
  initMovementState,
  type MovementState,
  type Position,
} from '../../../utils/movementUtils'
import type { InteractionExitKind } from './interaction'
import {
  routeFirstLeg,
  type Pathing,
  type PathWaypoint,
  type SendPlayerMove,
} from './movement-substrate'

export type MoveRequestDecision =
  | { kind: 'ignored' }
  | { kind: 'exit_pickup_and_retry' }
  | { kind: 'exit_object_and_delay' }
  | { kind: 'start' }

interface DecideMoveRequestInput {
  currentPlayerHealth: number | null
  interactionExit: InteractionExitKind
  hasCurrentPlayer: boolean
  hasKeyboardInput: boolean
}

export function decideMoveRequest({
  currentPlayerHealth,
  interactionExit,
  hasCurrentPlayer,
  hasKeyboardInput,
}: DecideMoveRequestInput): MoveRequestDecision {
  if (currentPlayerHealth !== null && currentPlayerHealth <= 0) {
    return { kind: 'ignored' }
  }

  if (interactionExit === 'pickup') return { kind: 'exit_pickup_and_retry' }
  if (interactionExit === 'object') return { kind: 'exit_object_and_delay' }

  if (!hasCurrentPlayer || hasKeyboardInput) {
    return { kind: 'ignored' }
  }

  return { kind: 'start' }
}

export function prepareMoveRequest(
  input: DecideMoveRequestInput,
  actions: Pick<MoveRequestActions, 'exitPickupAndRetry' | 'exitObjectAndDelay'>
): boolean {
  switch (decideMoveRequest(input).kind) {
    case 'ignored':
      return false
    case 'exit_pickup_and_retry':
      actions.exitPickupAndRetry()
      return false
    case 'exit_object_and_delay':
      actions.exitObjectAndDelay()
      return false
    case 'start':
      return true
  }
}

interface StartClickMovementInput extends Pathing {
  currentPos: Position
  clickPosition: Position
  sendPlayerMove: SendPlayerMove
  /** Carry the current speed so a mid-run redirect doesn't restart at 0. */
  startSpeed: number
}

export interface StartedClickMovement {
  pathWaypoints: PathWaypoint[]
  currentWaypointIndex: number
  movementState: MovementState
  movementTarget: Position
  playerRotation: number
}

export function startClickMovement({
  currentPos,
  clickPosition,
  sendPlayerMove,
  startSpeed,
  ...pathing
}: StartClickMovementInput): StartedClickMovement | null {
  const leg = routeFirstLeg(currentPos, clickPosition, pathing, sendPlayerMove)
  if (!leg) return null
  return {
    ...leg,
    currentWaypointIndex: 0,
    movementState: initMovementState(
      currentPos,
      leg.movementTarget,
      startSpeed
    ),
  }
}

interface MoveRequestPlayer {
  health: number
  position: Position
}

export interface MoveRequestActions {
  exitPickupAndRetry: () => void
  exitObjectAndDelay: () => void
  cancelBlockedMovement: () => void
  applyStartedMovement: (started: StartedClickMovement) => void
}

interface RunMoveRequestInput extends Pathing {
  clickPosition: Position
  currentPlayer: MoveRequestPlayer | null
  interactionExit: InteractionExitKind
  hasKeyboardInput: boolean
  sendPlayerMove: SendPlayerMove
  startSpeed: number
  actions: MoveRequestActions
}

export function runMoveRequest({
  clickPosition,
  currentPlayer,
  interactionExit,
  hasKeyboardInput,
  currentFloor,
  getFloorAt,
  findPath,
  waypointHeight,
  sendPlayerMove,
  startSpeed,
  actions,
}: RunMoveRequestInput) {
  if (
    !prepareMoveRequest(
      {
        currentPlayerHealth: currentPlayer?.health ?? null,
        interactionExit,
        hasCurrentPlayer: currentPlayer !== null,
        hasKeyboardInput,
      },
      actions
    ) ||
    !currentPlayer
  )
    return

  const started = startClickMovement({
    currentPos: {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    },
    clickPosition,
    currentFloor,
    getFloorAt,
    findPath,
    waypointHeight,
    sendPlayerMove,
    startSpeed,
  })
  if (started) actions.applyStartedMovement(started)
  else actions.cancelBlockedMovement()
}
