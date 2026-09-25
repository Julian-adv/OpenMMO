import { describe, expect, it, vi } from 'vitest'
import {
  applyMovementSubstrateOutcome,
  runMovementFrame,
  runPlayerMovementTick,
  syncPlayerTerrainHeight,
  type MovementOutcomeActions,
} from './movement-tick'
import { directPathing } from './pathing.fixture'
import {
  createKeyboardMoveSender,
  createKeyboardSpeedRamp,
  runKeyboardFrame,
} from './keyboard'
import { projectPlayerState } from './projection'
import {
  scaleMovementConfig,
  SPRINT_SPEED_MULT,
  type PlayerState,
} from '../../../utils/movementUtils'

function actions(): MovementOutcomeActions {
  return {
    stopMovement: vi.fn(),
    triggerJumpFeedback: vi.fn(),
    setNextWaypoint: vi.fn(),
    arrive: vi.fn(),
    continueMovement: vi.fn(),
  }
}

describe('syncPlayerTerrainHeight', () => {
  it('keeps non-interaction players aligned with terrain height', () => {
    const player = { position: { x: 1, y: 2, z: 3 } }

    const changed = syncPlayerTerrainHeight({
      player,
      hasHeightData: () => true,
      sampleHeight: () => 4,
    })

    expect(changed).toBe(true)
    expect(player.position.y).toBe(4)
  })

  it('keeps a posed player on the ground too, so a house floor offset that settles after the pose is followed', () => {
    const player = { position: { x: 1, y: 2, z: 3 } }

    const changed = syncPlayerTerrainHeight({
      player,
      hasHeightData: () => true,
      sampleHeight: () => 4,
    })

    expect(changed).toBe(true)
    expect(player.position.y).toBe(4)
  })

  it('does nothing without height data or meaningful y drift', () => {
    const player = { position: { x: 1, y: 2, z: 3 } }

    expect(
      syncPlayerTerrainHeight({
        player,
        hasHeightData: () => false,
        sampleHeight: () => 4,
      })
    ).toBe(false)

    expect(
      syncPlayerTerrainHeight({
        player,
        hasHeightData: () => true,
        sampleHeight: () => 2.0005,
      })
    ).toBe(false)
    expect(player.position.y).toBe(2)
  })
})

describe('applyMovementSubstrateOutcome', () => {
  it('stops movement on blocked outcomes', () => {
    const a = actions()

    applyMovementSubstrateOutcome({ kind: 'blocked' }, a)

    expect(a.stopMovement).toHaveBeenCalledOnce()
    expect(a.triggerJumpFeedback).not.toHaveBeenCalled()
  })

  it('stops movement and triggers jump feedback on slope blocks', () => {
    const a = actions()

    applyMovementSubstrateOutcome({ kind: 'slope_blocked' }, a)

    expect(a.stopMovement).toHaveBeenCalledOnce()
    expect(a.triggerJumpFeedback).toHaveBeenCalledOnce()
  })

  it('applies next waypoint state', () => {
    const a = actions()
    const movementTarget = { x: 1, y: 2, z: 3 }
    const movementState = {
      currentSpeed: 4,
      startPos: { x: 0, y: 0, z: 0 },
      targetPos: movementTarget,
      totalDistance: 5,
    }

    applyMovementSubstrateOutcome(
      {
        kind: 'next_waypoint',
        currentSpeed: 4,
        playerRotation: 0.5,
        movementTarget,
        movementState,
        currentWaypointIndex: 2,
      },
      a
    )

    expect(a.setNextWaypoint).toHaveBeenCalledWith(
      4,
      0.5,
      movementTarget,
      movementState,
      2
    )
  })

  it('routes arrival and continued outcomes', () => {
    const a = actions()

    applyMovementSubstrateOutcome(
      { kind: 'arrived', currentSpeed: 0, playerRotation: 1 },
      a
    )
    applyMovementSubstrateOutcome(
      {
        kind: 'continued',
        currentSpeed: 2,
        playerRotation: 3,
        totalDistance: 4,
      },
      a
    )

    expect(a.arrive).toHaveBeenCalledWith(0, 1)
    expect(a.continueMovement).toHaveBeenCalledWith(2, 3, 4)
  })
})

