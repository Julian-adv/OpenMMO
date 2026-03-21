import { writable } from 'svelte/store'
import type {
  RoofType,
  RoomData,
  RoomType,
  WallVariant,
} from '../types/housing'

export interface RoomTemplate {
  label: string
  sizeX: number
  sizeZ: number
  wallNorthVariant: WallVariant
  wallSouthVariant: WallVariant
  wallEastVariant: WallVariant
  wallWestVariant: WallVariant
}

export interface WallVariants {
  north: WallVariant
  south: WallVariant
  east: WallVariant
  west: WallVariant
}

export const WALL_VARIANT_OPTIONS: WallVariant[] = ['solid', 'door', 'window']

export const STAIR_TEMPLATES: RoomTemplate[] = [
  {
    label: 'Narrow (1×4)',
    sizeX: 1,
    sizeZ: 4,
    wallNorthVariant: 'solid',
    wallSouthVariant: 'solid',
    wallEastVariant: 'solid',
    wallWestVariant: 'solid',
  },
  {
    label: 'Wide (2×4)',
    sizeX: 2,
    sizeZ: 4,
    wallNorthVariant: 'solid',
    wallSouthVariant: 'solid',
    wallEastVariant: 'solid',
    wallWestVariant: 'solid',
  },
]

export const ROOM_TEMPLATES: RoomTemplate[] = [
  {
    label: 'Small (3×3)',
    sizeX: 3,
    sizeZ: 3,
    wallNorthVariant: 'solid',
    wallSouthVariant: 'door',
    wallEastVariant: 'solid',
    wallWestVariant: 'solid',
  },
  {
    label: 'Medium (4×4)',
    sizeX: 4,
    sizeZ: 4,
    wallNorthVariant: 'solid',
    wallSouthVariant: 'door',
    wallEastVariant: 'solid',
    wallWestVariant: 'window',
  },
  {
    label: 'Large (5×4)',
    sizeX: 5,
    sizeZ: 4,
    wallNorthVariant: 'window',
    wallSouthVariant: 'door',
    wallEastVariant: 'solid',
    wallWestVariant: 'window',
  },
  {
    label: 'Wide (6×4)',
    sizeX: 6,
    sizeZ: 4,
    wallNorthVariant: 'window',
    wallSouthVariant: 'door',
    wallEastVariant: 'solid',
    wallWestVariant: 'window',
  },
]

export const selectedRoomTemplate = writable<RoomTemplate | null>(null)
export const placementRotation = writable<number>(0)
export const placementPreview = writable<{ x: number; z: number } | null>(null)
export const placementFloorLevel = writable<number>(0)
export const placementRoomType = writable<RoomType>('normal')

// Wall texture index (0-3)
export const wallTextureIndex = writable<number>(0)
// Floor texture index (0-3)
export const floorTextureIndex = writable<number>(0)
// Roof texture index (0-3)
export const roofTextureIndex = writable<number>(0)
// Roof type
export const placementRoofType = writable<RoofType>('flat')

// Per-wall variant selection (initialized from template, user can override)
export const wallVariants = writable<WallVariants>({
  north: 'solid',
  south: 'door',
  east: 'solid',
  west: 'solid',
})

// Sync wall variants when a new template is selected
selectedRoomTemplate.subscribe((t) => {
  if (t) {
    wallVariants.set({
      north: t.wallNorthVariant,
      south: t.wallSouthVariant,
      east: t.wallEastVariant,
      west: t.wallWestVariant,
    })
  }
})

// Clear template selection when room type changes
placementRoomType.subscribe(() => {
  selectedRoomTemplate.set(null)
})

// Editor tool mode
export type HousingEditorTool = 'place' | 'select'
export const housingEditorTool = writable<HousingEditorTool>('place')

// Callback for deleting the currently selected room (set by HousingEditorCursor)
export let deleteSelectedRoom: (() => void) | null = null
export function setDeleteSelectedRoom(fn: (() => void) | null) {
  deleteSelectedRoom = fn
}

// Selection state for edit mode
export const selectedHouseId = writable<string | null>(null)
export const selectedRoomIndex = writable<number | null>(null)

// Clear selection when switching away from select mode
housingEditorTool.subscribe((tool) => {
  if (tool !== 'select') {
    selectedHouseId.set(null)
    selectedRoomIndex.set(null)
  }
  if (tool !== 'place') {
    selectedRoomTemplate.set(null)
  }
})

/** Populate texture stores from a selected room's current data */
export function populateEditStoresFromRoom(room: RoomData) {
  floorTextureIndex.set(room.floorTexture)
  roofTextureIndex.set(room.roofTexture)
  placementRoofType.set(room.roofType ?? 'flat')
  // Use the first non-open segment's texture as the wall texture
  for (const wall of [
    room.wallNorth,
    room.wallSouth,
    room.wallEast,
    room.wallWest,
  ]) {
    for (const seg of wall) {
      if (seg.variant !== 'open') {
        wallTextureIndex.set(seg.texture)
        return
      }
    }
  }
}
