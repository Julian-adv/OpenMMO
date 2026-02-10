import type { Position } from '../utils/movementUtils'

export interface MonsterInfo {
  state?: string
  isDeadPending?: boolean
}

export type CombatUpdateResult =
  | { action: 'none' }
  | { action: 'idle' }
  | { action: 'chasing'; newTarget?: Position }
  | { action: 'reached_attack_range' }
  | { action: 'attacking'; rotation: number }
  | { action: 'attack_cycle'; monsterId: string; rotation: number }

class CombatController {
  private _targetMonsterId: string | null = null
  private _attackTimer = 0
  private _attackCounter = 0
  private _lastChaseUpdate = 0

  get targetMonsterId(): string | null {
    return this._targetMonsterId
  }

  get attackCounter(): number {
    return this._attackCounter
  }

  get isInCombat(): boolean {
    return this._targetMonsterId !== null
  }

  beginCombat(monsterId: string, inRange: boolean) {
    this._targetMonsterId = monsterId
    this._attackTimer = 0
    if (inRange) {
      this._attackCounter = 1
    } else {
      this._attackCounter = 0
      this._lastChaseUpdate = Date.now()
    }
  }

  cancelCombat() {
    this._targetMonsterId = null
    this._attackCounter = 0
    this._attackTimer = 0
  }

  update(
    deltaTime: number,
    playerPos: Position,
    monsterInfo: MonsterInfo | undefined,
    monsterObjPos: Position | undefined,
    isMoving: boolean,
    cooldownMs: number,
    currentPlayerState: string
  ): CombatUpdateResult {
    if (!this._targetMonsterId) return { action: 'none' }

    const isFinishingAttack =
      currentPlayerState === 'attack' && this._attackTimer < cooldownMs

    // Monster data missing or dead (and not finishing attack)
    if (!monsterInfo || (monsterInfo.state === 'dead' && !isFinishingAttack)) {
      this._targetMonsterId = null
      this._attackCounter = 0
      this._attackTimer = 0
      return { action: 'idle' }
    }

    // Monster mesh not found
    if (!monsterObjPos) {
      this._targetMonsterId = null
      this._attackCounter = 0
      return { action: 'idle' }
    }

    const dx = monsterObjPos.x - playerPos.x
    const dz = monsterObjPos.z - playerPos.z
    const dist = Math.sqrt(dx * dx + dz * dz)

    if (isMoving) {
      // CHASING phase
      if (dist < 2.0) {
        return { action: 'reached_attack_range' }
      }

      // Throttled chase target update
      const now = Date.now()
      if (now - this._lastChaseUpdate >= 1000) {
        this._lastChaseUpdate = now
        return {
          action: 'chasing',
          newTarget: { x: monsterObjPos.x, y: 0, z: monsterObjPos.z },
        }
      }
      return { action: 'chasing' }
    }

    // COMBAT phase (in range)
    if (dist > 2.5 && !isFinishingAttack) {
      this._targetMonsterId = null
      this._attackCounter = 0
      return { action: 'idle' }
    }

    // Still in range - rotate and attack
    const rotation = Math.atan2(dx, dz)
    this._attackTimer += deltaTime

    const isMonsterAlive =
      monsterInfo.state !== 'dead' && !monsterInfo.isDeadPending

    if (this._attackTimer >= cooldownMs) {
      if (isMonsterAlive) {
        this._attackTimer = 0
        this._attackCounter++
        return {
          action: 'attack_cycle',
          monsterId: this._targetMonsterId,
          rotation,
        }
      } else {
        this._targetMonsterId = null
        this._attackCounter = 0
        this._attackTimer = 0
        return { action: 'idle' }
      }
    }

    return { action: 'attacking', rotation }
  }
}

export const combatController = new CombatController()
