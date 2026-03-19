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

  interface Props {
    playerPosition: { x: number; y: number; z: number } | null
  }

  let { playerPosition }: Props = $props()

  const housingGroup = new THREE.Group()
  housingGroup.name = 'housingLayer'
  const houses = new SvelteMap<string, HouseGroupResult>()
  let playerInsideHouseId: string | null = null
  const _tmpVec = new THREE.Vector3()

  // Hardcoded test house for Phase 1
  const TEST_HOUSES: HouseData[] = [
    {
      id: 'test_house_1',
      ownerId: 'test',
      origin: { x: -1530, y: 12, z: 475 },
      rooms: [
        {
          localX: 0,
          localZ: 0,
          sizeX: 5,
          sizeZ: 4,
          floorLevel: 0,
          floorTexture: 1,
          roofTexture: 0,
          wallHeight: 3,
          wallNorth: { variant: 'window', texture: 0 },
          wallSouth: { variant: 'door', texture: 0 },
          wallEast: { variant: 'open', texture: 0 },
          wallWest: { variant: 'window', texture: 0 },
        },
        {
          localX: 5,
          localZ: 0,
          sizeX: 4,
          sizeZ: 4,
          floorLevel: 0,
          floorTexture: 2,
          roofTexture: 0,
          wallHeight: 3,
          wallNorth: { variant: 'solid', texture: 1 },
          wallSouth: { variant: 'window', texture: 1 },
          wallEast: { variant: 'solid', texture: 1 },
          wallWest: { variant: 'open', texture: 1 },
        },
      ],
    },
    {
      id: 'test_house_2',
      ownerId: 'test',
      origin: { x: -1545, y: 6.57, z: 465 },
      rooms: [
        {
          localX: 0,
          localZ: 0,
          sizeX: 4,
          sizeZ: 4,
          floorLevel: 0,
          floorTexture: 0,
          roofTexture: 1,
          wallHeight: 3,
          wallNorth: { variant: 'solid', texture: 2 },
          wallSouth: { variant: 'door', texture: 2 },
          wallEast: { variant: 'solid', texture: 2 },
          wallWest: { variant: 'window', texture: 2 },
        },
        // Second floor
        {
          localX: 0,
          localZ: 0,
          sizeX: 4,
          sizeZ: 4,
          floorLevel: 1,
          floorTexture: 3,
          roofTexture: 1,
          wallHeight: 3,
          wallNorth: { variant: 'window', texture: 2 },
          wallSouth: { variant: 'solid', texture: 2 },
          wallEast: { variant: 'window', texture: 2 },
          wallWest: { variant: 'solid', texture: 2 },
        },
      ],
    },
  ]

  // Build test houses immediately
  for (const houseData of TEST_HOUSES) {
    addHouse(houseData)
  }

  onDestroy(() => {
    for (const [, result] of houses) {
      disposeHouseGroup(result.houseGroup)
    }
    houses.clear()
  })

  function addHouse(data: HouseData) {
    const result = buildHouseGroup(data)
    houses.set(data.id, result)
    housingGroup.add(result.houseGroup)
  }

  /** Called from game loop — checks player inside state and toggles front walls */
  export function update(_deltaTime: number) {
    if (!playerPosition) return

    _tmpVec.set(playerPosition.x, playerPosition.y, playerPosition.z)
    let insideId: string | null = null

    for (const [id, result] of houses) {
      if (result.aabb.containsPoint(_tmpVec)) {
        insideId = id
        break
      }
    }

    if (insideId !== playerInsideHouseId) {
      // Restore previous house
      if (playerInsideHouseId) {
        const prev = houses.get(playerInsideHouseId)
        if (prev) prev.frontGroup.visible = true
      }
      // Hide front of new house
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
