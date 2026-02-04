import { SvelteMap } from 'svelte/reactivity'
import type { Player } from '../stores/gameStore'
import {
  calculateMovementStep,
  initMovementState,
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

  // Move remote players toward their target positions with acceleration/deceleration
  update(deltaTime: number, otherPlayers: Map<string, Player>) {
    const dt = deltaTime / 1000 // Convert to seconds

    otherPlayers.forEach((player, playerId) => {
      if (!player.targetPosition) return

      // Get current interpolated position or initialize from player position
      const currentPlayer = this.players.get(playerId)
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
      if (result.arrived) {
        this.players.set(playerId, {
          position: result.newPos,
          state: 'idle',
          speed: 0,
          rotation: currentPlayer?.rotation ?? result.rotation,
          totalDistance: undefined,
        })
      } else {
        this.players.set(playerId, {
          position: result.newPos,
          state: 'moving',
          speed: result.newSpeed,
          rotation: result.rotation,
          totalDistance: movement.totalDistance,
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
  }

  // Reset all data
  reset() {
    this.players.clear()
    this.movementData.clear()
  }
}

export const remotePlayerManager = new PlayerStateManager()
