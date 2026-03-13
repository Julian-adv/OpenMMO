import { SvelteMap } from 'svelte/reactivity'
import {
  calculateMovementStep,
  initMovementState,
  getMovementMode,
  hasTargetChanged,
  DEFAULT_MOVEMENT_CONFIG,
  type Position,
  type MovementState,
  type MovementConfig,
  type PlayerState,
} from '../utils/movementUtils'

// Use the same movement config as local player
const MOVEMENT_CONFIG: MovementConfig = {
  ...DEFAULT_MOVEMENT_CONFIG,
}

class PlayerStateManager {
  players = new SvelteMap<string, PlayerState>()

  // Attack animation duration in seconds (updated from actual animation data)
  attackAnimationDuration = 1.0

  // Remote player movement data (for acceleration/deceleration)
  private movementData = new SvelteMap<string, MovementState>()

  // Server-authoritative movement targets for each remote player.
  private targetPositions = new SvelteMap<string, Position>()

  // Server-authoritative target rotation for each remote player.
  private targetRotations = new SvelteMap<string, number>()

  // Queue for pending attacks when player is still moving
  private attackQueue = new SvelteMap<string, string[]>()

  // Buffered position/rotation received during attack animation (1-slot queue).
  // Applied when the attack ends.
  private pendingMove = new Map<
    string,
    { position: Position; rotation: number }
  >()

  // Timestamp (performance.now()) when each player's attack animation started
  private attackStartTimes = new Map<string, number>()

  // Move remote players toward their target positions with acceleration/deceleration
  update(deltaTime: number) {
    const dt = deltaTime / 1000 // Convert to seconds
    const now = performance.now()

    // Check for attack animations that have finished
    this.attackStartTimes.forEach((startTime, playerId) => {
      const elapsed = (now - startTime) / 1000
      if (elapsed < this.attackAnimationDuration) return

      this.attackStartTimes.delete(playerId)

      const p = this.players.get(playerId)
      if (!p || p.state !== 'attack') return

      const pending = this.pendingMove.get(playerId)
      this.pendingMove.delete(playerId)

      if (pending) {
        // Apply buffered position/rotation from move received during attack
        this.targetPositions.set(playerId, pending.position)
        this.targetRotations.set(playerId, pending.rotation)
        this.players.set(playerId, {
          ...p,
          state: 'idle',
          rotation: pending.rotation,
        })
      } else {
        this.players.set(playerId, {
          ...p,
          state: 'idle',
        })
      }
    })

    // Update players
    this.targetPositions.forEach((targetPos, playerId) => {
      // Get current interpolated position or initialize from player position
      const currentPlayer = this.players.get(playerId)
      if (!currentPlayer) return

      // Skip movement update if player is attacking or dead
      if (
        currentPlayer?.state === 'attack' ||
        currentPlayer?.state === 'dead'
      ) {
        return
      }

      const currentPos = currentPlayer.position

      // Get or initialize movement data
      let movement = this.movementData.get(playerId)
      const targetChanged = hasTargetChanged(movement, targetPos)

      if (targetChanged) {
        // New target - initialize movement from current position
        movement = initMovementState(
          currentPos,
          targetPos,
          movement?.currentSpeed ?? 0
        )
        this.movementData.set(playerId, movement)
      }

      // movement is guaranteed to be defined after above block
      if (!movement) return

      // Calculate movement step
      const result = calculateMovementStep(
        currentPos,
        movement,
        MOVEMENT_CONFIG,
        dt
      )

      // Update movement state
      movement.currentSpeed = result.newSpeed
      this.movementData.set(playerId, movement)

      // Update player
      // Since we skip movement update if player is already attacking,
      // currentState will just be based on whether they arrived
      const currentState = result.arrived ? 'idle' : 'moving'

      if (result.arrived) {
        // Use server-authoritative rotation on arrival (handles face-only packets)
        const targetRotation = this.targetRotations.get(playerId)
        this.players.set(playerId, {
          position: result.newPos,
          state: currentState,
          speed: 0,
          rotation:
            targetRotation ?? currentPlayer?.rotation ?? result.rotation,
          movementMode: undefined,
        })

        // Check for queued attacks upon arrival
        const queue = this.attackQueue.get(playerId)
        if (queue && queue.length > 0) {
          console.log(
            `[RemotePlayerManager] Executing queued attack for ${playerId} upon arrival`
          )
          queue.shift() // Consume one attack
          if (queue.length === 0) {
            this.attackQueue.delete(playerId)
          } else {
            this.attackQueue.set(playerId, queue)
          }
          this.executeAttack(playerId)
        }
      } else {
        // Determine movement mode based on distance
        const movementMode = getMovementMode(movement.totalDistance)

        this.players.set(playerId, {
          position: result.newPos,
          state: currentState,
          speed: result.newSpeed,
          rotation: result.rotation,
          movementMode,
        })
      }
    })
  }

