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
} from '../utils/movementUtils'

export interface RemotePlayerState {
  state: 'idle' | 'moving'
  speed: number
  rotation: number
}

export type { Position }

// Use the same movement config as local player
const MOVEMENT_CONFIG: MovementConfig = {
  ...DEFAULT_MOVEMENT_CONFIG,
}

class RemotePlayerManager {
  // Remote player movement states (for animation)
  states = new SvelteMap<string, RemotePlayerState>()

  // Interpolated positions for remote players (separate from store for reactivity)
  positions = new SvelteMap<string, Position>()

  // Remote player movement data (for acceleration/deceleration)
  private movementData = new SvelteMap<string, MovementState>()

  // Move remote players toward their target positions with acceleration/deceleration
  update(deltaTime: number, otherPlayers: Map<string, Player>) {
    const dt = deltaTime / 1000 // Convert to seconds

    otherPlayers.forEach((player, playerId) => {
      if (!player.targetPosition) return

      // Get current interpolated position or initialize from player position
      let currentPos = this.positions.get(playerId)
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

      // Update position
      this.positions.set(playerId, result.newPos)

      // Update state for animation
      if (result.arrived) {
        this.states.set(playerId, {
          state: 'idle',
          speed: 0,
          rotation: this.states.get(playerId)?.rotation ?? result.rotation,
        })
      } else {
        this.states.set(playerId, {
          state: 'moving',
          speed: result.newSpeed,
          rotation: result.rotation,
        })
      }
    })
  }

  // Initialize remote player state with position and rotation
  initPlayer(playerId: string, position: Position, rotation: number) {
    this.positions.set(playerId, { ...position })
    this.states.set(playerId, {
      state: 'idle',
      speed: 0,
      rotation,
    })
  }

  // Clean up data for players that have left
  removePlayer(playerId: string) {
    this.states.delete(playerId)
    this.positions.delete(playerId)
    this.movementData.delete(playerId)
  }

  // Reset all data
  reset() {
    this.states.clear()
    this.positions.clear()
    this.movementData.clear()
  }
}

export const remotePlayerManager = new RemotePlayerManager()
