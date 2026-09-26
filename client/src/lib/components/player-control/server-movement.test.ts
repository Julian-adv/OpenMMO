import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ServerMovement } from './server-movement'
import type { MovePath, MoveProgress } from '../../network/networkTypes'
import { WORLD_MAX_X, WORLD_MIN_X } from '../../terrain/world-wrap'
import { projectPlayerState, projectStoppedPlayerState } from './fsm/projection'
import { buildAttackState } from './player-state-builders'
import { transitionAttackToIdle } from './fsm/combat'

function setup() {
  let id = 0
  const goal = vi.fn()
  const stop = vi.fn()
  const direction = vi.fn()
  const movement = new ServerMovement(() => ++id, goal, stop, direction)
  return { movement, goal, stop, direction }
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

function directionPath(serverTimeMs: number): MovePath {
  const x = serverTimeMs * 0.003
  return {
    ...path(),
    server_time_ms: serverTimeMs,
    position: { x, y: 0, z: 0 },
    rotation: Math.PI / 2,
    waypoints: Array.from({ length: 24 }, (_, index) => ({
      position: { x: x + (index + 1) * 0.05, y: 0, z: 0 },
      floor_level: 0,
      rotation: Math.PI / 2,
      travel_seconds: 1 / 60,
    })),
  }
}

function stopped(requestId: number, serverTimeMs: number): MoveProgress {
  return {
    ...directionPath(serverTimeMs),
    request_id: requestId,
    next_waypoint: 0,
    speed: 0,
    status: 'stopped',
  }
}

beforeEach(() =>
  vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'clearTimeout'] })
)
afterEach(() => vi.useRealTimers())

