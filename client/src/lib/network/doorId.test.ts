import { describe, expect, it } from 'vitest'
import { dungeonDoorId, houseDoorId, objectDoorId, parseDoorId } from './doorId'

describe('door ids', () => {
  it('round-trips every kind in the shared format', () => {
    expect(houseDoorId('r-23_+74_1', 2, 'west', 3)).toBe(
      'house:r-23_+74_1:2:west:3'
    )
    expect(parseDoorId('house:odd:id:0:north:1')).toEqual({
      kind: 'house',
      houseId: 'odd:id',
      roomIndex: 0,
      wallDir: 'north',
      segmentIndex: 1,
    })
    expect(parseDoorId(dungeonDoorId('old_crypt', 1, 7))).toEqual({
      kind: 'dungeon',
      entranceId: 'old_crypt',
      depth: 1,
      doorId: 7,
    })
    expect(parseDoorId(objectDoorId(-23, 74, 9, 1))).toEqual({
      kind: 'object',
      rx: -23,
      rz: 74,
      objectId: 9,
      door: 1,
    })
  })

  it('refuses malformed ids', () => {
    for (const door of [
      '',
      'object:1:2:3',
      'object:1:2:3:4:5',
      'object:1:2:3:8',
      'object:a:2:3:0',
      'dungeon::1:2',
      'house:h:0:up:1',
      'house::0:north:1',
      'gate:1:2:3',
    ])
      expect(parseDoorId(door), door).toBeNull()
  })
})
