import { afterEach, describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { TerrainHeightManager } from './terrainHeightManager'
import { encodeHeight, VERTS_PER_SIDE } from './terrain-height-types'
import { createTerrainGeometry } from '../components/game-scene/terrain-utils'
import { drainTileWork, enqueueTileWork } from '../utils/tileWorkQueue'

function heightmap(height: number) {
  return new Uint16Array(VERTS_PER_SIDE ** 2).fill(encodeHeight(height))
}

function snapshot(height: number) {
  return new Uint8Array(heightmap(height).buffer)
}

describe('terrain geometry reuse', () => {
  const geometries: THREE.BufferGeometry[] = []

  afterEach(() => {
    drainTileWork(1000)
    for (const geometry of geometries) geometry.dispose()
    geometries.length = 0
  })

  function setup() {
    const manager = new TerrainHeightManager()
    const geometry = createTerrainGeometry()
    geometries.push(geometry)
    manager.setHeightmap(-24, 75, heightmap(8))
    manager.setHeightmap(-23, 75, heightmap(1.5))
    manager.registerGeometry(-24, 75, geometry)
    manager.applyHeightToGeometry(-24, 75, geometry)
    return { manager, geometry }
  }

  function expectFlat(geometry: THREE.BufferGeometry, height: number) {
    const positions = geometry.getAttribute('position')
    for (let i = 0; i < positions.count; i++) {
      expect(positions.getY(i)).toBe(height)
    }
  }

  it('keeps a painted tile level when its geometry belonged to a neighboring tile', () => {
    const { manager, geometry } = setup()
    const neighbor = heightmap(8)
    for (let row = 0; row < VERTS_PER_SIDE; row++) {
      neighbor[row * VERTS_PER_SIDE + VERTS_PER_SIDE - 1] = encodeHeight(1.5)
    }
    manager.setHeightmap(-24, 75, neighbor)
    manager.registerGeometry(-23, 75, geometry)

    manager.applySnapshot(-23, 75, snapshot(1.5))

    expectFlat(geometry, 1.5)
  })

  it('does not apply snapshots for departed tiles to reused geometry', () => {
    const { manager, geometry } = setup()
    manager.registerGeometry(-23, 75, geometry)
    manager.applyHeightToGeometry(-23, 75, geometry)

    manager.applySnapshot(-24, 75, snapshot(9))

    expectFlat(geometry, 1.5)
  })

  it('ignores queued height work after geometry moves to another tile', () => {
    const { manager, geometry } = setup()
    enqueueTileWork(() => {
      manager.applyHeightToGeometry(-24, 75, geometry)
    })
    manager.unregisterGeometry(-24, 75)
    manager.registerGeometry(-23, 75, geometry)
    manager.applyHeightToGeometry(-23, 75, geometry)

    drainTileWork(1000)

    expectFlat(geometry, 1.5)
  })

  it('keeps released geometry unchanged while it waits in the pool', () => {
    const { manager, geometry } = setup()
    manager.unregisterGeometry(-24, 75)

    manager.applySnapshot(-24, 75, snapshot(9))
    manager.applyHeightToGeometry(-24, 75, geometry)

    expectFlat(geometry, 8)
  })

  it('refreshes the replacement when a tile is loaded again', () => {
    const { manager, geometry } = setup()
    manager.unregisterGeometry(-24, 75)
    const replacement = createTerrainGeometry()
    geometries.push(replacement)
    manager.registerGeometry(-24, 75, replacement)

    manager.applySnapshot(-24, 75, snapshot(9))
    manager.applyHeightToGeometry(-24, 75, geometry)

    expectFlat(replacement, 9)
    expectFlat(geometry, 8)
  })
})
