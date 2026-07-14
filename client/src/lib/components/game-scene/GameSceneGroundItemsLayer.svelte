<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { groundItemManager, nowMs } from '../../managers/groundItemManager'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import GroundItem from '../GroundItem.svelte'

  interface Props {
    heightManager?: TerrainHeightManager
  }

  let { heightManager }: Props = $props()

  let spinAngle = $state(0)
  let animationTimeMs = $state(nowMs())
  let group = $state<THREE.Group | undefined>(undefined)

  // Floor filter: dungeon items only show on their depth, surface items
  // only above ground (matches the monster/player visibility rules).
  let viewerFloor = $derived(
    $currentDungeonDepth >= 1 ? -$currentDungeonDepth : 0
  )
  const itemEntries = $derived(
    [...groundItemManager.items].filter(([, data]) =>
      viewerFloor < 0 ? data.floorLevel === viewerFloor : data.floorLevel >= 0
    )
  )

  export function update(deltaTime: number) {
    spinAngle += deltaTime * 1.5
    animationTimeMs = nowMs()
  }

  export function getGroup(): THREE.Group | undefined {
    return group
  }
</script>

<T.Group bind:ref={group}>
  {#each itemEntries as [id, data] (id)}
    <GroundItem {data} rotation={spinAngle} {animationTimeMs} {heightManager} />
  {/each}
</T.Group>
