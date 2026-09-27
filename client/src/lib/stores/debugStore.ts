import { writable } from 'svelte/store'

export const debugVisible = writable(false)
export const cameraRotationEnabled = writable(false)
export const calendarVisible = writable(false)
export const celestialDebugVisible = writable(false)
export const mapEditorMode = writable(false)
export const gridVisible = writable(false)
export const worldMapVisible = writable(false)
export const landPlotsVisible = writable(false)
export const inventoryVisible = writable(false)
export const characterPanelVisible = writable(false)
export type CharacterPanelTab = 'stats' | 'skills' | 'status' | 'titles'
export const characterPanelTab = writable<CharacterPanelTab>('stats')
export const debugSpeedMode = writable(false)
export const refractionEnabled = writable(true)
export const reflectionEnabled = writable(true)
export const teleportLoading = writable(false)
export const windDebugVisible = writable(false)
export const weatherRadarVisible = writable(false)
export const housingEditorMode = writable(false)
export const passabilityDebugVisible = writable(false)
export const riverWireframeVisible = writable(false)
export const shoreWaveDebugVisible = writable(false)
/** Prototype back cape on the local player, toggled by /cape. */
export const capeEnabled = writable(false)
/** Live override for the cape's collar bias (m), null = the model's recorded
 *  value. Set by /cape_depth while eyeballing a character. */
export const capeCollarBiasOverride = writable<number | null>(null)

export interface PlayerDebugInfo {
  position: { x: number; y: number; z: number }
  rotation: number
}

export const playerDebugInfo = writable<PlayerDebugInfo | null>(null)

/** Clear privileged debug flags when switching to a non-admin character. */
export function resetPrivilegedDebugFlags() {
  debugVisible.set(false)
  debugSpeedMode.set(false)
  mapEditorMode.set(false)
  housingEditorMode.set(false)
  weatherRadarVisible.set(false)
}
