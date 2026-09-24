import type { Position, PlayerState } from '../../../utils/movementUtils'
import {
  buildAttackState,
  buildIdleAfterAttack,
} from '../player-state-builders'

interface AttackTargetInfo {
  state?: string
  isDeadPending?: boolean
}

interface BeginAttackInput {
  monsterId: string
  monsterInfo: AttackTargetInfo | undefined
  currentPosition: Position | null
  playerRotation: number
  previousPlayerState: PlayerState
  /** Starts combat and returns the counter for this swing. */
  beginCombat: (monsterId: string, inRange: boolean) => number
  stopAndFace: (rotation: number) => void
  sendPlayerAttack: (monsterId: string) => void
}

export type BeginAttackOutcome =
  | { kind: 'ignored_unattackable_target' }
  | {
      kind: 'started'
      nextPlayerState: PlayerState
    }

export function beginAttack({
  monsterId,
  monsterInfo,
  currentPosition,
  playerRotation,
  previousPlayerState,
  beginCombat,
  stopAndFace,
  sendPlayerAttack,
}: BeginAttackInput): BeginAttackOutcome {
  if (
    !monsterInfo ||
    monsterInfo.state === 'dead' ||
    monsterInfo.isDeadPending
  ) {
    return { kind: 'ignored_unattackable_target' }
  }

  const attackCounter = beginCombat(monsterId, true)

  if (currentPosition) {
    stopAndFace(playerRotation)
  }

  sendPlayerAttack(monsterId)

  return {
    kind: 'started',
    nextPlayerState: buildAttackState(
      previousPlayerState,
      playerRotation,
      attackCounter
    ),
  }
}

export type AttackToIdleTransition =
  | { kind: 'ignored' }
  | { kind: 'idle'; nextPlayerState: PlayerState }

export function transitionAttackToIdle(
  previousPlayerState: PlayerState
): AttackToIdleTransition {
  if (previousPlayerState.state !== 'attack') return { kind: 'ignored' }
  return {
    kind: 'idle',
    nextPlayerState: buildIdleAfterAttack(previousPlayerState),
  }
}

export type EnsureAttackStateOutcome =
  | { kind: 'ignored' }
  | { kind: 'attack'; nextPlayerState: PlayerState }

export function ensureAttackState(
  previousPlayerState: PlayerState,
  playerRotation: number,
  attackCounter: number
): EnsureAttackStateOutcome {
  if (previousPlayerState.state === 'attack') return { kind: 'ignored' }
  return {
    kind: 'attack',
    nextPlayerState: buildAttackState(
      previousPlayerState,
      playerRotation,
      attackCounter
    ),
  }
}
