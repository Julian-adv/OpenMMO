<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import { onMount } from 'svelte'
  import WaterTile from '../WaterTile.svelte'
  import {
    createWaterMaterial,
    waterHeightFallbackTex,
    waterWetnessFallbackTex,
    type WaterMaterialResult,
  } from '../../shaders/water-material'
  import {
    createWetnessSystem,
    type WetnessResult,
  } from '../../shaders/wetness-compute'
  import { WebGPURenderer } from 'three/webgpu'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { AffectedTile } from '../../managers/terrainHeightManager'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    heightManager?: TerrainHeightManager | null
    normalMap?: THREE.Texture | null
    foamMap?: THREE.Texture | null
    causticsMap?: THREE.Texture | null
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
    cameraDirection?: THREE.Vector3 | null
    moonBrightness?: number
    refractionMap?: THREE.Texture | null
    reflectionMap?: THREE.Texture | null
    waterGroup?: THREE.Group | undefined
  }

  let {
    terrainGeometry,
    terrainTiles,
    heightManager = null,
    normalMap = null,
    foamMap = null,
    causticsMap = null,
    time = 0,
    sunDirection = null,
    sunColor = null,
    cameraDirection = null,
    moonBrightness = 0,
    refractionMap = null,
    reflectionMap = null,
    waterGroup = $bindable(undefined),
  }: Props = $props()

  /** Called from the game loop to render all wetness pre-passes */
  export function renderWetness(renderer: WebGPURenderer) {
    for (const [id, wetness] of wetnessMap) {
      const waterResult = waterMatMap.get(id)
      if (!waterResult) continue

      // Set water material uniforms for this frame (needed for capture pass)
      const u = waterResult.uniforms
      const heightTex = heightTexMap.get(id)
      if (heightTex) u.uHeightmapTexture.value = heightTex
      u.uTime.value = time
      waterResult.updateWaveDirections(time)
      if (sunDirection) u.uSunDirection.value.copy(sunDirection)
      if (sunColor) u.uSunColor.value.copy(sunColor)
      if (cameraDirection) u.uCameraDirection.value.copy(cameraDirection)
      u.uMoonBrightness.value = moonBrightness
      if (refractionMap) u.uRefractionMap.value = refractionMap
      if (reflectionMap) u.uReflectionMap.value = reflectionMap

      // Disable wetness for capture (avoid feedback loop: wetness → alpha → more wetness)
      u.uWetnessMap.value = waterWetnessFallbackTex
      // Output only holeAlpha in alpha channel for accurate shore edge capture
      u.uCaptureMode.value = 1

      // Zero out Gerstner wave steepness so capture mesh has no vertex displacement
      // (ensures UV↔screen pixel mapping stays exact in the ortho capture camera)
      const savedA = u.uWaveA.value.z
      const savedB = u.uWaveB.value.z
      const savedC = u.uWaveC.value.z
      u.uWaveA.value.z = 0
      u.uWaveB.value.z = 0
      u.uWaveC.value.z = 0

      // Capture water alpha + decay
      wetness.update(renderer, waterResult.material, time)

      // Restore wave steepness and normal rendering mode
      u.uWaveA.value.z = savedA
      u.uWaveB.value.z = savedB
      u.uWaveC.value.z = savedC
      u.uCaptureMode.value = 0
      // Set result for main render
      u.uWetnessMap.value = wetness.readTexture
    }
  }

  // Cache heightmap textures per tile (keyed by tile id)
  const heightTexMap = new SvelteMap<string, THREE.DataTexture>()

  // Track which tiles have water (keyed by tile id)
  const waterTileSet = new SvelteMap<string, boolean>()

  // ── Wetness compute system per tile (pooled) ──
  const wetnessMap = new SvelteMap<string, WetnessResult>()
  const wetnessPool: WetnessResult[] = []

  // ── Water material pool (reused across tile lifecycles) ──
  const waterMatPool: WaterMaterialResult[] = []
  const waterMatMap = new SvelteMap<string, WaterMaterialResult>()

  function acquireWaterMaterial(): WaterMaterialResult | null {
    const pooled = waterMatPool.pop()
    if (pooled) return pooled
    // Shared textures must be loaded before creating a new material
    if (!normalMap || !foamMap || !causticsMap) return null
    return createWaterMaterial({
      heightmapTexture: waterHeightFallbackTex,
      normalMap,
      foamMap,
      causticsMap,
      refractionMap,
      reflectionMap,
    })
  }

  function releaseWaterMaterial(id: string) {
    const result = waterMatMap.get(id)
    if (result) {
      result.uniforms.uHeightmapTexture.value = waterHeightFallbackTex
      waterMatMap.delete(id)
      waterMatPool.push(result)
    }
    const wetness = wetnessMap.get(id)
    if (wetness) {
      wetnessMap.delete(id)
      wetnessPool.push(wetness)
    }
  }

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
        // First time — create new texture + acquire pooled material
        const tex = heightManager.getHeightmapTexture(tileX, tileZ)
        if (tex) {
          heightTexMap.set(id, tex)
          waterTileSet.set(id, true)
          // Acquire material from pool and set ALL textures before rendering
          if (!waterMatMap.has(id)) {
            const matResult = acquireWaterMaterial()
            if (!matResult) return // shared textures not ready yet
            const u = matResult.uniforms
            u.uHeightmapTexture.value = tex
            if (normalMap) u.uNormalMap.value = normalMap
            if (foamMap) u.uFoamMap.value = foamMap
            if (causticsMap) u.uCausticsMap.value = causticsMap
            if (refractionMap) u.uRefractionMap.value = refractionMap
            if (reflectionMap) u.uReflectionMap.value = reflectionMap
            // Acquire or create wetness render system for this tile
            const pooledWetness = wetnessPool.pop()
            if (pooledWetness) {
              pooledWetness.reposition(tileX, tileZ)
              wetnessMap.set(id, pooledWetness)
            } else {
              const wetness = createWetnessSystem(
                terrainGeometry!,
                tileX,
                tileZ,
                TERRAIN_TILE_SIZE
              )
              wetnessMap.set(id, wetness)
            }
            waterMatMap.set(id, matResult)
          }
        }
      }
    } else {
      releaseWaterMaterial(id)
      // Don't dispose heightmap texture — Three.js Sampler binding listens for
      // 'dispose' events and nullifies .texture, but _init doesn't sync Sampler
      // bindings (only _update does). If the material is re-pooled and later
      // rendered on a new mesh, createBindGroup sees null → crash.
      // Let GC reclaim it instead; 3x3 grid = at most ~3 textures at a time.
      heightTexMap.delete(id)
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

    // Remove data for tiles no longer in the list, return materials to pool
    for (const [id] of heightTexMap) {
      if (!currentTileIds.has(id)) {
        releaseWaterMaterial(id)
        heightTexMap.delete(id)
        waterTileSet.delete(id)
        wetnessMap.delete(id)
      }
    }
    // Also clean waterTileSet entries without textures
    for (const [id] of waterTileSet) {
      if (!currentTileIds.has(id)) {
        releaseWaterMaterial(id)
        waterTileSet.delete(id)
      }
    }

    const mgr = heightManager
    for (const tile of terrainTiles) {
      if (heightTexMap.has(tile.id) || waterTileSet.has(tile.id)) continue

      const { tileX, tileZ } = getTileCoords(tile)

      mgr.loadHeightmap(tileX, tileZ).then(() => {
        // Route through work queue to prevent clustering when heightmaps are cached
        enqueueTileWork(() => {
          refreshTile(tile.id, tileX, tileZ)
          refreshAdjacentWaterTiles(tileX, tileZ)
        })
      })
    }
  })
</script>

{#if terrainGeometry && normalMap && foamMap && causticsMap}
  <T.Group bind:ref={waterGroup}>
    {#each terrainTiles as tile (tile.id)}
      {@const hasWater = waterTileSet.get(tile.id) ?? false}
      {@const heightTex = heightTexMap.get(tile.id) ?? null}
      {@const waterResult = waterMatMap.get(tile.id) ?? null}
      {#if hasWater && heightTex && waterResult}
        <WaterTile
          geometry={terrainGeometry}
          position={tile.position}
          heightmapTexture={heightTex}
          {waterResult}
          {time}
          {sunDirection}
          {sunColor}
          {cameraDirection}
          {moonBrightness}
          {refractionMap}
          {reflectionMap}
        />
      {/if}
    {/each}
  </T.Group>
{/if}
