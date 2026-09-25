import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { estateChests } from '../stores/estateFurnitureStore'
import catalog from '../../../public/models/objects/catalog.json'
import type { ObjectPlacement } from '../stores/editorStore'
import type { HouseData } from '../types/housing'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))

vi.mock('../utils/networkUtils', () => ({
  apiFetch,
  getTerrainApiUrl: () => '',
}))

const {
  ObjectManager,
  objectManager,
  isObjectInsideHouse,
  shouldMoveObjectWithHouse,
} = await import('./objectManager')

const chair = (id: number, x: number, z: number) => ({
  id,
  type: 'chair',
  x,
  y: 1.3,
  z,
  rotation: 0,
})

describe('world cache generation', () => {
  it('reuses a region until reset and rejects an older in-flight response', async () => {
    const manager = new ObjectManager()
    const old = { placements: [chair(1, 0, 0)] }
    const fresh = { placements: [chair(2, 1, 1)] }
    let release!: (value: Response) => void
    const fetch = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<Response>((resolve) => {
            release = resolve
          })
      )
      .mockResolvedValue({ ok: true, json: async () => fresh })
    vi.stubGlobal('fetch', fetch)
    try {
      const pending = manager.fetchObject(0, 0)
      manager.resetWorld()
      expect(await manager.fetchObject(0, 0)).toEqual(fresh)
      release({ ok: true, json: async () => old } as Response)
      expect(await pending).toEqual(fresh)
      expect(await manager.fetchObject(0, 0)).toEqual(fresh)
      expect(fetch).toHaveBeenCalledTimes(2)
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('leaves failed region requests retryable', async () => {
    const manager = new ObjectManager()
    const fetch = vi
      .fn()
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue({ ok: true, json: async () => ({ placements: [] }) })
    vi.stubGlobal('fetch', fetch)
    try {
      await expect(manager.fetchObject(0, 0)).rejects.toThrow('offline')
      expect(manager.getCached(0, 0)).toBeNull()
      expect(await manager.fetchObject(0, 0)).toEqual({ placements: [] })
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

describe('seat lookup', () => {
  afterEach(() => estateChests.set(new Map()))
  ;(objectManager as unknown as { cache: Map<string, unknown> }).cache.set(
    'test',
    { placements: [chair(42, -1451.7, 4750.3), chair(40, -1450.0, 4751.5)] }
  )

  it('takes the placement the server named over the nearest one', () => {
    expect(
      objectManager.findNearestPlacement('chair', -1449.0, 4753.4, 42)?.id
    ).toBe(42)
    expect(
      objectManager.findNearestPlacement('chair', -1451.7, 4750.3, 40)?.id
    ).toBe(40)
  })

  it('falls back to distance without an id', () => {
    expect(
      objectManager.findNearestPlacement('chair', -1449.0, 4753.4)?.id
    ).toBe(40)
    expect(
      objectManager.findNearestPlacement('chair', -1449.0, 4753.4, null)?.id
    ).toBe(40)
    expect(
      objectManager.findNearestPlacement('bed', -1449.0, 4753.4)
    ).toBeNull()
  })

  it.each([
    ['furniture_bed', 'bed', 0.78],
    ['furniture_rustic_bed', 'rustic_bed', 0.56],
  ])(
    'resolves %s by estate ID, including its saved rotation',
    async (itemId, modelId, height) => {
      const manager = new ObjectManager()
      const state = manager as unknown as {
        cache: Map<string, unknown>
        catalogCache: unknown[]
      }
      state.catalogCache = catalog
      state.cache.set('test', { placements: [chair(7, 1, 1)] })
      estateChests.set(
        new Map([
          [
            7,
            {
              id: 7,
              estate_id: 1,
              owner_id: 1,
              item_def_id: itemId as string,
              position: { x: 3, y: 4.15, z: 5 },
              rotation_deg: 270,
              floor_level: 1,
              overdue: false,
              revision: 0,
            },
          ],
        ])
      )

      const pose = await manager.resolvePose(itemId as string, 3, 5, 7)
      expect(pose.anim).toBe('sleep')
      expect(pose.interactOffset?.y).toBe(height)
      expect(pose.placement).toMatchObject({
        id: 7,
        type: modelId,
        x: 3,
        y: 4.15,
        z: 5,
        floorLevel: 1,
      })
      expect(pose.rotation).toBeCloseTo((3 * Math.PI) / 2)
      expect(
        (await manager.resolvePose(itemId as string, 3, 5, 8)).placement
      ).toBeNull()

      estateChests.set(new Map())
      expect(
        (await manager.resolvePose(itemId as string, 3, 5, 7)).placement
      ).toBeNull()
      expect(manager.findNearestPlacement('chair', 1, 1, 7)?.type).toBe('chair')
    }
  )

  it('uses Rowan’s ground-floor rustic bed and its pose for a bed schedule', async () => {
    const manager = new ObjectManager()
    const bed = {
      id: 71,
      type: 'rustic_bed',
      x: -1451.768,
      y: 1.05,
      z: 4758.83,
      floorLevel: 0,
      rotation: 90,
    }
    const state = manager as unknown as {
      cache: Map<string, unknown>
      catalogCache: unknown[]
    }
    state.cache.set('-2,4', {
      placements: [
        { ...bed, id: 64, type: 'bed', y: 4.15, floorLevel: 1 },
        bed,
      ],
    })
    state.catalogCache = [
      {
        id: 'bed',
        interaction: 'sleep',
        interactOffset: { x: 0, y: 0.78, z: 0 },
      },
      {
        id: 'rustic_bed',
        interaction: 'sleep',
        interactOffset: { x: 0, y: 0.56, z: 0 },
      },
    ]

    const pose = await manager.resolvePose('bed', bed.x, bed.z, 71)

    expect(pose.placement).toEqual(bed)
    expect(pose.anim).toBe('sleep')
    expect(pose.interactOffset).toEqual({ x: 0, y: 0.56, z: 0 })
    expect(pose.rotation).toBeCloseTo(Math.PI / 2)
  })
})

const house = {
  id: 'house',
  ownerId: 'local',
  origin: { x: 10, y: 2, z: 20 },
  rooms: [
    {
      roomType: 'normal',
      localX: 0,
      localZ: 0,
      sizeX: 4,
      sizeZ: 6,
      floorLevel: 0,
    },
    {
      roomType: 'normal',
      localX: 0,
      localZ: 0,
      sizeX: 4,
      sizeZ: 6,
      floorLevel: 1,
    },
  ],
} as HouseData

function placement(x: number, z: number, floorLevel: number): ObjectPlacement {
  return {
    id: 1,
    type: 'table',
    x,
    y: 0,
    z,
    rotation: 0,
    floorLevel,
  }
}

describe('isObjectInsideHouse', () => {
  it('matches the room footprint and floor', () => {
    expect(isObjectInsideHouse(placement(12, 23, 0), house)).toBe(true)
    expect(isObjectInsideHouse(placement(12, 23, 1), house)).toBe(true)
    expect(isObjectInsideHouse(placement(12, 23, 2), house)).toBe(false)
    expect(isObjectInsideHouse(placement(15, 23, 0), house)).toBe(false)
  })

  it('includes only shop signs attached near the exterior', () => {
    const attached = {
      ...placement(9.4, 23, 0),
      type: 'shop_sign_weathered',
    }
    expect(shouldMoveObjectWithHouse(attached, house)).toBe(true)
    expect(
      shouldMoveObjectWithHouse({ ...attached, type: 'signpost' }, house)
    ).toBe(false)
    expect(shouldMoveObjectWithHouse({ ...attached, x: 8.9 }, house)).toBe(
      false
    )
  })
})

describe('ObjectManager.moveHouseContents', () => {
  beforeEach(() => {
    apiFetch.mockReset()
    vi.unstubAllGlobals()
  })

  it('moves contained objects into the destination region', async () => {
    const sourceInside = {
      ...placement(991.5, 1, 0),
      id: 7,
      text: 'keep me',
    }
    const sourceOutside = { ...placement(980, 1, 0), id: 8 }
    const destinationObject = { ...placement(1000, 1, 0), id: 9 }
    const regions = new Map([
      ['0,0', { placements: [sourceInside, sourceOutside] }],
      ['1,0', { placements: [destinationObject] }],
    ])
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        const match = url.match(/objects\/(-?\d+)\/(-?\d+)$/)
        const data = match
          ? regions.get(`${Number(match[1])},${Number(match[2])}`)
          : undefined
        return {
          ok: true,
          json: async () => structuredClone(data ?? { placements: [] }),
        }
      })
    )
    apiFetch.mockResolvedValue({ ok: true })

    const boundaryHouse = {
      ...house,
      origin: { x: 990, y: 0, z: 0 },
      rooms: [{ ...house.rooms[0], sizeX: 4, sizeZ: 4 }],
    }
    const manager = new ObjectManager()
    const moved = await manager.moveHouseContents(boundaryHouse, 1, 0)

    expect(moved).toBe(true)
    expect(manager.getCached(0, 0)?.placements).toEqual([sourceOutside])
    expect(manager.getCached(1, 0)?.placements).toEqual([
      destinationObject,
      { ...sourceInside, x: 992.5 },
    ])
    expect(apiFetch).toHaveBeenCalledTimes(2)
  })
})
