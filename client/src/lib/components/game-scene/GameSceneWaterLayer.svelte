<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { NodeMaterial } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import { onMount, onDestroy } from 'svelte'
  import WaterTile from '../WaterTile.svelte'
  import { createWaterMaterial, type WaterMaterialResult } from '../../shaders/water-material'
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

  // ── Shared water material (created once) ──────────────────────
  let sharedMaterial = $state<NodeMaterial | null>(null)
  let waterResult = $state<WaterMaterialResult | null>(null)

  // Default 1x1 heightmap for initial material creation
  const defaultHeightmap = new THREE.DataTexture(
    new Float32Array([0]),
    1,
    1,
    THREE.RedFormat,
    THREE.FloatType
  )
  defaultHeightmap.minFilter = THREE.LinearFilter
  defaultHeightmap.magFilter = THREE.LinearFilter
  defaultHeightmap.needsUpdate = true

  $effect(() => {
    if (!normalMap || !foamMap || !surfaceMap) return
    if (sharedMaterial) return // already created

    const result = createWaterMaterial({
      heightmapTexture: defaultHeightmap,
      normalMap,
      foamMap,
      surfaceMap,
      refractionMap,
    })
    waterResult = result
    sharedMaterial = result.material
  })

  onDestroy(() => {
    sharedMaterial?.dispose()
  })

  // Update time uniform
  $effect(() => {
    if (waterResult) waterResult.uniforms.uTime.value = time
  })

  // Update sun uniforms
  $effect(() => {
    if (!waterResult) return
    if (sunDirection) waterResult.uniforms.uSunDirection.value.copy(sunDirection)
    if (sunColor) waterResult.uniforms.uSunColor.value.copy(sunColor)
    if (cameraDirection) waterResult.uniforms.uCameraDirection.value.copy(cameraDirection)
  })

  // Update refraction map
  $effect(() => {
    if (waterResult && refractionMap) {
      waterResult.uniforms.uRefractionMap.value = refractionMap
    }
  })

  // ── Heightmap texture management per tile ──────────────────────
  const heightTexMap = new SvelteMap<string, THREE.DataTexture>()
  const waterTileSet = new SvelteMap<string, boolean>()

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

  $effect(() => {
    if (!terrainGeometry || !heightManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    for (const [id, tex] of heightTexMap) {
      if (!currentTileIds.has(id)) {
        tex.dispose()
        heightTexMap.delete(id)
        waterTileSet.delete(id)
      }
    }
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

{#if terrainGeometry && sharedMaterial && waterResult}
  <T.Group bind:ref={waterGroup}>
    {#each terrainTiles as tile, index (tile.id)}
      {#if tileHasWater[index] && tileHeightTextures[index]}
        <WaterTile
          geometry={terrainGeometry}
          position={tile.position}
          heightmapTexture={tileHeightTextures[index]!}
          material={sharedMaterial}
          {waterResult}
        />
      {/if}
    {/each}
  </T.Group>
{/if}
