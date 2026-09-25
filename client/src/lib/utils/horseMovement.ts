import { shortestWrappedDeltaX } from '../terrain/world-wrap'
import type { Position, MovementResult } from './movementUtils'

const TURN_RATE = Math.PI / 2 / 0.6
export const KEYBOARD_TURN_RATE = TURN_RATE
export const BACKWARD_SPEED = 1.5
const REVERSE_RATE = Math.PI / 2 / 0.4
const MOVE_ANGLE = Math.PI / 6
export const HORSE_TURN_RADIUS = 0.65
export const HORSE_ARRIVAL_DISTANCE = 1
const STEP_SECONDS = 1 / 60

export function angleDelta(from: number, to: number): number {
  const tau = Math.PI * 2
  return ((((to - from + Math.PI) % tau) + tau) % tau) - Math.PI
}

export function keyboardRotation(from: number, to: number, dt: number): number {
  const step = TURN_RATE * Math.max(0, dt)
  return from + Math.max(-step, Math.min(step, angleDelta(from, to)))
}

export function horseTurnDuration(angle: number): number {
  angle = Math.abs(angle)
  return (
    Math.max(0, angle - Math.PI / 2) / REVERSE_RATE +
    Math.min(angle, Math.PI / 2) / TURN_RATE
  )
}

function movementCredit(angle: number): number {
  angle = Math.min(angle, MOVE_ANGLE)
  return (
    (angle / 2 +
      (MOVE_ANGLE * Math.sin((Math.PI * angle) / MOVE_ANGLE)) / (2 * Math.PI)) /
    TURN_RATE
  )
}

// Mirrors shared/src/mount_movement.rs, including travel across partial turns.
export function steerHorse(from: number, to: number, dt: number) {
  dt = Math.max(0, dt)
  const delta = angleDelta(from, to)
  const angle = Math.abs(delta)
  const duration = horseTurnDuration(angle)
  const remaining = Math.max(0, duration - dt)
  const slowTime = Math.PI / 2 / TURN_RATE
  const nextAngle =
    remaining > slowTime
      ? Math.PI / 2 + (remaining - slowTime) * REVERSE_RATE
      : remaining * TURN_RATE
  return {
    rotation: from + Math.sign(delta) * (angle - nextAngle),
    travelTime: Math.max(
      0,
      Math.min(
        dt,
        Math.max(0, dt - duration) +
          movementCredit(angle) -
          movementCredit(nextAngle)
      )
    ),
  }
}

export function horseArcStep(
  from: number,
  to: number,
  speed: number,
  dt: number,
  radius = HORSE_TURN_RADIUS
) {
  const steering = steerHorse(from, to, dt)
  radius = Math.min(radius, Math.max(0, speed) / REVERSE_RATE)
  const signedRadius = radius * Math.sign(angleDelta(from, to))
  const alignedTime = Math.max(0, dt - horseTurnDuration(angleDelta(from, to)))
  const forward = Math.max(
    0,
    speed * steering.travelTime -
      radius * TURN_RATE * (steering.travelTime - alignedTime)
  )
  return {
    x:
      signedRadius * (Math.cos(from) - Math.cos(steering.rotation)) +
      Math.sin(to) * forward,
    z:
      signedRadius * (Math.sin(steering.rotation) - Math.sin(from)) +
      Math.cos(to) * forward,
    rotation: steering.rotation,
  }
}

export function moveHorse(
  position: Position,
  rotation: number,
  speed: number,
  dt: number,
  goal: Position | number,
  turnRadius = HORSE_TURN_RADIUS
): MovementResult {
  let current = { ...position }
  let remaining = Math.max(0, dt)
  let distanceMoved = 0
  let arrived = false
  const mountSteps: NonNullable<MovementResult['mountSteps']> = []
  while (remaining > 1e-7) {
    const dx =
      typeof goal === 'number' ? 0 : shortestWrappedDeltaX(current.x, goal.x)
    const dz = typeof goal === 'number' ? 0 : goal.z - current.z
    const distance = Math.hypot(dx, dz)
    if (typeof goal !== 'number' && distance <= HORSE_ARRIVAL_DISTANCE) {
      arrived = true
      break
    }
    const desired = typeof goal === 'number' ? goal : Math.atan2(dx, dz)
    const stepTime = Math.min(remaining, STEP_SECONDS)
    const radius =
      typeof goal === 'number' ? turnRadius : Math.min(turnRadius, distance / 4)
    const step = horseArcStep(rotation, desired, speed, stepTime, radius)
    const next = { x: current.x + step.x, y: current.y, z: current.z + step.z }
    distanceMoved += Math.hypot(next.x - current.x, next.z - current.z)
    current = next
    rotation = step.rotation
    mountSteps.push({ position: current, rotation })
    remaining -= stepTime
    if (
      typeof goal !== 'number' &&
      Math.hypot(dx - step.x, dz - step.z) <= HORSE_ARRIVAL_DISTANCE
    ) {
      arrived = true
      break
    }
  }
  return {
    newPos: current,
    newSpeed: dt > 0 ? distanceMoved / dt : 0,
    rotation,
    arrived,
    mountSteps,
  }
}

export function resolveHorseSteps(
  steps: NonNullable<MovementResult['mountSteps']>,
  position: Position,
  rotation: number,
  physics: {
    sampleHeight: (x: number, z: number) => number
    isMovementBlocked: (
      fromX: number,
      fromZ: number,
      toX: number,
      toZ: number,
      y: number
    ) => boolean
    isUphillTooSteep: (
      x: number,
      z: number,
      y: number,
      dirX: number,
      dirZ: number
    ) => boolean
  }
) {
  for (const step of steps) {
    const next = step.position
    const dx = shortestWrappedDeltaX(position.x, next.x)
    const dz = next.z - position.z
    const distance = Math.hypot(dx, dz)
    const blocked = physics.isMovementBlocked(
      position.x,
      position.z,
      next.x,
      next.z,
      position.y
    )
    const slopeBlocked =
      !blocked &&
      distance > 1e-6 &&
      physics.isUphillTooSteep(
        position.x,
        position.z,
        position.y,
        dx / distance,
        dz / distance
      )
    if (blocked || slopeBlocked) {
      return {
        position,
        rotation,
        blocked: blocked ? ('blocked' as const) : ('slope_blocked' as const),
      }
    }
    next.y = physics.sampleHeight(next.x, next.z)
    position = next
    rotation = step.rotation
  }
  return { position, rotation, blocked: null }
}
