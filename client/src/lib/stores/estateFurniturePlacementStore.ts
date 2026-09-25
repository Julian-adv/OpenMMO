import { derived, get, writable } from 'svelte/store'
import type { EstatePlot } from '../terrain/estatePlacement'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import type { EstateChest } from '../network/networkTypes'

export type EstateFurniturePlacementMode = {
  item_def_id: string
  owner_id: number
  plots: EstatePlot[]
} & (
  | { kind: 'place'; instance_id: number }
  | { kind: 'move'; furniture: EstateChest }
)

export const selectedEstateFurniture = writable<EstateChest | null>(null)
export const estateFurnitureCatalogOpen = writable(false)
export const estateFurnitureSelectionMode = writable(false)
export const estateFurniturePlacementMode =
  writable<EstateFurniturePlacementMode | null>(null)
export const estateFurnitureEditorActive = derived(
  [
    estateFurnitureSelectionMode,
    estateFurniturePlacementMode,
    selectedEstateFurniture,
  ],
  ([selecting, placement, selected]) =>
    selecting || placement !== null || selected !== null
)
export const estateFurniturePlacementPending = writable(false)
export const estateFurniturePlacementError = writable<string | null>(null)
export const estateFurniturePlacementRotation = writable({
  degrees: 0,
  manual: false,
})
export const ESTATE_FURNITURE_HEIGHT_STEP = 0.05
export const estateFurniturePlacementHeight = writable<{
  offset: number | null
  manual: boolean
}>({ offset: null, manual: false })

export function showEstateFurnitureCatalog() {
  if (get(estateFurniturePlacementPending)) return
  stopEstateFurniturePlacement()
  estateFurnitureCatalogOpen.set(true)
}

export function startEstateFurnitureSelection() {
  if (get(estateFurniturePlacementPending)) return
  stopEstateFurniturePlacement()
  estateFurnitureSelectionMode.set(true)
}

export function selectEstateFurniture(furniture: EstateChest) {
  if (
    !get(estateFurnitureSelectionMode) ||
    get(estateFurniturePlacementPending)
  )
    return false
  selectedEstateFurniture.set(furniture)
  estateFurniturePlacementPending.set(true)
  estateFurniturePlacementError.set(null)
  return true
}

export function beginEstateFurnitureRecovery() {
  const furniture = get(selectedEstateFurniture)
  if (!furniture || get(estateFurniturePlacementPending)) return null
  estateFurniturePlacementMode.set(null)
  estateFurnitureSelectionMode.set(true)
  estateFurniturePlacementPending.set(true)
  estateFurniturePlacementError.set(null)
  return furniture.id
}

export function startEstateFurniturePlacement(
  mode: EstateFurniturePlacementMode
) {
  estateFurnitureCatalogOpen.set(false)
  estateFurnitureSelectionMode.set(false)
  estateFurniturePlacementRotation.set({
    degrees: mode.kind === 'move' ? mode.furniture.rotation_deg : 0,
    manual: mode.kind === 'move',
  })
  estateFurniturePlacementHeight.set({
    offset: null,
    manual: mode.kind === 'move',
  })
  estateFurniturePlacementPending.set(false)
  estateFurniturePlacementError.set(null)
  estateFurniturePlacementMode.set(mode)
}

export function rotateEstateFurniturePlacement(direction: 1 | -1 = 1) {
  const definition = getEstateStorageDef(
    get(estateFurniturePlacementMode)?.item_def_id
  )
  if (!definition || get(estateFurniturePlacementPending)) return
  estateFurniturePlacementRotation.update(({ degrees }) => ({
    degrees: (degrees + direction * definition.rotationStep + 360) % 360,
    manual: true,
  }))
  estateFurniturePlacementError.set(null)
}

export function initializeEstateFurniturePlacementHeight(baseY: number) {
  const mode = get(estateFurniturePlacementMode)
  if (
    mode?.kind !== 'move' ||
    get(estateFurniturePlacementHeight).offset !== null ||
    !Number.isFinite(baseY)
  )
    return
  const max = getEstateStorageDef(mode.item_def_id)?.maxHeightOffset ?? 0
  estateFurniturePlacementHeight.set({
    offset: Math.max(0, Math.min(max, mode.furniture.position.y - baseY)),
    manual: true,
  })
}

export function setEstateFurniturePlacementHeight(offset: number) {
  const definition = getEstateStorageDef(
    get(estateFurniturePlacementMode)?.item_def_id
  )
  if (
    !definition?.maxHeightOffset ||
    get(estateFurniturePlacementPending) ||
    !Number.isFinite(offset)
  )
    return
  estateFurniturePlacementHeight.set({
    offset: Math.max(
      0,
      Math.min(
        definition.maxHeightOffset,
        Math.round(offset / ESTATE_FURNITURE_HEIGHT_STEP) *
          ESTATE_FURNITURE_HEIGHT_STEP
      )
    ),
    manual: true,
  })
  estateFurniturePlacementError.set(null)
}

export function adjustEstateFurniturePlacementHeight(direction: 1 | -1) {
  setEstateFurniturePlacementHeight(
    (get(estateFurniturePlacementHeight).offset ?? 0) +
      direction * ESTATE_FURNITURE_HEIGHT_STEP
  )
}

export function beginEstateFurniturePlacementSave() {
  if (
    !get(estateFurniturePlacementMode) ||
    get(estateFurniturePlacementPending)
  )
    return false
  estateFurniturePlacementPending.set(true)
  estateFurniturePlacementError.set(null)
  return true
}

export function applyEstateFurnitureEditResult(error: string | null) {
  estateFurniturePlacementPending.set(false)
  estateFurniturePlacementError.set(error)
  if (!error) {
    if (get(estateFurnitureSelectionMode)) selectedEstateFurniture.set(null)
    else if (get(estateFurniturePlacementMode)?.kind !== 'move')
      stopEstateFurniturePlacement()
  }
}

export function stopEstateFurniturePlacement() {
  estateFurnitureCatalogOpen.set(false)
  estateFurnitureSelectionMode.set(false)
  selectedEstateFurniture.set(null)
  estateFurniturePlacementMode.set(null)
  estateFurniturePlacementPending.set(false)
  estateFurniturePlacementError.set(null)
  estateFurniturePlacementRotation.set({ degrees: 0, manual: false })
  estateFurniturePlacementHeight.set({ offset: null, manual: false })
}
