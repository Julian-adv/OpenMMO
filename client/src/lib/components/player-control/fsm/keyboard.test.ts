import { describe, expect, it, vi } from 'vitest'
import {
  DEFAULT_MOVEMENT_CONFIG,
  type Position,
} from '../../../utils/movementUtils'
import { angleDelta, BACKWARD_SPEED } from '../../../utils/horseMovement'
import { WORLD_MAX_X, shortestWrappedDeltaX } from '../../../terrain/world-wrap'
import type { KeyboardMovementMode } from '../../../stores/movementSettings'
import {
  applyKeyboardMovement,
  applyKeyboardMovementOutcome,
  createKeyboardMoveSender,
  createKeyboardSpeedRamp,
  runKeyboardFrame,
  type KeyboardInput,
} from './keyboard'

function setup(
  mounted = false,
  initialRotation = 0,
  movementMode: KeyboardMovementMode = 'character'
) {
  const player = { position: { x: 0, y: 5, z: 0 } }
  const send = vi.fn()
  const input = {
    currentPlayer: player,
    isKeyboardMoving: false,
    interactionExit: 'none' as 'none' | 'pickup' | 'object',
    hasMovementTarget: false,
    isInCombat: false,
    input: { forward: 1, turn: 0 } as KeyboardInput | null,
    movementMode,
    rotation: initialRotation,
    config: {
      ...DEFAULT_MOVEMENT_CONFIG,
      maxSpeed: mounted ? 13.5 : 3,
      ...(mounted ? { mountRotation: initialRotation } : {}),
    },
    deltaTimeSeconds: 1 / 60,
    sampleHeight: () => 5,
    isMovementBlocked: vi.fn(() => false),
    isUphillTooSteep: vi.fn(() => false),
    writePlayerPosition: (position: Position, rotation: number) => {
      player.position = { ...position }
      input.rotation = rotation
    },
    moveSender: createKeyboardMoveSender(send),
    speedRamp: createKeyboardSpeedRamp(),
    actions: {
      exitPickupInteraction: vi.fn(),
      exitObjectInteraction: vi.fn(),
      clearClickMovement: vi.fn(),
      cancelCombat: vi.fn(),
      markMoving: vi.fn(() => {
        input.isKeyboardMoving = true
      }),
      setKeyboardIdleRuntime: vi.fn(() => {
        input.isKeyboardMoving = false
      }),
      emitKeyboardPlayerState: vi.fn(),
      stopMovement: vi.fn(),
      triggerJumpFeedback: vi.fn(),
      setMoved: vi.fn(),
    },
  }
  return { input, player, send, frame: () => runKeyboardFrame(input) }
}

