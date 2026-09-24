import { describe, expect, it, vi } from 'vitest'
import {
  beginAttack,
  transitionAttackToIdle,
  ensureAttackState,
} from './combat'
import type { PlayerState } from '../../../utils/movementUtils'
const playerState: PlayerState = {
  state: 'idle',
  speed: 0,
  rotation: 0,
  position: { x: 0, y: 0, z: 0 },
}
describe('beginAttack', () => {
  function runBeginAttack(
    overrides: Partial<Parameters<typeof beginAttack>[0]>
  ) {
    const calls = {
      beginCombat: vi.fn(() => 1),
      stopAndFace: vi.fn(),
      sendPlayerAttack: vi.fn(),
    }
    const result = beginAttack({
      monsterId: 'm1',
      monsterInfo: { state: 'idle' },
      currentPosition: { x: 1, y: 0, z: 2 },
      playerRotation: 0.5,
      previousPlayerState: playerState,
      ...calls,
      ...overrides,
    })
    return { ...calls, result }
  }

  it('ignores dead targets', () => {
    const { beginCombat, result } = runBeginAttack({
      monsterInfo: { state: 'dead' },
    })

    expect(result.kind).toBe('ignored_unattackable_target')
    expect(beginCombat).not.toHaveBeenCalled()
  })

  it('ignores targets with no local data', () => {
    const { beginCombat, sendPlayerAttack, result } = runBeginAttack({
      monsterInfo: undefined,
    })

    expect(result.kind).toBe('ignored_unattackable_target')
    expect(beginCombat).not.toHaveBeenCalled()
    expect(sendPlayerAttack).not.toHaveBeenCalled()
  })

  it('starts combat, stops movement, sends attack, and returns attack state', () => {
    const currentPosition = { x: 1, y: 0, z: 2 }
    const { beginCombat, stopAndFace, sendPlayerAttack, result } =
      runBeginAttack({ currentPosition })

    expect(beginCombat).toHaveBeenCalledWith('m1', true)
    expect(stopAndFace).toHaveBeenCalledWith(0.5)
    expect(sendPlayerAttack).toHaveBeenCalledWith('m1')
    expect(result).toEqual({
      kind: 'started',
      nextPlayerState: {
        ...playerState,
        state: 'attack',
        rotation: 0.5,
        attackCounter: 1,
      },
    })
  })

  it('stops movement even when position and facing are unchanged', () => {
    const { stopAndFace } = runBeginAttack({
      currentPosition: { x: 1, y: 10, z: 2 },
      playerRotation: playerState.rotation,
    })

    expect(stopAndFace).toHaveBeenCalledWith(0)
  })

  it('syncs the new facing even when the position is unchanged', () => {
    const currentPosition = { x: 1, y: 10, z: 2 }
    const { stopAndFace } = runBeginAttack({
      currentPosition,
      playerRotation: 1.5,
    })

    expect(stopAndFace).toHaveBeenCalledWith(1.5)
  })
})

describe('transitionAttackToIdle', () => {
  it('ignores non-attack states', () => {
    expect(transitionAttackToIdle(playerState)).toEqual({
      kind: 'ignored',
    })
  })

  it('builds idle state after attack', () => {
    const attackState: PlayerState = {
      ...playerState,
      state: 'attack',
      attackCounter: 2,
    }

    expect(transitionAttackToIdle(attackState)).toEqual({
      kind: 'idle',
      nextPlayerState: {
        ...attackState,
        state: 'idle',
        speed: 0,
        attackCounter: 0,
      },
    })
  })
})

describe('ensureAttackState', () => {
  it('ignores already attacking states', () => {
    expect(
      ensureAttackState({ ...playerState, state: 'attack' }, 1, 3)
    ).toEqual({
      kind: 'ignored',
    })
  })

  it('builds attack state when not already attacking', () => {
    expect(ensureAttackState(playerState, 1.25, 3)).toEqual({
      kind: 'attack',
      nextPlayerState: {
        ...playerState,
        state: 'attack',
        rotation: 1.25,
        attackCounter: 3,
      },
    })
  })
})

// The bow's reach; the server gates the same shot on items.json `range`.
