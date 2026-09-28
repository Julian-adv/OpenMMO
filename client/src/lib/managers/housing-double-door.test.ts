import { describe, expect, it } from 'vitest'
import { crossRoomDoorPartner, doubleDoorPartner } from './housing-passability'
import type { RoomData, WallConfig, WallVariant } from '../types/housing'
import { makeRoom } from '../utils/house-room.fixture'

const wall = (...v: WallVariant[]): WallConfig[] =>
  v.map((variant) => ({ variant, texture: 0 }))

describe('doubleDoorPartner', () => {
  it('pairs a centered double door both ways', () => {
    const w = wall('solid', 'double-door', 'double-door', 'solid')
    expect(doubleDoorPartner(w, 1)).toBe(2)
    expect(doubleDoorPartner(w, 2)).toBe(1)
  })

  it('pairs runs from their start and leaves a trailing half alone', () => {
    const w = wall('double-door', 'double-door', 'double-door')
    expect(doubleDoorPartner(w, 0)).toBe(1)
    expect(doubleDoorPartner(w, 1)).toBe(0)
    expect(doubleDoorPartner(w, 2)).toBe(-1)
  })

  it('returns -1 for non-double segments', () => {
    expect(doubleDoorPartner(wall('door', 'solid'), 0)).toBe(-1)
  })
})

const room = (localX: number, south: WallConfig[]): RoomData =>
  makeRoom({ localX, sizeX: 3, sizeZ: 3, wallSouth: south })

describe('crossRoomDoorPartner', () => {
  it('pairs lone halves across the shared wall end of adjacent rooms', () => {
    const rooms = [
      room(0, wall('solid', 'solid', 'double-door')),
      room(3, wall('double-door', 'solid', 'solid')),
    ]
    expect(crossRoomDoorPartner(rooms, 0, 'south', 2)).toEqual({
      roomIndex: 1,
      segmentIndex: 0,
    })
    expect(crossRoomDoorPartner(rooms, 1, 'south', 0)).toEqual({
      roomIndex: 0,
      segmentIndex: 2,
    })
  })

  it('ignores non-adjacent rooms and in-wall pairs', () => {
    const rooms = [
      room(0, wall('solid', 'solid', 'double-door')),
      room(4, wall('double-door', 'double-door', 'solid')),
    ]
    expect(crossRoomDoorPartner(rooms, 0, 'south', 2)).toBeNull()
    expect(crossRoomDoorPartner(rooms, 1, 'south', 0)).toBeNull()
  })
})
