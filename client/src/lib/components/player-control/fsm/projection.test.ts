import { describe, expect, it } from 'vitest'
import type { PlayerState } from '../../../utils/movementUtils'
import {
  projectPlayerState,
  projectStoppedPlayerState,
  shouldEmitProjectedPlayerState,
} from './projection'

const idleState: PlayerState = {
  state: 'idle',
  speed: 0,
  rotation: 0,
  position: { x: 0, y: 0, z: 0 },
}

describe('projectPlayerState', () => {
  it('keeps stationary keyboard steering idle while publishing the new facing', () => {
    const state = projectPlayerState({
      currentPosition: idleState.position,
      isMoving: true,
      currentSpeed: 0,
      playerRotation: 0.5,
      hasTorch: false,
      isInCombat: false,
      attackCounter: 0,
      isSprinting: false,
    })
    expect(state.state).toBe('idle')
    expect(state.movementMode).toBeUndefined()
    expect(state.rotation).toBe(0.5)
    expect(shouldEmitProjectedPlayerState(idleState, state)).toBe(true)
  })

  it('projects click movement mode from movement distance', () => {
    const state = projectPlayerState({
      currentPosition: { x: 1, y: 2, z: 3 },
      isMoving: true,
      currentSpeed: 2.5,
      playerRotation: 0.25,
      totalDistance: 4,
      hasTorch: false,
      isInCombat: false,
      attackCounter: 0,
      isSprinting: false,
    })

    expect(state).toEqual({
      state: 'moving',
      speed: 2.5,
      rotation: 0.25,
      position: { x: 1, y: 2, z: 3 },
      movementMode: 'jog',
      attackCounter: undefined,
    })
  })

  it('projects combat movement as run with attack counter', () => {
    const state = projectPlayerState({
      currentPosition: { x: 0, y: 0, z: 0 },
      isMoving: true,
      currentSpeed: 3,
      playerRotation: 1,
      hasTorch: false,
      isInCombat: true,
      attackCounter: 7,
      isSprinting: false,
    })

    expect(state.movementMode).toBe('run')
    expect(state.attackCounter).toBe(7)
  })

  it('projects sprinting as run', () => {
    const state = projectPlayerState({
      currentPosition: { x: 0, y: 0, z: 0 },
      isMoving: true,
      currentSpeed: 4.5,
      playerRotation: 0,
      totalDistance: 20,
      hasTorch: false,
      isInCombat: false,
      attackCounter: 0,
      isSprinting: true,
    })

    expect(state.movementMode).toBe('run')
  })

  it('reserves torch run for sprinting during combat', () => {
    const walking = projectPlayerState({
      currentPosition: { x: 0, y: 0, z: 0 },
      isMoving: true,
      currentSpeed: 3,
      playerRotation: 0,
      hasTorch: true,
      isInCombat: true,
      attackCounter: 1,
      isSprinting: false,
    })
    const sprinting = projectPlayerState({
      currentPosition: { x: 0, y: 0, z: 0 },
      isMoving: true,
      currentSpeed: 4.5,
      playerRotation: 0,
      hasTorch: true,
      isInCombat: true,
      attackCounter: 1,
      isSprinting: true,
    })

    expect(walking.movementMode).toBe('walk')
    expect(sprinting.movementMode).toBe('run')
  })
})

describe('projectStoppedPlayerState', () => {
  it('ends movement at the server pose', () => {
    expect(
      projectStoppedPlayerState(
        { ...idleState, state: 'moving', speed: 3, movementMode: 'jog' },
        { x: 1, y: 2, z: 3 },
        0.5
      )
    ).toEqual({
      ...idleState,
      position: { x: 1, y: 2, z: 3 },
      rotation: 0.5,
      movementMode: undefined,
    })
  })

  it('keeps a pickup animation running when its approach stop is acknowledged', () => {
    const pickupState: PlayerState = {
      ...idleState,
      state: 'interact',
      interactionAnim: 'pickup',
      interactionCounter: 2,
    }
    expect(
      projectStoppedPlayerState(pickupState, { x: 1, y: 0, z: 0 }, 0.5)
    ).toEqual({
      ...pickupState,
      position: { x: 1, y: 0, z: 0 },
      rotation: 0.5,
      movementMode: undefined,
    })
  })
})

describe('shouldEmitProjectedPlayerState', () => {
  it('ignores y-only projection changes', () => {
    const next: PlayerState = {
      ...idleState,
      position: { x: 0, y: 10, z: 0 },
    }

    expect(shouldEmitProjectedPlayerState(idleState, next)).toBe(false)
  })

  it('emits when movement mode changes', () => {
    const next: PlayerState = {
      ...idleState,
      movementMode: 'walk',
    }

    expect(shouldEmitProjectedPlayerState(idleState, next)).toBe(true)
  })
})
