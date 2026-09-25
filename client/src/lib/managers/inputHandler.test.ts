import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as THREE from 'three'

const MIN_FISHABLE_DEPTH_M = 0.3
const MAX_CAST_DISTANCE_M = 8

vi.mock('../wasm/onlinerpg_shared', () => ({
  min_fishable_depth_m: () => MIN_FISHABLE_DEPTH_M,
  max_cast_distance_m: () => MAX_CAST_DISTANCE_M,
}))

import {
  inputHandler,
  movementInput,
  shouldSuppressContextMenu,
  type RaycastContext,
} from './inputHandler'
import { resetFishingStore } from '../stores/fishingStore'
import {
  closeInstrumentPanel,
  openInstrumentPanel,
} from '../stores/instrumentStore'
import { alwaysRun } from '../stores/movementSettings'
import { estateFurnitureInteractionData } from '../utils/estateFurnitureModels'
import { KeyboardDirectionSender } from '../components/player-control/keyboard-direction'

describe('movement keys', () => {
  beforeEach(() => {
    vi.stubGlobal('HTMLElement', class {})
    resetFishingStore()
    closeInstrumentPanel()
    inputHandler.clearTransientInput()
  })

  afterEach(() => {
    inputHandler.clearTransientInput()
    vi.unstubAllGlobals()
  })

  it.each([
    ['ArrowUp', 'KeyW', 1, 0],
    ['ArrowDown', 'KeyS', -1, 0],
    ['ArrowLeft', 'KeyA', 0, -1],
    ['ArrowRight', 'KeyD', 0, 1],
  ] as const)(
    'maps %s and %s to the same controls',
    (arrow, key, forward, turn) => {
      expect(movementInput(new Set([arrow]))).toEqual({ forward, turn })
      expect(movementInput(new Set([key]))).toEqual({ forward, turn })
      expect(movementInput(new Set([arrow, key]))).toEqual({ forward, turn })
    }
  )

  it('combines travel and steering independently and cancels opposing keys', () => {
    expect(movementInput(new Set(['KeyW', 'ArrowRight']))).toEqual({
      forward: 1,
      turn: 1,
    })
    expect(movementInput(new Set(['ArrowDown', 'KeyA']))).toEqual({
      forward: -1,
      turn: -1,
    })
    expect(
      movementInput(new Set(['KeyW', 'ArrowDown', 'KeyA', 'ArrowRight']))
    ).toBeNull()
    expect(movementInput(new Set(['ShiftLeft']))).toBeNull()
  })

  it.each(['blur', 'hidden'])(
    'stops keyboard movement on %s without another frame',
    (reason) => {
      vi.stubGlobal('window', new EventTarget())
      vi.stubGlobal(
        'document',
        Object.assign(new EventTarget(), { hidden: false })
      )
      const send = vi.fn()
      const stop = vi.fn()
      const sender = new KeyboardDirectionSender(send, stop)
      const cleanup = inputHandler.setupEventListeners(
        new EventTarget() as HTMLCanvasElement,
        vi.fn(),
        vi.fn(),
        () => sender.clear()
      )
      try {
        document.dispatchEvent(
          Object.assign(new Event('keydown'), { code: 'KeyW' })
        )
        sender.update(inputHandler.getMovementInput(), 0, false, 'world', false)
        expect(send).toHaveBeenCalledTimes(1)
        document.dispatchEvent(new Event('visibilitychange'))
        expect(stop).not.toHaveBeenCalled()

        if (reason === 'hidden') {
          Object.assign(document, { hidden: true })
          document.dispatchEvent(new Event('visibilitychange'))
        } else {
          window.dispatchEvent(new Event('blur'))
        }
        expect(stop).toHaveBeenCalledTimes(1)
        expect(inputHandler.getMovementInput()).toBeNull()
        sender.update(inputHandler.getMovementInput(), 0, false, 'world', false)
        expect(send).toHaveBeenCalledTimes(1)
        expect(stop).toHaveBeenCalledTimes(1)
      } finally {
        cleanup()
      }
    }
  )
})

const RECT = { left: 0, top: 0, width: 100, height: 100 }

