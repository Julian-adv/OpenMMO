import { afterEach, describe, expect, it, vi } from 'vitest'
import { groundItemManager } from '../../../managers/groundItemManager'
import type { ClickIntent } from '../../../managers/inputHandler'
import type { PlayerState } from '../../../utils/movementUtils'
import {
  beginObjectInteraction,
  beginPickupInteraction,
  exitObjectInteraction,
  exitPickupInteraction,
  finishPendingPickup,
  getInteractionExitKind,
  handleInteractKey,
  handlePickupGrab,
  shouldFinishPendingPickup,
} from './interaction'
import { buildInteractState, buildPickupState } from '../player-state-builders'

const previousPlayerState: PlayerState = {
  state: 'idle',
  speed: 3,
  rotation: 0,
  position: { x: 0, y: 0, z: 0 },
}

const intent: Extract<ClickIntent, { type: 'interact_object' }> = {
  type: 'interact_object',
  objectType: 'chair',
  objectId: 1,
  interaction: 'sit',
  position: { x: 10, y: 2, z: 20 },
  rotation: 1.5,
  interactOffset: { x: 0.25, y: 0.5, z: -0.75 },
}

describe('beginObjectInteraction', () => {
  it('builds the animation from the approved pose', () => {
    const cancelCombat = vi.fn()

    const result = beginObjectInteraction({
      intent,
      previousPlayerState,
      cancelCombat,
    })

    expect(cancelCombat).toHaveBeenCalledOnce()
    expect(result.playerRotation).toBe(1.5)
    expect(result.nextPlayerState).toEqual({
      ...previousPlayerState,
      state: 'interact',
      speed: 0,
      rotation: 1.5,
      position: { x: 10, y: 2, z: 20 },
      interactionAnim: 'sit',
      interactionCounter: 1,
      interactOffsetY: 0.5,
    })
  })
})

describe('exitObjectInteraction', () => {
  it('builds idle-after-interact state', () => {
    const interactingState: PlayerState = {
      ...previousPlayerState,
      state: 'interact',
      speed: 0,
      interactionAnim: 'sit',
      interactOffsetY: 0.5,
    }

    expect(exitObjectInteraction(interactingState)).toEqual({
      ...interactingState,
      state: 'idle',
      speed: 0,
      interactionAnim: undefined,
      interactOffsetY: undefined,
    })
  })
})

describe('interactionCounter', () => {
  it('increments on every build so a repeated anim still re-triggers', () => {
    const first = buildPickupState(previousPlayerState)
    const second = buildInteractState(
      first,
      { x: 0, y: 0, z: 0 },
      0,
      'pickup',
      0
    )

    expect(first.interactionCounter).toBe(1)
    expect(second.interactionCounter).toBe(2)
  })
})

const pickupIdleState: PlayerState = {
  state: 'idle',
  speed: 1,
  rotation: 0,
  position: { x: 0, y: 0, z: 0 },
  interactOffsetY: 5,
}

describe('beginPickupInteraction', () => {
  it('ignores dead players', () => {
    const beginPickup = vi.fn()

    const result = beginPickupInteraction({
      instanceId: 1,
      previousPlayerState: { ...pickupIdleState, state: 'dead' },
      hasGroundItem: vi.fn(() => true),
      beginPickup,
      cancelCombat: vi.fn(),
    })

    expect(result.kind).toBe('ignored')
    expect(beginPickup).not.toHaveBeenCalled()
  })

  it('ignores missing items', () => {
    const beginPickup = vi.fn()

    const result = beginPickupInteraction({
      instanceId: 1,
      previousPlayerState: pickupIdleState,
      hasGroundItem: vi.fn(() => false),
      beginPickup,
      cancelCombat: vi.fn(),
    })

    expect(result.kind).toBe('ignored')
    expect(beginPickup).not.toHaveBeenCalled()
  })

  it('starts pickup and clears movement runtime fields touched by current behavior', () => {
    const beginPickup = vi.fn()
    const cancelCombat = vi.fn()

    const result = beginPickupInteraction({
      instanceId: 42,
      previousPlayerState: pickupIdleState,
      hasGroundItem: vi.fn(() => true),
      beginPickup,
      cancelCombat,
    })

    expect(result.kind).toBe('started')
    if (result.kind !== 'started') return
    expect(beginPickup).toHaveBeenCalledWith(42)
    expect(cancelCombat).toHaveBeenCalledOnce()
    expect(result.pendingPickupInstanceId).toBe(42)
    expect(result.isMoving).toBe(false)
    expect(result.movementTarget).toBeNull()
    expect(result.movementState).toBeNull()
    expect(result.currentSpeed).toBe(0)
    expect(result.nextPlayerState).toEqual({
      ...pickupIdleState,
      state: 'interact',
      speed: 0,
      interactionAnim: 'pickup',
      interactionCounter: 1,
      interactOffsetY: 0,
    })
  })
})

