import { writable } from 'svelte/store'
import type { LayerConfig } from '../utils/splatLayerLoader'
import type { TerrainMetaManager } from '../managers/terrainMetaManager'

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

// Splat layer: 0=R, 1=G, 2=B, 3=A (texture depends on region)
export const splatLayer = writable<number>(0)

// Per-region layer info for the SplatBrushPanel
export interface SplatLayerInfo {
  label: string
  color: string
}

const DEFAULT_SPLAT_LAYER_INFO: SplatLayerInfo[] = [
  { label: 'Grass', color: '#66cc66' },
  { label: 'Rock', color: '#999999' },
  { label: 'Dirt', color: '#bb7744' },
  { label: 'Snow', color: '#ddeeff' },
]

export const currentRegionLayers = writable<SplatLayerInfo[]>(
  DEFAULT_SPLAT_LAYER_INFO
)

/** Derive human-readable label from texture name, e.g. "rocky_terrain_02_1k" → "Rocky Terrain" */
export function textureNameToLabel(name: string): string {
  return name
    .replace(/_\d+k$/, '') // remove resolution suffix
    .replace(/_\d+$/, '') // remove trailing numbers
    .replace(/_/g, ' ') // underscores to spaces
    .replace(/\b\w/g, (c) => c.toUpperCase()) // title case
}

// Camera pan offset for map editor (world-space XZ displacement from player)
export const editorPanOffset = writable<{ x: number; z: number }>({
  x: 0,
  z: 0,
})

// Current region the editor cursor is in
export const currentEditorRegion = writable<{
  rx: number
  rz: number
} | null>(null)

// Actual LayerConfig data for the current region (texture + tileScale)
export const currentRegionConfigs = writable<LayerConfig[]>([])

// MetaManager reference for SplatBrushPanel to save region texture changes
export const editorMetaManager = writable<TerrainMetaManager | null>(null)

// Incremented after saving region meta to trigger terrain re-render
export const regionMetaVersion = writable<number>(0)