for (const mounted of [false, true]) {
  describe(
    mounted ? 'mounted relative controls' : 'walking directional controls',
    () => {
      it.each([0, Math.PI / 2, Math.PI, -Math.PI / 2])(
        'handles the up key from facing %f',
        (rotation) => {
          const { player, send, frame } = setup(mounted, rotation)
          for (let i = 0; i < 60; i++) frame()
          const { x, z } = player.position
          const facing = rotation
          expect(Math.hypot(x, z)).toBeGreaterThan(2)
          expect(x * Math.cos(facing) - z * Math.sin(facing)).toBeCloseTo(0, 5)
          expect(x * Math.sin(facing) + z * Math.cos(facing)).toBeGreaterThan(0)
          expect(send.mock.calls.every((call) => call[2] === 1)).toBe(true)
        }
      )

      it.each([-1, 1])('handles the horizontal key %i', (turn) => {
        const { input, player, frame } = setup(mounted, Math.PI)
        input.input = { forward: 0, turn }
        for (let i = 0; i < 15; i++) frame()
        if (mounted) {
          expect(player.position).toEqual({ x: 0, y: 5, z: 0 })
          expect(angleDelta(Math.PI, input.rotation) * turn).toBeLessThan(0)
          expect(input.actions.setMoved).toHaveBeenLastCalledWith(
            0,
            input.rotation
          )
        } else {
          expect(player.position.x * turn).toBeGreaterThan(0)
          expect(player.position.z).toBeCloseTo(0)
          expect(input.rotation).toBe(Math.PI - (turn * Math.PI) / 2)
        }
      })

      it('turns right toward east while facing north and moving forward', () => {
        const { input, player, frame } = setup(mounted, Math.PI)
        input.input = { forward: 1, turn: 1 }
        for (let i = 0; i < 15; i++) frame()
        expect(player.position.x).toBeGreaterThan(0)
        expect(player.position.z).toBeLessThan(0)
        expect(input.rotation).toBeLessThan(Math.PI)
      })

      it('uses the correct down-key direction and speed', () => {
        const { input, player, send, frame } = setup(mounted, Math.PI / 2)
        input.config.maxSpeed = 13.5
        input.input = { forward: -1, turn: 0 }
        for (let i = 0; i < 60; i++) frame()
        if (mounted) {
          expect(player.position.x).toBeCloseTo(-BACKWARD_SPEED, 5)
          expect(player.position.z).toBeCloseTo(0, 5)
          expect(angleDelta(Math.PI / 2, input.rotation)).toBeCloseTo(0, 5)
          expect(send.mock.calls.every((call) => call[2] === -1)).toBe(true)
        } else {
          expect(player.position.x).toBeLessThan(-BACKWARD_SPEED)
          expect(player.position.z).toBeCloseTo(0)
          expect(angleDelta(Math.PI / 2, input.rotation)).toBeCloseTo(-Math.PI)
          expect(send.mock.calls.every((call) => call[2] === 1)).toBe(true)
        }
      })

      it.each(['wall', 'slope', 'release', 'opposing keys'])(
        'stops once on %s and does not flood while held',
        (reason) => {
          const { input, player, send, frame } = setup(mounted)
          for (let i = 0; i < 10; i++) frame()
          const sent = send.mock.calls.length
          if (reason === 'wall') input.isMovementBlocked.mockReturnValue(true)
          if (reason === 'slope') input.isUphillTooSteep.mockReturnValue(true)
          if (reason === 'release' || reason === 'opposing keys')
            input.input = null
          const stopped = { ...player.position }
          for (let i = 0; i < 60; i++) frame()
          expect(player.position).toEqual(stopped)
          expect(send).toHaveBeenCalledTimes(sent + 1)
          expect(send).toHaveBeenLastCalledWith(stopped, input.rotation, 1)
        }
      )

      it('does not reverse direction to finish a short backward tap', () => {
        const { input, player, send, frame } = setup(mounted)
        input.input = { forward: -1, turn: 0 }
        frame()
        const stopped = { ...player.position }
        input.input = null
        frame()
        expect(player.position).toEqual(stopped)
        expect(send).toHaveBeenLastCalledWith(
          stopped,
          input.rotation,
          mounted ? -1 : 1
        )
        expect(input.actions.setKeyboardIdleRuntime).toHaveBeenCalledOnce()
      })
    }
  )
}

