import type { PlayerState, Position } from '../../../utils/movementUtils'
import { buildDeadState, buildRespawnedState } from '../player-state-builders'

// ───────────────────────────────────────────────────────────────────────────
// Runtime state reset helpers
// ───────────────────────────────────────────────────────────────────────────

export interface ControlRuntimeState {
  currentSpeed: number
}

export function resetMovementRuntimeState(): ControlRuntimeState {
  return { currentSpeed: 0 }
}

export function resetRespawnRuntimeState(): ControlRuntimeState & {
  playerRotation: number
} {
  return {
    ...resetMovementRuntimeState(),
    playerRotation: 0,
  }
}

// ───────────────────────────────────────────────────────────────────────────
// Death / respawn transitions
// ───────────────────────────────────────────────────────────────────────────

export type DeadTransitionOutcome =
  | { kind: 'ignored_already_dead' }
  | {
      kind: 'dead'
      runtime: ControlRuntimeState
      nextPlayerState: PlayerState
    }

export function transitionToDeadState(
  previousPlayerState: PlayerState
): DeadTransitionOutcome {
  if (previousPlayerState.state === 'dead') {
    return { kind: 'ignored_already_dead' }
  }

  return {
    kind: 'dead',
    runtime: resetMovementRuntimeState(),
    nextPlayerState: buildDeadState(previousPlayerState),
  }
}

export function transitionToRespawnedState(
  previousPlayerState: PlayerState,
  position: Position
): {
  runtime: ControlRuntimeState & { playerRotation: number }
  nextPlayerState: PlayerState
} {
  const runtime = resetRespawnRuntimeState()
  return {
    runtime,
    nextPlayerState: buildRespawnedState(
      previousPlayerState,
      position,
      runtime.playerRotation
    ),
  }
}
