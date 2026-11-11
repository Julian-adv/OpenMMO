import { vi } from 'vitest'
import type { CanvasClickActions } from './canvas-click-dispatcher'

export function makeCanvasClickActions() {
  return {
    attackInRange: vi.fn(),
    chaseAndAttack: vi.fn(),
    toggleDoor: vi.fn(),
    toggleDungeonDoor: vi.fn(),
    interactObject: vi.fn(),
    pickupItem: vi.fn(),
    interactNpc: vi.fn(),
    breakProp: vi.fn(),
    mineProp: vi.fn(),
    pushCart: vi.fn(),
    openProp: vi.fn(),
    moveToGround: vi.fn(),
    castFishing: vi.fn(),
    tipHat: vi.fn(),
    tradeAtStall: vi.fn(),
    eatMeal: vi.fn(),
    inspectAltar: vi.fn(),
    openNoticeBoard: vi.fn(),
    toggleObjectDoor: vi.fn(),
    openMailbox: vi.fn(),
  } satisfies CanvasClickActions
}