describe('keyboard release', () => {
  it.each([20, 150, 250])(
    'finishes smoothly with %ims RTT and a delayed stop acknowledgement',
    (rtt) => {
      const { movement, stop } = setup()
      movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
      vi.advanceTimersByTime(rtt / 2)
      movement.acceptPath(directionPath(0))
      vi.advanceTimersByTime(100)
      const released = movement.sample(() => false)!
      movement.stopDirection()
      movement.stopDirection()
      expect(stop).toHaveBeenCalledExactlyOnceWith(2)
      expect(movement.stopping).toBe(true)
      for (let elapsed = 10; elapsed <= rtt + 40; elapsed += 10) {
        vi.advanceTimersByTime(10)
        const pose = movement.sample(() => false)!
        expect(pose.position.x).toBeCloseTo(
          released.position.x + elapsed * 0.003,
          5
        )
        expect(pose.speed).toBeCloseTo(3, 5)
      }
      const before = movement.sample(() => false)!
      const progress = stopped(2, 100 + rtt)
      expect(movement.acceptStopped(progress)).toBe(true)
      expect(movement.sample(() => false)).toEqual(before)
      vi.advanceTimersByTime(50)
      const halfway = movement.sample(() => false)!
      expect(halfway.position.x).toBeGreaterThan(progress.position.x)
      expect(halfway.position.x).toBeLessThan(before.position.x)
      expect(halfway.speed).toBeGreaterThan(0)
      expect(halfway.speed).toBeLessThan(before.speed)
      vi.advanceTimersByTime(150)
      expect(movement.sample(() => false)).toEqual({
        position: progress.position,
        rotation: progress.rotation,
        speed: 0,
      })
      expect(movement.stopping).toBe(false)
      expect(movement.active).toBe(false)
      expect(movement.sample(() => false)).toBeNull()
      expect(movement.acceptStopped(progress)).toBe(false)
      expect(movement.acceptPath(directionPath(500))).toBe(false)
    }
  )

  it('does not move beyond the approved route while waiting for stop', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.acceptPath(directionPath(0))
    vi.advanceTimersByTime(100)
    movement.sample(() => false)
    movement.stopDirection()
    vi.advanceTimersByTime(3000)
    const pose = movement.sample(() => false)!
    expect(pose.position.x).toBeCloseTo(1.2, 5)
    expect(pose.speed).toBe(0)
  })

  it.each([false, true])(
    'restarts the same direction with a fresh request, acknowledged stop: %s',
    (acknowledged) => {
      const { movement, direction } = setup()
      const input = { rotation: 0, forward: 1, turn: 0, sprinting: false }
      movement.direction(input)
      movement.acceptPath(directionPath(0))
      vi.advanceTimersByTime(100)
      movement.sample(() => false)
      movement.stopDirection()
      if (acknowledged) movement.acceptStopped(stopped(2, 150))
      movement.direction(input)
      expect(direction.mock.lastCall?.[0].request_id).toBe(3)
      expect(movement.stopping).toBe(false)
      expect(movement.acceptStopped(stopped(2, 150))).toBe(false)
      vi.advanceTimersByTime(200)
      expect(movement.sample(() => false)).toBeNull()
      expect(
        movement.acceptPath({ ...directionPath(200), request_id: 3 })
      ).toBe(true)
      expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.6, 5)
    }
  )

  it.each(['click', 'clear', 'relocate'])(
    'cancels the final stop blend on %s',
    (action) => {
      const { movement } = setup()
      movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
      movement.acceptPath(directionPath(0))
      vi.advanceTimersByTime(100)
      movement.sample(() => false)
      movement.stopDirection()
      movement.acceptStopped(stopped(2, 150))
      if (action === 'click') movement.request(10, 0, false)
      else movement.clear(action !== 'relocate')
      vi.advanceTimersByTime(200)
      expect(movement.stopping).toBe(false)
      expect(movement.sample(() => false)).toBeNull()
    }
  )

  it('blends height and the short position and rotation across their seams', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.stopDirection()
    const progress = {
      ...stopped(2, 100),
      position: { x: WORLD_MIN_X + 0.05, y: 2.2, z: 0 },
      rotation: -Math.PI + 0.05,
    }
    movement.acceptStopped(progress, {
      position: { x: WORLD_MAX_X - 0.05, y: 2, z: 0 },
      rotation: Math.PI - 0.05,
      speed: 3,
    })
    vi.advanceTimersByTime(60)
    const pose = movement.sample(() => false)!
    expect(pose.position.x).toBeCloseTo(WORLD_MIN_X + 0.025, 5)
    expect(pose.position.y).toBeCloseTo(2.15, 5)
    expect(pose.rotation).toBeCloseTo(Math.PI + 0.025, 5)
    vi.advanceTimersByTime(60)
    expect(movement.sample(() => false)).toEqual({
      position: progress.position,
      rotation: progress.rotation,
      speed: 0,
    })
  })

  it('stays idle when a short key press ends before the first approved path', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.stopDirection()
    const displayed = {
      position: { x: 0, y: 0, z: 0 },
      rotation: Math.PI / 2,
      speed: 0,
    }
    movement.acceptStopped(stopped(2, 20), displayed)
    expect(movement.sample(() => false)).toEqual(displayed)
    vi.advanceTimersByTime(60)
    expect(movement.sample(() => false)?.speed).toBe(0)
    vi.advanceTimersByTime(60)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.06, 5)
    expect(movement.stopping).toBe(false)
  })

  it('finishes immediately when the display already matches the stop', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.acceptPath(directionPath(0))
    vi.advanceTimersByTime(100)
    movement.sample(() => false)
    movement.stopDirection()
    movement.acceptStopped(stopped(2, 100))
    expect(movement.sample(() => false)?.speed).toBe(0)
    expect(movement.stopping).toBe(false)
  })

  it('uses the official stop pose when an obstacle prevents blending', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.acceptPath(directionPath(0))
    vi.advanceTimersByTime(100)
    movement.sample(() => false)
    movement.stopDirection()
    const progress = stopped(2, 150)
    movement.acceptStopped(progress)
    vi.advanceTimersByTime(60)
    expect(movement.sample(() => true)).toEqual({
      position: progress.position,
      rotation: progress.rotation,
      speed: 0,
    })
    expect(movement.stopping).toBe(false)
  })
})

