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
  hitCounter?: number
  deadPendingTimer?: number
  lastAttackStartedAt?: number
  impactDelay?: number // Delay until hit state starts
  isLastHitSuccess?: boolean // Whether the last attack was a hit
  isDeadPending?: boolean // Death packet received, waiting for impact/hit visuals
  droppedWeaponItemDefId?: string
  lastDamageInfo?: {
    damage: number
    hit: boolean
    trigger: number
  }
  pendingSwordHitSoundUrl?: string
  // Damage number scheduled from the attack start. Captures damage/hit at
  // schedule time so a follow-up attack cannot overwrite it.
  pendingDamageText?: { delay: number; damage: number; hit: boolean }
  health: number
  maxHealth: number
  /** Wire floor_level: 0 = overworld, 1..3 housing, negative = dungeon depth. */
  floorLevel?: number
}
