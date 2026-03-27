<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import type { HouseData } from '../../types/housing'
  import {
    buildHouseGroup,
    disposeHouseGroup,
    applyDoorGhostMaterials,
    resetDoorGhostMaterials,
    DEFAULT_WALL_HEIGHT,
    FLOOR_THICKNESS,
    MAX_FLOOR_LEVEL,
    OFFSCREEN_Y,
    floorYBase,
    getStairwellYOffset,
    type HouseGroupResult,
  } from '../../utils/house-geometry'
  import {
    initHousingTextures,
    disposeHousingMaterials,
  } from '../../utils/housing-textures'
  import { getWallByDir } from '../../managers/housingManager'
  import { housingManager } from '../../managers/housingManager'
  import {
    TERRAIN_TILE_SIZE,
    getTerrainChunkFromPosition,
  } from './terrain-utils'
  import { playerFloorOffset, playerFloorLevel } from '../../stores/housingStore'
  import { debugVisible, passabilityDebugVisible } from '../../stores/debugStore'
  import { EDGE_N, EDGE_E, EDGE_S, EDGE_W } from '../../managers/housing-passability'
  import { get } from 'svelte/store'

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
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const _seenRooms = new Set<string>()
  let lastChunkX = NaN
  let lastChunkZ = NaN

  // Debug passability wireframe
  const debugPassGroup = new THREE.Group()
  debugPassGroup.name = 'passabilityDebug'
  debugPassGroup.visible = false
  housingGroup.add(debugPassGroup)

  const debugLineMaterial = new THREE.LineBasicMaterial({ color: 0xff0000 })
  let debugPassDirty = false

  const unsubPassDebug = passabilityDebugVisible.subscribe((v) => {
    debugPassGroup.visible = v
    if (v) debugPassDirty = true
  })

  function rebuildPassabilityDebug() {
    // Clear old
    while (debugPassGroup.children.length > 0) {
      const child = debugPassGroup.children[0]
      debugPassGroup.remove(child)
      if (child instanceof THREE.LineSegments) {
        child.geometry.dispose()
      }
    }

    const WALL_Y = 0.05 // slightly above floor
    const WALL_H = 0.1  // line height

    for (const [houseId, rp] of housingManager.getPassabilityEntries()) {
      const house = housingManager.getHouseById(houseId)
      if (!house) continue

      const vertices: number[] = []

      for (const floor of rp.floors) {
        const baseY = floor.yBase
        const ox = house.origin.x + floor.originX
        const oz = house.origin.z + floor.originZ

        for (let gz = 0; gz < floor.depth; gz++) {
          for (let gx = 0; gx < floor.width; gx++) {
            const bits = floor.cells[gx + gz * floor.width]
            if (bits === 0) continue

            const cx = ox + gx
            const cz = oz + gz
            const y0 = baseY + WALL_Y
            const y1 = baseY + WALL_Y + WALL_H

            const pushQuad = (x0: number, z0: number, x1: number, z1: number) => {
              vertices.push(x0, y0, z0, x1, y0, z1) // bottom
              vertices.push(x0, y1, z0, x1, y1, z1) // top
              vertices.push(x0, y0, z0, x0, y1, z0) // left vertical
              vertices.push(x1, y0, z1, x1, y1, z1) // right vertical
            }
            if (bits & EDGE_N) pushQuad(cx, cz, cx + 1, cz)
            if (bits & EDGE_S) pushQuad(cx, cz + 1, cx + 1, cz + 1)
            if (bits & EDGE_W) pushQuad(cx, cz, cx, cz + 1)
            if (bits & EDGE_E) pushQuad(cx + 1, cz, cx + 1, cz + 1)
          }
        }
      }

      if (vertices.length > 0) {
        const geo = new THREE.BufferGeometry()
        geo.setAttribute(
          'position',
          new THREE.Float32BufferAttribute(vertices, 3)
        )
        const lines = new THREE.LineSegments(geo, debugLineMaterial)
        lines.frustumCulled = false
        debugPassGroup.add(lines)
      }
    }

    debugPassDirty = false
  }

  // Load housing textures (materials update in-place via needsUpdate)
  initHousingTextures().then(() => {
    // Re-apply ghost materials now that textures are loaded
    if (playerInsideHouseId) {
      const curr = houses.get(playerInsideHouseId)
      if (curr) {
        resetDoorGhostMaterials(curr)
        applyDoorGhostMaterials(curr, playerInsideFloor)
      }
    }
  })

  // Listen for housing data changes from the manager
  const unsubHouses = housingManager.onHousesChanged((allHouses) => {
    syncHouses(allHouses)
    if (debugPassGroup.visible) debugPassDirty = true
  })

  onDestroy(() => {
    unsubHouses()
    unsubPassDebug()
    for (const [, result] of houses) {
      disposeHouseGroup(result.houseGroup)
    }
    houses.clear()
    disposeHousingMaterials()
    debugLineMaterial.dispose()
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

      // Fast path: if only door isOpen changed, sync door states without rebuild
      if (existing && existing.roomsHash === newHash) continue
      if (existing && syncDoorStates(existing, data, newHash)) continue

      if (existing) {
        housingGroup.remove(existing.houseGroup)
        disposeHouseGroup(existing.houseGroup)
      }
      const result = buildHouseGroup(data, newHash)
      houses.set(data.id, result)
      housingGroup.add(result.houseGroup)

      // Re-apply visibility if player is inside this house
      if (data.id === playerInsideHouseId) {
        applyFloorVisibility(result, playerInsideFloor)
      }
    }

    if (houses.size > 0 && get(debugVisible)) {
      const s = getStats()
      console.log(
        `[housing] ${s.houses} houses | ${s.mergedMeshes} merged meshes (draw calls)`
      )
    }
  }

  const isOpenReplacer = (_k: string, v: unknown) => _k === 'isOpen' ? undefined : v

  /** Returns true if the only changes were door isOpen flags (no geometry rebuild needed). */
  function syncDoorStates(existing: HouseGroupResult, data: HouseData, newHash: string): boolean {
    // Compare geometry excluding isOpen — both sides stripped from their full hashes
    if (
      JSON.stringify(JSON.parse(newHash), isOpenReplacer) !==
      JSON.stringify(JSON.parse(existing.roomsHash), isOpenReplacer)
    ) return false

    for (const door of existing.doors) {
      const room = data.rooms[door.roomIndex]
      if (!room) continue
      const seg = getWallByDir(room, door.wallDir)[door.segmentIndex]
      if (seg) door.isOpen = seg.isOpen ?? false
    }

    existing.roomsHash = newHash
    return true
  }

  const DOOR_SWING_SPEED = Math.PI // radians per second (~0.5s for 90°)

  /** Called from game loop — loads chunks + checks player inside state */
  export function update(_deltaTime: number) {
    if (!playerPosition) return

    // Rebuild passability debug lines if needed
    if (debugPassDirty && debugPassGroup.visible) rebuildPassabilityDebug()

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
      for (let fl = MAX_FLOOR_LEVEL; fl >= 0; fl--) {
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

      // Pick the stairwell whose Y offset is closest to the player's
      // current Y (lastFloorOffset). This correctly handles stacked
      // stairwells at the same XZ — the player smoothly transitions
      // onto the nearest one rather than jumping to a distant floor.
      const currentFL = Math.max(0, playerInsideFloor)
      let stairResult: typeof _allRooms[0] | null = null
      let bestStairDist = Infinity
      let bestStairOffset = 0
      let floorResult: typeof _allRooms[0] | null = null
      for (const roomResult of _allRooms) {
        if (roomResult.house.id !== id) continue
        const room = roomResult.house.rooms[roomResult.roomIndex]
        if (room.roomType === 'stairwell') {
          // Only consider stairwells whose floor range includes the player's
          // current floor — prevents adjacent/stacked stairwells on other
          // floors from catching the player
          if (
            playerInsideFloor >= 0 &&
            (playerInsideFloor > room.floorLevel + 1 ||
              playerInsideFloor < room.floorLevel)
          ) continue
          const offset = getStairwellYOffset(
            room,
            roomResult.house.origin.x,
            roomResult.house.origin.z,
            playerPosition.x,
            playerPosition.z
          )
          const dist = Math.abs(offset - lastFloorOffset)
          if (dist < bestStairDist) {
            bestStairDist = dist
            bestStairOffset = offset
            stairResult = roomResult
          }
        } else if (!floorResult || room.floorLevel === currentFL) {
          floorResult = roomResult
        }
      }

      if (stairResult) {
        const room = stairResult.house.rooms[stairResult.roomIndex]
        insideId = id
        newOffset = bestStairOffset
        // Determine effective floor from stairwell Y offset
        // Stairwell connects room.floorLevel to room.floorLevel+1
        const entryFloor = room.floorLevel
        const exitFloor = room.floorLevel + 1
        const entryFloorY = floorYBase(entryFloor, room.wallHeight)
        // Hysteresis: transition at 95% of stairwell rise to avoid flickering
        const exitThreshold =
          entryFloorY +
          (floorYBase(exitFloor, room.wallHeight) - entryFloorY) * 0.95
        if (playerInsideFloor <= entryFloor) {
          effectiveFloor = newOffset >= exitThreshold ? exitFloor : entryFloor
        } else {
          effectiveFloor = newOffset <= exitThreshold ? entryFloor : exitFloor
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
      playerFloorLevel.set(effectiveFloor)
    }

    if (newOffset !== lastFloorOffset) {
      lastFloorOffset = newOffset
      playerFloorOffset.set(newOffset)
    }

    // Animate door pivots
    const dt = _deltaTime / 1000
    for (const [, result] of houses) {
      for (const door of result.doors) {
        const target = door.isOpen ? door.openAngle : door.closedAngle
        const current = door.pivot.rotation.y
        if (Math.abs(current - target) > 0.01) {
          const step = DOOR_SWING_SPEED * dt
          if (current < target) {
            door.pivot.rotation.y = Math.min(current + step, target)
          } else {
            door.pivot.rotation.y = Math.max(current - step, target)
          }
        }
      }
    }
  }

  /**
   * Hide front/back groups based on player floor.
   * Current floor: hide front (south+west walls, roof)
   * Higher floors: hide front and back, keep stair visible
   * Lower floors: fully visible
   */
  function applyFloorVisibility(
    result: HouseGroupResult,
    floor: number
  ) {
    for (const [fl, groups] of result.floorGroups) {
      if (fl === floor) {
        groups.front.position.y = OFFSCREEN_Y
      } else if (fl > floor) {
        groups.front.position.y = OFFSCREEN_Y
        groups.back.position.y = OFFSCREEN_Y
      }
    }
    applyDoorGhostMaterials(result, floor)
  }

  /** Restore merged groups to normal position */
  function resetFloorVisibility(result: HouseGroupResult) {
    for (const [, groups] of result.floorGroups) {
      groups.front.position.y = 0
      groups.back.position.y = 0
      groups.stair.position.y = 0
    }
    resetDoorGhostMaterials(result)
  }

  export function getGroup(): THREE.Group {
    return housingGroup
  }

  export function getDoorMeshes(): THREE.Object3D[] {
    const result: THREE.Object3D[] = []
    for (const h of houses.values()) {
      for (const door of h.doors) result.push(door.pivot)
    }
    return result
  }

  /** Return housing draw call stats for profiling. */
  export function getStats() {
    let mergedMeshes = 0
    for (const [, result] of houses) {
      mergedMeshes += result.mergedMeshCount
    }
    return {
      houses: houses.size,
      mergedMeshes,
    }
  }
</script>

<T is={housingGroup} />
