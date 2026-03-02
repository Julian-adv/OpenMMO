<script lang="ts">
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
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
  }

  let {
    terrainGeometry,
    terrainTiles,
    heightManager = null,
    normalMap = null,
    time = 0,
    sunDirection = null,
    sunColor = null,
  }: Props = $props()

  // Cache heightmap textures per tile
  const heightTexMap = new SvelteMap<string, THREE.DataTexture>()

  // Track which tiles have water (true/false/undefined=not checked yet)
  const waterTileSet = new SvelteMap<string, boolean>()

  // Reactive arrays parallel to terrainTiles
  let tileHeightTextures = $state<(THREE.DataTexture | null)[]>([])
  let tileHasWater = $state<boolean[]>([])

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
      const oldTex = heightTexMap.get(id)
      oldTex?.dispose()

      const tex = heightManager.getHeightmapTexture(tileX, tileZ)
      if (tex) {
        heightTexMap.set(id, tex)
        waterTileSet.set(id, true)
      }
    } else {
      const oldTex = heightTexMap.get(id)
      if (oldTex) {
        oldTex.dispose()
        heightTexMap.delete(id)
      }
      waterTileSet.set(id, false)
    }
    syncArrays()
  }

  // Subscribe to height changes from brush edits
  onMount(() => {
    if (!heightManager) return
    const unsub = heightManager.onHeightChanged((tiles: AffectedTile[]) => {
      for (const { tileX, tileZ } of tiles) {
        const id = tileIdFromCoords(tileX, tileZ)
        refreshTile(id, tileX, tileZ)
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
      })
    }

    syncArrays()
  })

  function syncArrays() {
    tileHeightTextures = terrainTiles.map((t) => heightTexMap.get(t.id) ?? null)
    tileHasWater = terrainTiles.map((t) => waterTileSet.get(t.id) ?? false)
  }
</script>

{#if terrainGeometry && normalMap}
  {#each terrainTiles as tile, index (tile.id)}
    {#if tileHasWater[index] && tileHeightTextures[index]}
      <WaterTile
        geometry={terrainGeometry}
        position={tile.position}
        heightmapTexture={tileHeightTextures[index]!}
        {normalMap}
        {time}
        {sunDirection}
        {sunColor}
      />
    {/if}
  {/each}
{/if}
