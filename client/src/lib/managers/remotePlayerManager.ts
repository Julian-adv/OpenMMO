import { SvelteMap } from 'svelte/reactivity'
import type { Player } from '../stores/gameStore'

// Movement settings for remote players (should match PlayerControl)
const REMOTE_MOVEMENT_SPEED = 3 // units per second (same as local player)
const REMOTE_ACCELERATION = 6 // units per second squared
const REMOTE_DECELERATION = 6 // units per second squared
const ACCEL_DISTANCE =
  (REMOTE_MOVEMENT_SPEED * REMOTE_MOVEMENT_SPEED) / (2 * REMOTE_ACCELERATION)
const DECEL_DISTANCE =
  (REMOTE_MOVEMENT_SPEED * REMOTE_MOVEMENT_SPEED) / (2 * REMOTE_DECELERATION)
const MOVEMENT_THRESHOLD = 0.05 // Distance threshold to consider "stopped"

export interface RemotePlayerState {
  state: 'idle' | 'moving'
  speed: number
  rotation: number
}

export interface Position {
  x: number
  y: number
  z: number
}

interface MovementData {
  startPos: Position
  targetPos: Position
  totalDistance: number
  currentSpeed: number
}

class RemotePlayerManager {
  // Remote player movement states (for animation)
  states = new SvelteMap<string, RemotePlayerState>()

  // Interpolated positions for remote players (separate from store for reactivity)
  positions = new SvelteMap<string, Position>()

  // Remote player movement data (for acceleration/deceleration)
  private movementData = new SvelteMap<string, MovementData>()

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
      const targetChanged =
        !movement ||
        movement.targetPos.x !== targetPos.x ||
        movement.targetPos.y !== targetPos.y ||
        movement.targetPos.z !== targetPos.z

      if (targetChanged) {
        // New target - initialize movement from current position
        const tdx = targetPos.x - currentPos.x
        const tdy = targetPos.y - currentPos.y
        const tdz = targetPos.z - currentPos.z
        const totalDistance = Math.sqrt(tdx * tdx + tdy * tdy + tdz * tdz)

        movement = {
          startPos: { ...currentPos },
          targetPos: { x: targetPos.x, y: targetPos.y, z: targetPos.z },
          totalDistance,
          currentSpeed: movement?.currentSpeed ?? 0,
        }
        this.movementData.set(playerId, movement)
      }

      // movement is guaranteed to be defined after above block
      if (!movement) return

      // Calculate distances
      const dx = targetPos.x - currentPos.x
      const dy = targetPos.y - currentPos.y
      const dz = targetPos.z - currentPos.z
      const remainingDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

      if (remainingDistance > MOVEMENT_THRESHOLD) {
        const traveledDistance = movement.totalDistance - remainingDistance

        // Determine speed based on phase (acceleration, cruise, deceleration)
        let newSpeed = movement.currentSpeed
        if (traveledDistance < ACCEL_DISTANCE) {
          // Acceleration phase
          newSpeed = Math.min(
            newSpeed + REMOTE_ACCELERATION * dt,
            REMOTE_MOVEMENT_SPEED
          )
        } else if (remainingDistance > DECEL_DISTANCE) {
          // Cruise phase
          newSpeed = REMOTE_MOVEMENT_SPEED
        } else {
          // Deceleration phase
          newSpeed = Math.max(newSpeed - REMOTE_DECELERATION * dt, 0.1)
        }

        movement.currentSpeed = newSpeed
        this.movementData.set(playerId, movement)

        // Calculate rotation (direction of movement)
        const rotation = Math.atan2(dx, dz)

        // Move at current speed
        const moveDistance = newSpeed * dt
        let newPos
        if (moveDistance >= remainingDistance) {
          newPos = { x: targetPos.x, y: targetPos.y, z: targetPos.z }
        } else {
          const dirX = dx / remainingDistance
          const dirY = dy / remainingDistance
          const dirZ = dz / remainingDistance
          newPos = {
            x: currentPos.x + dirX * moveDistance,
            y: currentPos.y + dirY * moveDistance,
            z: currentPos.z + dirZ * moveDistance,
          }
        }

        this.positions.set(playerId, newPos)
        this.states.set(playerId, {
          state: 'moving',
          speed: newSpeed,
          rotation,
        })
      } else {
        // Arrived at destination
        this.positions.set(playerId, {
          x: targetPos.x,
          y: targetPos.y,
          z: targetPos.z,
        })

        if (movement) {
          movement.currentSpeed = 0
          this.movementData.set(playerId, movement)
        }

        this.states.set(playerId, {
          state: 'idle',
          speed: 0,
          rotation: this.states.get(playerId)?.rotation ?? 0,
        })
      }
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
