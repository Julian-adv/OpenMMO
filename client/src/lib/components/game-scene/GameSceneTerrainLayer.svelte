<script lang="ts">
  import * as THREE from 'three'
  import SplatTerrain from '../SplatTerrain.svelte'
  import type { TerrainTile } from './terrain-utils'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
  }: Props = $props()
</script>

{#if terrainGeometry}
  {#each terrainTiles as tile, index (tile.id)}
    <SplatTerrain
      geometry={terrainGeometry}
      position={tile.position}
      bind:mesh={terrainMeshes[index]}
    />
  {/each}
{/if}
