<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import type { HouseData } from '../../types/housing'
  import {
    buildHouseGroup,
    disposeHouseGroup,
    DEFAULT_WALL_HEIGHT,
    FLOOR_THICKNESS,
    OFFSCREEN_Y,
    floorYBase,
    getStairwellYOffset,
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
  let playerInsideFloor = -1
  let lastFloorOffset = 0
  const _tmpVec = new THREE.Vector3()
  // Preallocated for per-frame room detection (avoid GC)
  const _allRooms: { house: HouseData; roomIndex: number }[] = []
  const _seenRooms = new SvelteSet<string>()
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
      // Re-apply visibility if player is inside this house
      if (data.id === playerInsideHouseId) {
        applyFloorVisibility(result, playerInsideFloor)
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
    // Use ground-level Y for AABB check, then try multiple floor levels
    // to detect both 1F and 2F rooms
    const groundY = playerPosition.y - lastFloorOffset
    let insideId: string | null = null
    let newOffset = 0
    let effectiveFloor = -1

    for (const [id, result] of houses) {
      // Expand AABB check to cover both floors
      _tmpVec.set(playerPosition.x, groundY, playerPosition.z)
      if (!result.aabb.containsPoint(_tmpVec)) {
        // Also try at elevated Y in case AABB spans 2 floors
        _tmpVec.set(playerPosition.x, playerPosition.y, playerPosition.z)
        if (!result.aabb.containsPoint(_tmpVec)) continue
      }

      // Try all floor levels to find matching rooms
      _allRooms.length = 0
      _seenRooms.clear()
      for (let fl = 1; fl >= 0; fl--) {
        const testY = groundY + floorYBase(fl, DEFAULT_WALL_HEIGHT) + 1
        for (const r of housingManager.findAllRoomsAtPoint(
          playerPosition.x,
          testY,
          playerPosition.z
        )) {
          const key = `${r.house.id}:${r.roomIndex}`
          if (!_seenRooms.has(key)) {
            _seenRooms.add(key)
            _allRooms.push(r)
          }
        }
      }

      // First pass: find stairwell (always takes priority)
      // Second pass: find room matching current floor
      const currentFL = Math.max(0, playerInsideFloor)
      let stairResult: typeof _allRooms[0] | null = null
      let floorResult: typeof _allRooms[0] | null = null
      for (const roomResult of _allRooms) {
        if (roomResult.house.id !== id) continue
        const room = roomResult.house.rooms[roomResult.roomIndex]
        if (room.roomType === 'stairwell') {
          stairResult = roomResult
        } else if (!floorResult || room.floorLevel === currentFL) {
          floorResult = roomResult
        }
      }

      if (stairResult) {
        const room = stairResult.house.rooms[stairResult.roomIndex]
        insideId = id
        newOffset = getStairwellYOffset(
          room,
          stairResult.house.origin.x,
          stairResult.house.origin.z,
          playerPosition.x,
          playerPosition.z
        )
        if (playerInsideFloor <= 0) {
          effectiveFloor =
            newOffset >= floorYBase(1, room.wallHeight) ? 1 : 0
        } else {
          effectiveFloor = newOffset <= FLOOR_THICKNESS ? 0 : 1
        }
      } else if (floorResult) {
        const room = floorResult.house.rooms[floorResult.roomIndex]
        insideId = id
        newOffset =
          floorYBase(room.floorLevel, room.wallHeight) + FLOOR_THICKNESS / 2
        effectiveFloor = room.floorLevel
      }
      if (insideId) break
    }

    // Update visibility when house or floor changes
    if (
      insideId !== playerInsideHouseId ||
      effectiveFloor !== playerInsideFloor
    ) {
      // Restore previous house
      if (playerInsideHouseId) {
        const prev = houses.get(playerInsideHouseId)
        if (prev) resetFloorVisibility(prev)
      }
      // Apply new visibility
      if (insideId) {
        const curr = houses.get(insideId)
        if (curr) applyFloorVisibility(curr, effectiveFloor)
      }
      playerInsideHouseId = insideId
      playerInsideFloor = effectiveFloor
    }

    if (newOffset !== lastFloorOffset) {
      lastFloorOffset = newOffset
      playerFloorOffset.set(newOffset)
    }
  }

  /**
   * On 1F: hide 1F front + all of 2F (front+back) so ceiling/upper floor doesn't block view
   * On 2F: hide 2F front only, 1F stays fully visible
   */
  function applyFloorVisibility(
    result: HouseGroupResult,
    floor: number
  ) {
    for (const [fl, groups] of result.floorGroups) {
      if (fl === floor) {
        // Current floor: hide front (south/west walls + roof)
        groups.front.position.y = OFFSCREEN_Y
      } else if (fl > floor) {
        // Floors above: hide everything (front + back) so they don't block view
        groups.front.position.y = OFFSCREEN_Y
        groups.back.position.y = OFFSCREEN_Y
      }
    }
  }

  /** Restore all groups to normal position */
  function resetFloorVisibility(result: HouseGroupResult) {
    for (const [, groups] of result.floorGroups) {
      groups.front.position.y = 0
      groups.back.position.y = 0
    }
  }

  export function getGroup(): THREE.Group {
    return housingGroup
  }
</script>

<T is={housingGroup} />
