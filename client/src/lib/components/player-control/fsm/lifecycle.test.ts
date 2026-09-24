import { describe, expect, it } from 'vitest'
import type { PlayerState } from '../../../utils/movementUtils'
import {
  resetMovementRuntimeState,
  resetRespawnRuntimeState,
  transitionToDeadState,
  transitionToRespawnedState,
} from './lifecycle'

describe('transition reset helpers', () => {
  it('resets movement runtime state', () => {
    expect(resetMovementRuntimeState()).toEqual({
      currentSpeed: 0,
    })
  })

  it('resets respawn runtime state including rotation', () => {
    expect(resetRespawnRuntimeState()).toEqual({
      currentSpeed: 0,
      playerRotation: 0,
    })
  })
})

const movingState: PlayerState = {
  state: 'moving',
  speed: 3,
  rotation: 1,
  position: { x: 1, y: 2, z: 3 },
  movementMode: 'run',
  attackCounter: 4,
}

describe('transitionToDeadState', () => {
  it('ignores repeated dead transitions', () => {
    expect(transitionToDeadState({ ...movingState, state: 'dead' })).toEqual({
      kind: 'ignored_already_dead',
    })
  })

  it('returns dead player state and reset runtime', () => {
    const result = transitionToDeadState(movingState)

    expect(result.kind).toBe('dead')
    if (result.kind !== 'dead') return
    expect(result.runtime.currentSpeed).toBe(0)
    expect(result.nextPlayerState).toEqual({
      ...movingState,
      state: 'dead',
      speed: 0,
      movementMode: undefined,
    })
  })
})

describe('transitionToRespawnedState', () => {
  it('returns idle respawn state and reset runtime', () => {
    const position = { x: 10, y: 0, z: 20 }

    const result = transitionToRespawnedState(movingState, position)

    expect(result.runtime.playerRotation).toBe(0)
    expect(result.nextPlayerState).toEqual({
      ...movingState,
      state: 'idle',
      speed: 0,
      rotation: 0,
      movementMode: undefined,
      attackCounter: 0,
      position,
    })
  })
})
