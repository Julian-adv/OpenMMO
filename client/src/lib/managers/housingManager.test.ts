import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { HouseData } from '../types/housing'
import { worldView, type WorldUpdate } from '../network/worldView'

const removed: string[] = []
vi.mock('../wasm/onlinerpg_shared', () => ({
  world_constants: () => ({ eventDeliveryRadius: 32 }),
  passability_add_house: vi.fn(),
  passability_remove_house: (id: string) => removed.push(id),
  passability_update_door: vi.fn(),
  passability_is_movement_blocked: () => false,
  passability_is_circle_blocked: () => false,
}))
const { HousingManager } = await import('./housingManager')

function house(id: string, x = 0): HouseData {
  return {
    id,
    origin: { x, y: 0, z: 0 },
    rooms: [],
    passability: [{ floorLevel: 0, cells: [] }],
  } as unknown as HouseData
}

describe('house subscriptions', () => {
  let manager: InstanceType<typeof HousingManager>
  let generation = 0
  let snapshot: WorldUpdate
  beforeEach(() => {
    manager = new HousingManager()
    removed.length = 0
    snapshot = {
      world_epoch: 'housing-test',
      generation: ++generation,
      sequence: 1,
      position: { x: 0, y: 0, z: 0 },
      floor_level: 0,
      ready: true,
      reset: true,
      events: [],
    }
    worldView.pendingTerrain.clear()
    worldView.accept(snapshot)
  })

  it('waits for the complete snapshot, including an empty one', async () => {
    let ready = false
    const waiting = manager.waitForSnapshot().then(() => {
      ready = true
    })
    await Promise.resolve()
    expect(ready).toBe(false)
    manager.completeSnapshot()
    await waiting
    expect(manager.isSynchronized(0, 0)).toBe(true)
  })

  it('removes rendering and collision together on leave and reset', () => {
    manager.handleRemoteHousesBatch([house('a'), house('b')])
    manager.completeSnapshot()
    manager.handleRemoteHouseRemoved('a')
    expect(manager.getAllHouses().map((h) => h.id)).toEqual(['b'])
    expect(removed).toEqual(['a'])
    manager.resetView()
    expect(manager.getAllHouses()).toEqual([])
    expect(removed).toEqual(['a', 'b'])
    expect(manager.isSynchronized(0, 0)).toBe(false)
  })

  it('waits for the respawn destination even after the dungeon snapshot completed', () => {
    worldView.accept({
      ...snapshot,
      reset: false,
      sequence: 2,
      position: { x: -1088.6, y: -71.05, z: 4273.4 },
      floor_level: -18,
    })
    manager.completeSnapshot()
    expect(manager.isSynchronized(-1451.5, 4754.05)).toBe(false)

    manager.handleRemoteHousesBatch([house('inn', -1451.5)])
    expect(manager.isSynchronized(-1451.5, 4754.05)).toBe(false)
    worldView.accept({
      ...snapshot,
      reset: false,
      sequence: 3,
      position: { x: -1451.5, y: 4.4, z: 4754.05 },
      floor_level: 1,
    })
    worldView.pendingTerrain.add('-23,74')
    expect(manager.isSynchronized(-1451.5, 4754.05)).toBe(false)
    worldView.pendingTerrain.clear()
    expect(manager.isSynchronized(-1451.5, 4754.05)).toBe(true)
  })

  it('does not reuse nearby surface data or an underground snapshot for a teleport', () => {
    manager.completeSnapshot()
    expect(manager.isSynchronized(40, 0)).toBe(false)
    worldView.accept({
      ...snapshot,
      reset: false,
      sequence: 2,
      floor_level: -1,
    })
    expect(manager.isSynchronized(0, 0)).toBe(false)
  })

  it('replaces a returning house with the server snapshot', () => {
    manager.handleRemoteHouseSpawned(house('a', 0))
    manager.handleRemoteHouseRemoved('a')
    manager.handleRemoteHouseSpawned(house('a', 90))
    expect(manager.getHouseById('a')?.origin.x).toBe(90)
  })
})
