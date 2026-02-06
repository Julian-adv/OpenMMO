export interface MonsterData {
    id: string
    type: 'scp939'
    position: { x: number; y: number; z: number }
    rotation: number
    state: 'idle' | 'walk' | 'run' | 'attack'
    ownerId?: string
    targetPosition?: { x: number; y: number; z: number }
    moveSpeed: number
    stateTimer: number
}
