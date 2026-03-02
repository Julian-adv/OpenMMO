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

// Brush world position for shader overlay (null = no overlay)
export const brushWorldPos = writable<{ x: number; z: number } | null>(null)

// Effective brush mode (accounts for Shift/Ctrl modifiers)
export type BrushMode = 'raise' | 'lower' | 'flatten'
export const brushMode = writable<BrushMode>('raise')

// Editor tool selection
export type EditorTool = 'height' | 'splat'
export const editorTool = writable<EditorTool>('height')

// Splat layer: 0=R(grass), 1=G(rock), 2=B(dirt), 3=A(snow)
export const splatLayer = writable<number>(0)

// Camera pan offset for map editor (world-space XZ displacement from player)
export const editorPanOffset = writable<{ x: number; z: number }>({
  x: 0,
  z: 0,
})
