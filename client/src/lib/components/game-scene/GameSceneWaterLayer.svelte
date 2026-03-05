<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import { onMount } from 'svelte'
  import WaterTile from '../WaterTile.svelte'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { AffectedTile } from '../../managers/terrainHeightManager'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    heightManager?: TerrainHeightManager | null
    normalMap?: THREE.Texture | null
    foamMap?: THREE.Texture | null
    surfaceMap?: THREE.Texture | null
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
    cameraDirection?: THREE.Vector3 | null
    refractionMap?: THREE.Texture | null
    waterGroup?: THREE.Group | undefined
  }

  let {
    terrainGeometry,
    terrainTiles,
    heightManager = null,
    normalMap = null,
    foamMap = null,
    surfaceMap = null,
    time = 0,
    sunDirection = null,
    sunColor = null,
    cameraDirection = null,
    refractionMap = null,
    waterGroup = $bindable(undefined),
  }: Props = $props()

  // Cache heightmap textures per tile (keyed by tile id)
  const heightTexMap = new SvelteMap<string, THREE.DataTexture>()

  // Track which tiles have water (keyed by tile id)
  const waterTileSet = new SvelteMap<string, boolean>()

  function tileIdFromCoords(tileX: number, tileZ: number): string {
    return `${tileX}_${tileZ}`
  }

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  function refreshTile(id: string, tileX: number, tileZ: number) {
    if (!heightManager) return

    const hasW = heightManager.hasWater(tileX, tileZ)
    if (hasW) {
      const existingTex = heightTexMap.get(id)
      if (existingTex) {
        // In-place update — no new texture, no SvelteMap trigger, no material recompile
        heightManager.updateHeightmapTexture(tileX, tileZ, existingTex)
      } else {
        // First time — create new texture (triggers reactivity once)
        const tex = heightManager.getHeightmapTexture(tileX, tileZ)
        if (tex) {
          heightTexMap.set(id, tex)
          waterTileSet.set(id, true)
        }
      }
    } else {
      const oldTex = heightTexMap.get(id)
      if (oldTex) {
        oldTex.dispose()
        heightTexMap.delete(id)
      }
      waterTileSet.set(id, false)
    }
  }

  /** Re-create water textures for adjacent tiles whose 65th edge row/column
   *  references this tile's height data (mirrors refreshAdjacentTileEdges). */
  function refreshAdjacentWaterTiles(tileX: number, tileZ: number) {
    const neighbors = [
      { dx: -1, dz: 0 },
      { dx: 0, dz: -1 },
      { dx: -1, dz: -1 },
    ]
    for (const { dx, dz } of neighbors) {
      const nx = tileX + dx
      const nz = tileZ + dz
      const id = tileIdFromCoords(nx, nz)
      if (heightTexMap.has(id)) {
        refreshTile(id, nx, nz)
      }
    }
  }

  // Subscribe to height changes from brush edits
  onMount(() => {
    if (!heightManager) return
    const unsub = heightManager.onHeightChanged((tiles: AffectedTile[]) => {
      for (const { tileX, tileZ } of tiles) {
        const id = tileIdFromCoords(tileX, tileZ)
        refreshTile(id, tileX, tileZ)
        refreshAdjacentWaterTiles(tileX, tileZ)
      }
    })
    return unsub
  })

  // Initial tile loading + tile list changes
  $effect(() => {
    if (!terrainGeometry || !heightManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove data for tiles no longer in the list
    for (const [id, tex] of heightTexMap) {
      if (!currentTileIds.has(id)) {
        tex.dispose()
        heightTexMap.delete(id)
        waterTileSet.delete(id)
      }
    }
    // Also clean waterTileSet entries without textures
    for (const [id] of waterTileSet) {
      if (!currentTileIds.has(id)) {
        waterTileSet.delete(id)
      }
    }

    const mgr = heightManager
    for (const tile of terrainTiles) {
      if (heightTexMap.has(tile.id) || waterTileSet.has(tile.id)) continue

      const { tileX, tileZ } = getTileCoords(tile)

      mgr.loadHeightmap(tileX, tileZ).then(() => {
        refreshTile(tile.id, tileX, tileZ)
        refreshAdjacentWaterTiles(tileX, tileZ)
      })
    }
  })
</script>

{#if terrainGeometry && normalMap && foamMap && surfaceMap}
  <T.Group bind:ref={waterGroup}>
    {#each terrainTiles as tile (tile.id)}
      {@const hasWater = waterTileSet.get(tile.id) ?? false}
      {@const heightTex = heightTexMap.get(tile.id) ?? null}
      {#if hasWater && heightTex}
        <WaterTile
          geometry={terrainGeometry}
          position={tile.position}
          heightmapTexture={heightTex}
          {normalMap}
          foamMap={foamMap!}
          surfaceMap={surfaceMap!}
          {time}
          {sunDirection}
          {sunColor}
          {cameraDirection}
          {refractionMap}
        />
      {/if}
    {/each}
  </T.Group>
{/if}
