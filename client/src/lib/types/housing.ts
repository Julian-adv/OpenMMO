export type RoomType = 'normal' | 'stairwell'

export type WallVariant = 'solid' | 'door' | 'window' | 'open'

export interface WallConfig {
  variant: WallVariant
  texture: number
}

export type RoofType = 'flat' | 'gabled' | 'steep'
export type RoofRidgeDir = 'auto' | 'x' | 'z'

export interface RoomData {
  roomType?: RoomType
  roofType?: RoofType
  roofRidgeDir?: RoofRidgeDir
  localX: number
  localZ: number
  sizeX: number
  sizeZ: number
  floorLevel: number
  floorTexture: number
  roofTexture: number
  wallHeight: number
  /** 1m segments: north wall (length = sizeX) */
  wallNorth: WallConfig[]
  /** 1m segments: south wall (length = sizeX) */
  wallSouth: WallConfig[]
  /** 1m segments: east wall (length = sizeZ) */
  wallEast: WallConfig[]
  /** 1m segments: west wall (length = sizeZ) */
  wallWest: WallConfig[]
}

export interface HouseData {
  id: string
  ownerId: string
  origin: { x: number; y: number; z: number }
  rooms: RoomData[]
}
