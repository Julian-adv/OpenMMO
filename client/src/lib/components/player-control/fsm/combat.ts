import type {
  CombatUpdateResult,
  MonsterInfo,
} from '../../../managers/combatController'
import {
  initMovementState,
  type MovementState,
  type PlayerState,
  type PlayerStateName,
  type Position,
} from '../../../utils/movementUtils'
import {
  shortestWrappedDeltaX,
  shortestWrappedDeltaZ,
} from '../../../terrain/world-wrap'
import {
  buildAttackState,
  buildIdleAfterAttack,
} from '../player-state-builders'

// ───────────────────────────────────────────────────────────────────────────
// Chase target update
// ───────────────────────────────────────────────────────────────────────────

interface ApplyChaseTargetInput {
  currentPos: Position
  newTarget?: Position
  movementTarget: Position | null
  movementState: MovementState | null
  currentSpeed: number
  sendPlayerMove: (position: Position, rotation: number) => void
}

export type ChaseTargetOutcome =
  | { kind: 'unchanged' }
  | {
      kind: 'updated'
      movementTarget: Position
      movementState: MovementState
      playerRotation: number
    }

export function applyChaseTargetUpdate({
  currentPos,
  newTarget,
  movementTarget,
  movementState,
  currentSpeed,
  sendPlayerMove,
}: ApplyChaseTargetInput): ChaseTargetOutcome {
  if (!newTarget) return { kind: 'unchanged' }

  const changed =
    !movementTarget ||
    Math.abs(movementTarget.x - newTarget.x) > 0.1 ||
    Math.abs(movementTarget.z - newTarget.z) > 0.1

  if (!changed) return { kind: 'unchanged' }

  const dx = shortestWrappedDeltaX(currentPos.x, newTarget.x)
  const dz = shortestWrappedDeltaZ(currentPos.z, newTarget.z)
  const nextMovementState =
    movementState ??
    initMovementState(
      {
        x: currentPos.x,
        y: currentPos.y,
        z: currentPos.z,
      },
      newTarget,
      currentSpeed
    )

  if (movementState) {
    nextMovementState.targetPos = { ...newTarget }
    nextMovementState.totalDistance = Math.sqrt(dx * dx + dz * dz)
    nextMovementState.startPos = {
      x: currentPos.x,
      y: currentPos.y,
      z: currentPos.z,
    }
  }

  const playerRotation = Math.atan2(dx, dz)
  sendPlayerMove(newTarget, playerRotation)

  return {
    kind: 'updated',
    movementTarget: newTarget,
    movementState: nextMovementState,
    playerRotation,
  }
}

// ───────────────────────────────────────────────────────────────────────────
// Combat tick
// ───────────────────────────────────────────────────────────────────────────

export interface CombatControllerLike {
  targetMonsterId: string | null
  update(
    deltaTime: number,
    playerPos: Position,
    monsterInfo: MonsterInfo | undefined,
    monsterObjPos: Position | undefined,
    isMoving: boolean,
    cooldownMs: number,
    currentPlayerState: string
  ): CombatUpdateResult
}

export interface TickCombatInput {
  combatController: CombatControllerLike
  deltaTime: number
  playerPos: Position
  playerStateName: PlayerStateName
  isMoving: boolean
  currentSpeed: number
  movementTarget: Position | null
  movementState: MovementState | null
  cooldownMs: number
  getMonsterInfo: (monsterId: string) => MonsterInfo | undefined
  findMonsterPosition: (monsterId: string) => Position | undefined
  sendPlayerMove: (position: Position, rotation: number) => void
}

export type CombatTickOutcome =
  | { kind: 'none' }
  | { kind: 'idle' }
  | { kind: 'reached_attack_range'; monsterId: string }
  | { kind: 'chasing_unchanged' }
  | {
      kind: 'chasing_updated'
      movementTarget: Position
      movementState: MovementState
      playerRotation: number
    }
  | { kind: 'attacking'; playerRotation: number }
  | {
      kind: 'attack_cycle'
      monsterId: string
      playerRotation: number
    }

export function tickCombat({
  combatController,
  deltaTime,
  playerPos,
  playerStateName,
  isMoving,
  currentSpeed,
  movementTarget,
  movementState,
  cooldownMs,
  getMonsterInfo,
  findMonsterPosition,
  sendPlayerMove,
}: TickCombatInput): CombatTickOutcome {
  const targetId = combatController.targetMonsterId
  if (!targetId) return { kind: 'none' }

  const result = combatController.update(
    deltaTime,
    playerPos,
    getMonsterInfo(targetId),
    findMonsterPosition(targetId),
    isMoving,
    cooldownMs,
    playerStateName
  )

  switch (result.action) {
    case 'none':
      return { kind: 'none' }
    case 'idle':
      return { kind: 'idle' }
    case 'reached_attack_range':
      return { kind: 'reached_attack_range', monsterId: targetId }
    case 'chasing': {
      const chase = applyChaseTargetUpdate({
        currentPos: playerPos,
        newTarget: result.newTarget,
        movementTarget,
        movementState,
        currentSpeed,
        sendPlayerMove,
      })

      if (chase.kind === 'unchanged') return { kind: 'chasing_unchanged' }
      return {
        kind: 'chasing_updated',
        movementTarget: chase.movementTarget,
        movementState: chase.movementState,
        playerRotation: chase.playerRotation,
      }
    }
    case 'attacking':
      return { kind: 'attacking', playerRotation: result.rotation }
    case 'attack_cycle':
      return {
        kind: 'attack_cycle',
        monsterId: result.monsterId,
        playerRotation: result.rotation,
      }
    default: {
      const _exhaustive: never = result
      return _exhaustive
    }
  }
}