/** A canvas click at the viewport center, as processCanvasClick consumes it. */
function centerClick(shiftKey = false): MouseEvent {
  return {
    clientX: RECT.width / 2,
    clientY: RECT.height / 2,
    target: { getBoundingClientRect: () => RECT },
    shiftKey,
  } as unknown as MouseEvent
}

/** Camera 10m above the origin looking straight down at a flat ground plane
 *  at y = 0 — the center click ray hits the ground at (0, 0, 0). */
function groundScene() {
  const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 100)
  camera.position.set(0, 10, 0)
  camera.lookAt(0, 0, 0)
  camera.updateMatrixWorld(true)

  const ground = new THREE.Mesh(new THREE.PlaneGeometry(100, 100))
  ground.rotateX(-Math.PI / 2)
  ground.updateMatrixWorld(true)

  return { camera, ground }
}

function contextWith(overrides: Partial<RaycastContext> = {}): RaycastContext {
  const { camera, ground } = groundScene()
  return {
    camera,
    monsterMeshes: [],
    npcMeshes: [],
    doorMeshes: [],
    objectMeshes: [],
    propMeshes: [],
    groundItemMeshes: [],
    tipHatMeshes: [],
    mealMeshes: [],
    stallMeshes: [],
    groundMeshes: [ground],
    playerPosition: { x: 0, y: 0, z: 0 },
    playerVisualFloorLevel: 0,
    isMonsterDead: () => false,
    ...overrides,
  }
}

