import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ServerMovement } from './server-movement'
import type { MovePath, MoveProgress } from '../../network/networkTypes'
import { WORLD_MAX_X, WORLD_MIN_X } from '../../terrain/world-wrap'
import {
  calculateMovementStep,
  DEFAULT_MOVEMENT_CONFIG,
  initMovementState,
  scaleMovementConfig,
  SPRINT_SPEED_MULT,
} from '../../utils/movementUtils'

function setup() {
  let id = 0
  const goal = vi.fn()
  const stop = vi.fn()
  const movement = new ServerMovement(() => ++id, goal, stop)
  return { movement, goal, stop }
}

function path(requestId = 1): MovePath {
  return {
    request_id: requestId,
    server_time_ms: 0,
    position: { x: 0, y: 0, z: 0 },
    rotation: 0,
    floor_level: 0,
    waypoints: [
      { position: { x: 3, y: 0, z: 0 }, floor_level: 0 },
      { position: { x: 3, y: 0, z: 3 }, floor_level: 0 },
    ],
    speed: 3,
    termination: 'reached',
  }
}

beforeEach(() =>
  vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'clearTimeout'] })
)
afterEach(() => vi.useRealTimers())

describe('server approved movement', () => {
  it.each([
    { sprinting: false, speed: 3 },
    { sprinting: true, speed: 4.5 },
  ])(
    'preserves legacy cruising speed at $speed m/s',
    ({ sprinting, speed }) => {
      const { movement } = setup()
      movement.request(60, 0, sprinting)
      const approved = path()
      approved.speed = speed
      approved.waypoints = Array.from({ length: 60 }, (_, index) => ({
        position: { x: index + 1, y: 0, z: 0 },
        floor_level: 0,
      }))
      movement.acceptPath(approved)
      let legacyPosition = { x: 0, y: 0, z: 0 }
      const legacy = initMovementState(
        legacyPosition,
        { x: 60, y: 0, z: 0 },
        speed
      )
      const config = scaleMovementConfig(
        DEFAULT_MOVEMENT_CONFIG,
        sprinting ? SPRINT_SPEED_MULT : 1
      )
      for (let frame = 1; frame <= 500; frame++) {
        vi.advanceTimersByTime(20)
        if (frame % 10 === 0) {
          const x = speed * frame * 0.02
          movement.acceptProgress({
            request_id: 1,
            server_time_ms: frame * 20,
            position: { x, y: 0, z: 0 },
            rotation: Math.PI / 2,
            floor_level: 0,
            next_waypoint: Math.floor(x),
            speed,
            status: 'moving',
          })
        }
        const step = calculateMovementStep(legacyPosition, legacy, config, 0.02)
        legacyPosition = step.newPos
        legacy.currentSpeed = step.newSpeed
        expect(movement.sample(() => false)?.position.x).toBeCloseTo(
          legacyPosition.x,
          5
        )
      }
      expect(legacyPosition.x).toBeCloseTo(speed * 10, 5)
    }
  )

  it('stays still until approval and starts from the server pose', () => {
    const { movement } = setup()
    movement.request(3, 3, false)
    vi.advanceTimersByTime(300)
    expect(movement.sample(() => false)).toBeNull()
    movement.acceptPath(path())
    expect(movement.sample(() => false)?.position).toEqual({ x: 0, y: 0, z: 0 })
    vi.advanceTimersByTime(200)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.6)
  })

  it('sends the first click immediately, then only the latest at a fixed deadline', () => {
    const { movement, goal } = setup()
    movement.request(1, 0, false)
    vi.advanceTimersByTime(40)
    movement.request(2, 0, false)
    vi.advanceTimersByTime(40)
    movement.request(3, 0, false)
    expect(goal).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(20)
    expect(goal).toHaveBeenCalledTimes(2)
    expect(goal.mock.lastCall?.[0]).toMatchObject({ request_id: 3, x: 3 })
    expect(movement.acceptPath(path(1))).toBe(false)
    expect(movement.acceptPath(path(3))).toBe(true)
  })

  it('stop bypasses throttling, removes the trailing send and ignores late approval', () => {
    const { movement, goal, stop } = setup()
    movement.request(1, 0, false)
    movement.request(2, 0, false)
    movement.clear()
    expect(stop).toHaveBeenCalledWith(3)
    vi.advanceTimersByTime(200)
    expect(goal).toHaveBeenCalledTimes(1)
    expect(movement.acceptPath(path(2))).toBe(false)
  })

  it.each([200, 500, 1000])(
    'adopts server progress after a %ims frame stall',
    (pause) => {
      const { movement } = setup()
      movement.request(3, 3, false)
      movement.acceptPath(path())
      vi.advanceTimersByTime(pause)
      const update: MoveProgress = {
        request_id: 1,
        server_time_ms: pause,
        position: { x: pause * 0.003, y: 0, z: 0 },
        rotation: Math.PI / 2,
        floor_level: 0,
        next_waypoint: 0,
        speed: 3,
        status: 'moving',
      }
      movement.acceptProgress(update)
      expect(movement.sample(() => false)?.position.x).toBeCloseTo(
        pause * 0.003
      )
    }
  )

  it('does not continue indefinitely when progress packets stop', () => {
    const { movement } = setup()
    movement.request(3, 3, false)
    movement.acceptPath(path())
    vi.advanceTimersByTime(4000)
    const pose = movement.sample(() => false)
    expect(pose?.position.x).toBeCloseTo(1.5)
    expect(pose?.speed).toBe(0)
  })

  it('checks every segment rather than cutting a corner during a delayed frame', () => {
    const { movement } = setup()
    movement.request(3, 3, false)
    const approved = path()
    approved.speed = 12
    movement.acceptPath(approved)
    const blocked = vi.fn((from, to) => from.z < 1 && to.z >= 1)
    vi.advanceTimersByTime(500)
    const pose = movement.sample(blocked)
    expect(pose?.position.x).toBeCloseTo(3)
    expect(pose?.position.z).toBeLessThan(1)
    expect(pose?.speed).toBe(0)
    expect(blocked).toHaveBeenCalledTimes(2)
  })

  it('accepts a stop acknowledgement once, but not after a newer click', () => {
    const { movement } = setup()
    movement.request(3, 3, false)
    movement.clear()
    const stopped: MoveProgress = {
      request_id: 2,
      server_time_ms: 1,
      position: { x: 1, y: 0, z: 0 },
      rotation: 0,
      floor_level: 0,
      next_waypoint: 0,
      speed: 0,
      status: 'stopped',
    }
    expect(movement.acceptStopped(stopped)).toBe(true)
    expect(movement.acceptStopped(stopped)).toBe(false)
    movement.request(4, 4, false)
    movement.clear()
    movement.request(5, 5, false)
    expect(movement.acceptStopped({ ...stopped, request_id: 4 })).toBe(false)
  })

  it('does not rewind to an older server sample or accept a completed request again', () => {
    const { movement } = setup()
    movement.request(3, 3, false)
    movement.acceptPath(path())
    const update: MoveProgress = {
      request_id: 1,
      server_time_ms: 10,
      position: { x: 2, y: 0, z: 0 },
      rotation: 0,
      floor_level: 0,
      next_waypoint: 0,
      speed: 3,
      status: 'moving',
    }
    expect(movement.acceptProgress(update)).toBe(true)
    expect(movement.acceptProgress({ ...update, server_time_ms: 9 })).toBe(
      false
    )
    movement.finish()
    expect(movement.acceptPath(path())).toBe(false)
  })

  it('uses the short segment across the world seam', () => {
    const { movement } = setup()
    movement.request(WORLD_MIN_X + 1, 0, false)
    const approved = path()
    approved.position.x = WORLD_MAX_X - 1
    approved.waypoints = [
      { position: { x: WORLD_MIN_X + 1, y: 0, z: 0 }, floor_level: 0 },
    ]
    movement.acceptPath(approved)
    vi.advanceTimersByTime(500)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(
      WORLD_MIN_X + 0.5
    )
  })
})
