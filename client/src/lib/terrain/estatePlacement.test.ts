import { describe, expect, it } from 'vitest'
import {
  footprintOnOwnedEstate,
  footprintOnHouseFloor,
  furnitureFootprintsOverlap,
  furniturePlacementRotation,
  houseFloorY,
  housingPlacementFloor,
  pointOnHouseFloor,
  pointOnOwnedEstate,
  snapPlacementCoordinate,
} from './estatePlacement'
import * as THREE from 'three'
import type { HouseData } from '../types/housing'

const plots = [{ x: 0, z: 0 }]
const house = {
  id: 'house',
  ownerId: 'owner',
  origin: { x: 0, y: 2, z: 0 },
  rooms: [
    {
      roomType: 'normal',
      localX: 0,
      localZ: 0,
      sizeX: 6,
      sizeZ: 6,
      floorLevel: 0,
      floorTexture: 0,
      roofTexture: 0,
      wallHeight: 3,
      wallNorth: [],
      wallSouth: [],
      wallEast: [],
      wallWest: [],
    },
    {
      roomType: 'stairwell',
      localX: 4,
      localZ: 1,
      sizeX: 1,
      sizeZ: 4,
      floorLevel: 0,
      floorTexture: 0,
      roofTexture: 0,
      wallHeight: 3,
      wallNorth: [],
      wallSouth: [],
      wallEast: [],
      wallWest: [],
    },
  ],
} as HouseData

describe('estate furniture placement', () => {
  it('snaps furniture independently from its model', () => {
    expect(snapPlacementCoordinate(1.24, 0.5)).toBe(1)
    expect(snapPlacementCoordinate(1.26, 0.5)).toBe(1.5)
  })

  it('accepts points inside an owned plot and rejects its far edge', () => {
    expect(pointOnOwnedEstate(0, 0, plots)).toBe(true)
    expect(pointOnOwnedEstate(31.99, 31.99, plots)).toBe(true)
    expect(pointOnOwnedEstate(32, 16, plots)).toBe(false)
  })

  it('requires the complete rotated footprint to remain on owned land', () => {
    const footprint = { width: 1.48, depth: 0.62 }
    expect(footprintOnOwnedEstate(1, 1, 0, footprint, plots)).toBe(true)
    expect(footprintOnOwnedEstate(0.5, 1, 0, footprint, plots)).toBe(false)
    expect(footprintOnOwnedEstate(0.5, 1, 90, footprint, plots)).toBe(true)
  })

  it('detects overlapping furniture footprints without blocking edge contact', () => {
    const footprint = { width: 1.48, depth: 0.62 }
    const placed = { x: 3, z: 3, rotationDeg: 0, footprint }
    expect(furnitureFootprintsOverlap(placed, placed)).toBe(true)
    expect(
      furnitureFootprintsOverlap(placed, {
        x: 4.48,
        z: 3,
        rotationDeg: 0,
        footprint,
      })
    ).toBe(false)
    expect(
      furnitureFootprintsOverlap(placed, {
        x: 3.5,
        z: 3,
        rotationDeg: 90,
        footprint,
      })
    ).toBe(true)
  })

  it('uses the model offset when checking beds against land and nearby furniture', () => {
    const footprint = {
      width: 2,
      depth: 1,
      minX: 0,
      maxX: 2,
      minZ: -0.5,
      maxZ: 0.5,
    }
    expect(footprintOnOwnedEstate(0.2, 2, 0, footprint, plots)).toBe(true)
    expect(footprintOnOwnedEstate(30.5, 2, 0, footprint, plots)).toBe(false)
    expect(footprintOnOwnedEstate(2, 1, 90, footprint, plots)).toBe(false)
    expect(footprintOnHouseFloor(house, 0, 0.2, 3, 0, footprint)).toBe(true)
    expect(
      furnitureFootprintsOverlap(
        { x: 1, z: 3, rotationDeg: 0, footprint },
        { x: 2.8, z: 3, rotationDeg: 0, footprint: { width: 0.2, depth: 0.2 } }
      )
    ).toBe(true)
  })

  it('inherits the housing floor from the placement surface group', () => {
    const floor = new THREE.Group()
    floor.userData.housingPlacementFloorLevel = 1
    const mesh = new THREE.Mesh()
    floor.add(mesh)

    expect(housingPlacementFloor(mesh)).toBe(1)
    expect(housingPlacementFloor(new THREE.Mesh())).toBeNull()
  })

  it('clips indoor placement and its grid points to the usable floor', () => {
    expect(pointOnHouseFloor(house, 0, 3, 3)).toBe(true)
    expect(pointOnHouseFloor(house, 0, 4.5, 3)).toBe(false)
    expect(pointOnHouseFloor(house, 0, 6.5, 3)).toBe(false)
    expect(houseFloorY(house, 0, 3, 3)).toBeCloseTo(2.05)
  })

  it('can rotate a chest inward when its current direction crosses an edge', () => {
    const chest = { width: 1.48, depth: 0.62 }
    expect(footprintOnHouseFloor(house, 0, 0.5, 3, 0, chest, 0.1)).toBe(false)
    expect(footprintOnHouseFloor(house, 0, 0.5, 3, 90, chest, 0.1)).toBe(true)
  })

  it('preserves a manually rotated bed at a floor edge instead of turning it back', () => {
    const bed = { width: 1.2, depth: 2.6 }
    const fits = (rotation: number) =>
      footprintOnHouseFloor(house, 0, 2, 0.8, rotation, bed)
    expect(fits(0)).toBe(false)
    expect(fits(90)).toBe(true)
    expect(furniturePlacementRotation(0, 90, false, fits)).toBe(90)
    expect(furniturePlacementRotation(0, 90, true, fits)).toBe(0)
    expect(furniturePlacementRotation(270, 90, true, fits)).toBe(270)
  })

  it('rejects a chest that is flush with the outer floor edge', () => {
    const chest = { width: 1.48, depth: 0.62 }
    expect(footprintOnHouseFloor(house, 0, 0.31, 3, 90, chest, 0.1)).toBe(false)
  })
})
