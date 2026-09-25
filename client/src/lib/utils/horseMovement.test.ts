import { describe, expect, it } from 'vitest'
import {
  angleDelta,
  horseTurnDuration,
  steerHorse,
  horseArcStep,
  moveHorse,
  resolveHorseSteps,
  HORSE_ARRIVAL_DISTANCE,
} from './horseMovement'
import {
  calculateMovementStep,
  DEFAULT_MOVEMENT_CONFIG,
  initMovementState,
} from './movementUtils'

describe('mounted steering', () => {
  it('matches server timing and mirrors left and right turns', () => {
    for (const sign of [-1, 1]) {
      const start = steerHorse(0, (sign * Math.PI) / 2, 0.2)
      expect(start.rotation).toBeCloseTo((sign * Math.PI) / 6)
      expect(start.travelTime).toBeCloseTo(0)
      const finish = steerHorse(0, (sign * Math.PI) / 2, 0.6)
      expect(finish.rotation).toBeCloseTo((sign * Math.PI) / 2)
      expect(finish.travelTime).toBeCloseTo(0.1)
    }
    expect(horseTurnDuration(Math.PI)).toBeCloseTo(1)
    expect(angleDelta(Math.PI - 0.1, -Math.PI + 0.1)).toBeCloseTo(0.2)
  })

  it('preserves travel and rotation across different frame rates', () => {
    for (const target of [0.1, Math.PI / 2, Math.PI, -2]) {
      const expected = steerHorse(0, target, 1.2)
      let rotation = 0
      let travel = 0
      for (let i = 0; i < 120; i++) {
        const step = steerHorse(rotation, target, 0.01)
        rotation = step.rotation
        travel += step.travelTime
      }
      expect(angleDelta(rotation, expected.rotation)).toBeCloseTo(0)
      expect(travel).toBeCloseTo(expected.travelTime)
    }
  })

  it('moves around a radius, then reaches the click target', () => {
    const origin = { x: 0, y: 5, z: 0 }
    const movement = initMovementState(origin, { x: 20, y: 5, z: 0 })
    const config = { ...DEFAULT_MOVEMENT_CONFIG, maxSpeed: 6, mountRotation: 0 }
    const turn = calculateMovementStep(origin, movement, config, 0.2)
    expect(turn.arrived).toBe(false)
    expect(turn.newPos.x).toBeCloseTo(0.65 * (1 - Math.cos(Math.PI / 6)))
    expect(turn.newPos.z).toBeCloseTo(0.325)
    const reverse = steerHorse(turn.rotation, -Math.PI / 2, 0.1)
    expect(reverse.rotation).toBeLessThan(turn.rotation)
    const move = calculateMovementStep(
      turn.newPos,
      movement,
      { ...config, mountRotation: turn.rotation },
      6
    )
    expect(move.arrived).toBe(true)
    expect(Math.hypot(20 - move.newPos.x, move.newPos.z)).toBeLessThanOrEqual(
      HORSE_ARRIVAL_DISTANCE
    )
    expect(move.newPos.x).toBeLessThan(20)
  })

  it('mirrors the arc and respects slow movement speeds', () => {
    const right = horseArcStep(0, Math.PI / 2, 6, 0.2)
    const left = horseArcStep(0, -Math.PI / 2, 6, 0.2)
    expect(left.x).toBeCloseTo(-right.x)
    expect(left.z).toBeCloseTo(right.z)
    for (const speed of [0, 0.5, 3, 6, 9]) {
      for (const angle of [0.01, 0.2, 1, 2, Math.PI]) {
        const step = horseArcStep(0, angle, speed, 1 / 60)
        expect(Math.hypot(step.x, step.z)).toBeLessThanOrEqual(
          speed / 60 + 1e-6
        )
      }
    }
  })

  it('stays in place for goals within one metre without turning or snapping', () => {
    const origin = { x: 0, y: 0, z: 0 }
    for (const goal of [
      { x: 0, y: 0, z: -0.1 },
      { x: 0.1, y: 0, z: 0 },
      { x: 1, y: 0, z: 0 },
    ]) {
      const result = moveHorse(origin, 0, 6, 5, goal)
      expect(result.arrived).toBe(true)
      expect(result.newPos).toEqual(origin)
      expect(result.rotation).toBe(0)
      expect(result.newSpeed).toBe(0)
    }
  })

  it('stops within one metre at sprint speed without overshooting or snapping', () => {
    for (const origin of [
      { x: 0, y: 5, z: 0 },
      { x: -1498, y: 5, z: 4740 },
    ]) {
      const goal = { ...origin, z: origin.z + 1.1 }
      const result = moveHorse(origin, 0.01, 13.5, 0.2, goal)
      expect(result.arrived).toBe(true)
      expect(result.newPos.z).toBeGreaterThan(origin.z)
      expect(result.newPos.z).toBeLessThan(goal.z)
      expect(
        Math.hypot(result.newPos.x - goal.x, result.newPos.z - goal.z)
      ).toBeLessThanOrEqual(HORSE_ARRIVAL_DISTANCE)
      expect(
        moveHorse(result.newPos, result.rotation, 13.5, 0.2, goal).newPos
      ).toEqual(result.newPos)
    }
  })

  it('checks the arc between endpoints and stops at the last clear pose', () => {
    const origin = { x: 0, y: 0, z: 0 }
    const result = moveHorse(origin, 0, 6, 0.6, Math.PI / 2)
    expect(result.newPos.x).toBeGreaterThan(0.65)
    const path = resolveHorseSteps(result.mountSteps!, origin, 0, {
      sampleHeight: () => 0,
      isMovementBlocked: (_x, _z, x, z) => x < 0.3 && z > 0.3,
      isUphillTooSteep: () => false,
    })
    expect(path.blocked).toBe('blocked')
    expect(path.position.z).toBeLessThanOrEqual(0.3)
    expect(path.rotation).toBeLessThan(Math.PI / 2)
  })
})
