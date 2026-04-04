import { writable } from 'svelte/store'

export const debugVisible = writable(true)
export const cameraRotationEnabled = writable(false)
export const calendarVisible = writable(false)
export const celestialDebugVisible = writable(false)
export const mapEditorMode = writable(false)
export const gridVisible = writable(false)
export const worldMapVisible = writable(false)
export const inventoryVisible = writable(false)
export const characterPanelVisible = writable(false)
export const debugSpeedMode = writable(false)
export const refractionEnabled = writable(true)
export const reflectionEnabled = writable(true)
export const teleportLoading = writable(false)
export const torchLightEnabled = writable(false)
export const windDebugVisible = writable(false)
export const housingEditorMode = writable(false)
export const passabilityDebugVisible = writable(false)

export interface PlayerDebugInfo {
  position: { x: number; y: number; z: number }
  rotation: number
}

export const playerDebugInfo = writable<PlayerDebugInfo | null>(null)
