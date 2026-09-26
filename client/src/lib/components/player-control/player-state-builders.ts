import type { Position, PlayerState } from '../../utils/movementUtils'

/** Used after exiting both pickup and object interactions — same shape. */
export function buildIdleAfterInteract(prev: PlayerState): PlayerState {
  return {
    ...prev,
    state: 'idle',
    speed: 0,
    interactionAnim: undefined,
    interactOffsetY: undefined,
  }
}

/** A new counter restarts the swing animation. */
export function buildAttackState(
  prev: PlayerState,
  rotation: number,
  attackCounter: number
): PlayerState {
  return {
    ...prev,
    state: 'attack',
    speed: 0,
    movementMode: undefined,
    rotation,
    attackCounter,
  }
}

export function buildIdleAfterAttack(prev: PlayerState): PlayerState {
  return { ...prev, state: 'idle', attackCounter: 0 }
}

export function buildDeadState(prev: PlayerState): PlayerState {
  return {
    ...prev,
    state: 'dead',
    speed: 0,
    movementMode: undefined,
  }
}

export function buildRespawnedState(
  prev: PlayerState,
  position: Position,
  rotation: number
): PlayerState {
  return {
    ...prev,
    state: 'idle',
    speed: 0,
    rotation,
    movementMode: undefined,
    attackCounter: 0,
    position,
  }
}

export function buildInteractState(
  prev: PlayerState,
  position: Position,
  rotation: number,
  anim: string,
  offsetY: number
): PlayerState {
  return {
    ...prev,
    state: 'interact',
    speed: 0,
    rotation,
    position,
    interactionAnim: anim,
    // Bump so repeating the same anim still re-triggers (see attackCounter).
    interactionCounter: (prev.interactionCounter ?? 0) + 1,
    interactOffsetY: offsetY,
  }
}

export function buildPickupState(prev: PlayerState): PlayerState {
  return buildInteractState(prev, prev.position, prev.rotation, 'pickup', 0)
}
