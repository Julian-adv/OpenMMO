import type { Position } from '../utils/movementUtils'
import { startBattleMusic, stopBattleMusic } from './bgmManager'
import { createEvent } from '../network/networkEvents'

export interface MonsterInfo {
  state?: string
  isDeadPending?: boolean
  health?: number
}

export type CombatUpdateResult =
  | { action: 'none' }
  | { action: 'idle' }
  | { action: 'chasing'; newTarget?: Position }
  | { action: 'reached_attack_range' }
  | { action: 'attacking'; rotation: number }
  | { action: 'attack_cycle'; rotation: number }

export interface CombatStart {
  attackCounter: number
  startRequested: boolean
}

/** Out-of-range retries per target; a landed swing refills them. */
const RANGE_RETRIES = 2
/** Retries swing from this much closer so the same view lag cannot miss again. */
const RETRY_CLOSER_M = 0.5

export class CombatController {
  private _targetMonsterId: string | null = null
  private _attackTimer = 0
  private _attackCounter = 0
  private _lastChaseUpdate = 0
  private _attackRequested = false
  private _attackRequestId = 0
  private _pendingSwing = false
  private _serverStopped = false
  private _rejected: { monsterId: string; reason: string } | null = null
  private _rangeRejected = false
  private _rangeRetries = RANGE_RETRIES
  readonly attackStopRequested = createEvent<(requestId: number) => void>()

  get attackRequested(): boolean {
    return this._attackRequested
  }

  get attackRequestId(): number {
    return this._attackRequestId
  }

  get targetMonsterId(): string | null {
    return this._targetMonsterId
  }

  get attackCounter(): number {
    return this._attackCounter
  }

  get isInCombat(): boolean {
    return this._targetMonsterId !== null
  }

  getAbilityTarget(
    hoveredMonsterId: string | null,
    getMonster: (id: string) => MonsterInfo | undefined
  ): string | null {
    for (const id of [hoveredMonsterId, this._targetMonsterId]) {
      if (!id) continue
      const monster = getMonster(id)
      if (
        monster &&
        monster.state !== 'dead' &&
        !monster.isDeadPending &&
        (monster.health === undefined || monster.health > 0)
      )
        return id
    }
    return null
  }

  beginCombat(monsterId: string, inRange: boolean): CombatStart {
    const wasInCombat = this.isInCombat
    const targetChanged = monsterId !== this._targetMonsterId
    const startRequested = inRange && (targetChanged || !this._attackRequested)
    if (!inRange) {
      this.stopAttack(false)
      this._lastChaseUpdate = Date.now()
    }
    if (targetChanged) {
      this._attackCounter = 0
      this._attackTimer = 0
      this._pendingSwing = false
      this.resetRangeRetries()
    }
    this._targetMonsterId = monsterId
    this._serverStopped = false
    if (startRequested) {
      this._attackRequested = true
      this._attackRequestId = (this._attackRequestId + 1) >>> 0
    }
    if (!wasInCombat) startBattleMusic()
    return { attackCounter: this._attackCounter, startRequested }
  }

  /** A new attack request even when one on `monsterId` is running, so the
   *  server's stop for the old one cannot end it. */
  restartCombat(monsterId: string): CombatStart {
    this.stopAttack(false)
    return this.beginCombat(monsterId, true)
  }

  private stopAttack(notifyServer = true) {
    if (!this._attackRequested) return
    this._attackRequested = false
    if (notifyServer) this.attackStopRequested.emit(this._attackRequestId)
  }

  cancelCombat({ notifyServer = true } = {}) {
    const wasInCombat = this.isInCombat
    this.stopAttack(notifyServer)
    this._targetMonsterId = null
    this._attackCounter = 0
    this._attackTimer = 0
    this._pendingSwing = false
    this._serverStopped = false
    this.resetRangeRetries()
    if (wasInCombat) stopBattleMusic()
  }

  private resetRangeRetries() {
    this._rangeRejected = false
    this._rangeRetries = RANGE_RETRIES
  }

