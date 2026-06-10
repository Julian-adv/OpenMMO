import { describe, expect, it, vi } from 'vitest'
import type { ClickIntent } from '../../managers/inputHandler'
import { PLAYER_PICKUP_RANGE_METERS } from '../../data/combatTiming'
import {
  dispatchCanvasClickIntent,
  type CanvasClickActions,
} from './canvas-click-dispatcher'

function makeActions() {
  return {
    attackInRange: vi.fn(),
    chaseAndAttack: vi.fn(),
    toggleDoor: vi.fn(),
    enterInteraction: vi.fn(),
    enterPickup: vi.fn(),
    approachAndPickup: vi.fn(),
    interactNpc: vi.fn(),
    moveToGround: vi.fn(),
  } satisfies CanvasClickActions
}

describe('dispatchCanvasClickIntent pickup handling', () => {
  it('starts pickup immediately when the ground item is within pickup range', () => {
    const actions = makeActions()
    const intent: ClickIntent = {
      type: 'pickup_ground_item',
      instanceId: 42,
      position: { x: 1, y: 0, z: 2 },
      distance: PLAYER_PICKUP_RANGE_METERS,
    }

    dispatchCanvasClickIntent(intent, false, actions)

    expect(actions.enterPickup).toHaveBeenCalledWith(42)
    expect(actions.approachAndPickup).not.toHaveBeenCalled()
  })

  it('moves toward the ground item before pickup when it is out of range', () => {
    const actions = makeActions()
    const intent: ClickIntent = {
      type: 'pickup_ground_item',
      instanceId: 42,
      position: { x: 1, y: 0, z: 2 },
      distance: PLAYER_PICKUP_RANGE_METERS + 0.01,
    }

    dispatchCanvasClickIntent(intent, false, actions)

    expect(actions.approachAndPickup).toHaveBeenCalledWith(intent)
    expect(actions.enterPickup).not.toHaveBeenCalled()
  })
})
