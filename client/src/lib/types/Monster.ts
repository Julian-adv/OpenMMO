import type { PlayerDamageInfo } from '../stores/gameStore'

export interface MonsterData {
  id: string
  type: string
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'walk' | 'run' | 'attack' | 'hit' | 'dead'
  targetPosition?: { x: number; y: number; z: number }
  syncCorrection?: { x: number; z: number } // server offset still to absorb
  targetPlayerId?: number // Who the monster is attacking
  chaseAim?: { playerId: number; stopRange: number } // engage-ring chase leg from the last Move sync
  moveSpeed: number
  attackCounter?: number
  attackFromLeft?: boolean // Last swing's target was on the monster's left
  /** The telegraphed skill under way; `counter` restarts its clip. */
  skill?: {
    id: string
    counter: number
    startedAt: number
    rotation: number
    range: number
    halfAngle: number
  }
  hitCounter?: number
  /** `performance.now()` until which Primal Roar's dizzy marker shows. */
  stunnedUntil?: number
  /** Valhalla Charge: moves at `slowMult` until this `performance.now()`. */
  slowedUntil?: number
  slowMult?: number
  deadPendingTimer?: number
  lastAttackStartedAt?: number
  impactDelay?: number // Delay until hit state starts
  isLastHitSuccess?: boolean // Whether the last attack was a hit
  isDeadPending?: boolean // Death packet received, waiting for impact/hit visuals
  droppedWeaponItemDefId?: string
  lastDamageInfo?: PlayerDamageInfo
  pendingSwordHitSoundUrl?: string
  pendingDamageText?: Omit<PlayerDamageInfo, 'trigger'> & { delay: number }
  health: number
  maxHealth: number
  /** Wire floor_level: 0 = overworld, 1..3 housing, negative = dungeon depth. */
  floorLevel?: number
  /** Stands frozen as stone, nameless, until it wakes. */
  dormant?: boolean
  /** `Date.now()` of its last waking; it shakes off the stone from here. */
  wokeAt?: number
  /** A raid boss going back to sleep sinks from this `Date.now()`; the server
   *  has already removed it, so it stays only until the sink ends. */
  sinkAt?: number
}
