<script lang="ts">
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import SplatTerrain from '../SplatTerrain.svelte'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    heightManager?: TerrainHeightManager | null
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    heightManager = null,
  }: Props = $props()

  // Internal map for geometry tracking
  const geoMap = new SvelteMap<string, THREE.BufferGeometry>()

  // Reactive array of geometries parallel to terrainTiles
  let tileGeometries = $state<(THREE.BufferGeometry | null)[]>([])

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  $effect(() => {
    if (!terrainGeometry || !heightManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove geometries for tiles no longer in the list
    for (const [id, geo] of geoMap) {
      if (!currentTileIds.has(id)) {
        geo.dispose()
        geoMap.delete(id)
      }
    }

    // Create geometries for new tiles
    const mgr = heightManager
    for (const tile of terrainTiles) {
      if (geoMap.has(tile.id)) continue

      const geo = terrainGeometry.clone()
      geoMap.set(tile.id, geo)

      const { tileX, tileZ } = getTileCoords(tile)
      mgr.registerGeometry(tileX, tileZ, geo)

      mgr.loadHeightmap(tileX, tileZ).then(() => {
        mgr.applyHeightToGeometry(tileX, tileZ, geo)
        // Trigger reactivity update after async height load
        tileGeometries = terrainTiles.map((t) => geoMap.get(t.id) ?? null)
      })
    }

    // Sync reactive array
    tileGeometries = terrainTiles.map((t) => geoMap.get(t.id) ?? null)
  })
</script>

{#if terrainGeometry}
  {#each terrainTiles as tile, index (tile.id)}
    {@const geo = tileGeometries[index]}
    {#if geo}
      <SplatTerrain
        geometry={geo}
        position={tile.position}
        bind:mesh={terrainMeshes[index]}
      />
    {/if}
  {/each}
{/if}
