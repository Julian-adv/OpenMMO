<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'
  import {
    createGrassBladeGeometry,
    createGrassMaterial,
  } from '../../shaders/grass-material'

  interface Props {
    terrainTiles: TerrainTile[]
    heightManager: TerrainHeightManager | null
    splatManager: TerrainSplatManager | null
    time?: number
  }

  let {
    terrainTiles,
    heightManager = null,
    splatManager = null,
    time = 0,
  }: Props = $props()

  // ── Constants ──────────────────────────────────────────
  const TILE_DIM = 64
  const CHANNELS = 4
  const GRASS_THRESHOLD = 128 // R channel value to consider "grass"
  const GRASS_DENSITY = 6 // blades per cell per axis → 36 blades/cell

  // ── Shared grass blade geometry (created once) ─────────
  const bladeGeometry = createGrassBladeGeometry(0.03, 0.5, 0.4, 0.5)

  // ── Shared TSL grass material (created once) ───────────
  const { material: grassMaterial, uniforms: grassUniforms } =
    createGrassMaterial()

  // Update wind time
  $effect(() => {
    grassUniforms.uTime.value = time
  })

  // ── Seeded pseudo-random (deterministic per-tile) ──────
  function mulberry32(seed: number) {
    return () => {
      seed |= 0
      seed = (seed + 0x6d2b79f5) | 0
      let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296
    }
  }

  function tileSeed(tileX: number, tileZ: number): number {
    return ((tileX * 73856093) ^ (tileZ * 19349663)) | 0
  }

  // ── Per-tile InstancedMesh ─────────────────────────────
  const tileGrassMap = new SvelteMap<string, THREE.InstancedMesh>()

  // Track in-flight generation to avoid duplicates
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  function generateGrassForTile(
    tileX: number,
    tileZ: number,
    tileId: string,
    splatData: Uint8Array,
    hMgr: TerrainHeightManager,
  ) {
    const rand = mulberry32(tileSeed(tileX, tileZ))
    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2

    // First pass: count grass instances for pre-allocation
    let count = 0
    const step = 1.0 / GRASS_DENSITY
    for (let cz = 0; cz < TILE_DIM; cz++) {
      for (let cx = 0; cx < TILE_DIM; cx++) {
        const pixelIdx = (cz * TILE_DIM + cx) * CHANNELS
        if (splatData[pixelIdx] < GRASS_THRESHOLD) continue
        for (let dz = 0; dz < GRASS_DENSITY; dz++) {
          for (let dx = 0; dx < GRASS_DENSITY; dx++) {
            const worldX = tileMinX + cx + dx * step + rand() * step
            const worldZ = tileMinZ + cz + dz * step + rand() * step
            const worldY = hMgr.getHeightAtWorldPosition(worldX, worldZ)
            if (worldY >= 0.05) count++
          }
        }
      }
    }

    if (count === 0 || !pendingTiles.has(tileId)) {
      pendingTiles.delete(tileId)
      return
    }

    // Second pass: fill instance matrices + colors
    const rand2 = mulberry32(tileSeed(tileX, tileZ))
    const instancedMesh = new THREE.InstancedMesh(
      bladeGeometry,
      grassMaterial,
      count,
    )
    instancedMesh.castShadow = false
    instancedMesh.receiveShadow = false
    instancedMesh.frustumCulled = false

    const dummy = new THREE.Object3D()
    let idx = 0

    for (let cz = 0; cz < TILE_DIM; cz++) {
      for (let cx = 0; cx < TILE_DIM; cx++) {
        const pixelIdx = (cz * TILE_DIM + cx) * CHANNELS
        if (splatData[pixelIdx] < GRASS_THRESHOLD) continue

        for (let dz = 0; dz < GRASS_DENSITY; dz++) {
          for (let dx = 0; dx < GRASS_DENSITY; dx++) {
            const jitterX = rand2() * step
            const jitterZ = rand2() * step
            const worldX = tileMinX + cx + dx * step + jitterX
            const worldZ = tileMinZ + cz + dz * step + jitterZ
            const worldY = hMgr.getHeightAtWorldPosition(worldX, worldZ)

            if (worldY < 0.05) continue

            const rotation = rand2() * Math.PI * 2
            const scale = 0.7 + rand2() * 0.6 // 0.7 ~ 1.3

            dummy.position.set(worldX, worldY, worldZ)
            dummy.rotation.set(0, rotation, 0)
            dummy.scale.setScalar(scale)
            dummy.updateMatrix()
            instancedMesh.setMatrixAt(idx, dummy.matrix)

            idx++
          }
        }
      }
    }

    instancedMesh.instanceMatrix.needsUpdate = true

    // Only add if tile is still active
    if (!pendingTiles.has(tileId)) {
      instancedMesh.dispose()
      return
    }
    pendingTiles.delete(tileId)
    tileGrassMap.set(tileId, instancedMesh)
  }

  // ── Tile lifecycle ─────────────────────────────────────
  $effect(() => {
    if (!heightManager || !splatManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove grass for tiles no longer visible
    for (const [id, mesh] of tileGrassMap) {
      if (!currentTileIds.has(id)) {
        mesh.dispose()
        tileGrassMap.delete(id)
        pendingTiles.delete(id)
      }
    }

    // Generate grass for new tiles
    const hMgr = heightManager
    const sMgr = splatManager
    for (const tile of terrainTiles) {
      if (tileGrassMap.has(tile.id) || pendingTiles.has(tile.id)) continue

      const { tileX, tileZ } = getTileCoords(tile)
      const tileId = tile.id
      pendingTiles.add(tileId)

      Promise.all([
        hMgr.loadHeightmap(tileX, tileZ),
        sMgr.loadSplatmap(tileX, tileZ),
      ])
        .then(() => {
          const splatData = sMgr.getSplatData(tileX, tileZ)
          if (!splatData || !pendingTiles.has(tileId)) {
            pendingTiles.delete(tileId)
            return
          }

          enqueueTileWork(() => {
            generateGrassForTile(tileX, tileZ, tileId, splatData, hMgr)
          })
        })
        .catch(() => {
          pendingTiles.delete(tileId)
        })
    }
  })
</script>

{#each [...tileGrassMap] as [tileId, mesh] (tileId)}
  <T is={mesh} />
{/each}
