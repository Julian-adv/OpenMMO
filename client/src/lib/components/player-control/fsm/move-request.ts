import type { InteractionExitKind } from './interaction'

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
  actions: { exitPickupAndRetry: () => void; exitObjectAndDelay: () => void }
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