describe('runMovementFrame', () => {
  it('steps movement substrate and applies the outcome', () => {
    const a = actions()

    runMovementFrame({
      currentPos: { x: 0, y: 0, z: 0 },
      movementTarget: { x: 0.01, y: 0, z: 0 },
      movementState: {
        currentSpeed: 0,
        startPos: { x: 0, y: 0, z: 0 },
        targetPos: { x: 0.01, y: 0, z: 0 },
        totalDistance: 0.01,
      },
      pathWaypoints: [],
      currentWaypointIndex: 0,
      config: {
        maxSpeed: 3,
        acceleration: 6,
        deceleration: 6,
        arrivalThreshold: 0.05,
      },
      deltaTimeSeconds: 0.016,
      sampleHeight: () => 0,
      waypointHeight: () => 0,
      isMovementBlocked: () => false,
      isUphillTooSteep: () => false,
      setFloorLevel: vi.fn(),
      writePlayerPosition: vi.fn(),
      sendPlayerMove: vi.fn(),
      actions: a,
    })

    expect(a.arrive).toHaveBeenCalledOnce()
  })
})

function baseInput() {
  return {
    deltaTime: 16,
    currentPlayer: { health: 10, position: { x: 0, y: 0, z: 0 } },
    playerStateName: 'idle' as const,
    isMoving: false,
    currentSpeed: 0,
    movementTarget: null,
    movementState: null,
    pathWaypoints: [],
    currentWaypointIndex: 0,
    config: {
      maxSpeed: 3,
      acceleration: 6,
      deceleration: 6,
      arrivalThreshold: 0.05,
    },
    isInCombat: false,
    combatController: {
      targetMonsterId: null,
      update: vi.fn(),
    },
    cooldownMs: 1500,
    attackRange: 2,
    chaseGoal: null,
    chasePathing: directPathing(),
    getMonsterInfo: vi.fn(),
    attackLineBlocked: () => false,
    findMonsterPosition: vi.fn(),
    sampleHeight: () => 0,
    waypointHeight: () => 0,
    hasHeightData: () => true,
    isMovementBlocked: () => false,
    isUphillTooSteep: () => false,
    setFloorLevel: vi.fn(),
    writePlayerPosition: vi.fn(),
    sendPlayerMove: vi.fn(),
    actions: {
      transitionToDead: vi.fn(),
      transitionToRespawned: vi.fn(),
      resetStoppedSpeed: vi.fn(),
      combat: {
        stopMovingToIdle: vi.fn(),
        cancelBlockedMovement: vi.fn(),
        prepareReachedAttackRange: vi.fn(),
        beginAttack: vi.fn(),
        setChasingMovement: vi.fn(),
        showAttackState: vi.fn(),
        sendAttackCycle: vi.fn(),
      },
      movement: {
        stopMovement: vi.fn(),
        triggerJumpFeedback: vi.fn(),
        setNextWaypoint: vi.fn(),
        arrive: vi.fn(),
        continueMovement: vi.fn(),
      },
    },
  }
}

