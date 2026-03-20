<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import type { HouseData } from '../../types/housing'
  import {
    buildHouseGroup,
    disposeHouseGroup,
    FLOOR_THICKNESS,
    OFFSCREEN_Y,
    type HouseGroupResult,
  } from '../../utils/house-geometry'
  import {
    initHousingTextures,
    disposeHousingMaterials,
  } from '../../utils/housing-textures'
  import { housingManager } from '../../managers/housingManager'
  import {
    TERRAIN_TILE_SIZE,
    getTerrainChunkFromPosition,
  } from './terrain-utils'
  import { playerFloorOffset } from '../../stores/housingStore'

  interface Props {
    playerPosition: { x: number; y: number; z: number } | null
  }

  let { playerPosition }: Props = $props()

  const housingGroup = new THREE.Group()
  housingGroup.name = 'housingLayer'

  const houses = new SvelteMap<string, HouseGroupResult>()
  let playerInsideHouseId: string | null = null
  let lastFloorOffset = 0
  const _tmpVec = new THREE.Vector3()
  let lastChunkX = NaN
  let lastChunkZ = NaN

  // Load housing textures (materials update in-place via needsUpdate)
  initHousingTextures()

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
    disposeHousingMaterials()
  })

  function syncHouses(allHouses: HouseData[]) {
    const incomingById = new Map(allHouses.map((h) => [h.id, h]))

    // Remove houses no longer present
    for (const [id, result] of houses) {
      if (!incomingById.has(id)) {
        housingGroup.remove(result.houseGroup)
        disposeHouseGroup(result.houseGroup)
        houses.delete(id)
      }
    }

    // Add or rebuild changed houses
    for (const data of allHouses) {
      const existing = houses.get(data.id)
      const newHash = JSON.stringify(data.rooms)
      if (existing && existing.roomsHash === newHash) continue

      if (existing) {
        housingGroup.remove(existing.houseGroup)
        disposeHouseGroup(existing.houseGroup)
      }
      const result = buildHouseGroup(data)
      // Re-apply front group offset if player is inside this house
      if (data.id === playerInsideHouseId) {
        result.frontGroup.position.y = OFFSCREEN_Y
      }
      houses.set(data.id, result)
      housingGroup.add(result.houseGroup)
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

    // Player-inside detection (per-room, floor-aware)
    _tmpVec.set(playerPosition.x, playerPosition.y, playerPosition.z)
    let insideId: string | null = null
    let insideRoom: { floorLevel: number; wallHeight: number } | null = null

    for (const [id, result] of houses) {
      if (result.aabb.containsPoint(_tmpVec)) {
        // Fine-grained per-room check
        const roomResult = housingManager.findRoomAtPoint(
          playerPosition.x,
          playerPosition.y,
          playerPosition.z
        )
        if (roomResult && roomResult.house.id === id) {
          insideId = id
          const room = roomResult.house.rooms[roomResult.roomIndex]
          insideRoom = { floorLevel: room.floorLevel, wallHeight: room.wallHeight }
          break
        }
      }
    }

    if (insideId !== playerInsideHouseId) {
      // Hide/show front walls by moving off-screen instead of toggling visible,
      // to avoid WebGPU render bundle recompilation.
      if (playerInsideHouseId) {
        const prev = houses.get(playerInsideHouseId)
        if (prev) prev.frontGroup.position.y = 0
      }
      if (insideId) {
        const curr = houses.get(insideId)
        if (curr) curr.frontGroup.position.y = OFFSCREEN_Y
      }
      playerInsideHouseId = insideId
    }

    const newOffset = insideRoom
      ? insideRoom.floorLevel * insideRoom.wallHeight + FLOOR_THICKNESS / 2
      : 0
    if (newOffset !== lastFloorOffset) {
      lastFloorOffset = newOffset
      playerFloorOffset.set(newOffset)
    }
  }

  export function getGroup(): THREE.Group {
    return housingGroup
  }
</script>

<T is={housingGroup} />
