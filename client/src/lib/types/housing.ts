export type WallVariant = 'solid' | 'door' | 'window' | 'open'

export interface WallConfig {
  variant: WallVariant
  texture: number
}

export interface RoomData {
  localX: number
  localZ: number
  sizeX: number
  sizeZ: number
  floorLevel: number
  floorTexture: number
  roofTexture: number
  wallHeight: number
  wallNorth: WallConfig
  wallSouth: WallConfig
  wallEast: WallConfig
  wallWest: WallConfig
}

export interface HouseData {
  id: string
  ownerId: string
  origin: { x: number; y: number; z: number }
  rooms: RoomData[]
}
