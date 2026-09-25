import { afterEach, describe, expect, it, vi } from 'vitest'
import { bridgeManager } from '../../managers/bridgeManager'
import { dungeonManager } from '../../managers/dungeonManager'
import { TerrainHeightManager } from '../../managers/terrainHeightManager'
import { floatingSurfaceY } from '../../utils/floatingSurface'
import { syncPlayerTerrainHeight } from './fsm/movement-tick'
import { createPlayerPhysics } from './player-physics'

afterEach(() => {
  bridgeManager.reset()
  vi.restoreAllMocks()
})

function boatPhysics() {
  const heightManager = new TerrainHeightManager()
  vi.spyOn(heightManager, 'groundYOrNull').mockReturnValue(-5)
  vi.spyOn(heightManager, 'hasHeightData').mockReturnValue(true)
  const player = { position: { x: 0, y: 0, z: 0 } }
  const hasWaterSurfaceData = vi.fn(() => true)
  const physics = createPlayerPhysics({
    getHeightManager: () => heightManager,
    getCurrentPlayerY: () => player.position.y,
    getFloorOffset: () => 0,
    getPassabilityFloor: () => 0,
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
  return { physics, player, heightManager, hasWaterSurfaceData }
}

describe('floating player physics', () => {
  it('keeps the rider and waypoints under a nearby bridge deck', () => {
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
    expect(physics.waypointHeight(0, 0, 0)).toBe(0)
  })

  it('holds the rider above the bed while water data is loading', () => {
    const { physics, player, heightManager, hasWaterSurfaceData } =
      boatPhysics()
    hasWaterSurfaceData.mockReturnValue(false)
    syncPlayerTerrainHeight({
      player,
      hasHeightData: (x, z) => heightManager.hasHeightData(x, z),
      sampleHeight: physics.sampleHeight,
    })
    expect(player.position.y).toBe(0)
    expect(physics.waypointHeight(0, 1, 0)).toBe(0)
  })

  it('keeps underground waypoints on the dungeon floor', () => {
    vi.spyOn(dungeonManager, 'sampleHeightAt').mockReturnValue(-10)
    expect(boatPhysics().physics.waypointHeight(-1, 0, 0)).toBe(-10)
  })
})