describe('server approved movement', () => {
  it.each([20, 75, 125])(
    'keeps keyboard motion continuous with %ims initial latency and jitter',
    (latency) => {
      const { movement } = setup()
      movement.direction({
        rotation: Math.PI / 2,
        forward: 1,
        turn: 0,
        sprinting: false,
      })
      vi.advanceTimersByTime(latency)
      movement.acceptPath(directionPath(0))
      const updates = [
        { received: 140, server: 100 },
        { received: 220, server: 200 },
        { received: 280, server: 300 },
        { received: 380, server: 400 },
        { received: 600, server: 500 },
        { received: 600, server: 600 },
        { received: 780, server: 700 },
        { received: 790, server: 800 },
        { received: 900, server: 900 },
      ]
      for (let elapsed = 10; elapsed <= 1000; elapsed += 10) {
        vi.advanceTimersByTime(10)
        for (const update of updates.filter((u) => u.received === elapsed))
          movement.acceptPath(directionPath(update.server))
        const pose = movement.sample(() => false)!
        expect(pose.position.x).toBeCloseTo(elapsed * 0.003, 5)
        expect(pose.speed).toBeCloseTo(3, 5)
        expect(
          projectPlayerState({
            currentPosition: pose.position,
            currentSpeed: pose.speed,
            playerRotation: pose.rotation,
            isMoving: true,
            hasTorch: false,
            isInCombat: false,
            attackCounter: 0,
            isSprinting: false,
          })
        ).toMatchObject({ state: 'moving', movementMode: 'jog' })
      }
    }
  )

  it('preserves approved turn timing across early keyboard path updates', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 0, turn: 1, sprinting: false })
    const turningPath = (time: number): MovePath => ({
      ...directionPath(time),
      position: { x: 0, y: 0, z: 0 },
      rotation: time / 1000,
      waypoints: [
        {
          position: { x: 0, y: 0, z: 0 },
          floor_level: 0,
          rotation: time / 1000 + 0.4,
          travel_seconds: 0.4,
        },
      ],
    })
    vi.advanceTimersByTime(250)
    movement.acceptPath(turningPath(0))
    vi.advanceTimersByTime(50)
    movement.acceptPath(turningPath(100))
    movement.acceptPath(turningPath(200))
    for (let elapsed = 50; elapsed <= 300; elapsed += 10) {
      const pose = movement.sample(() => false)!
      expect(pose.rotation).toBeCloseTo(elapsed / 1000, 5)
      expect(pose.speed).toBe(0)
      vi.advanceTimersByTime(10)
    }
  })

  it('keeps a mounted click turn continuous across jittery path renewals', () => {
    const { movement } = setup()
    movement.request(3, 0, false)
    const turningPath = (time: number): MovePath => ({
      ...path(),
      server_time_ms: time,
      rotation: time / 1000,
      waypoints: [
        {
          position: { x: 0, y: 0, z: 0 },
          floor_level: 0,
          rotation: time / 1000 + 0.4,
          travel_seconds: 0.4,
        },
      ],
    })
    vi.advanceTimersByTime(250)
    movement.acceptPath(turningPath(0))
    vi.advanceTimersByTime(50)
    movement.acceptPath(turningPath(100))
    movement.acceptPath(turningPath(200))
    for (let elapsed = 50; elapsed <= 300; elapsed += 10) {
      expect(movement.sample(() => false)!.rotation).toBeCloseTo(
        elapsed / 1000,
        5
      )
      vi.advanceTimersByTime(10)
    }
  })

  it('does not extend the approved keyboard route when a renewal arrives late', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    movement.acceptPath(directionPath(0))
    vi.advanceTimersByTime(250)
    movement.acceptPath(directionPath(100))
    vi.advanceTimersByTime(240)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(1.47, 5)
    vi.advanceTimersByTime(110)
    expect(movement.sample(() => false)).toMatchObject({ speed: 0 })
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(1.5, 5)
  })

  it.each(['stop', 'click', 'direction', 'finish'])(
    'discards queued keyboard paths after %s',
    (action) => {
      const { movement } = setup()
      movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
      vi.advanceTimersByTime(250)
      movement.acceptPath(directionPath(0))
      vi.advanceTimersByTime(50)
      movement.acceptPath(directionPath(100))
      if (action === 'stop') movement.clear()
      else if (action === 'click') movement.request(3, 0, false)
      else if (action === 'finish') movement.finish()
      else
        movement.direction({
          rotation: 1,
          forward: 1,
          turn: 0,
          sprinting: false,
        })
      vi.advanceTimersByTime(150)
      expect(movement.sample(() => false)).toBeNull()
      expect(movement.acceptPath(directionPath(200))).toBe(false)
    }
  )

  it('rejects samples older than a queued keyboard path', () => {
    const { movement } = setup()
    movement.direction({ rotation: 0, forward: 1, turn: 0, sprinting: false })
    vi.advanceTimersByTime(250)
    movement.acceptPath(directionPath(0))
    vi.advanceTimersByTime(50)
    expect(movement.acceptPath(directionPath(100))).toBe(true)
    expect(movement.acceptPath(directionPath(90))).toBe(false)
    expect(
      movement.acceptProgress({
        ...directionPath(90),
        next_waypoint: 0,
        status: 'moving',
      })
    ).toBe(false)
    vi.advanceTimersByTime(100)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.45, 5)
  })

  it.each([
    { sprinting: false, speed: 3 },
    { sprinting: true, speed: 4.5 },
  ])(
    'displays the approved cruising speed at $speed m/s',
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
        expect(movement.sample(() => false)?.position.x).toBeCloseTo(
          speed * frame * 0.02,
          5
        )
      }
      expect(movement.sample(() => false)?.position.x).toBeCloseTo(
        speed * 10,
        5
      )
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

  it.each([20, 150, 500])(
    'keeps a prop swing playing after a stop acknowledgement delayed by %ims',
    (delay) => {
      const { movement, stop } = setup()
      movement.request(3, 3, false)
      movement.acceptPath(path())
      vi.advanceTimersByTime(100)
      const pose = movement.sample(() => false)!
      const attackState = buildAttackState(
        { ...pose, state: 'moving', movementMode: 'jog' },
        1,
        3
      )
      movement.clear()
      expect(stop).toHaveBeenCalledExactlyOnceWith(2)

      vi.advanceTimersByTime(delay)
      const progress = stopped(2, 100 + delay)
      expect(movement.acceptStopped(progress)).toBe(true)
      const state = projectStoppedPlayerState(
        attackState,
        progress.position,
        progress.rotation
      )
      expect(state).toEqual({
        ...attackState,
        position: progress.position,
        speed: 0,
        movementMode: undefined,
      })
      expect(movement.sample(() => false)).toBeNull()
      expect(transitionAttackToIdle(state)).toMatchObject({
        kind: 'idle',
        nextPlayerState: { state: 'idle', attackCounter: 0 },
      })
    }
  )

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

