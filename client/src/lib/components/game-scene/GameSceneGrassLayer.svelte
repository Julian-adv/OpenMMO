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
    playerPosition?: THREE.Vector3 | null
  }

  let {
    terrainTiles,
    heightManager = null,
    splatManager = null,
    time = 0,
    playerPosition = null,
  }: Props = $props()

  const GRASS_RADIUS = 30 // grass render distance from player (meters)

  // ── Constants ──────────────────────────────────────────
  const TILE_DIM = 64
  const CHANNELS = 4
  const GRASS_DENSITY_MIN = 230 // R channel minimum for grass blades
  const GRASS_DENSITY_MAX = 255
  const GRASS_DENSITY = 10 // blades per cell per axis → 100 blades/cell

  // ── Shared grass blade geometry (created once) ─────────
  const bladeGeometry = createGrassBladeGeometry(0.03, 0.4, 0.4, 0.5)

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

  const ROWS_PER_CHUNK = 2 // rows of cells to process per work item

  function generateGrassForTile(
    tileX: number,
    tileZ: number,
    tileId: string,
    splatData: Uint8Array,
    hMgr: TerrainHeightManager,
  ) {
    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const step = 1.0 / GRASS_DENSITY

    // --- Chunked count pass ---
    const countRand = mulberry32(tileSeed(tileX, tileZ))
    let count = 0
    let countRow = 0

    const countChunk = () => {
      if (!pendingTiles.has(tileId)) {
        pendingTiles.delete(tileId)
        return
      }

      const rowEnd = Math.min(countRow + ROWS_PER_CHUNK, TILE_DIM)
      for (let cz = countRow; cz < rowEnd; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
          if (rVal < GRASS_DENSITY_MIN) continue
          const density = (rVal - GRASS_DENSITY_MIN) / (GRASS_DENSITY_MAX - GRASS_DENSITY_MIN)
          for (let dz = 0; dz < GRASS_DENSITY; dz++) {
            for (let dx = 0; dx < GRASS_DENSITY; dx++) {
              const worldX = tileMinX + cx + dx * step + countRand() * step
              const worldZ = tileMinZ + cz + dz * step + countRand() * step
              if (countRand() >= density) continue
              const worldY = hMgr.getHeightAtWorldPosition(worldX, worldZ)
              if (worldY < 0.05) continue
              count++
            }
          }
        }
      }

      countRow = rowEnd
      if (countRow < TILE_DIM) {
        enqueueTileWork(countChunk)
      } else {
        // Count done — start placement
        if (count === 0 || !pendingTiles.has(tileId)) {
          pendingTiles.delete(tileId)
          return
        }
        startPlacement()
      }
    }

    // --- Chunked placement pass ---
    function startPlacement() {
      const instancedMesh = new THREE.InstancedMesh(
        bladeGeometry,
        grassMaterial,
        count,
      )
      instancedMesh.castShadow = false
      instancedMesh.receiveShadow = false
      instancedMesh.frustumCulled = true

      const placeRand = mulberry32(tileSeed(tileX, tileZ))
      const dummy = new THREE.Object3D()
      let idx = 0
      let placeRow = 0

      const placeChunk = () => {
        if (!pendingTiles.has(tileId)) {
          instancedMesh.dispose()
          return
        }

        const rowEnd = Math.min(placeRow + ROWS_PER_CHUNK, TILE_DIM)
        for (let cz = placeRow; cz < rowEnd; cz++) {
          for (let cx = 0; cx < TILE_DIM; cx++) {
            const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
            if (rVal < GRASS_DENSITY_MIN) continue
            const density = (rVal - GRASS_DENSITY_MIN) / (GRASS_DENSITY_MAX - GRASS_DENSITY_MIN)

            for (let dz = 0; dz < GRASS_DENSITY; dz++) {
              for (let dx = 0; dx < GRASS_DENSITY; dx++) {
                const jitterX = placeRand() * step
                const jitterZ = placeRand() * step
                const worldX = tileMinX + cx + dx * step + jitterX
                const worldZ = tileMinZ + cz + dz * step + jitterZ
                if (placeRand() >= density) continue
                const worldY = hMgr.getHeightAtWorldPosition(worldX, worldZ)
                if (worldY < 0.05) continue

                const rotation = placeRand() * Math.PI * 2
                const scale = 0.7 + placeRand() * 0.6

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

        placeRow = rowEnd
        if (placeRow < TILE_DIM) {
          enqueueTileWork(placeChunk)
        } else {
          instancedMesh.instanceMatrix.needsUpdate = true
          instancedMesh.computeBoundingBox()
          instancedMesh.computeBoundingSphere()

          if (!pendingTiles.has(tileId)) {
            instancedMesh.dispose()
            return
          }
          pendingTiles.delete(tileId)
          tileGrassMap.set(tileId, instancedMesh)
        }
      }

      enqueueTileWork(placeChunk)
    }

    // Kick off count pass
    enqueueTileWork(countChunk)
  }

  function isTileInGrassRange(tile: TerrainTile): boolean {
    if (!playerPosition) return false
    const half = TERRAIN_TILE_SIZE / 2
    const tileMinX = tile.position[0] - half
    const tileMaxX = tile.position[0] + half
    const tileMinZ = tile.position[2] - half
    const tileMaxZ = tile.position[2] + half
    // Distance from player to nearest point on tile AABB
    const dx = Math.max(tileMinX - playerPosition.x, 0, playerPosition.x - tileMaxX)
    const dz = Math.max(tileMinZ - playerPosition.z, 0, playerPosition.z - tileMaxZ)
    return dx * dx + dz * dz < GRASS_RADIUS * GRASS_RADIUS
  }

  // ── Tile lifecycle ─────────────────────────────────────
  $effect(() => {
    if (!heightManager || !splatManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove grass for tiles no longer in range or no longer visible
    for (const [id, mesh] of tileGrassMap) {
      const tile = terrainTiles.find((t) => t.id === id)
      if (!currentTileIds.has(id) || !tile || !isTileInGrassRange(tile)) {
        mesh.dispose()
        tileGrassMap.delete(id)
        pendingTiles.delete(id)
      }
    }

    // Generate grass for new tiles in range
    const hMgr = heightManager
    const sMgr = splatManager
    for (const tile of terrainTiles) {
      if (tileGrassMap.has(tile.id) || pendingTiles.has(tile.id)) continue
      if (!isTileInGrassRange(tile)) continue

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
