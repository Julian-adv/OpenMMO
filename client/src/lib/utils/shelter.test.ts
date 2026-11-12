import { describe, expect, it } from 'vitest'
import { Vector3 } from 'three'
import {
  doorwaysOf,
  isInsideShelter,
  normalizeYaw,
  shelterRoomAt,
} from './shelter'
import type { ObjectPlacement } from '../stores/editorStore'
import { WORLD_WIDTH_X } from '../terrain/world-wrap'

const perimeter = { width: 2.4, depth: 3.2, height: 2.3, entranceWidth: 1.8 }
const placement: ObjectPlacement = {
  id: 1,
  type: 'ysolde_tent',
  x: 10,
  y: 7,
  z: 20,
  rotation: 0,
  floorLevel: 0,
}

describe('tent rotation', () => {
  it('normalizes existing wrapped yaw values', () => {
    expect(normalizeYaw(-90)).toBe(270)
    expect(normalizeYaw(360)).toBe(0)
    expect(normalizeYaw(450)).toBe(90)
  })
})

describe('rotated tent cutaway', () => {
  it.each([0, 90, 180, 270, 45])(
    'uses the local footprint at yaw %s',
    (rotation) => {
      const placed = { ...placement, rotation }
      const world = (x: number, z: number) => {
        const point = new Vector3(x, 0, z).applyAxisAngle(
          new Vector3(0, 1, 0),
          (rotation * Math.PI) / 180
        )
        return { x: point.x + placed.x, y: placed.y, z: point.z + placed.z }
      }
      expect(isInsideShelter(world(0, 1.4), placed, perimeter)).toBe(true)
      expect(isInsideShelter(world(1.3, 0), placed, perimeter)).toBe(false)
      expect(isInsideShelter(world(0, 1.7), placed, perimeter)).toBe(false)
    }
  )

  it('keeps the front visible on another floor', () => {
    expect(
      isInsideShelter({ x: 10, y: 10.1, z: 20 }, placement, perimeter)
    ).toBe(false)
    expect(isInsideShelter({ x: 10, y: 6, z: 20 }, placement, perimeter)).toBe(
      false
    )
  })

  it('uses the nearby representation at the world seam', () => {
    expect(
      isInsideShelter(
        { x: 10 + WORLD_WIDTH_X, y: 7, z: 20 },
        placement,
        perimeter
      )
    ).toBe(true)
  })
})

describe('shelter room', () => {
  it('tells the perimeter from an annex', () => {
    const hall: ObjectPlacement = {
      id: 121,
      type: 'union_hall',
      x: 100,
      y: 0,
      z: 100,
      rotation: 0,
      floorLevel: 0,
    }
    const perimeter = { width: 12, depth: 17, height: 9, entranceWidth: 3.8 }
    const annexes = [
      {
        x: [-4, 4] as [number, number],
        z: [-14.1, -8.5] as [number, number],
        opening: 6,
      },
    ]
    const at = (x: number, z: number) =>
      shelterRoomAt({ x, y: 0.5, z }, hall, perimeter, annexes)
    expect(at(100, 100)).toBe('perimeter')
    expect(at(100, 89)).toBe(0)
    expect(at(105, 89)).toBeNull()
  })
})

describe('object doorway', () => {
  it('spans the entrance of a west-facing hall', () => {
    const hall: ObjectPlacement = {
      id: 121,
      type: 'union_hall',
      x: -1445,
      y: 1,
      z: 4797,
      rotation: 270,
      floorLevel: 0,
    }
    const [door, side] = doorwaysOf(hall, {
      perimeter: { width: 12, depth: 17, height: 9, entranceWidth: 3.8 },
      swingDoor: { leaves: [] },
      sideDoors: [{ segment: [4.27, -13.75, 4.27, -12.65], leaves: [] }],
    })
    expect(door.door).toBe(0)
    expect((door.ax + door.bx) / 2).toBeCloseTo(-1453.5)
    expect((door.az + door.bz) / 2).toBeCloseTo(4797)
    expect(Math.abs(door.az - door.bz)).toBeCloseTo(3.8)
    expect(door.ax).toBeCloseTo(door.bx)
    expect(side.door).toBe(1)
    expect((side.ax + side.bx) / 2).toBeCloseTo(-1431.8)
    expect(side.az).toBeCloseTo(4801.27)
    expect(side.bz).toBeCloseTo(4801.27)
  })
})