it.each([20, 150, 250])(
  'waits for approval at %ims RTT and ignores an approval overtaken by stop',
  (rtt) => {
    const { movement } = setup()
    movement.request(3, 3, false)
    vi.advanceTimersByTime(rtt)
    expect(movement.sample(() => false)).toBeNull()
    movement.acceptPath(path())
    vi.advanceTimersByTime(100)
    expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.3)
    movement.clear()
    vi.advanceTimersByTime(rtt)
    expect(movement.acceptPath(path())).toBe(false)
    expect(movement.sample(() => false)).toBeNull()
  }
)

it('holds a locally blocked pose until the next official update', () => {
  const { movement } = setup()
  movement.request(3, 3, false)
  movement.acceptPath(path())
  vi.advanceTimersByTime(100)
  const stopped = movement.sample(() => true)
  vi.advanceTimersByTime(100)
  expect(movement.sample(() => false)).toEqual(stopped)
  movement.acceptPath({ ...path(), server_time_ms: 200 })
  vi.advanceTimersByTime(100)
  expect(movement.sample(() => false)?.position.x).toBeCloseTo(0.3)
})

it('displays approved turn timing even when position does not change', () => {
  const { movement } = setup()
  movement.direction({ rotation: 0, forward: 0, turn: 1, sprinting: false })
  const approved = path()
  approved.waypoints = [
    {
      position: approved.position,
      floor_level: 0,
      rotation: Math.PI / 2,
      travel_seconds: 0.4,
    },
  ]
  movement.acceptPath(approved)
  vi.advanceTimersByTime(200)
  const pose = movement.sample(() => false)
  expect(pose?.position).toEqual(approved.position)
  expect(pose?.rotation).toBeCloseTo(Math.PI / 4)
})
