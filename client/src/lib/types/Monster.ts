export interface MonsterData {
  id: string
  type: 'scp939'
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'walk' | 'run' | 'attack' | 'hit'
  ownerId?: string
  targetPosition?: { x: number; y: number; z: number }
  targetPlayerId?: string // Who the monster is attacking
  moveSpeed: number
  stateTimer: number
  hitTrigger?: number // Keep for backward compat or remove if not needed
  impactDelay?: number // Delay until hit state starts
  isLastHitSuccess?: boolean // Whether the last attack was a hit
}
