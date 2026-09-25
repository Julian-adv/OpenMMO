import { describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { currentDungeonDepth } from '../stores/dungeonStore'
import { playerVisualFloorLevel } from '../stores/housingStore'
import { syncOwnFloor } from './ownFloor'

vi.mock('../managers/dungeonManager', () => ({
  dungeonManager: {
    syncFromFloorLevel: (floor: number) =>
      currentDungeonDepth.set(Math.max(0, -floor)),
  },
}))

function reportedFloors(startFloor: number, destinationFloor?: number) {
  currentDungeonDepth.set(Math.max(0, -startFloor))
  playerVisualFloorLevel.set(Math.max(0, startFloor))
  const reported: number[] = []
  let lastFloor = startFloor
  const sync = () => {
    const depth = get(currentDungeonDepth)
    const floor = depth > 0 ? -depth : get(playerVisualFloorLevel)
    if (floor === lastFloor) return
    lastFloor = floor
    reported.push(floor)
  }
  const unsubscribeDepth = currentDungeonDepth.subscribe(sync)
  const unsubscribeFloor = playerVisualFloorLevel.subscribe(sync)
  try {
    syncOwnFloor(destinationFloor, -1451.5, 4754.05)
    return reported
  } finally {
    unsubscribeDepth()
    unsubscribeFloor()
  }
}

describe('server floor synchronization', () => {
  it('respawns from a dungeon directly onto the inn storey', () => {
    expect(reportedFloors(-18, 1)).toEqual([1])
  })

  it('teleports from an upper storey directly into a dungeon', () => {
    expect(reportedFloors(1, -2)).toEqual([-2])
  })

  it('keeps an in-place dungeon revive on the same floor', () => {
    expect(reportedFloors(-18, -18)).toEqual([])
  })

  it('defaults an omitted floor to the surface', () => {
    expect(reportedFloors(-18)).toEqual([0])
  })
})
