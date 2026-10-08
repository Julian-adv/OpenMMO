import type { Position, PlayerState } from '../../../utils/movementUtils'
import type { CombatStart } from '../../../managers/combatController'
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
  beginCombat: (monsterId: string, inRange: boolean) => CombatStart
  stopAndFace: (rotation: number) => void
  startPlayerAttack: (monsterId: string) => void
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
  startPlayerAttack,
}: BeginAttackInput): BeginAttackOutcome {
  if (
    !monsterInfo ||
    monsterInfo.state === 'dead' ||
    monsterInfo.isDeadPending
  ) {
    return { kind: 'ignored_unattackable_target' }
  }

  const { attackCounter, startRequested } = beginCombat(monsterId, true)

  if (currentPosition) {
    stopAndFace(playerRotation)
  }

  if (startRequested) startPlayerAttack(monsterId)

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
