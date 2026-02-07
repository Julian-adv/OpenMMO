import { SvelteMap } from 'svelte/reactivity'
import type { Player } from '../stores/gameStore'
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

  // Remote player movement data (for acceleration/deceleration)
  private movementData = new SvelteMap<string, MovementState>()

  // Queue for pending attacks when player is still moving
  private attackQueue = new SvelteMap<string, string[]>()

  // Move remote players toward their target positions with acceleration/deceleration
  update(deltaTime: number, otherPlayers: Map<string, Player>) {
    const dt = deltaTime / 1000 // Convert to seconds

    // Update players
    otherPlayers.forEach((player, playerId) => {
      // Get current interpolated position or initialize from player position
      const currentPlayer = this.players.get(playerId)

      // Skip movement update if player is attacking
      if (currentPlayer?.state === 'attack') {
        return
      }

      if (!player.targetPosition) return

      let currentPos = currentPlayer?.position
      if (!currentPos) {
        currentPos = {
          x: player.position.x,
          y: player.position.y,
          z: player.position.z,
        }
      }

      const targetPos = player.targetPosition

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
        this.players.set(playerId, {
          position: result.newPos,
          state: currentState,
          speed: 0,
          rotation: currentPlayer?.rotation ?? result.rotation,
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
    this.attackQueue.delete(playerId)
  }

  // Reset all data
  reset() {
    this.players.clear()
    this.movementData.clear()
    this.attackQueue.clear()
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

  private executeAttack(playerId: string) {
    const player = this.players.get(playerId)
    if (!player) return

    // Set state to attack
    this.players.set(playerId, {
      ...player,
      state: 'attack',
    })

    // Auto-reset to idle after a short delay
    setTimeout(() => {
      const p = this.players.get(playerId)
      if (p && p.state === 'attack') {
        this.players.set(playerId, {
          ...p,
          state: 'idle',
        })
      }
    }, 1000)
  }
}

export const remotePlayerManager = new PlayerStateManager()
