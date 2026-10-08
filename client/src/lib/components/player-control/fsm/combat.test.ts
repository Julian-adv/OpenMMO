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
      beginCombat: vi.fn(() => ({ attackCounter: 1, startRequested: true })),
      stopAndFace: vi.fn(),
      startPlayerAttack: vi.fn(),
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
    const { beginCombat, startPlayerAttack, result } = runBeginAttack({
      monsterInfo: undefined,
    })

    expect(result.kind).toBe('ignored_unattackable_target')
    expect(beginCombat).not.toHaveBeenCalled()
    expect(startPlayerAttack).not.toHaveBeenCalled()
  })

  it('starts combat, stops movement, sends attack, and returns attack state', () => {
    const currentPosition = { x: 1, y: 0, z: 2 }
    const { beginCombat, stopAndFace, startPlayerAttack, result } =
      runBeginAttack({ currentPosition })

    expect(beginCombat).toHaveBeenCalledWith('m1', true)
    expect(stopAndFace).toHaveBeenCalledWith(0.5)
    expect(startPlayerAttack).toHaveBeenCalledWith('m1')
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

  it('does not resend a start request for an active attack', () => {
    const { startPlayerAttack, stopAndFace, result } = runBeginAttack({
      beginCombat: vi.fn(() => ({ attackCounter: 3, startRequested: false })),
    })

    expect(startPlayerAttack).not.toHaveBeenCalled()
    expect(stopAndFace).toHaveBeenCalledWith(0.5)
    expect(result).toMatchObject({
      kind: 'started',
      nextPlayerState: { state: 'attack', attackCounter: 3 },
    })
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
