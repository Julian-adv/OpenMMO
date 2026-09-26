import {
  getMovementMode,
  type PlayerState,
  type Position,
} from '../../../utils/movementUtils'

const SEARCH_ANIMATION_GRACE_MS = 120

export interface PlayerStateProjectionInput {
  currentPosition: Position
  isMoving: boolean
  currentSpeed: number
  playerRotation: number
  totalDistance?: number
  hasTorch: boolean
  isInCombat: boolean
  attackCounter: number
  isSprinting: boolean
  previousState?: PlayerState
  searchElapsedMs?: number | null
}

export function projectPlayerState({
  currentPosition,
  isMoving,
  currentSpeed,
  playerRotation,
  totalDistance,
  hasTorch,
  isInCombat,
  attackCounter,
  isSprinting,
  previousState,
  searchElapsedMs,
}: PlayerStateProjectionInput): PlayerState {
  const searchMovementMode =
    currentSpeed === 0 &&
    searchElapsedMs != null &&
    searchElapsedMs >= 0 &&
    searchElapsedMs < SEARCH_ANIMATION_GRACE_MS &&
    previousState?.state === 'moving'
      ? previousState.movementMode
      : undefined
  const movementMode =
    isMoving && currentSpeed > 0
      ? getMovementMode(totalDistance, hasTorch, isSprinting, isInCombat)
      : searchMovementMode

  return {
    state: movementMode !== undefined ? 'moving' : 'idle',
    speed: currentSpeed,
    rotation: playerRotation,
    position: currentPosition,
    movementMode,
    attackCounter: isInCombat ? attackCounter : undefined,
  }
}

export function projectStoppedPlayerState(
  previousState: PlayerState,
  position: Position,
  rotation: number
): PlayerState {
  return {
    ...previousState,
    state: previousState.state === 'moving' ? 'idle' : previousState.state,
    position,
    rotation:
      previousState.state === 'attack' ? previousState.rotation : rotation,
    speed: 0,
    movementMode: undefined,
  }
}

export function shouldEmitProjectedPlayerState(
  previousState: PlayerState,
  nextState: PlayerState
): boolean {
  return (
    nextState.state !== previousState.state ||
    Math.abs(nextState.speed - previousState.speed) > 0.01 ||
    nextState.rotation !== previousState.rotation ||
    Math.abs(nextState.position.x - previousState.position.x) > 0.01 ||
    Math.abs(nextState.position.z - previousState.position.z) > 0.01 ||
    nextState.movementMode !== previousState.movementMode ||
    nextState.attackCounter !== previousState.attackCounter
  )
}
