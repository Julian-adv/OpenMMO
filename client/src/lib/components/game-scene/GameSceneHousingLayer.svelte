<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import type { HouseData } from '../../types/housing'
  import {
    buildHouseGroup,
    disposeHouseGroup,
    type HouseGroupResult,
  } from '../../utils/house-geometry'
  import { housingManager } from '../../managers/housingManager'
  import {
    TERRAIN_TILE_SIZE,
    getTerrainChunkFromPosition,
  } from './terrain-utils'

  interface Props {
    playerPosition: { x: number; y: number; z: number } | null
  }

  let { playerPosition }: Props = $props()

  const housingGroup = new THREE.Group()
  housingGroup.name = 'housingLayer'
  const houses = new SvelteMap<string, HouseGroupResult>()
  let playerInsideHouseId: string | null = null
  const _tmpVec = new THREE.Vector3()
  let lastChunkX = NaN
  let lastChunkZ = NaN

  // Listen for housing data changes from the manager
  housingManager.onHousesChanged = (allHouses: HouseData[]) => {
    syncHouses(allHouses)
  }

  onDestroy(() => {
    housingManager.onHousesChanged = null
    for (const [, result] of houses) {
      disposeHouseGroup(result.houseGroup)
    }
    houses.clear()
  })

  function syncHouses(allHouses: HouseData[]) {
    const incoming = new Set(allHouses.map((h) => h.id))

    // Remove houses no longer present
    for (const [id, result] of houses) {
      if (!incoming.has(id)) {
        housingGroup.remove(result.houseGroup)
        disposeHouseGroup(result.houseGroup)
        houses.delete(id)
      }
    }

    // Add new houses
    for (const data of allHouses) {
      if (!houses.has(data.id)) {
        const result = buildHouseGroup(data)
        houses.set(data.id, result)
        housingGroup.add(result.houseGroup)
      }
    }
  }

  /** Called from game loop — loads chunks + checks player inside state */
  export function update(_deltaTime: number) {
    if (!playerPosition) return

    // Load housing chunks around player when chunk changes
    const { x: cx, z: cz } = getTerrainChunkFromPosition(
      playerPosition,
      TERRAIN_TILE_SIZE
    )
    if (cx !== lastChunkX || cz !== lastChunkZ) {
      lastChunkX = cx
      lastChunkZ = cz
      housingManager.loadChunksAround(playerPosition.x, playerPosition.z)
    }

    // Player-inside detection
    _tmpVec.set(playerPosition.x, playerPosition.y, playerPosition.z)
    let insideId: string | null = null

    for (const [id, result] of houses) {
      if (result.aabb.containsPoint(_tmpVec)) {
        insideId = id
        break
      }
    }

    if (insideId !== playerInsideHouseId) {
      if (playerInsideHouseId) {
        const prev = houses.get(playerInsideHouseId)
        if (prev) prev.frontGroup.visible = true
      }
      if (insideId) {
        const curr = houses.get(insideId)
        if (curr) curr.frontGroup.visible = false
      }
      playerInsideHouseId = insideId
    }
  }

  export function getGroup(): THREE.Group {
    return housingGroup
  }
</script>

<T is={housingGroup} />