describe.each([false, true])('fixed directions (mounted: %s)', (mounted) => {
  it.each([
    { forward: 1, turn: 0, x: 0, z: -1 },
    { forward: -1, turn: 0, x: 0, z: 1 },
    { forward: 0, turn: -1, x: -1, z: 0 },
    { forward: 0, turn: 1, x: 1, z: 0 },
    { forward: 1, turn: -1, x: -Math.SQRT1_2, z: -Math.SQRT1_2 },
    { forward: 1, turn: 1, x: Math.SQRT1_2, z: -Math.SQRT1_2 },
    { forward: -1, turn: -1, x: -Math.SQRT1_2, z: Math.SQRT1_2 },
    { forward: -1, turn: 1, x: Math.SQRT1_2, z: Math.SQRT1_2 },
  ])('moves toward ($x, $z) for input ($forward, $turn)', (direction) => {
    for (const rotation of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) {
      const { input, player, send, frame } = setup(mounted, rotation, 'world')
      input.input = { forward: direction.forward, turn: direction.turn }
      for (let i = 0; i < 180; i++) frame()
      const { x, z } = player.position
      expect(x * direction.x + z * direction.z).toBeGreaterThan(8)
      expect(Math.abs(x * direction.z - z * direction.x)).toBeLessThan(1)
      expect(
        angleDelta(Math.atan2(direction.x, direction.z), input.rotation)
      ).toBeCloseTo(0, 2)
      expect(input.actions.setMoved.mock.lastCall![0]).toBeCloseTo(
        input.config.maxSpeed,
        2
      )
      expect(send.mock.calls.every((call) => call[2] === 1)).toBe(true)
      expect(send.mock.calls.length).toBeLessThanOrEqual(mounted ? 12 : 6)
    }
  })

  it('recomputes a held direction immediately when the mode changes', () => {
    const { input, player, send, frame } = setup(mounted, Math.PI / 2)
    input.input = { forward: -1, turn: 0 }
    frame()
    const before = { ...player.position }
    input.movementMode = 'world'
    frame()
    expect(send).toHaveBeenCalledTimes(2)
    const [target, facing, forward] = send.mock.lastCall!
    expect(target.x).toBeCloseTo(before.x)
    expect(target.z).toBeCloseTo(before.z + (mounted ? 6.75 : 4))
    expect(facing).toBe(0)
    expect(forward).toBe(1)
    input.movementMode = 'character'
    frame()
    expect(send).toHaveBeenCalledTimes(3)
    expect(send.mock.lastCall![2]).toBe(mounted ? -1 : 1)
    if (!mounted) expect(angleDelta(0, input.rotation)).toBeCloseTo(-Math.PI)
  })

  it('keeps moving east after releasing and pressing right again', () => {
    const { input, player, frame } = setup(mounted, Math.PI, 'world')
    input.input = { forward: 0, turn: 1 }
    for (let i = 0; i < 120; i++) frame()
    input.input = null
    frame()
    const stopped = { ...player.position }
    input.input = { forward: 0, turn: 1 }
    for (let i = 0; i < 120; i++) frame()
    expect(player.position.x).toBeGreaterThan(stopped.x + 5)
    expect(player.position.z).toBeCloseTo(stopped.z, 1)
    expect(angleDelta(Math.PI / 2, input.rotation)).toBeCloseTo(0, 2)
  })
})

