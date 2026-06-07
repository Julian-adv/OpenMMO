import type { WebGPURenderer } from 'three/webgpu'
import { TERRAIN_TILE_SIZE, type TerrainTile } from './terrain-utils'
import { drainTileWork } from '../../utils/tileWorkQueue'
import { loadSplatLayers } from '../../utils/splatLayerLoader'
import { initHousingTextures } from '../../utils/housing-textures'
import { loadFlowerColorTexture } from '../../shaders/grass-material'
import type { GraphicsPreset } from '../../stores/graphicsSettings'
import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
import type GameSceneGrassLayer from './GameSceneGrassLayer.svelte'
import type GameSceneHousingLayer from './GameSceneHousingLayer.svelte'

export interface AssetBootstrapDeps {
  renderer: WebGPURenderer
  terrainTiles: TerrainTile[]
  heightManager: TerrainHeightManager
  playerPosition: { x: number; z: number } | null
  grassLayerRef: GameSceneGrassLayer | undefined
  housingLayerRef: GameSceneHousingLayer | undefined
  graphicsPreset: GraphicsPreset
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()))
}

/** Resolves once initial data is ready and pipelines are warmed under the loading dialog. */
export async function bootstrapSceneAssets(
  deps: AssetBootstrapDeps
): Promise<void> {
  const {
    renderer,
    terrainTiles,
    heightManager,
    playerPosition,
    grassLayerRef,
    housingLayerRef,
    graphicsPreset,
  } = deps

  // Pre-fetch all tile heightmaps so they're cached when the TerrainLayer
  // $effect fires. This allows work items to be enqueued immediately.
  const heightPromises = terrainTiles.map((t) => {
    const x = Math.round(t.position[0] / TERRAIN_TILE_SIZE)
    const z = Math.round(t.position[2] / TERRAIN_TILE_SIZE)
    return heightManager.loadHeightmap(x, z).catch(() => {})
  })

  const splatPromise = loadSplatLayers()
  // Await flower texture loading only when grass is enabled. On mobile the
  // grass layer is skipped entirely to avoid WebGPU memory spikes.
  const grassAssetsPromise = graphicsPreset.enableGrassLayer
    ? loadFlowerColorTexture()
    : Promise.resolve()
  const housingTexturesPromise = initHousingTextures()
  // Pre-load housing chunks around the player in parallel with other assets
  // so house geometry is built before the loading screen is dismissed.
  const housingChunksPromise = playerPosition
    ? housingLayerRef?.preloadChunks(playerPosition.x, playerPosition.z)
    : Promise.resolve()

  await Promise.all([
    splatPromise,
    grassAssetsPromise,
    housingTexturesPromise,
    housingChunksPromise,
    ...heightPromises,
  ])

  // Wait two frames: one for Svelte to flush the $effect that enqueues
  // tile work, and another to ensure all microtask .then() chains complete.
  await nextFrame()
  await nextFrame()

  drainTileWork(graphicsPreset.initialTileWorkDrainCount)

  if (graphicsPreset.warmupScenePipelines) {
    // Preallocate all grass slots + seed dummy blades below world so
    // every compute/render pipeline compiles under the loading dialog
    // instead of stalling mid-movement when a new sub-chunk activates.
    grassLayerRef?.warmupGrassPipelines(renderer)
    housingLayerRef?.warmupHousingPipelines()
  }
  // Wind particles: lazy init on first spawn (MeshBasicNodeMaterial
  // compiles fast, not worth blocking the loading screen for)
}
