import { writable } from 'svelte/store'

export interface HoveredCell {
  tileX: number
  tileZ: number
  cellX: number
  cellZ: number
  worldX: number
  worldZ: number
}

export const hoveredCell = writable<HoveredCell | null>(null)

// Height brush settings
export const brushSize = writable<number>(3)
export const brushStrength = writable<number>(5)
export const brushRaiseMode = writable<boolean>(true)
export const cursorHeight = writable<number | null>(null)