describe('walking virtual destinations', () => {
  it.each([
    { forward: 1, turn: 0 },
    { forward: -1, turn: 0 },
    { forward: 0, turn: -1 },
    { forward: 0, turn: 1 },
    { forward: 1, turn: -1 },
    { forward: 1, turn: 1 },
    { forward: -1, turn: -1 },
    { forward: -1, turn: 1 },
  ])('holds a straight path for input ($forward, $turn)', (direction) => {
    const { input, player, send, frame } = setup(false, Math.PI / 4)
    input.input = direction
    input.config.maxSpeed = 4.5
    input.config.acceleration = 9
    for (let i = 0; i < 120; i++) frame()
    const facing = Math.PI / 4 + Math.atan2(-direction.turn, direction.forward)
    const { x, z } = player.position
    expect(x * Math.cos(facing) - z * Math.sin(facing)).toBeCloseTo(0, 5)
    expect(x * Math.sin(facing) + z * Math.cos(facing)).toBeGreaterThan(7.5)
    expect(Math.hypot(x, z)).toBeLessThan(9)
    expect(input.rotation).toBe(facing)
    expect(send.mock.calls.every((call) => call[2] === 1)).toBe(true)
    expect(send.mock.calls.length).toBeLessThanOrEqual(5)
  })

  it('replaces the destination immediately when left changes to right', () => {
    const { input, player, send, frame } = setup()
    input.input = { forward: 0, turn: -1 }
    for (let i = 0; i < 30; i++) frame()
    const before = { ...player.position }
    const sent = send.mock.calls.length
    input.input = { forward: 0, turn: 1 }
    frame()
    expect(player.position.x).toBeCloseTo(before.x)
    expect(player.position.z).toBeGreaterThan(before.z)
    expect(input.rotation).toBe(0)
    expect(send).toHaveBeenCalledTimes(sent + 1)
    expect(send.mock.calls.at(-1)![0].z).toBeGreaterThan(before.z)
  })

  it('takes another 90-degree turn only after releasing and pressing left again', () => {
    const { input, player, frame } = setup(false, Math.PI)
    input.input = { forward: 0, turn: -1 }
    for (let i = 0; i < 60; i++) frame()
    expect(player.position.x).toBeLessThan(0)
    expect(player.position.z).toBeCloseTo(0)
    input.input = null
    frame()
    const stopped = { ...player.position }
    input.input = { forward: 0, turn: -1 }
    for (let i = 0; i < 60; i++) frame()
    expect(player.position.x).toBeCloseTo(stopped.x)
    expect(player.position.z).toBeGreaterThan(stopped.z)
    expect(angleDelta(0, input.rotation)).toBeCloseTo(0)
  })

  it('keeps the selected heading while blocked and resumes it when clear', () => {
    const { input, player, frame } = setup(false, Math.PI)
    input.input = { forward: 0, turn: -1 }
    for (let i = 0; i < 30; i++) frame()
    const stopped = { ...player.position }
    const facing = input.rotation
    input.isMovementBlocked.mockReturnValue(true)
    for (let i = 0; i < 60; i++) frame()
    expect(player.position).toEqual(stopped)
    expect(input.rotation).toBe(facing)
    input.isMovementBlocked.mockReturnValue(false)
    for (let i = 0; i < 30; i++) frame()
    expect(player.position.x).toBeLessThan(stopped.x)
    expect(player.position.z).toBeCloseTo(stopped.z)
    expect(input.rotation).toBe(facing)
  })

  it('keeps its chosen direction when starting a sprint', () => {
    const { input, frame } = setup(false, Math.PI)
    input.input = { forward: 0, turn: -1 }
    frame()
    const facing = input.rotation
    input.config.maxSpeed = 4.5
    for (let i = 0; i < 60; i++) frame()
    expect(input.rotation).toBe(facing)
  })
})