describe('runPlayerMovementTick', () => {
  it.each([
    { mounted: false, sprinting: false },
    { mounted: false, sprinting: true },
    { mounted: true, sprinting: false },
    { mounted: true, sprinting: true },
  ])(
    'preserves keyboard animation through the full frame ($mounted, $sprinting)',
    ({ mounted, sprinting }) => {
      const input = baseInput()
      input.config = scaleMovementConfig(
        input.config,
        (mounted ? 3 : 1) * (sprinting ? SPRINT_SPEED_MULT : 1)
      )
      let rotation = 0
      let keyboardMoving = false
      let hasKeysPressed = true
      let forward = 1
      let turn = 0
      let rendered: PlayerState
      const publish = () => {
        rendered = projectPlayerState({
          currentPosition: { ...input.currentPlayer.position },
          isMoving: input.isMoving,
          currentSpeed: input.currentSpeed,
          playerRotation: rotation,
          totalDistance: mounted && forward < 0 ? 0 : 100,
          hasTorch: false,
          isInCombat: false,
          attackCounter: 0,
          isSprinting: sprinting && (!mounted || forward > 0),
        })
      }
      input.actions.resetStoppedSpeed.mockImplementation(() => {
        input.currentSpeed = 0
        publish()
      })
      const moveSender = createKeyboardMoveSender(vi.fn())
      const speedRamp = createKeyboardSpeedRamp()
      const frame = () => {
        runKeyboardFrame({
          ...input,
          isKeyboardMoving: keyboardMoving,
          interactionExit: 'none',
          hasMovementTarget: false,
          input: hasKeysPressed ? { forward, turn } : null,
          movementMode: 'character',
          rotation,
          config: {
            ...input.config,
            ...(mounted ? { mountRotation: rotation } : {}),
          },
          deltaTimeSeconds: input.deltaTime / 1000,
          writePlayerPosition: (position, facing) => {
            input.currentPlayer.position = position
            rotation = facing
          },
          moveSender,
          speedRamp,
          actions: {
            exitPickupInteraction: vi.fn(),
            exitObjectInteraction: vi.fn(),
            clearClickMovement: vi.fn(),
            cancelCombat: vi.fn(),
            markMoving: () => {
              keyboardMoving = input.isMoving = true
            },
            setKeyboardIdleRuntime: () => {
              keyboardMoving = input.isMoving = false
              input.currentSpeed = 0
            },
            emitKeyboardPlayerState: publish,
            stopMovement: vi.fn(),
            triggerJumpFeedback: vi.fn(),
            setMoved: (speed, facing) => {
              input.currentSpeed = speed
              rotation = facing
            },
          },
        })
        runPlayerMovementTick({ ...input, playerStateName: rendered.state })
        return rendered
      }

      for (let i = 0; i < 60; i++) {
        const state = frame()
        expect(state.state).toBe('moving')
        expect(state.speed).toBeGreaterThan(0)
        expect(state.movementMode).toBe(sprinting ? 'run' : 'jog')
      }
      expect(input.currentPlayer.position.z).toBeGreaterThan(0)
      expect(input.actions.resetStoppedSpeed).not.toHaveBeenCalled()
      expect(input.writePlayerPosition).not.toHaveBeenCalled()

      forward = -1
      const backward = frame()
      expect(backward.state).toBe('moving')
      expect(backward.speed).toBeCloseTo(mounted ? 1.5 : input.config.maxSpeed)
      expect(backward.movementMode).toBe(
        mounted ? 'walk' : sprinting ? 'run' : 'jog'
      )

      forward = 0
      turn = 1
      const horizontal = frame()
      expect(horizontal.state).toBe(mounted ? 'idle' : 'moving')
      expect(horizontal.speed).toBeCloseTo(mounted ? 0 : input.config.maxSpeed)
      expect(horizontal.rotation).not.toBe(backward.rotation)

      forward = 1
      turn = 0
      expect(frame().state).toBe('moving')
      hasKeysPressed = false
      const stopped = frame()
      expect(stopped.state).toBe('idle')
      expect(stopped.speed).toBe(0)
    }
  )

  it('keeps the route while waiting for the world snapshot', () => {
    const input = {
      ...baseInput(),
      canAdvance: () => false,
      isMoving: true,
      movementTarget: { x: 10, y: 0, z: 0 },
      movementState: {
        currentSpeed: 3,
        startPos: { x: 0, y: 0, z: 0 },
        targetPos: { x: 10, y: 0, z: 0 },
        totalDistance: 10,
      },
    }
    runPlayerMovementTick(input)
    expect(input.writePlayerPosition).not.toHaveBeenCalled()
    expect(input.sendPlayerMove).not.toHaveBeenCalled()
    expect(input.actions.movement.stopMovement).not.toHaveBeenCalled()
    expect(input.actions.movement.setNextWaypoint).not.toHaveBeenCalled()
  })

  it('transitions dead players before terrain or combat processing', () => {
    const input = {
      ...baseInput(),
      currentPlayer: { health: 0, position: { x: 0, y: 3, z: 0 } },
    }

    runPlayerMovementTick(input)

    expect(input.actions.transitionToDead).toHaveBeenCalledOnce()
    expect(input.currentPlayer.position.y).toBe(3)
  })

  it('recovers respawned players if the control state is still dead', () => {
    const input = {
      ...baseInput(),
      playerStateName: 'dead' as const,
      currentPlayer: { health: 10, position: { x: 0, y: 3, z: 0 } },
    }

    runPlayerMovementTick(input)

    expect(input.actions.transitionToRespawned).toHaveBeenCalledOnce()
    expect(input.actions.resetStoppedSpeed).not.toHaveBeenCalled()
    expect(input.currentPlayer.position.y).toBe(3)
  })

  it('resets stale speed when there is no active movement', () => {
    const input = {
      ...baseInput(),
      currentSpeed: 1,
    }

    runPlayerMovementTick(input)

    expect(input.actions.resetStoppedSpeed).toHaveBeenCalledOnce()
  })

  it('runs movement when runtime movement is active', () => {
    const input = {
      ...baseInput(),
      isMoving: true,
      movementTarget: { x: 0.01, y: 0, z: 0 },
      movementState: {
        currentSpeed: 0,
        startPos: { x: 0, y: 0, z: 0 },
        targetPos: { x: 0.01, y: 0, z: 0 },
        totalDistance: 0.01,
      },
    }

    runPlayerMovementTick(input)

    expect(input.actions.movement.arrive).toHaveBeenCalledOnce()
  })
})
