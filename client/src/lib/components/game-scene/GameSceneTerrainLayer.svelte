<script lang="ts">
  import { REFLECTION_LAYER } from '../../effects/ground-reflection'
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial, WebGPURenderer } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import { onDestroy } from 'svelte'
  import {
    makeSplatStandardMaterial,
    createSplatBrushUniforms,
    createSplatHoleUniforms,
    padTileScales,
    padTileSwapUvs,
    type SplatBrushUniforms,
    type SplatHoleUniforms,
  } from '../makeSplatStandardMaterial'
  import type { SplatLayer } from '../makeSplatStandardMaterial'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import {
    loadSplatLayers,
    buildSplatAtlas,
  } from '../../utils/splatLayerLoader'
  import type { SplatAtlasSet } from '../../utils/splatLayerLoader'
  import { mapEditorMode, gridVisible } from '../../stores/debugStore'
  import {
    brushWorldPos,
    brushSize,
    brushMode,
    editorTool,
  } from '../../stores/editorStore'
  import type { BrushMode, EditorTool } from '../../stores/editorStore'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'
  import { currentDungeonId } from '../../stores/dungeonStore'
  import { dungeonManager } from '../../managers/dungeonManager'
  import type { RainPuddleUniforms } from '../../shaders/rain-puddle-nodes'
  import { RainPuddleTracker, type RainSampler } from '../../utils/rainPuddles'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    terrainGroup?: THREE.Group | undefined
    heightManager?: TerrainHeightManager | null
    splatManager?: TerrainSplatManager | null
    syncTileMeshes?: () => void
    terrainMaterialPrecompilePoolSize?: number
    renderer?: WebGPURenderer | null
    camera?: THREE.Camera | null
    rainPuddleUniforms?: RainPuddleUniforms
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    terrainGroup = $bindable<THREE.Group | undefined>(undefined),
    heightManager = null,
    splatManager = null,
    syncTileMeshes = $bindable<() => void>(() => {}),
    terrainMaterialPrecompilePoolSize = 8,
    renderer = null,
    camera = null,
    rainPuddleUniforms,
  }: Props = $props()

  // ── Default resources (created once) ──────────────────
  let _defaultLayers: SplatLayer[] | null = null
  let defaultAtlas: SplatAtlasSet | null = null
  let materialsReady = $state(false)
  let brushUnsubs: (() => void)[] = []

  // Default 1x1 all-grass splatmap for tiles whose splatmap hasn't loaded yet
  const defaultSplat = new THREE.DataTexture(
    new Uint8Array([255, 0, 0, 0]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  defaultSplat.wrapS = defaultSplat.wrapT = THREE.ClampToEdgeWrapping
  defaultSplat.minFilter = THREE.LinearFilter
  defaultSplat.magFilter = THREE.LinearFilter
  defaultSplat.needsUpdate = true

  // Shared brush/grid uniforms
  const brushUniforms: SplatBrushUniforms = createSplatBrushUniforms()

  // Shared dungeon-entrance hole uniforms — referenced by every pooled
  // terrain material so a single sync opens the hole on whichever tile
  // covers the entrance.
  const holeUniforms: SplatHoleUniforms = createSplatHoleUniforms()
  const puddleTracker = new RainPuddleTracker()
  const puddleTarget = new THREE.Vector4()

  export function pauseRainPuddles(seconds: number) {
    puddleTracker.pause(seconds)
  }

  export function updateRainPuddles(
    seconds: number,
    sampleRain: RainSampler,
    restoreHistory: boolean
  ) {
    const samples = puddleTracker.update(seconds, sampleRain, restoreHistory)
    const blend = -Math.expm1(-Math.max(0, seconds) / 0.35)
    for (const tile of terrainTiles) {
      const material = meshMap.get(tile.id)?.material as
        | THREE.Material
        | undefined
      const uniforms = material?.userData.rainPuddles
      if (!uniforms) continue
      const x = tile.position[0] - TERRAIN_TILE_SIZE / 2
      const z = tile.position[2] - TERRAIN_TILE_SIZE / 2
      const a = puddleTracker.sample(x, z, restoreHistory)
      const b = puddleTracker.sample(x + TERRAIN_TILE_SIZE, z, restoreHistory)
      const c = puddleTracker.sample(x, z + TERRAIN_TILE_SIZE, restoreHistory)
      const d = puddleTracker.sample(
        x + TERRAIN_TILE_SIZE,
        z + TERRAIN_TILE_SIZE,
        restoreHistory
      )
      uniforms.origin.value.set(x, z)
      puddleTarget.set(a.wetness, b.wetness, c.wetness, d.wetness)
      uniforms.wetness.value.lerp(puddleTarget, blend)
      puddleTarget.set(a.rain, b.rain, c.rain, d.rain)
      uniforms.rain.value.lerp(puddleTarget, blend)
    }
    return samples
  }

  // Open/close the entrance hole as dungeons register near the player. The
  // shader holes a single rect, which matches the one-dungeon-at-a-time
  // registration (dungeonManager.updateAutoRegister); it would need a rect
  // array if simultaneous registrations ever become possible.
  const holeUnsub = currentDungeonId.subscribe((id) => {
    const rect = id ? dungeonManager.entranceHoleRect() : null
    if (rect) {
      holeUniforms.holeRect.value.set(
        rect.minX,
        rect.minZ,
        rect.maxX,
        rect.maxZ
      )
      holeUniforms.holeActive.value = 1.0
    } else {
      holeUniforms.holeActive.value = 0.0
    }
  })

  // Track whether editor overlay is compiled into materials.
  // Starts false for faster initial pipeline compilation; upgraded on first editor use.
  let editorOverlayCompiled = false

  // ── Tile mesh pool (created on demand, reused across tile lifecycles) ──
  // Meshes are pooled with their geometry and material: the renderer keeps
  // compiled shader state per object, so a fresh mesh per tile recompiles it.
  const meshPool: THREE.Mesh[] = []
  // Template arrays for fast geometry reset (flat plane positions/normals)
  let templatePositions: Float32Array | null = null
  let templateNormals: Float32Array | null = null

  loadSplatLayers().then((layers) => {
    _defaultLayers = layers
    defaultAtlas = buildSplatAtlas(layers)
    materialsReady = true
    setupBrushSync()
  })

  // ── Pipeline precompile (avoids stutters when new tiles enter the scene) ──
  // Every new MeshStandardNodeMaterial produces a distinct WebGPU pipeline that
  // gets compiled lazily on its first render. When the player moves and new
  // tiles appear, this lazy compile stalls the main thread for 100–1000ms per
  // hitch. We preseed the pool with enough meshes to cover tile cycling, then
  // precompile them for every render target (main, refraction, reflection)
  // before they're ever needed.
  const precompileScene = new THREE.Scene()
  let poolPrecompiled = false

  async function preseedAndPrecompilePool() {
    if (poolPrecompiled) return
    if (!renderer || !camera || !terrainGeometry) return
    if (!defaultAtlas || !_defaultLayers) return
    poolPrecompiled = true

    // Preseed the mesh pool with N ready-to-use tile meshes.
    const twins: THREE.Mesh[] = []
    const poolSize = Math.max(1, terrainMaterialPrecompilePoolSize)
    for (let i = 0; i < poolSize; i++) {
      const twin = createTileMesh(
        createDefaultMaterial(),
        terrainGeometry.clone()
      )
      // Spread twins apart so they remain individually visible to the compiler's
      // traversal (some three.js paths merge identical transforms).
      twin.position.set(i * 1000, -10000, 0)
      precompileScene.add(twin)
      twins.push(twin)
    }

    const compileForTarget = async (target: THREE.RenderTarget | null) => {
      const saved = renderer!.getRenderTarget()
      renderer!.setRenderTarget(target)
      try {
        await renderer!.compileAsync(precompileScene, camera!)
      } finally {
        renderer!.setRenderTarget(saved)
      }
    }

    try {
      await compileForTarget(null)
      // TODO: re-enable refraction/reflection target precompile once the
      // WebGPU "texture used as RenderAttachment + TextureBinding" error
      // it triggered (after the river layer landed) is root-caused.
      // Until then: first new-tile refraction/reflection render will stall
      // 100–1000ms compiling pipelines lazily.
    } catch (e) {
      console.warn('[TerrainLayer] pipeline precompile failed', e)
    }

    for (const twin of twins) {
      precompileScene.remove(twin)
      meshPool.push(twin)
    }
  }

  $effect(() => {
    if (
      materialsReady &&
      renderer &&
      camera &&
      terrainGeometry &&
      !poolPrecompiled
    ) {
      preseedAndPrecompilePool()
    }
  })

  /** Create a new terrain material using the default atlas. */
  function createDefaultMaterial(): THREE.Material {
    const mat = makeSplatStandardMaterial({
      atlas: defaultAtlas!,
      tileScales: _defaultLayers!.map((l) => l.tile),
      tileSwapUvs: _defaultLayers!.map((l) => l.swapUv),
      splatMap: defaultSplat,
      splatScale: 1.0,
      sharedBrushUniforms: brushUniforms,
      sharedHoleUniforms: holeUniforms,
      includeEditorOverlay: editorOverlayCompiled,
      rainPuddleUniforms,
    })
    return mat
  }

  /** Upgrade all existing materials to include editor overlay.
   *  Called once when the map editor is first activated. The TSL graph
   *  differs between overlay and non-overlay variants, so every existing
   *  material must be replaced (not just mutated). */
  function upgradeToEditorMaterials() {
    if (editorOverlayCompiled) return
    editorOverlayCompiled = true
    // Pooled meshes lack editor overlay; new ones are built on demand
    for (const mesh of meshPool) {
      ;(mesh.material as THREE.Material).dispose()
      mesh.geometry.dispose()
      mesh.dispose()
    }
    meshPool.length = 0
    for (const mesh of meshMap.values()) {
      const oldMat = mesh.material as THREE.Material
      const newMat = createDefaultMaterial()
      const oldU = oldMat.userData.uniforms
      const newU = newMat.userData.uniforms
      const oldPuddles = oldMat.userData.rainPuddles
      const newPuddles = newMat.userData.rainPuddles
      if (oldPuddles && newPuddles) {
        newPuddles.wetness.value.copy(oldPuddles.wetness.value)
        newPuddles.rain.value.copy(oldPuddles.rain.value)
        newPuddles.origin.value.copy(oldPuddles.origin.value)
      }
      for (const k of Object.keys(oldU)) {
        if (k in newU && oldU[k]?.value !== undefined) {
          newU[k].value = oldU[k].value
        }
      }
      newU.uTileScales.array = oldU.uTileScales.array
      newU.uTileSwapUvs.array = oldU.uTileSwapUvs.array
      mesh.material = newMat
      oldMat.dispose()
    }
  }

  /** Fast memcpy reset to flat plane — avoids full clone cost. */
  function resetGeometry(geo: THREE.BufferGeometry) {
    if (!templatePositions || !templateNormals) return
    ;(geo.getAttribute('position').array as Float32Array).set(templatePositions)
    geo.getAttribute('position').needsUpdate = true
    ;(geo.getAttribute('normal').array as Float32Array).set(templateNormals)
    geo.getAttribute('normal').needsUpdate = true
  }

  function createTileMesh(
    mat: THREE.Material,
    geo: THREE.BufferGeometry
  ): THREE.Mesh {
    const mesh = new THREE.Mesh(geo, mat)
    mesh.receiveShadow = true
    mesh.frustumCulled = false
    mesh.layers.enable(REFLECTION_LAYER)
    return mesh
  }

  /** Take a tile mesh from the pool (reset to defaults), or build one. */
  function acquireMesh(): THREE.Mesh | null {
    const pooled = meshPool.pop()
    if (pooled) {
      resetGeometry(pooled.geometry)
      resetMaterialToDefaults(pooled.material as THREE.Material)
      return pooled
    }
    // Past the preseeded pool a new mesh stalls once for TSL→WGSL compile,
    // then cycles back through the pool.
    if (!defaultAtlas || !_defaultLayers) return null
    return createTileMesh(createDefaultMaterial(), terrainGeometry!.clone())
  }

  function releaseTile(tileId: string) {
    const mesh = meshMap.get(tileId)
    if (!mesh) return
    const [tileX, tileZ] = tileId.split('_').map(Number)
    heightManager?.unregisterGeometry(tileX, tileZ)
    meshMap.delete(tileId)
    meshPool.push(mesh)
  }

  /** Reset a pooled material's uniforms back to defaults. */
  function resetMaterialToDefaults(mat: THREE.Material) {
    const splatMat = mat as MeshStandardNodeMaterial
    const u = splatMat.userData?.uniforms
    if (!u || !defaultAtlas || !_defaultLayers) return
    u.splatMap.value = defaultSplat
    u.diffuseAtlas.value = defaultAtlas.diffuseAtlas
    if (u.normalAtlas && defaultAtlas.normalAtlas) {
      u.normalAtlas.value = defaultAtlas.normalAtlas
    }
    if (u.ormAtlas && defaultAtlas.ormAtlas) {
      u.ormAtlas.value = defaultAtlas.ormAtlas
    }
    u.uTileScales.array = padTileScales(_defaultLayers.map((l) => l.tile))
    u.uTileSwapUvs.array = padTileSwapUvs(_defaultLayers.map((l) => l.swapUv))
    splatMat.userData.rainPuddles?.wetness.value.set(0, 0, 0, 0)
    splatMat.userData.rainPuddles?.rain.value.set(0, 0, 0, 0)
  }

  // ── Brush sync (updates shared uniform nodes → affects all materials) ──
  function setupBrushSync() {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []

    let editorActive = false
    let gridOn = false
    let pos: { x: number; z: number } | null = null
    let size = 3
    let mode: BrushMode = 'raise'
    let tool: EditorTool = 'height'

    const modeToShaderValue: Record<BrushMode, number> = {
      lower: 0.0,
      raise: 1.0,
      flatten: 2.0,
    }

    function sync() {
      brushUniforms.gridVisible.value = editorActive || gridOn ? 1.0 : 0.0
      if (editorActive && pos) {
        brushUniforms.brushActive.value = 1.0
        brushUniforms.brushCenter.value.set(pos.x, pos.z)
        brushUniforms.brushRadius.value = size
        brushUniforms.brushRaise.value = modeToShaderValue[mode]
        brushUniforms.brushToolMode.value = tool === 'splat' ? 1.0 : 0.0
      } else {
        brushUniforms.brushActive.value = 0.0
      }
    }

    brushUnsubs.push(
      mapEditorMode.subscribe((v) => {
        editorActive = v
        if (v) upgradeToEditorMaterials()
        sync()
      }),
      gridVisible.subscribe((v) => {
        gridOn = v
        if (v) upgradeToEditorMaterials()
        sync()
      }),
      brushWorldPos.subscribe((v) => {
        pos = v
        sync()
      }),
      brushSize.subscribe((v) => {
        size = v
        sync()
      }),
      brushMode.subscribe((v) => {
        mode = v
        sync()
      }),
      editorTool.subscribe((v) => {
        tool = v
        sync()
      })
    )
  }

  onDestroy(() => {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []
    holeUnsub()
    for (const id of [...meshMap.keys()]) releaseTile(id)
    for (const mesh of meshPool) mesh.dispose()
  })

  // Keyed by tile id: an index-bound array drifts when streaming evicts tiles
  const meshMap = new SvelteMap<string, THREE.Mesh>()
  $effect(() => {
    terrainMeshes = terrainTiles.map((t) => meshMap.get(t.id))
  })

  function getTileCoords(tile: TerrainTile): {
    tileX: number
    tileZ: number
  } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  // No-op: per-tile materials handle their own textures, no onBeforeRender needed.
  syncTileMeshes = () => {}

  // ── Edge refresh queue ──────────────────────────────────
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const edgeRefreshQueued = new Set<string>()

  function scheduleEdgeRefresh(tileX: number, tileZ: number) {
    if (!heightManager) return
    for (let dz = -1; dz <= 1; dz++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dz === 0) continue
        const nx = tileX + dx
        const nz = tileZ + dz
        const key = `${nx},${nz}`
        if (edgeRefreshQueued.has(key)) continue
        const geo = meshMap.get(`${nx}_${nz}`)?.geometry
        if (geo && heightManager.getHeightmap(nx, nz)) {
          edgeRefreshQueued.add(key)
          enqueueTileWork(() => {
            edgeRefreshQueued.delete(key)
            heightManager?.applyHeightToGeometry(nx, nz, geo)
          })
        }
      }
    }
  }

  // ── Tile lifecycle (geometry + per-tile material + async data loading) ──
  $effect(() => {
    if (!terrainGeometry || !heightManager || !materialsReady) return

    // Capture template data once for geometry pool resets
    if (!templatePositions) {
      const pos = terrainGeometry.getAttribute('position')
      templatePositions = new Float32Array(pos.array as Float32Array)
      const norm = terrainGeometry.getAttribute('normal')
      templateNormals = new Float32Array(norm.array as Float32Array)
    }

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove data for tiles no longer in the list, return to pools
    for (const id of [...meshMap.keys()]) {
      if (!currentTileIds.has(id)) releaseTile(id)
    }

    // Create geometries + assign pooled material + kick off async loads for new tiles
    const mgr = heightManager
    const sMgr = splatManager
    for (const tile of terrainTiles) {
      if (meshMap.has(tile.id)) continue

      const mesh = acquireMesh()
      if (!mesh) continue // pool exhausted (shouldn't happen)
      const geo = mesh.geometry
      mesh.position.set(...tile.position)
      meshMap.set(tile.id, mesh)

      const { tileX, tileZ } = getTileCoords(tile)
      mgr.registerGeometry(tileX, tileZ, geo)

      // Route heightmap application through work queue to prevent
      // multiple applyHeightToGeometry calls from clustering in one frame
      // (especially when heightmaps are already cached and .then() resolves as microtask)
      mgr
        .loadHeightmap(tileX, tileZ)
        .then(() => {
          enqueueTileWork(() => {
            mgr.applyHeightToGeometry(tileX, tileZ, geo)
            scheduleEdgeRefresh(tileX, tileZ)
          })
        })
        .catch(() => {})

      const tileId = tile.id
      if (sMgr) {
        sMgr
          .loadSplatmap(tileX, tileZ)
          .then((tex) => {
            const mat = meshMap.get(tileId)?.material as
              | THREE.Material
              | undefined
            if (!mat) return
            // Mutate the pool material's splatmap uniform in place instead
            // of creating a fresh MeshStandardNodeMaterial — creating a new
            // material triggers a TSL→WGSL compile + pipeline compile on
            // first render that stalls the frame for 100–1000ms. three.js
            // WebGPU rebuilds the texture binding when a TextureNode's
            // `.value` changes. Filter/mipmap/needsUpdate state is already
            // configured by `terrainSplatManager.createTexture`.
            mat.userData.uniforms.splatMap.value = tex
          })
          .catch(() => {})
      }
    }
  })
</script>

{#if terrainGeometry && materialsReady}
  <T.Group bind:ref={terrainGroup}>
    {#each terrainTiles as tile (tile.id)}
      {@const mesh = meshMap.get(tile.id)}
      {#if mesh}
        <T is={mesh} dispose={false} />
      {/if}
    {/each}
  </T.Group>
{/if}