  // Initialize remote player state with position and rotation
  initPlayer(playerId: string, position: Position, rotation: number) {
    this.targetPositions.set(playerId, { ...position })
    this.players.set(playerId, {
      position: { ...position },
      state: 'idle',
      speed: 0,
      rotation,
    })
  }

  // Clean up data for players that have left
  removePlayer(playerId: string) {
    this.players.delete(playerId)
    this.movementData.delete(playerId)
    this.targetPositions.delete(playerId)
    this.targetRotations.delete(playerId)
    this.attackQueue.delete(playerId)
    this.pendingMove.delete(playerId)
    this.attackStartTimes.delete(playerId)
  }

  // Reset all data
  reset() {
    this.players.clear()
    this.movementData.clear()
    this.targetPositions.clear()
    this.targetRotations.clear()
    this.attackQueue.clear()
    this.pendingMove.clear()
    this.attackStartTimes.clear()
  }

  handleDead(playerId: string) {
    const player = this.players.get(playerId)
    if (!player) return

    this.attackStartTimes.delete(playerId)
    this.players.set(playerId, {
      ...player,
      state: 'dead',
      speed: 0,
    })
  }

  handleRespawn(playerId: string, position: Position, rotation: number) {
    this.movementData.delete(playerId)
    this.attackQueue.delete(playerId)
    this.attackStartTimes.delete(playerId)
    this.targetPositions.set(playerId, { ...position })
    this.players.set(playerId, {
      position: { ...position },
      state: 'idle',
      speed: 0,
      rotation,
    })
  }

  teleportPlayer(playerId: string, position: Position, rotation: number) {
    this.targetPositions.set(playerId, { ...position })
    this.movementData.delete(playerId)
    this.players.set(playerId, {
      position: { ...position },
      state: 'idle',
      speed: 0,
      rotation,
    })
  }

  handleAttack(playerId: string) {
    const player = this.players.get(playerId)
    if (!player) return

    const movement = this.movementData.get(playerId)
    const isMoving = movement && movement.currentSpeed > 0.01

    if (isMoving) {
      // Still moving - queue the attack
      console.log(`[RemotePlayerManager] Queueing attack for ${playerId}`)
      const queue = this.attackQueue.get(playerId) || []
      queue.push('attack') // Currently monsterId isn't stored in PlayerState, so just queue an 'attack' event
      this.attackQueue.set(playerId, queue)
    } else {
      // Not moving - execute immediately
      this.executeAttack(playerId)
    }
  }

  setTargetPosition(
    playerId: string,
    targetPosition: Position,
    rotation: number
  ) {
    const player = this.players.get(playerId)

    // During attack animation, buffer the move for after the animation ends.
    if (player?.state === 'attack') {
      this.pendingMove.set(playerId, {
        position: { ...targetPosition },
        rotation,
      })
      return
    }

    this.targetPositions.set(playerId, { ...targetPosition })
    this.targetRotations.set(playerId, rotation)
  }

  private executeAttack(playerId: string) {
    const player = this.players.get(playerId)
    if (!player) return

    // Set state to attack and record start time for update() to check
    this.players.set(playerId, {
      ...player,
      state: 'attack',
      attackCounter: (player.attackCounter ?? 0) + 1,
    })
    this.attackStartTimes.set(playerId, performance.now())
  }
}

export const remotePlayerManager = new PlayerStateManager()
