import { writable } from 'svelte/store'
import type { WallVariant } from '../types/housing'

export interface RoomTemplate {
  label: string
  sizeX: number
  sizeZ: number
  wallNorthVariant: WallVariant
  wallSouthVariant: WallVariant
  wallEastVariant: WallVariant
  wallWestVariant: WallVariant
}

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

// Wall texture index (0-3)
export const wallTextureIndex = writable<number>(0)
// Floor texture index (0-3)
export const floorTextureIndex = writable<number>(0)
// Roof texture index (0-3)
export const roofTextureIndex = writable<number>(0)

// Delete mode: when true, clicking a house deletes it
export const housingDeleteMode = writable<boolean>(false)
