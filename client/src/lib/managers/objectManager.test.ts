import { describe, expect, it } from 'vitest'
import { ObjectManager, objectManager } from './objectManager'

const chair = (id: number, x: number, z: number) => ({
  id,
  type: 'chair',
  x,
  y: 1.3,
  z,
  rotation: 0,
})

describe('seat lookup', () => {
  // The server's placement ID takes precedence over distance.
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