const baseState: PlayerState = {
  state: 'idle',
  speed: 0,
  rotation: 0,
  position: { x: 0, y: 0, z: 0 },
}

describe('shouldFinishPendingPickup', () => {
  it('keeps pending pickup while pickup interaction is active', () => {
    expect(
      shouldFinishPendingPickup(1, {
        ...baseState,
        state: 'interact',
        interactionAnim: 'pickup',
      })
    ).toBe(false)
  })

  it('finishes pending pickup after leaving pickup interaction', () => {
    expect(shouldFinishPendingPickup(1, baseState)).toBe(true)
  })
})

describe('finishPendingPickup', () => {
  it('finishes and clears pending pickup', () => {
    const finishPickup = vi.fn()

    const next = finishPendingPickup(42, finishPickup)

    expect(next).toBeNull()
    expect(finishPickup).toHaveBeenCalledWith(42)
  })
})

describe('handlePickupGrab', () => {
  afterEach(() => groundItemManager.reset())

  it('sends normal pickup items to the server', () => {
    const actions = {
      setInHand: vi.fn(),
      remove: vi.fn(),
      sendPickupItem: vi.fn(),
      sendPickupNearbyItems: vi.fn(),
    }

    handlePickupGrab(42, actions)

    expect(actions.setInHand).toHaveBeenCalledWith(42)
    expect(actions.sendPickupItem).toHaveBeenCalledWith(42)
    expect(actions.sendPickupNearbyItems).not.toHaveBeenCalled()
    expect(actions.remove).not.toHaveBeenCalled()
  })

  it('holds the nearest item while requesting a nearby pickup', () => {
    const actions = {
      setInHand: vi.fn(),
      remove: vi.fn(),
      sendPickupItem: vi.fn(),
      sendPickupNearbyItems: vi.fn(),
    }

    handlePickupGrab(42, actions, true)

    expect(actions.setInHand).toHaveBeenCalledWith(42)
    expect(actions.sendPickupNearbyItems).toHaveBeenCalledOnce()
    expect(actions.sendPickupItem).not.toHaveBeenCalled()
    expect(actions.remove).not.toHaveBeenCalled()
    expect(actions.setInHand.mock.invocationCallOrder[0]).toBeLessThan(
      actions.sendPickupNearbyItems.mock.invocationCallOrder[0]!
    )
  })

  it.each([true, false])(
    'settles the held nearby item after the animation (server accepted: %s)',
    (accepted) => {
      groundItemManager.spawn({
        instance_id: 42,
        item_def_id: 'worn_bard_helmet',
        quantity: 1,
        enchant: 0,
        position: { x: 0, y: 0, z: 0 },
        floor_level: 0,
        dropped_by: null,
        claimed_by: null,
      })
      beginPickupInteraction({
        instanceId: 42,
        previousPlayerState: pickupIdleState,
        hasGroundItem: (id) => groundItemManager.items.has(id),
        beginPickup: (id) => groundItemManager.beginPickup(id),
        cancelCombat: vi.fn(),
      })

      handlePickupGrab(
        42,
        {
          setInHand: (id) => groundItemManager.setInHand(id),
          remove: (id) => groundItemManager.remove(id),
          sendPickupItem: vi.fn(),
          sendPickupNearbyItems: () => {
            if (accepted) groundItemManager.remove(42)
          },
        },
        true
      )
      expect(groundItemManager.items.get(42)?.inHand).toBe(true)

      finishPendingPickup(42, (id) => groundItemManager.finishPickup(id))
      if (accepted) expect(groundItemManager.items.has(42)).toBe(false)
      else expect(groundItemManager.items.get(42)?.inHand).toBe(false)
    }
  )

  it('removes local temporary pickup items without sending', () => {
    const actions = {
      setInHand: vi.fn(),
      remove: vi.fn(),
      sendPickupItem: vi.fn(),
      sendPickupNearbyItems: vi.fn(),
    }

    handlePickupGrab(-1, actions)

    expect(actions.setInHand).toHaveBeenCalledWith(-1)
    expect(actions.remove).toHaveBeenCalledWith(-1)
    expect(actions.sendPickupItem).not.toHaveBeenCalled()
    expect(actions.sendPickupNearbyItems).not.toHaveBeenCalled()
  })
})