describe('keyboard target publication', () => {
  it('reuses a straight target and refreshes before reaching it', () => {
    const send = vi.fn()
    const sender = createKeyboardMoveSender(send)
    const position = { x: 0, y: 5, z: 0 }
    const input = { forward: 1, turn: 0 }
    const target = sender.target(
      position,
      0,
      input,
      13.5,
      1 / 60,
      true,
      'character'
    )
    sender.commitTarget()
    expect(target.position.z).toBe(6.75)
    expect(
      sender.target(
        { ...position, z: 1 },
        0,
        input,
        13.5,
        1 / 60,
        true,
        'character'
      )
    ).toBe(target)
    sender.commitTarget()
    expect(send).toHaveBeenCalledOnce()
    const renewed = sender.target(
      { ...position, z: 4 },
      0,
      input,
      13.5,
      1 / 60,
      true,
      'character'
    )
    expect(renewed.position.z).toBe(10.75)
    sender.commitTarget()
    expect(send).toHaveBeenCalledTimes(2)
  })

  it('changes travel direction immediately and limits sustained steering sends', () => {
    const { input, send, frame } = setup(true)
    input.input = { forward: 1, turn: 1 }
    for (let i = 0; i < 60; i++) frame()
    expect(send.mock.calls.length).toBeLessThanOrEqual(11)
    const before = send.mock.calls.length
    input.input = { forward: -1, turn: 0 }
    frame()
    expect(send).toHaveBeenCalledTimes(before + 1)
    expect(send.mock.calls.at(-1)![2]).toBe(-1)
  })

  it('wraps a forward target across the world seam', () => {
    const sender = createKeyboardMoveSender(vi.fn())
    const start = { x: WORLD_MAX_X - 1, y: 5, z: 0 }
    const target = sender.target(
      start,
      Math.PI / 2,
      { forward: 1, turn: 0 },
      13.5,
      1 / 60,
      true,
      'character'
    )
    expect(target.position.x).toBeLessThan(start.x)
    expect(shortestWrappedDeltaX(start.x, target.position.x)).toBe(6.75)
  })

  it('drops unpublished or superseded targets without sending a stop', () => {
    const send = vi.fn()
    const sender = createKeyboardMoveSender(send)
    const position = { x: 0, y: 5, z: 0 }
    sender.target(
      position,
      0,
      { forward: 1, turn: 0 },
      3,
      1 / 60,
      true,
      'character'
    )
    sender.flush(position, 0)
    expect(send).not.toHaveBeenCalled()
    sender.target(
      position,
      0,
      { forward: 1, turn: 0 },
      3,
      1 / 60,
      true,
      'character'
    )
    sender.commitTarget()
    sender.reset()
    sender.flush(position, 0)
    expect(send).toHaveBeenCalledOnce()
  })
})

describe('keyboard frame lifecycle', () => {
  it.each(['pickup', 'object'] as const)(
    'exits %s interaction and cancels click movement before moving',
    (interactionExit) => {
      const { input, frame } = setup()
      input.interactionExit = interactionExit
      input.hasMovementTarget = true
      frame()
      expect(
        input.actions[
          interactionExit === 'pickup'
            ? 'exitPickupInteraction'
            : 'exitObjectInteraction'
        ]
      ).toHaveBeenCalledOnce()
      expect(input.actions.clearClickMovement).toHaveBeenCalledOnce()
      expect(input.actions.cancelCombat).toHaveBeenCalledOnce()
    }
  )

  it('hands over to a mouse path without overriding its destination', () => {
    const { input, send, frame } = setup()
    frame()
    input.input = null
    input.hasMovementTarget = true
    frame()
    expect(send).toHaveBeenCalledOnce()
  })

  it('cancels combat before applying keyboard input', () => {
    const { input, frame } = setup()
    input.isInCombat = true
    frame()
    expect(input.actions.cancelCombat).toHaveBeenCalledOnce()
    expect(input.actions.markMoving).toHaveBeenCalledOnce()
  })

  it('clamps long frames and honors backward speed modifiers', () => {
    const { input, player } = setup(true)
    applyKeyboardMovement({
      ...input,
      currentPos: player.position,
      input: { forward: -1, turn: 0 },
      backwardSpeed: 0.75,
      deltaTimeSeconds: 2,
    })
    expect(player.position.z).toBeCloseTo(-0.075)
  })

  it('reports blocked and slope feedback', () => {
    const { input } = setup()
    applyKeyboardMovementOutcome({ kind: 'blocked' }, input.actions)
    expect(input.actions.stopMovement).toHaveBeenCalledOnce()
    applyKeyboardMovementOutcome({ kind: 'slope_blocked' }, input.actions)
    expect(input.actions.triggerJumpFeedback).toHaveBeenCalledOnce()
  })

  it('resets walking acceleration between sessions', () => {
    const ramp = createKeyboardSpeedRamp()
    expect(ramp.advance(DEFAULT_MOVEMENT_CONFIG, 0.25)).toBe(1.5)
    expect(ramp.advance(DEFAULT_MOVEMENT_CONFIG, 0.25)).toBe(3)
    ramp.reset()
    expect(ramp.advance(DEFAULT_MOVEMENT_CONFIG, 0.25)).toBe(1.5)
  })
})
