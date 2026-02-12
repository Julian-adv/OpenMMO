import { writable } from 'svelte/store'

export const debugVisible = writable(true)
export const cameraRotationEnabled = writable(false)

export interface PlayerDebugInfo {
  position: { x: number; y: number; z: number }
  rotation: number
}

export const playerDebugInfo = writable<PlayerDebugInfo | null>(null)