describe('exitPickupInteraction', () => {
  it('ignores non-pickup states', () => {
    expect(exitPickupInteraction(baseState)).toEqual({ kind: 'ignored' })
  })

  it('returns idle-after-interact state for active pickup interaction', () => {
    const state: PlayerState = {
      ...baseState,
      state: 'interact',
      interactionAnim: 'pickup',
      interactOffsetY: 0,
    }

    const outcome = exitPickupInteraction(state)

    expect(outcome.kind).toBe('exited')
    if (outcome.kind !== 'exited') return
    expect(outcome.nextPlayerState).toEqual({
      ...state,
      state: 'idle',
      speed: 0,
      interactionAnim: undefined,
      interactOffsetY: undefined,
    })
  })
})

const currentPlayer = {
  health: 10,
  position: { x: 1, y: 2, z: 3 },
}

describe('handleInteractKey', () => {
  it('does nothing when the interact key was not pressed', () => {
    const sendToggleDoor = vi.fn()

    const handled = handleInteractKey({
      currentPlayer,
      consumeInteract: vi.fn(() => false),
      findNearestDoor: vi.fn(),
      sendToggleDoor,
    })

    expect(handled).toBe(false)
    expect(sendToggleDoor).not.toHaveBeenCalled()
  })

  it('does nothing for dead players', () => {
    const consumeInteract = vi.fn()

    const handled = handleInteractKey({
      currentPlayer: { ...currentPlayer, health: 0 },
      consumeInteract,
      findNearestDoor: vi.fn(),
      sendToggleDoor: vi.fn(),
    })

    expect(handled).toBe(false)
    expect(consumeInteract).not.toHaveBeenCalled()
  })

  it('toggles the nearest door when available', () => {
    const sendToggleDoor = vi.fn()

    const handled = handleInteractKey({
      currentPlayer,
      consumeInteract: vi.fn(() => true),
      findNearestDoor: vi.fn(() => ({
        houseId: 'house-1',
        roomIndex: 2,
        wallDir: 'north' as const,
        segmentIndex: 3,
      })),
      sendToggleDoor,
    })

    expect(handled).toBe(true)
    expect(sendToggleDoor).toHaveBeenCalledWith('house-1', 2, 'north', 3)
  })
})

describe('getInteractionExitKind', () => {
  it('returns none outside interaction state', () => {
    expect(getInteractionExitKind(baseState)).toBe('none')
  })

  it('returns pickup for pickup interaction', () => {
    expect(
      getInteractionExitKind({
        ...baseState,
        state: 'interact',
        interactionAnim: 'pickup',
      })
    ).toBe('pickup')
  })

  it('returns object for non-pickup interaction', () => {
    expect(
      getInteractionExitKind({
        ...baseState,
        state: 'interact',
        interactionAnim: 'sit',
      })
    ).toBe('object')
  })
})
