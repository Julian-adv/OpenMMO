<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { groundItemManager } from '../../managers/groundItemManager'
  import GroundItem from '../GroundItem.svelte'

  let spinAngle = $state(0)
  let group = $state<THREE.Group | undefined>(undefined)

  const itemEntries = $derived([...groundItemManager.items])

  export function update(deltaTime: number) {
    spinAngle += deltaTime * 1.5
  }

  export function getGroup(): THREE.Group | undefined {
    return group
  }
</script>

<T.Group bind:ref={group}>
  {#each itemEntries as [id, data] (id)}
    <GroundItem {data} rotation={spinAngle} />
  {/each}
</T.Group>
