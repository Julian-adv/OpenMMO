import { afterEach, describe, expect, it, vi } from 'vitest'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'
import { housingManager } from '../../managers/housingManager'
import { TerrainHeightManager } from '../../managers/terrainHeightManager'
import { floatingSurfaceY } from '../../utils/floatingSurface'
import { createPlayerPhysics } from './player-physics'

afterEach(() => {
  bridgeManager.reset()
  vi.restoreAllMocks()
})

function boatPhysics(floor = 0) {
  const heightManager = new TerrainHeightManager()
  vi.spyOn(heightManager, 'groundYOrNull').mockReturnValue(-5)
  vi.spyOn(heightManager, 'hasHeightData').mockReturnValue(true)
  const player = { position: { x: 0, y: 0, z: 0 } }
  const hasWaterSurfaceData = vi.fn(() => true)
  const physics = createPlayerPhysics({
    getHeightManager: () => heightManager,
    getCurrentPlayerY: () => player.position.y,
    getPassabilityFloor: () => floor,
    getFloatSurfaceY: (x, z) =>
      floatingSurfaceY({
        x,
        z,
        fallbackY: player.position.y,
        heightManager,
        waterSurfaceAt: () => 0,
        hasWaterSurfaceData,
      }),
  })
  return { physics, player, hasWaterSurfaceData }
}

describe('floating player physics', () => {
  it('keeps the rider under a nearby bridge deck', () => {
    bridgeManager.syncRegion(
      0,
      0,
      [
        {
          id: 1,
          type: 'low_bridge',
          x: 0,
          y: 0,
          z: 0,
          rotation: 0,
          floorLevel: 0,
        },
      ],
      new Map([
        [
          'low_bridge',
          {
            id: 'low_bridge',
            name: 'Low bridge',
            kind: 'bridge',
            bridge: {
              deckMinX: -2,
              deckMaxX: 2,
              deckMinZ: -10,
              deckMaxZ: 10,
              deckCrownY: 1,
              deckEndY: 1,
              deckAxis: 'z',
            },
          },
        ],
      ])
    )
    expect(bridgeManager.findDeckYAt(0, 0, 0)).toBe(1)
    const { physics } = boatPhysics()
    expect(physics.sampleHeight(0, 0)).toBe(0)
  })

  it('holds the rider above the bed while water data is loading', () => {
    const { physics, player, hasWaterSurfaceData } = boatPhysics()
    hasWaterSurfaceData.mockReturnValue(false)
    expect(physics.sampleHeight(0, 0)).toBe(0)
    expect(player.position.y).toBe(0)
    expect(physics.sampleHeight(1, 0)).toBe(0)
  })

  it('keeps underground interactions on the dungeon floor', () => {
    vi.spyOn(dungeonManager, 'sampleHeightAt').mockReturnValue(-10)
    expect(boatPhysics(-1).physics.sampleHeight(0, 0)).toBe(-10)
  })
})

describe('house interaction heights', () => {
  it('samples the destination floor and falls back to terrain outside the house', () => {
    const heightManager = new TerrainHeightManager()
    vi.spyOn(heightManager, 'getHeightAtWorldPosition').mockReturnValue(-5)
    const houseHeight = vi.spyOn(housingManager, 'floorHeightAt')
    houseHeight.mockReturnValueOnce(67.15).mockReturnValueOnce(null)
    const physics = createPlayerPhysics({
      getHeightManager: () => heightManager,
      getCurrentPlayerY: () => 67.1,
      getPassabilityFloor: () => 1,
    })

    expect(physics.sampleHeight(3.5, 3.75)).toBe(67.15)
    expect(houseHeight).toHaveBeenLastCalledWith(1, 3.5, 3.75)
    expect(physics.sampleHeight(99, 99)).toBe(-5)
  })
})