  attackConfirmed(monsterId: string) {
    if (!this._attackRequested || monsterId !== this._targetMonsterId) return
    this._attackCounter++
    this._attackTimer = 0
    this._pendingSwing = true
    this.resetRangeRetries()
  }

  attackRejected(monsterId: string, reason: string) {
    this._rejected = { monsterId, reason }
  }

  attackStopped(monsterId: string, requestId: number) {
    // The server follows a rejection with the stop for that request. A chase
    // drops the request locally, but a rejected one must still end combat, or
    // reaching range re-sends it every goal interval.
    const rejected =
      this._rejected?.monsterId === monsterId ? this._rejected.reason : null
    this._rejected = null
    if (
      (!this._attackRequested && !rejected) ||
      monsterId !== this._targetMonsterId ||
      requestId !== this._attackRequestId
    )
      return
    this.stopAttack(false)
    if (rejected === 'out_of_range' && this._rangeRetries > 0) {
      this._rangeRetries--
      this._rangeRejected = true
      return
    }
    this._serverStopped = true
  }

  private startChase(
    monsterObjPos: Position,
    now = Date.now()
  ): CombatUpdateResult {
    this.stopAttack(false)
    this._lastChaseUpdate = now
    return {
      action: 'chasing',
      newTarget: {
        x: monsterObjPos.x,
        y: monsterObjPos.y,
        z: monsterObjPos.z,
      },
    }
  }

  update(
    deltaTime: number,
    playerPos: Position,
    monsterInfo: MonsterInfo | undefined,
    monsterObjPos: Position | undefined,
    isMoving: boolean,
    cooldownMs: number,
    currentPlayerState: string,
    lineBlocked: boolean,
    attackRange: number
  ): CombatUpdateResult {
    if (!this._targetMonsterId) return { action: 'none' }
    if (this._serverStopped) {
      this.cancelCombat({ notifyServer: false })
      return { action: 'idle' }
    }

    const isFinishingAttack =
      currentPlayerState === 'attack' &&
      this._attackCounter > 0 &&
      this._attackTimer < cooldownMs

    if (!monsterInfo || (monsterInfo.state === 'dead' && !isFinishingAttack)) {
      this.cancelCombat()
      return { action: 'idle' }
    }

    if (!monsterObjPos) {
      this.cancelCombat()
      return { action: 'idle' }
    }

    if (this._rangeRejected && !isFinishingAttack) {
      this._rangeRejected = false
      // A wall we can see explains the rejection; chasing would not help.
      if (lineBlocked) {
        this.cancelCombat({ notifyServer: false })
        return { action: 'idle' }
      }
      return this.startChase(monsterObjPos)
    }

    const reach =
      this._rangeRetries < RANGE_RETRIES
        ? attackRange - RETRY_CLOSER_M
        : attackRange
    const dx = monsterObjPos.x - playerPos.x
    const dz = monsterObjPos.z - playerPos.z
    const dist = Math.sqrt(dx * dx + dz * dz)
    const inRange = dist <= reach && !lineBlocked

    if (isMoving) {
      if (inRange) {
        return { action: 'reached_attack_range' }
      }

      const now = Date.now()
      if (now - this._lastChaseUpdate >= 1000) {
        return this.startChase(monsterObjPos, now)
      }
      return { action: 'chasing' }
    }

    if (!inRange && !isFinishingAttack) {
      return this.startChase(monsterObjPos)
    }

    const rotation = Math.atan2(dx, dz)
    this._attackTimer += deltaTime

    if (this._pendingSwing) {
      this._pendingSwing = false
      return { action: 'attack_cycle', rotation }
    }
    if (!inRange && this._attackTimer >= cooldownMs) {
      return this.startChase(monsterObjPos)
    }
    if (monsterInfo.state === 'dead' && this._attackTimer >= cooldownMs) {
      this.cancelCombat()
      return { action: 'idle' }
    }

    return { action: 'attacking', rotation }
  }
}

export const combatController = new CombatController()
