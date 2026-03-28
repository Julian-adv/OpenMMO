export interface MonsterData {
  id: string
  type: string
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'walk' | 'run' | 'attack' | 'hit' | 'dead'
  ownerId?: string
  targetPosition?: { x: number; y: number; z: number }
  targetPlayerId?: string // Who the monster is attacking
  moveSpeed: number
  stateTimer: number
  hitTrigger?: number // Keep for backward compat or remove if not needed
  impactDelay?: number // Delay until hit state starts
  isLastHitSuccess?: boolean // Whether the last attack was a hit
  isDeadPending?: boolean // Death packet received, waiting for impact delay
  lastDamageInfo?: {
    damage: number
    hit: boolean
    trigger: number
  }
  pendingDamage?: number // Temporary storage for impact sync
  health: number
  maxHealth: number
  currentFloor?: number
  pathState?: {
    waypoints: Array<{ x: number; z: number; floor: number }>
    currentWaypointIndex: number
    lastPathTime: number
    lastTargetX: number
    lastTargetZ: number
  }
}