// ───────────────────────────────────────────────────────────────────────────
// Combat tick outcome application
// ───────────────────────────────────────────────────────────────────────────

export type CombatOutcomeApplication =
  | { kind: 'continue_movement' }
  | { kind: 'handled' }

export interface CombatOutcomeActions {
  stopMovingToIdle: () => void
  prepareReachedAttackRange: () => void
  beginAttack: (monsterId: string) => void
  setChasingMovement: (
    movementTarget: Position,
    movementState: MovementState,
    playerRotation: number
  ) => void
  showAttackState: (playerRotation: number) => void
  sendAttackCycle: (monsterId: string, playerRotation: number) => void
}

export function applyCombatTickOutcome(
  outcome: CombatTickOutcome,
  actions: CombatOutcomeActions
): CombatOutcomeApplication {
  switch (outcome.kind) {
    case 'idle':
      actions.stopMovingToIdle()
      return { kind: 'handled' }

    case 'reached_attack_range':
      actions.prepareReachedAttackRange()
      actions.beginAttack(outcome.monsterId)
      return { kind: 'handled' }

    case 'chasing_updated':
      actions.setChasingMovement(
        outcome.movementTarget,
        outcome.movementState,
        outcome.playerRotation
      )
      return { kind: 'continue_movement' }

    case 'chasing_unchanged':
    case 'none':
      return { kind: 'continue_movement' }

    case 'attacking':
      actions.showAttackState(outcome.playerRotation)
      return { kind: 'handled' }

    case 'attack_cycle':
      actions.sendAttackCycle(outcome.monsterId, outcome.playerRotation)
      return { kind: 'handled' }

    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}

// ───────────────────────────────────────────────────────────────────────────
// Combat frame (combat sub-step of the movement tick)
// ───────────────────────────────────────────────────────────────────────────

interface CombatFramePlayer {
  position: Position
}

interface RunCombatFrameInput {
  isInCombat: boolean
  combatController: CombatControllerLike
  deltaTime: number
  currentPlayer: CombatFramePlayer | null
  playerStateName: PlayerStateName
  isMoving: boolean
  currentSpeed: number
  movementTarget: Position | null
  movementState: MovementState | null
  cooldownMs: number
  getMonsterInfo: (monsterId: string) => MonsterInfo | undefined
  findMonsterPosition: (monsterId: string) => Position | undefined
  sendPlayerMove: (position: Position, rotation: number) => void
  actions: CombatOutcomeActions
}

export function runCombatFrame({
  isInCombat,
  combatController,
  deltaTime,
  currentPlayer,
  playerStateName,
  isMoving,
  currentSpeed,
  movementTarget,
  movementState,
  cooldownMs,
  getMonsterInfo,
  findMonsterPosition,
  sendPlayerMove,
  actions,
}: RunCombatFrameInput): CombatOutcomeApplication {
  if (!isInCombat || !currentPlayer) return { kind: 'continue_movement' }

  const combat = tickCombat({
    combatController,
    deltaTime,
    playerPos: {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    },
    playerStateName,
    isMoving,
    currentSpeed,
    movementTarget,
    movementState,
    cooldownMs,
    getMonsterInfo,
    findMonsterPosition,
    sendPlayerMove,
  })

  return applyCombatTickOutcome(combat, actions)
}

// ───────────────────────────────────────────────────────────────────────────
// Attack state transitions
// ───────────────────────────────────────────────────────────────────────────

export interface AttackTargetInfo {
  state?: string
  isDeadPending?: boolean
}

interface BeginAttackInput {
  monsterId: string
  monsterInfo: AttackTargetInfo | undefined
  currentPosition: Position | null
  playerRotation: number
  previousPlayerState: PlayerState
  lastSentPosition: Position | null
  beginCombat: (monsterId: string, inRange: boolean) => void
  sendPlayerMove: (position: Position, rotation: number) => void
  sendPlayerAttack: (monsterId: string) => void
}

export type BeginAttackOutcome =
  | { kind: 'ignored_dead_target' }
  | {
      kind: 'started'
      nextPlayerState: PlayerState
      pendingPickupAfterMoveInstanceId: null
    }

export function beginAttack({
  monsterId,
  monsterInfo,
  currentPosition,
  playerRotation,
  previousPlayerState,
  lastSentPosition,
  beginCombat,
  sendPlayerMove,
  sendPlayerAttack,
}: BeginAttackInput): BeginAttackOutcome {
  if (monsterInfo?.state === 'dead' || monsterInfo?.isDeadPending) {
    return { kind: 'ignored_dead_target' }
  }

  beginCombat(monsterId, true)

  if (currentPosition) {
    const shouldSendMove =
      !lastSentPosition ||
      Math.abs(currentPosition.x - lastSentPosition.x) > 0.01 ||
      Math.abs(currentPosition.z - lastSentPosition.z) > 0.01

    if (shouldSendMove) {
      sendPlayerMove(currentPosition, playerRotation)
    }
  }

  sendPlayerAttack(monsterId)

  return {
    kind: 'started',
    nextPlayerState: buildAttackState(previousPlayerState),
    pendingPickupAfterMoveInstanceId: null,
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
  playerRotation: number
): EnsureAttackStateOutcome {
  if (previousPlayerState.state === 'attack') return { kind: 'ignored' }
  return {
    kind: 'attack',
    nextPlayerState: buildAttackState(previousPlayerState, playerRotation),
  }
}
