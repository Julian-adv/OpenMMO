<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'
  import { createRng } from '../../utils/simplex-noise'
  import {
    createGrassBladeGeometry,
    createGrassMaterial,
    GRASS_INSTANCE_POS_ATTR,
    GRASS_TRAIL_COUNT,
    TALL_GRASS_CONFIG,
    SHORT_GRASS_R_MIN,
    SHORT_GRASS_R_MAX,
    TALL_GRASS_R_MIN,
    TALL_GRASS_R_MAX,
    type GrassMaterialUniforms,
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

  const GRASS_BLADES_PER_AXIS = 10 // blades per cell per axis → 100 blades/cell

  // ── Shared geometries (created once) ─────────────────
  const shortBladeGeometry = createGrassBladeGeometry(0.03, 0.4, 0.4, 0.5)
  const tallBladeGeometry = createGrassBladeGeometry(0.05, 0.8, 0.35, 0.4)

  // ── Shared TSL materials (created once) ──────────────
  const { material: shortGrassMaterial, uniforms: shortGrassUniforms } =
    createGrassMaterial()
  const { material: tallGrassMaterial, uniforms: tallGrassUniforms } =
    createGrassMaterial(TALL_GRASS_CONFIG)

  const allUniforms: GrassMaterialUniforms[] = [
    shortGrassUniforms,
    tallGrassUniforms,
  ]

  // ── Player interaction trail with decay ────────────────────
  const TRAIL_MIN_DIST = 0.5 // min distance between trail points
  const TRAIL_RISE = 8.0 // strength gained per second (ramp up over ~0.15s)
  const TRAIL_DECAY = 1.5 // strength lost per second
  const trail: { x: number; z: number; strength: number; decaying: boolean }[] = []
  let lastTrailX = 0
  let lastTrailZ = 0
  let prevTime = 0

  $effect(() => {
    const dt = Math.min(time - prevTime, 0.1)
    prevTime = time

    // Rise until peak, then decay. Prune dead points.
    for (let i = trail.length - 1; i >= 0; i--) {
      if (trail[i].strength < 1.0 && !trail[i].decaying) {
        trail[i].strength = Math.min(1.0, trail[i].strength + TRAIL_RISE * dt)
        if (trail[i].strength >= 1.0) trail[i].decaying = true
      } else {
        trail[i].decaying = true
        trail[i].strength -= TRAIL_DECAY * dt
      }
      if (trail[i].strength <= 0) trail.splice(i, 1)
    }

    // Add new trail point if player moved enough
    if (playerPosition) {
      const dx = playerPosition.x - lastTrailX
      const dz = playerPosition.z - lastTrailZ
      if (dx * dx + dz * dz > TRAIL_MIN_DIST * TRAIL_MIN_DIST) {
        if (trail.length >= GRASS_TRAIL_COUNT) trail.shift()
        trail.push({ x: playerPosition.x, z: playerPosition.z, strength: 0, decaying: false })
        lastTrailX = playerPosition.x
        lastTrailZ = playerPosition.z
      }
    }

    // Write trail into all material uniforms
    for (const u of allUniforms) {
      u.uTime.value = time
      for (let i = 0; i < GRASS_TRAIL_COUNT; i++) {
        if (i < trail.length) {
          u.uTrail[i].value.set(trail[i].x, trail[i].z, trail[i].strength)
        } else {
          u.uTrail[i].value.set(0, 0, 0)
        }
      }
    }
  })

  function tileSeed(tileX: number, tileZ: number): number {
    return ((tileX * 73856093) ^ (tileZ * 19349663)) | 0
  }

  // ── Per-tile InstancedMesh maps ──────────────────────
  const shortGrassMap = new SvelteMap<string, THREE.InstancedMesh>()
  const tallGrassMap = new SvelteMap<string, THREE.InstancedMesh>()
  const allMaps = [shortGrassMap, tallGrassMap]

  // Track in-flight generation to avoid duplicates (prefixed keys)
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  const ROWS_PER_CHUNK = 2 // rows of cells to process per work item

  interface VegetationConfig {
    keyPrefix: string
    rMin: number
    rMax: number
    bladesPerAxis: number
    geometry: THREE.BufferGeometry
    material: THREE.Material
    outputMap: SvelteMap<string, THREE.InstancedMesh>
    scaleMin: number
    scaleRange: number
  }

  const SHORT_GRASS_CFG: VegetationConfig = {
    keyPrefix: 's',
    rMin: SHORT_GRASS_R_MIN,
    rMax: SHORT_GRASS_R_MAX,
    bladesPerAxis: GRASS_BLADES_PER_AXIS,
    geometry: shortBladeGeometry,
    material: shortGrassMaterial,
    outputMap: shortGrassMap,
    scaleMin: 0.7,
    scaleRange: 0.6,
  }

  const TALL_GRASS_CFG: VegetationConfig = {
    keyPrefix: 't',
    rMin: TALL_GRASS_R_MIN,
    rMax: TALL_GRASS_R_MAX,
    bladesPerAxis: GRASS_BLADES_PER_AXIS,
    geometry: tallBladeGeometry,
    material: tallGrassMaterial,
    outputMap: tallGrassMap,
    scaleMin: 0.8,
    scaleRange: 0.5,
  }

  const allConfigs = [SHORT_GRASS_CFG, TALL_GRASS_CFG]

  function generateVegetationForTile(
    cfg: VegetationConfig,
    tileX: number,
    tileZ: number,
    tileId: string,
    splatData: Uint8Array,
    hMgr: TerrainHeightManager,
  ) {
    const pendingKey = `${cfg.keyPrefix}:${tileId}`
    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const step = 1.0 / cfg.bladesPerAxis
    const densityRange = cfg.rMax - cfg.rMin

    // --- Chunked count pass ---
    const countRand = createRng(tileSeed(tileX, tileZ) ^ cfg.rMin)
    let count = 0
    let countRow = 0

    const countChunk = () => {
      if (!pendingTiles.has(pendingKey)) return

      const rowEnd = Math.min(countRow + ROWS_PER_CHUNK, TILE_DIM)
      for (let cz = countRow; cz < rowEnd; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
          if (rVal < cfg.rMin || rVal > cfg.rMax) continue
          const density = densityRange > 0 ? (rVal - cfg.rMin) / densityRange : 1
          for (let dz = 0; dz < cfg.bladesPerAxis; dz++) {
            for (let dx = 0; dx < cfg.bladesPerAxis; dx++) {
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
        if (count === 0 || !pendingTiles.has(pendingKey)) {
          pendingTiles.delete(pendingKey)
          return
        }
        startPlacement()
      }
    }

    // --- Chunked placement pass ---
    function startPlacement() {
      const tileGeometry = cfg.geometry.clone()
      const instancedMesh = new THREE.InstancedMesh(
        tileGeometry,
        cfg.material,
        count,
      )
      instancedMesh.castShadow = false
      instancedMesh.receiveShadow = true
      instancedMesh.frustumCulled = true

      const worldXZArray = new Float32Array(count * 2)

      const placeRand = createRng(tileSeed(tileX, tileZ) ^ cfg.rMin)
      const dummy = new THREE.Object3D()
      let idx = 0
      let placeRow = 0

      const placeChunk = () => {
        if (!pendingTiles.has(pendingKey)) {
          instancedMesh.geometry.dispose()
          instancedMesh.dispose()
          return
        }

        const rowEnd = Math.min(placeRow + ROWS_PER_CHUNK, TILE_DIM)
        for (let cz = placeRow; cz < rowEnd; cz++) {
          for (let cx = 0; cx < TILE_DIM; cx++) {
            const rVal = splatData[(cz * TILE_DIM + cx) * CHANNELS]
            if (rVal < cfg.rMin || rVal > cfg.rMax) continue
            const density = densityRange > 0 ? (rVal - cfg.rMin) / densityRange : 1

            for (let dz = 0; dz < cfg.bladesPerAxis; dz++) {
              for (let dx = 0; dx < cfg.bladesPerAxis; dx++) {
                const jitterX = placeRand() * step
                const jitterZ = placeRand() * step
                const worldX = tileMinX + cx + dx * step + jitterX
                const worldZ = tileMinZ + cz + dz * step + jitterZ
                if (placeRand() >= density) continue
                const worldY = hMgr.getHeightAtWorldPosition(worldX, worldZ)
                if (worldY < 0.05) continue

                const rotation = placeRand() * Math.PI * 2
                const scale = cfg.scaleMin + placeRand() * cfg.scaleRange

                dummy.position.set(worldX, worldY, worldZ)
                dummy.rotation.set(0, rotation, 0)
                dummy.scale.setScalar(scale)
                dummy.updateMatrix()
                instancedMesh.setMatrixAt(idx, dummy.matrix)
                worldXZArray[idx * 2] = worldX
                worldXZArray[idx * 2 + 1] = worldZ
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

          const xzAttr = new THREE.InstancedBufferAttribute(worldXZArray, 2)
          instancedMesh.geometry.setAttribute(GRASS_INSTANCE_POS_ATTR, xzAttr)

          instancedMesh.computeBoundingBox()
          instancedMesh.computeBoundingSphere()

          if (!pendingTiles.has(pendingKey)) {
            instancedMesh.geometry.dispose()
            instancedMesh.dispose()
            return
          }
          pendingTiles.delete(pendingKey)
          cfg.outputMap.set(tileId, instancedMesh)
        }
      }

      enqueueTileWork(placeChunk)
    }

    enqueueTileWork(countChunk)
  }

  function isTileInGrassRange(tile: TerrainTile): boolean {
    if (!playerPosition) return false
    const half = TERRAIN_TILE_SIZE / 2
    const tileMinX = tile.position[0] - half
    const tileMaxX = tile.position[0] + half
    const tileMinZ = tile.position[2] - half
    const tileMaxZ = tile.position[2] + half
    const dx = Math.max(tileMinX - playerPosition.x, 0, playerPosition.x - tileMaxX)
    const dz = Math.max(tileMinZ - playerPosition.z, 0, playerPosition.z - tileMaxZ)
    return dx * dx + dz * dz < GRASS_RADIUS * GRASS_RADIUS
  }

  function disposeMeshFromMap(map: SvelteMap<string, THREE.InstancedMesh>, id: string) {
    const mesh = map.get(id)
    if (mesh) {
      mesh.geometry.dispose()
      mesh.dispose()
      map.delete(id)
    }
  }

  // ── Tile lifecycle ─────────────────────────────────────
  $effect(() => {
    if (!heightManager || !splatManager) return

    const tileById = new Map(terrainTiles.map((t) => [t.id, t]))

    // Remove grass for tiles no longer in range or no longer visible
    for (const map of allMaps) {
      for (const [id] of map) {
        const tile = tileById.get(id)
        if (!tile || !isTileInGrassRange(tile)) {
          disposeMeshFromMap(map, id)
          for (const cfg of allConfigs) pendingTiles.delete(`${cfg.keyPrefix}:${id}`)
        }
      }
    }

    // Generate grass for new tiles in range
    const hMgr = heightManager
    const sMgr = splatManager
    for (const tile of terrainTiles) {
      if (!isTileInGrassRange(tile)) continue

      // Skip if all types already exist or are pending
      const allReady = allConfigs.every(
        (cfg) => cfg.outputMap.has(tile.id) || pendingTiles.has(`${cfg.keyPrefix}:${tile.id}`),
      )
      if (allReady) continue

      const { tileX, tileZ } = getTileCoords(tile)
      const tileId = tile.id

      Promise.all([
        hMgr.loadHeightmap(tileX, tileZ),
        sMgr.loadSplatmap(tileX, tileZ),
      ])
        .then(() => {
          const splatData = sMgr.getSplatData(tileX, tileZ)
          if (!splatData) return

          for (const cfg of allConfigs) {
            const key = `${cfg.keyPrefix}:${tileId}`
            if (cfg.outputMap.has(tileId) || pendingTiles.has(key)) continue
            pendingTiles.add(key)
            enqueueTileWork(() => {
              generateVegetationForTile(cfg, tileX, tileZ, tileId, splatData, hMgr)
            })
          }
        })
        .catch(() => {
          for (const cfg of allConfigs) pendingTiles.delete(`${cfg.keyPrefix}:${tileId}`)
        })
    }
  })
</script>

{#each [...shortGrassMap] as [tileId, mesh] (tileId)}
  <T is={mesh} />
{/each}
{#each [...tallGrassMap] as [tileId, mesh] (`tall_${tileId}`)}
  <T is={mesh} />
{/each}
