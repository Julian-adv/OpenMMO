import { beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

vi.mock('../wasm/onlinerpg_shared', () => ({
  passability_set_furniture: vi.fn(),
}))

import { passability_set_furniture } from '../wasm/onlinerpg_shared'
import {
  selectedEstateFurniture,
  estateFurnitureCatalogOpen,
  showEstateFurnitureCatalog,
  estateFurnitureSelectionMode,
  estateFurnitureEditorActive,
  startEstateFurnitureSelection,
  selectEstateFurniture,
  beginEstateFurnitureRecovery,
  beginEstateFurniturePlacementSave,
  estateFurniturePlacementMode,
  estateFurniturePlacementRotation,
  estateFurniturePlacementHeight,
  estateFurniturePlacementPending,
  estateFurniturePlacementError,
  applyEstateFurnitureEditResult,
  initializeEstateFurniturePlacementHeight,
  setEstateFurniturePlacementHeight,
  adjustEstateFurniturePlacementHeight,
  startEstateFurniturePlacement,
  rotateEstateFurniturePlacement,
  stopEstateFurniturePlacement,
} from './estateFurniturePlacementStore'
import {
  applyEstateChestVisibility,
  estateChests,
  openEstateChest,
  resetEstateStorage,
} from './estateStorageStore'

const chest = {
  id: 7,
  estate_id: 2,
  owner_id: 3,
  item_def_id: 'storage_chest',
  position: { x: 2.5, y: 5, z: 2.5 },
  rotation_deg: 90,
  floor_level: 0,
  overdue: false,
  revision: 0,
}

describe('estate storage visibility', () => {
  beforeEach(() => {
    resetEstateStorage()
    vi.clearAllMocks()
  })

  it('requests moving on selection and blocks further selection until the server responds', () => {
    expect(selectEstateFurniture(chest)).toBe(false)
    expect(get(selectedEstateFurniture)).toBeNull()
    expect(get(estateFurnitureEditorActive)).toBe(false)

    startEstateFurnitureSelection()
    expect(get(estateFurnitureEditorActive)).toBe(true)
    expect(selectEstateFurniture(chest)).toBe(true)
    expect(get(selectedEstateFurniture)).toEqual(chest)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(estateFurniturePlacementPending)).toBe(true)

    expect(selectEstateFurniture({ ...chest, id: 8 })).toBe(false)
    startEstateFurnitureSelection()
    showEstateFurnitureCatalog()
    expect(get(selectedEstateFurniture)).toEqual(chest)
    expect(get(estateFurniturePlacementPending)).toBe(true)
    expect(get(estateFurnitureCatalogOpen)).toBe(false)

    applyEstateFurnitureEditResult('The furniture cannot be moved.')
    expect(get(selectedEstateFurniture)).toEqual(chest)
    expect(get(estateFurnitureSelectionMode)).toBe(true)
    expect(selectEstateFurniture({ ...chest, id: 8 })).toBe(true)
    expect(get(selectedEstateFurniture)?.id).toBe(8)
    expect(get(estateFurniturePlacementError)).toBeNull()

    stopEstateFurniturePlacement()
    expect(get(estateFurnitureSelectionMode)).toBe(false)
    expect(get(estateFurnitureEditorActive)).toBe(false)
  })

  it('finishes editing in the catalog without selecting again or changing saved furniture', () => {
    applyEstateChestVisibility([chest], [])
    startEstateFurnitureSelection()
    selectEstateFurniture(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    expect(get(estateFurnitureSelectionMode)).toBe(false)
    expect(get(estateFurnitureEditorActive)).toBe(true)
    rotateEstateFurniturePlacement(1)
    selectEstateFurniture({ ...chest, id: 8 })
    expect(get(selectedEstateFurniture)).toEqual(chest)

    showEstateFurnitureCatalog()
    expect(get(estateFurnitureCatalogOpen)).toBe(true)
    expect(get(estateFurnitureSelectionMode)).toBe(false)
    expect(get(estateFurnitureEditorActive)).toBe(false)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(selectedEstateFurniture)).toBeNull()
    expect(get(estateChests).get(chest.id)).toEqual(chest)

    expect(selectEstateFurniture(chest)).toBe(false)
    startEstateFurnitureSelection()
    expect(get(estateFurnitureCatalogOpen)).toBe(false)
    expect(get(estateFurnitureSelectionMode)).toBe(true)
    showEstateFurnitureCatalog()
    expect(get(estateFurnitureCatalogOpen)).toBe(true)
    resetEstateStorage()
    expect(get(estateFurnitureCatalogOpen)).toBe(false)
  })

  it('recovers directly from move mode and keeps Select mode available after errors and success', () => {
    expect(beginEstateFurnitureRecovery()).toBeNull()
    applyEstateChestVisibility([chest, { ...chest, id: 8 }], [])
    startEstateFurnitureSelection()
    selectEstateFurniture(chest)
    expect(beginEstateFurnitureRecovery()).toBeNull()
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    rotateEstateFurniturePlacement(1)
    expect(beginEstateFurnitureRecovery()).toBe(chest.id)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(estateFurnitureSelectionMode)).toBe(true)
    expect(get(estateChests).get(chest.id)).toEqual(chest)
    expect(beginEstateFurnitureRecovery()).toBeNull()

    applyEstateFurnitureEditResult('Empty the storage before recovering it.')
    expect(get(selectedEstateFurniture)).toEqual(chest)
    expect(get(estateFurniturePlacementError)).not.toBeNull()
    expect(beginEstateFurnitureRecovery()).toBe(chest.id)
    expect(get(estateFurniturePlacementError)).toBeNull()

    applyEstateChestVisibility([], [chest.id])
    expect(get(selectedEstateFurniture)).toBeNull()
    expect(get(estateFurniturePlacementPending)).toBe(true)
    applyEstateFurnitureEditResult(null)
    expect(get(estateFurnitureSelectionMode)).toBe(true)
    expect(get(estateFurnitureEditorActive)).toBe(true)
    expect(get(estateFurniturePlacementPending)).toBe(false)

    selectEstateFurniture({ ...chest, id: 8 })
    expect(get(selectedEstateFurniture)?.id).toBe(8)
    resetEstateStorage()
    expect(get(estateFurnitureEditorActive)).toBe(false)
  })

  it('keeps contents private while syncing the visible solid chest', () => {
    applyEstateChestVisibility([chest], [])

    expect([...get(estateChests).values()]).toEqual([chest])
    expect(get(openEstateChest)).toBeNull()
    expect(passability_set_furniture).toHaveBeenLastCalledWith(
      'furniture:estate-storage:0,0',
      [
        {
          id: 7,
          type: 'chest_animated',
          x: 2.5,
          y: 5,
          z: 2.5,
          rotation: 90,
          floorLevel: 0,
        },
      ]
    )
  })

  it('removal clears collision and closes the matching window', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    openEstateChest.set({
      chest_id: chest.id,
      item_def_id: 'storage_chest',
      revision: 0,
      max_weight: 500,
      can_deposit: true,
      items: [],
    })
    vi.mocked(passability_set_furniture).mockClear()

    applyEstateChestVisibility([], [chest.id])

    expect(get(estateChests).size).toBe(0)
    expect(get(openEstateChest)).toBeNull()
    expect(get(selectedEstateFurniture)).toBeNull()
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(passability_set_furniture).toHaveBeenLastCalledWith(
      'furniture:estate-storage:0,0',
      []
    )
  })

  it('cancels a rotated move without changing the original furniture', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    expect(get(estateFurniturePlacementRotation)).toEqual({
      degrees: 90,
      manual: true,
    })
    rotateEstateFurniturePlacement(1)
    expect(get(estateFurniturePlacementRotation).degrees).toBe(180)
    stopEstateFurniturePlacement()
    expect(get(estateChests).get(chest.id)).toEqual(chest)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(selectedEstateFurniture)).toBeNull()
  })

  it('moves collision between buckets and refreshes the selected furniture', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    const moved = { ...chest, revision: 1, position: { x: 34.5, y: 5, z: 2.5 } }
    vi.mocked(passability_set_furniture).mockClear()
    applyEstateChestVisibility([moved], [])
    expect(get(estateChests).size).toBe(1)
    expect(get(selectedEstateFurniture)).toEqual(moved)
    expect(passability_set_furniture).toHaveBeenCalledWith(
      'furniture:estate-storage:0,0',
      []
    )
    expect(passability_set_furniture).toHaveBeenCalledWith(
      'furniture:estate-storage:1,0',
      [expect.objectContaining({ id: chest.id, x: 34.5 })]
    )
  })

  it('keeps moving across saves with the latest revision until explicitly finished', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    rotateEstateFurniturePlacement(1)

    for (const revision of [1, 2]) {
      expect(beginEstateFurniturePlacementSave()).toBe(true)
      const moved = {
        ...chest,
        revision,
        position: { ...chest.position, x: 2.5 + revision },
        rotation_deg: 180,
      }
      applyEstateChestVisibility([moved], [])
      applyEstateFurnitureEditResult(null)

      expect(get(estateFurniturePlacementMode)).toMatchObject({
        kind: 'move',
        furniture: moved,
      })
      expect(get(selectedEstateFurniture)).toEqual(moved)
      expect(get(estateFurniturePlacementPending)).toBe(false)
      expect(get(estateFurniturePlacementRotation).degrees).toBe(180)
    }

    stopEstateFurniturePlacement()
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(estateChests).get(chest.id)?.revision).toBe(2)
  })

  it.each(['before', 'after'] as const)(
    'keeps editing a saved adjustment when visibility arrives %s the save result',
    (visibilityOrder) => {
      applyEstateChestVisibility([chest], [])
      selectedEstateFurniture.set(chest)
      startEstateFurniturePlacement({
        kind: 'move',
        furniture: chest,
        item_def_id: chest.item_def_id,
        owner_id: chest.owner_id,
        plots: [{ x: 0, z: 0 }],
      })
      const moved = {
        ...chest,
        revision: 1,
        position: { ...chest.position, x: 2.55, z: 2.45 },
      }

      expect(beginEstateFurniturePlacementSave()).toBe(true)
      expect(beginEstateFurniturePlacementSave()).toBe(false)
      showEstateFurnitureCatalog()
      expect(get(estateFurniturePlacementPending)).toBe(true)
      expect(get(estateFurniturePlacementMode)?.kind).toBe('move')
      expect(get(estateFurnitureCatalogOpen)).toBe(false)
      expect(get(estateChests).get(chest.id)).toEqual(chest)

      if (visibilityOrder === 'before') applyEstateChestVisibility([moved], [])
      applyEstateFurnitureEditResult(null)
      if (visibilityOrder === 'after') applyEstateChestVisibility([moved], [])

      expect(get(estateChests).get(chest.id)).toEqual(moved)
      expect(get(estateFurniturePlacementPending)).toBe(false)
      expect(get(estateFurniturePlacementMode)).toMatchObject({
        kind: 'move',
        furniture: moved,
      })
      expect(get(selectedEstateFurniture)).toEqual(moved)
      expect(get(estateFurnitureEditorActive)).toBe(true)
      expect(get(estateFurnitureCatalogOpen)).toBe(false)
    }
  )

  it('keeps failed adjustments editable and stays in move mode after retrying', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    rotateEstateFurniturePlacement(1)
    expect(beginEstateFurniturePlacementSave()).toBe(true)
    applyEstateFurnitureEditResult('Position is blocked.')

    expect(get(estateFurniturePlacementPending)).toBe(false)
    expect(get(estateFurniturePlacementError)).toBe('Position is blocked.')
    expect(get(estateFurniturePlacementMode)?.kind).toBe('move')
    expect(get(selectedEstateFurniture)).toEqual(chest)
    expect(get(estateFurniturePlacementRotation).degrees).toBe(180)
    expect(get(estateFurnitureCatalogOpen)).toBe(false)
    expect(get(estateChests).get(chest.id)).toEqual(chest)

    expect(beginEstateFurniturePlacementSave()).toBe(true)
    expect(get(estateFurniturePlacementError)).toBeNull()
    applyEstateFurnitureEditResult(null)
    expect(get(estateFurniturePlacementMode)?.kind).toBe('move')
    expect(get(estateFurnitureCatalogOpen)).toBe(false)

    showEstateFurnitureCatalog()
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(estateFurnitureCatalogOpen)).toBe(true)
  })

  it('keeps failed edits available to retry and finishes successful new placements', () => {
    startEstateFurniturePlacement({
      kind: 'place',
      instance_id: 42,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    estateFurniturePlacementPending.set(true)
    applyEstateFurnitureEditResult('Position is blocked.')
    expect(get(estateFurniturePlacementPending)).toBe(false)
    expect(get(estateFurniturePlacementError)).toBe('Position is blocked.')
    expect(get(estateFurniturePlacementMode)?.kind).toBe('place')

    estateFurniturePlacementPending.set(true)
    applyEstateFurnitureEditResult(null)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(estateFurniturePlacementError)).toBeNull()
  })

  it('refreshes saved sign text and revisions without ending or resetting placement', () => {
    const sign = {
      ...chest,
      item_def_id: 'furniture_shop_sign',
      text: 'Old sign',
    }
    applyEstateChestVisibility([sign], [])
    selectedEstateFurniture.set(sign)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: sign,
      item_def_id: sign.item_def_id,
      owner_id: sign.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    rotateEstateFurniturePlacement(1)
    setEstateFurniturePlacementHeight(1.25)

    for (const [index, text] of [
      '우리 집\nWelcome',
      'New sign',
      '',
    ].entries()) {
      estateFurniturePlacementPending.set(true)
      const saved = { ...sign, text, revision: index + 1 }
      applyEstateChestVisibility([saved], [])
      applyEstateFurnitureEditResult(null)

      expect(get(selectedEstateFurniture)).toEqual(saved)
      expect(get(estateFurniturePlacementMode)).toMatchObject({
        kind: 'move',
        furniture: saved,
      })
      expect(get(estateFurniturePlacementPending)).toBe(false)
      expect(get(estateFurnitureEditorActive)).toBe(true)
      expect(get(estateFurniturePlacementRotation).degrees).toBe(180)
      expect(get(estateFurniturePlacementHeight)).toEqual({
        offset: 1.25,
        manual: true,
      })
      expect(get(estateChests).get(sign.id)?.position).toEqual(sign.position)
    }

    estateFurniturePlacementPending.set(true)
    applyEstateFurnitureEditResult('The sign could not be saved.')
    expect(get(estateFurniturePlacementMode)).toMatchObject({
      kind: 'move',
      furniture: { text: '', revision: 3 },
    })
    expect(get(estateFurniturePlacementPending)).toBe(false)
    expect(get(estateFurniturePlacementError)).toBe(
      'The sign could not be saved.'
    )
  })

  it('starts at the saved decoration height and keeps adjustments across saves', () => {
    const torch = { ...chest, item_def_id: 'furniture_torch_wall' }
    applyEstateChestVisibility([torch], [])
    selectedEstateFurniture.set(torch)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: torch,
      item_def_id: torch.item_def_id,
      owner_id: torch.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    initializeEstateFurniturePlacementHeight(3.35)
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.65)
    adjustEstateFurniturePlacementHeight(1)
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.7)

    estateFurniturePlacementPending.set(true)
    setEstateFurniturePlacementHeight(2)
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.7)
    applyEstateFurnitureEditResult('Position is blocked.')
    expect(get(estateFurniturePlacementMode)?.kind).toBe('move')
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.7)

    adjustEstateFurniturePlacementHeight(-1)
    estateFurniturePlacementPending.set(true)
    applyEstateChestVisibility(
      [{ ...torch, revision: 1, position: { x: 3, y: 5, z: 2.5 } }],
      []
    )
    applyEstateFurnitureEditResult(null)
    initializeEstateFurniturePlacementHeight(4)
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.65)
    expect(get(estateFurniturePlacementHeight).manual).toBe(true)

    stopEstateFurniturePlacement()
    expect(get(estateFurniturePlacementHeight)).toEqual({
      offset: null,
      manual: false,
    })
  })

  it('limits height adjustments to supported decorations and resets between items', () => {
    const mode = {
      kind: 'place' as const,
      instance_id: 42,
      item_def_id: 'furniture_torch_wall',
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    }
    startEstateFurniturePlacement(mode)
    setEstateFurniturePlacementHeight(1.23)
    expect(get(estateFurniturePlacementHeight).offset).toBeCloseTo(1.25)
    setEstateFurniturePlacementHeight(10)
    expect(get(estateFurniturePlacementHeight).offset).toBe(3)
    setEstateFurniturePlacementHeight(-1)
    expect(get(estateFurniturePlacementHeight).offset).toBe(0)
    setEstateFurniturePlacementHeight(NaN)
    expect(get(estateFurniturePlacementHeight).offset).toBe(0)

    startEstateFurniturePlacement({ ...mode, item_def_id: 'storage_chest' })
    initializeEstateFurniturePlacementHeight(3)
    setEstateFurniturePlacementHeight(2)
    expect(get(estateFurniturePlacementHeight)).toEqual({
      offset: null,
      manual: false,
    })
  })
})
