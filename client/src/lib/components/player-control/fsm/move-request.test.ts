import { describe, expect, it, vi } from 'vitest'
import { prepareMoveRequest } from './move-request'

describe('move request preparation', () => {
  it('waits for an object exit and refuses movement while dead or using the keyboard', () => {
    const actions = { exitPickupAndRetry: vi.fn(), exitObjectAndDelay: vi.fn() }
    const input = {
      currentPlayerHealth: 100,
      hasCurrentPlayer: true,
      hasKeyboardInput: false,
      interactionExit: 'none' as const,
    }
    expect(prepareMoveRequest(input, actions)).toBe(true)
    expect(
      prepareMoveRequest({ ...input, currentPlayerHealth: 0 }, actions)
    ).toBe(false)
    expect(
      prepareMoveRequest({ ...input, hasKeyboardInput: true }, actions)
    ).toBe(false)
    expect(
      prepareMoveRequest({ ...input, interactionExit: 'object' }, actions)
    ).toBe(false)
    expect(actions.exitObjectAndDelay).toHaveBeenCalledOnce()
  })
})
