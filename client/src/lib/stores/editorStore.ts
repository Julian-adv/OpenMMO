import { writable } from 'svelte/store'
import type { Position } from '../utils/movementUtils'
import type { LayerConfig } from '../utils/splatLayerLoader'
import type { TerrainMetaManager } from '../managers/terrainMetaManager'
import type { TerrainHeightManager } from '../managers/terrainHeightManager'
import type { TerrainSplatManager } from '../managers/terrainSplatManager'
import type { TerrainGrassDataManager } from '../managers/terrainGrassDataManager'
import type { TerrainTreeDataManager } from '../managers/terrainTreeDataManager'
import type { ZoneManager, ZoneData } from '../managers/zoneManager'
import type { NpcScheduleData } from '../managers/npcScheduleManager'

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
export const brushStrength = writable<number>(8)
export const brushRaiseMode = writable<boolean>(true)
export const cursorHeight = writable<number | null>(null)

// Brush world position for shader overlay (null = no overlay)
export const brushWorldPos = writable<{ x: number; z: number } | null>(null)

// Effective brush mode (accounts for Shift/Ctrl modifiers)
export type BrushMode = 'raise' | 'lower' | 'flatten'
export const brushMode = writable<BrushMode>('raise')

// Editor tool selection
export type EditorTool = 'height' | 'splat' | 'zone' | 'npc' | 'furniture'
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

/** Display-name overrides for texture assets whose filenames don't match their appearance */
const TEXTURE_LABEL_OVERRIDES: Record<string, string> = {
  rocky_terrain_02_1k: 'Meadow Grass',
}

/** Derive human-readable label from texture name, e.g. "rocky_terrain_02_1k" → "Meadow Grass" */
export function textureNameToLabel(name: string): string {
  if (TEXTURE_LABEL_OVERRIDES[name]) return TEXTURE_LABEL_OVERRIDES[name]
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

// Procedural terrain generation dialog (stores the target region snapshot, null = closed)
export const showGenerateDialog = writable<{ rx: number; rz: number } | null>(
  null
)

// Region minimap generation dialog (stores the target region snapshot, null = closed)
export const showMinimapDialog = writable<{ rx: number; rz: number } | null>(
  null
)

// Bumped after minimap upload to bust cached img src
export const minimapVersion = writable<number>(0)

// Bumped to force terrain tile rebuild (e.g. after region delete)
export const terrainForceRebuild = writable<number>(0)

// Manager references for terrain generation dialog
export const editorHeightManager = writable<TerrainHeightManager | null>(null)
export const editorSplatManager = writable<TerrainSplatManager | null>(null)
export const editorGrassDataManager = writable<TerrainGrassDataManager | null>(
  null
)
export const editorTreeDataManager = writable<TerrainTreeDataManager | null>(
  null
)

// Zone editor stores
export type ZoneSubTool = 'noSpawn' | 'spawn'
export const zoneSubTool = writable<ZoneSubTool>('noSpawn')
export const zoneDrawStart = writable<{ x: number; z: number } | null>(null)
export const editorZoneManager = writable<ZoneManager | null>(null)

// Current region's zone data — shared between ZoneBrushPanel and ZoneOverlay
export const currentZoneData = writable<ZoneData>({
  monsterSpawns: [],
  noSpawnZones: [],
})

// Spawn zone form values (shared between panel and cursor)
export const spawnFormMonsterType = writable('scp939')
export const spawnFormMaxPerPlayer = writable(3)
export const spawnFormMaxTotal = writable(10)
export const spawnFormIntervalSecs = writable(30)
export const noSpawnFormLabel = writable('')

// Hovered zone in panel list: { type: 'noSpawn' | 'spawn', index: number } or null
export const hoveredZoneIndex = writable<{
  type: 'noSpawn' | 'spawn'
  index: number
} | null>(null)

// Furniture editor stores
export interface FurnitureDef {
  id: string
  name: string
  model: string
  interaction: string
  interactOffset?: Position
}

export interface FurniturePlacement {
  id: number
  type: string
  x: number
  y: number
  z: number
  rotation: number
  floorLevel: number
}

export interface FurnitureRegionData {
  placements: FurniturePlacement[]
}

export type FurnitureSubTool = 'place' | 'select'
export const furnitureSubTool = writable<FurnitureSubTool>('place')
export const furnitureCatalog = writable<FurnitureDef[]>([])
export const selectedFurnitureType = writable<string | null>(null)
export const furnitureRotation = writable<number>(0)
export const currentFurnitureData = writable<FurnitureRegionData>({
  placements: [],
})
export const selectedFurniturePlacementId = writable<number | null>(null)
export const furniturePreviewPos = writable<{
  x: number
  y: number
  z: number
} | null>(null)

// NPC editor stores
export const npcNames = writable<string[]>([])
export const selectedNpc = writable<string | null>(null)
export const selectedNpcSchedule = writable<NpcScheduleData | null>(null)
export const selectedScheduleIndex = writable<number>(0)
// -1 = dragging home pos, 0..n = dragging waypoint index, null = not dragging
export const draggingWaypointIndex = writable<number | null>(null)
