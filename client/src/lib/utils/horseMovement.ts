const TURN_RATE = Math.PI / 2 / 0.6
const REVERSE_RATE = Math.PI / 2 / 0.4
const MOVE_ANGLE = Math.PI / 6

export function angleDelta(from: number, to: number): number {
  const tau = Math.PI * 2
  return ((((to - from + Math.PI) % tau) + tau) % tau) - Math.PI
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