describe('processCanvasClick cast-vs-walk', () => {
  beforeEach(() => {
    resetFishingStore()
    closeInstrumentPanel()
    inputHandler.clearTransientInput()
    alwaysRun.set(false)
  })

  it.each([
    ['furniture_bed', 0.78],
    ['furniture_rustic_bed', 0.56],
  ] as const)('clicks a rotated %s to approach and sleep', (itemId, height) => {
    const visual = new THREE.Group()
    visual.add(new THREE.Mesh(new THREE.BoxGeometry(1.2, 0.8, 2.2)))
    visual.rotation.y = Math.PI / 2
    Object.assign(
      visual.userData,
      estateFurnitureInteractionData({
        id: 7,
        item_def_id: itemId,
        estate_id: 1,
        owner_id: 1,
        position: { x: 0, y: 0, z: 0 },
        rotation_deg: 90,
        floor_level: 0,
        overdue: false,
        revision: 0,
      })
    )
    visual.updateMatrixWorld(true)

    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        objectMeshes: [visual],
        playerPosition: { x: 12, y: 0, z: 0 },
      })
    )
    expect(intent).toMatchObject({
      type: 'interact_object',
      objectType: itemId,
      objectId: 7,
      interaction: 'sleep',
      rotation: Math.PI / 2,
      interactOffset: { y: height },
    })
  })

  it('ignores canvas movement while the instrument keyboard owns input', () => {
    openInstrumentPanel()

    expect(
      inputHandler.processCanvasClick(centerClick(), contextWith())
    ).toEqual({
      type: 'none',
    })
  })

  it('casts when a rod is equipped and the click lands on deep enough water', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => 1.0,
      })
    )

    expect(intent.type).toBe('cast_fishing')
    if (intent.type === 'cast_fishing') {
      expect(intent.position.x).toBeCloseTo(0)
      expect(intent.position.z).toBeCloseTo(0)
    }
  })

  it('targets water through an NPC without opening an interaction', () => {
    const npc = new THREE.Mesh(new THREE.BoxGeometry(2, 2, 2))
    npc.position.y = 1
    npc.userData.npcPlayerId = 7
    npc.updateMatrixWorld(true)
    expect(
      inputHandler.processCanvasClick(
        centerClick(),
        contextWith({
          npcMeshes: [npc],
          fishingTargeting: true,
          canCastFishing: true,
          waterSurfaceAt: () => 1,
        })
      ).type
    ).toBe('cast_fishing')
  })

  it.each([
    { waterSurfaceAt: () => 0 },
    { playerPosition: { x: 20, y: 0, z: 0 } },
    { canCastFishing: false },
    { groundMeshes: [] },
  ])(
    'does not move when a fishing skill target is invalid: %j',
    (overrides) => {
      expect(
        inputHandler.processCanvasClick(
          centerClick(),
          contextWith({
            fishingTargeting: true,
            canCastFishing: true,
            waterSurfaceAt: () => 1,
            ...overrides,
          })
        )
      ).toEqual({ type: 'none' })
    }
  )

  it('moves through NPC hits when fence placement requests ground only', () => {
    const npc = new THREE.Mesh(new THREE.BoxGeometry(2, 2, 2))
    npc.position.y = 1
    npc.userData.npcPlayerId = 7
    npc.updateMatrixWorld(true)
    const context = contextWith({ npcMeshes: [npc] })
    expect(inputHandler.processCanvasClick(centerClick(), context).type).toBe(
      'interact_npc'
    )
    const intent = inputHandler.processCanvasClick(centerClick(true), {
      ...context,
      groundOnly: true,
    })
    expect(intent.type).toBe('move_to_ground')
    if (intent.type === 'move_to_ground') {
      expect(intent.position.y).toBeCloseTo(0)
      expect(intent.sprinting).toBe(true)
    }
  })

  it('moves instead of fishing when fence placement requests ground only', () => {
    expect(
      inputHandler.processCanvasClick(
        centerClick(),
        contextWith({
          groundOnly: true,
          canCastFishing: true,
          waterSurfaceAt: () => 1,
        })
      ).type
    ).toBe('move_to_ground')
  })

  it('walks when no rod is equipped, even over water', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: false,
        waterSurfaceAt: () => 1.0,
      })
    )

    expect(intent.type).toBe('move_to_ground')
  })

  it('marks a Shift-click ground path as sprinting', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(true),
      contextWith()
    )

    expect(intent.type).toBe('move_to_ground')
    if (intent.type === 'move_to_ground') {
      expect(intent.sprinting).toBe(true)
    }
  })

  describe('always-run preference', () => {
    beforeEach(() => alwaysRun.set(true))
    afterEach(() => alwaysRun.set(false))

    it('sprints on a plain click', () => {
      const intent = inputHandler.processCanvasClick(
        centerClick(),
        contextWith()
      )

      expect(intent.type).toBe('move_to_ground')
      if (intent.type === 'move_to_ground') {
        expect(intent.sprinting).toBe(true)
      }
    })

    it('walks on a Shift-click — the modifier inverts', () => {
      const intent = inputHandler.processCanvasClick(
        centerClick(true),
        contextWith()
      )

      expect(intent.type).toBe('move_to_ground')
      if (intent.type === 'move_to_ground') {
        expect(intent.sprinting).toBe(false)
      }
    })
  })

  it('walks when the water is too shallow to fish', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => MIN_FISHABLE_DEPTH_M - 0.05,
      })
    )

    expect(intent.type).toBe('move_to_ground')
  })

  it('walks at exactly the minimum depth — the cast needs strictly deeper water', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => MIN_FISHABLE_DEPTH_M,
      })
    )

    expect(intent.type).toBe('move_to_ground')
  })

  it('walks toward water beyond the cast range instead of sending a doomed cast', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => 1.0,
        playerPosition: { x: MAX_CAST_DISTANCE_M + 1, y: 0, z: 0 },
      })
    )

    expect(intent.type).toBe('move_to_ground')
  })

  it('still casts just inside the maximum range', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => 1.0,
        // Not exactly MAX: the raycast hit lands within float epsilon of the
        // origin, which would flip an exact-boundary check either way.
        playerPosition: { x: MAX_CAST_DISTANCE_M - 0.01, y: 0, z: 0 },
      })
    )

    expect(intent.type).toBe('cast_fishing')
  })

  it('range is measured in XZ, ignoring height difference', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        canCastFishing: true,
        waterSurfaceAt: () => 1.0,
        playerPosition: { x: 3, y: 50, z: 4 },
      })
    )

    expect(intent.type).toBe('cast_fishing')
  })

  it('walks when no water sampler is wired up (dungeon / upper floors)', () => {
    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({ canCastFishing: true })
    )

    expect(intent.type).toBe('move_to_ground')
  })

  it('uses a door hit on the visible floor after an overlapping lower floor', () => {
    const makeDoor = (y: number, floor: number, roomIndex: number) => {
      const door = new THREE.Mesh(new THREE.PlaneGeometry(2, 2))
      door.rotateX(-Math.PI / 2)
      door.position.y = y
      door.userData = {
        doorHouseId: 'house',
        doorRoomIndex: roomIndex,
        doorWallDir: 'south',
        doorSegmentIndex: 0,
        doorFloorLevel: floor,
      }
      door.updateMatrixWorld(true)
      return door
    }
    const lowerFloorDoor = makeDoor(5, 0, 0)
    const visibleFloorDoor = makeDoor(4, 1, 1)

    const intent = inputHandler.processCanvasClick(
      centerClick(),
      contextWith({
        doorMeshes: [lowerFloorDoor, visibleFloorDoor],
        playerPosition: { x: 0, y: 4, z: 0 },
        playerVisualFloorLevel: 1,
      })
    )

    expect(intent).toMatchObject({
      type: 'toggle_door',
      houseId: 'house',
      roomIndex: 1,
    })
  })

  it('uses the canvas bounds for drag events captured by another element', () => {
    const event = { clientX: 50, clientY: 50, shiftKey: false } as MouseEvent
    const context = contextWith({ movementOnly: true })
    const intent = inputHandler.processCanvasClick(
      event,
      context,
      RECT as DOMRect
    )
    expect(intent).toEqual(
      inputHandler.processCanvasClick(centerClick(), context)
    )
  })

  it('drags across monsters and water without triggering interactions', () => {
    const monster = new THREE.Group()
    monster.add(new THREE.Mesh(new THREE.BoxGeometry(2, 2, 2)))
    monster.position.y = 1
    monster.userData.monsterId = 'monster'
    monster.updateMatrixWorld(true)
    const context = contextWith({
      monsterMeshes: [monster],
      canCastFishing: true,
      waterSurfaceAt: () => 1,
    })
    expect(inputHandler.processCanvasClick(centerClick(), context).type).toBe(
      'attack_monster'
    )
    expect(
      inputHandler.processCanvasClick(centerClick(true), {
        ...context,
        movementOnly: true,
      })
    ).toMatchObject({ type: 'move_to_ground', sprinting: true })
  })

  it.each([false, true])(
    'uses a nearby stair hit for clicks and drags (movementOnly: %s)',
    (movementOnly) => {
      const { camera, ground } = groundScene()
      const stair = new THREE.Group()
      stair.userData.housingStairFloor = 0
      const landing = new THREE.Mesh(new THREE.PlaneGeometry(0.7, 0.7))
      landing.rotateX(-Math.PI / 2)
      landing.position.set(1.28, 0.2, 0)
      stair.add(landing)
      stair.updateMatrixWorld(true)
      const target = { x: 3.5, y: 3.15, z: 3.75 }
      const resolveHousingStairTarget = vi.fn(() => target)

      const intent = inputHandler.processCanvasClick(
        centerClick(),
        contextWith({
          camera,
          groundMeshes: [ground, stair],
          resolveHousingStairTarget,
          movementOnly,
        })
      )

      expect(resolveHousingStairTarget).toHaveBeenCalled()
      expect(intent).toEqual({
        type: 'move_to_ground',
        sprinting: false,
        position: target,
        viaHousingStair: true,
      })
    }
  )
})

describe('shouldSuppressContextMenu', () => {
  it('always suppresses on canvases, even under selected text', () => {
    expect(
      shouldSuppressContextMenu({
        canvasTarget: true,
        typingTarget: false,
        selectionAtPointer: true,
      })
    ).toBe(true)
  })

  it('suppresses on HUD targets when text is selected elsewhere', () => {
    expect(
      shouldSuppressContextMenu({
        canvasTarget: false,
        typingTarget: false,
        selectionAtPointer: false,
      })
    ).toBe(true)
  })

  it('keeps the paste menu on text fields', () => {
    expect(
      shouldSuppressContextMenu({
        canvasTarget: false,
        typingTarget: true,
        selectionAtPointer: false,
      })
    ).toBe(false)
  })

  it('keeps the copy menu when the pointer is over selected HUD text', () => {
    expect(
      shouldSuppressContextMenu({
        canvasTarget: false,
        typingTarget: false,
        selectionAtPointer: true,
      })
    ).toBe(false)
  })
})
