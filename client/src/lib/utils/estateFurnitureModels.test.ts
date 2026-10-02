import { describe, expect, it } from 'vitest'
import { estateFurnitureRenderY } from './estateFurnitureModels'
import type { EstateChest } from '../network/networkTypes'
import type { HouseData } from '../types/housing'

const house: HouseData = {
  id: 'house',
  ownerId: 'owner',
  origin: { x: 0, y: 5, z: 0 },
  rooms: [0, 1].map((floorLevel) => ({
    roomType: 'normal',
    localX: 0,
    localZ: 0,
    sizeX: 6,
    sizeZ: 6,
    floorLevel,
    floorTexture: 0,
    roofTexture: 0,
    wallHeight: 3,
    wallNorth: [],
    wallSouth: [],
    wallEast: [],
    wallWest: [],
  })),
}

const rug: EstateChest = {
  id: 1,
  estate_id: 1,
  owner_id: 1,
  item_def_id: 'furniture_hearthbound_rug',
  position: { x: 3, y: 5, z: 3 },
  rotation_deg: 0,
  floor_level: 0,
  overdue: false,
  revision: 0,
}

describe('estate rug rendering', () => {
  it('shows existing rugs above the floor slab on both building floors', () => {
    expect(estateFurnitureRenderY(rug, [house])).toBeCloseTo(5.05)
    expect(
      estateFurnitureRenderY(
        { ...rug, floor_level: 1, position: { ...rug.position, y: 8.1 } },
        [house]
      )
    ).toBeCloseTo(8.15)
  })

  it('preserves adjusted heights and outdoor positions', () => {
    const raised = { ...rug, position: { ...rug.position, y: 5.25 } }
    expect(estateFurnitureRenderY(raised, [house])).toBe(5.25)
    expect(estateFurnitureRenderY(rug, [])).toBe(5)
    expect(
      estateFurnitureRenderY({ ...rug, position: { ...rug.position, x: 10 } }, [
        house,
      ])
    ).toBe(5)
  })
})
