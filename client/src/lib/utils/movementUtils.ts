// Common movement calculation utilities shared between local and remote players

export interface Position {
  x: number
  y: number
  z: number
}

export interface MovementConfig {
  maxSpeed: number
  acceleration: number
  deceleration: number
  arrivalThreshold: number
}

export interface MovementState {
  currentSpeed: number
  startPos: Position
  targetPos: Position
  totalDistance: number
}

export interface MovementResult {
  newPos: Position
  newSpeed: number
  rotation: number
  arrived: boolean
}

export interface PlayerState {
  position: Position
  state: 'idle' | 'moving'
  speed: number
  rotation: number
  totalDistance?: number // Total distance for current movement (used for animation selection)
}

// Default movement configuration
export const DEFAULT_MOVEMENT_CONFIG: MovementConfig = {
  maxSpeed: 3, // units per second
  acceleration: 6, // units per second squared
  deceleration: 6, // units per second squared
  arrivalThreshold: 0.05,
}

// Calculate acceleration and deceleration distances based on config
export function getAccelDistance(config: MovementConfig): number {
  return (config.maxSpeed * config.maxSpeed) / (2 * config.acceleration)
}

export function getDecelDistance(config: MovementConfig): number {
  return (config.maxSpeed * config.maxSpeed) / (2 * config.deceleration)
}

/**
 * Calculate the next movement step with acceleration/deceleration
 *
 * @param currentPos - Current position
 * @param movement - Movement state (start, target, total distance, current speed)
 * @param config - Movement configuration
 * @param deltaTimeSeconds - Time delta in seconds
 * @returns Movement result with new position, speed, rotation, and arrival status
 */
export function calculateMovementStep(
  currentPos: Position,
  movement: MovementState,
  config: MovementConfig,
  deltaTimeSeconds: number
): MovementResult {
  const { targetPos, totalDistance } = movement
  const accelDistance = getAccelDistance(config)
  const decelDistance = getDecelDistance(config)

  // Calculate remaining distance
  const dx = targetPos.x - currentPos.x
  const dy = targetPos.y - currentPos.y
  const dz = targetPos.z - currentPos.z
  const remainingDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

  // Check if arrived
  if (remainingDistance <= config.arrivalThreshold) {
    return {
      newPos: { x: targetPos.x, y: targetPos.y, z: targetPos.z },
      newSpeed: 0,
      rotation: Math.atan2(dx, dz),
      arrived: true,
    }
  }

  // Calculate traveled distance
  const traveledDistance = totalDistance - remainingDistance

  // Determine speed based on phase (acceleration, cruise, deceleration)
  let newSpeed = movement.currentSpeed
  if (traveledDistance < accelDistance) {
    // Acceleration phase
    newSpeed = Math.min(
      newSpeed + config.acceleration * deltaTimeSeconds,
      config.maxSpeed
    )
  } else if (remainingDistance > decelDistance) {
    // Cruise phase
    newSpeed = config.maxSpeed
  } else {
    // Deceleration phase
    newSpeed = Math.max(newSpeed - config.deceleration * deltaTimeSeconds, 0)
  }

  // Calculate rotation (direction of movement)
  const rotation = Math.atan2(dx, dz)

  // Move at current speed
  const moveDistance = newSpeed * deltaTimeSeconds

  let newPos: Position
  if (moveDistance >= remainingDistance || newSpeed <= 0.001) {
    // Arrived at destination
    newPos = { x: targetPos.x, y: targetPos.y, z: targetPos.z }
    return {
      newPos,
      newSpeed: 0,
      rotation,
      arrived: true,
    }
  } else {
    // Continue moving
    const dirX = dx / remainingDistance
    const dirY = dy / remainingDistance
    const dirZ = dz / remainingDistance
    newPos = {
      x: currentPos.x + dirX * moveDistance,
      y: currentPos.y + dirY * moveDistance,
      z: currentPos.z + dirZ * moveDistance,
    }
  }

  return {
    newPos,
    newSpeed,
    rotation,
    arrived: false,
  }
}

/**
 * Initialize movement state from current position to target
 *
 * @param currentPos - Current position
 * @param targetPos - Target position
 * @param currentSpeed - Current movement speed (default 0)
 * @returns Movement state
 */
export function initMovementState(
  currentPos: Position,
  targetPos: Position,
  currentSpeed: number = 0
): MovementState {
  const dx = targetPos.x - currentPos.x
  const dy = targetPos.y - currentPos.y
  const dz = targetPos.z - currentPos.z
  const totalDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

  return {
    currentSpeed,
    startPos: { ...currentPos },
    targetPos: { ...targetPos },
    totalDistance,
  }
}

/**
 * Check if target position has changed
 */
export function hasTargetChanged(
  movement: MovementState | undefined,
  newTarget: Position
): boolean {
  if (!movement) return true
  return (
    movement.targetPos.x !== newTarget.x ||
    movement.targetPos.y !== newTarget.y ||
    movement.targetPos.z !== newTarget.z
  )
}
