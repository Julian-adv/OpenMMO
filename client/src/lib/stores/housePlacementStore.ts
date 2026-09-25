import { writable } from 'svelte/store'
import type { FencePlot } from '../terrain/fenceEdges'
import type { HouseData } from '../types/housing'

export interface HousePlacementMode {
  instanceId: number
  itemName: string
  house: HouseData
  plots: FencePlot[]
  pending: boolean
  quarterTurns: number
  error: string | null
  target: { x: number; y: number; z: number } | null
  valid: boolean
  reason: string | null
}

export interface HouseDemolitionMode {
  targetHouseId: string | null
  valid: boolean
  reason: string
}

export const housePlacementMode = writable<HousePlacementMode | null>(null)
export const houseDemolitionMode = writable<HouseDemolitionMode | null>(null)
export const houseDemolitionConfirmation = writable<HouseData | null>(null)
export const houseDemolitionPending = writable<string | null>(null)
export const houseDemolitionError = writable<string | null>(null)

export function openHousePlacement(
  instanceId: number,
  itemName: string,
  house: HouseData,
  plots: FencePlot[]
) {
  stopHouseDemolitionSelection()
  houseDemolitionConfirmation.set(null)
  housePlacementMode.set({
    instanceId,
    itemName,
    house,
    plots,
    pending: false,
    quarterTurns: 0,
    error: null,
    target: null,
    valid: false,
    reason: 'Point at your estate to choose a position',
  })
}

export function rotateHousePlacement() {
  housePlacementMode.update((mode) =>
    mode && !mode.pending
      ? {
          ...mode,
          quarterTurns: (mode.quarterTurns + 1) % 4,
          error: null,
        }
      : mode
  )
}

export function stopHousePlacement() {
  housePlacementMode.set(null)
}

export function startHouseDemolitionSelection() {
  stopHousePlacement()
  houseDemolitionConfirmation.set(null)
  houseDemolitionError.set(null)
  houseDemolitionMode.set({
    targetHouseId: null,
    valid: false,
    reason: 'Point at one of your houses',
  })
}

export function stopHouseDemolitionSelection() {
  houseDemolitionMode.set(null)
}

export function stopHouseInteraction() {
  stopHousePlacement()
  stopHouseDemolitionSelection()
}

export function requestHouseDemolitionConfirmation(house: HouseData) {
  houseDemolitionConfirmation.set(house)
}

export function beginHouseDemolition(houseId: string) {
  houseDemolitionPending.set(houseId)
  houseDemolitionError.set(null)
}

export function applyHouseDemolitionResult(
  houseId: string,
  error: string | null
) {
  houseDemolitionPending.update((pending) =>
    pending === houseId ? null : pending
  )
  houseDemolitionError.set(error)
}

export function applyHousePlacementResult(error: string | null) {
  if (!error) {
    stopHousePlacement()
    return
  }
  housePlacementMode.update((mode) =>
    mode ? { ...mode, pending: false, error } : mode
  )
}

export function resetHousePlacement() {
  stopHouseInteraction()
  houseDemolitionConfirmation.set(null)
  houseDemolitionPending.set(null)
  houseDemolitionError.set(null)
}
