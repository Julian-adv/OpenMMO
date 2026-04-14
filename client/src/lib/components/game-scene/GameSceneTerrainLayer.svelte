<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial, WebGPURenderer } from 'three/webgpu'
  import type { RefractionRenderManager } from '../../managers/refractionRenderManager'
  import type { ReflectionRenderManager } from '../../managers/reflectionRenderManager'
  import { SvelteMap } from 'svelte/reactivity'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import SplatTerrain from '../SplatTerrain.svelte'
  import {
    makeSplatStandardMaterial,
    createSplatBrushUniforms,
    type SplatBrushUniforms,
  } from '../makeSplatStandardMaterial'
  import type { SplatLayer } from '../makeSplatStandardMaterial'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import { loadSplatLayers, buildSplatAtlas } from '../../utils/splatLayerLoader'
  import type { SplatAtlasSet } from '../../utils/splatLayerLoader'
  import { mapEditorMode, gridVisible } from '../../stores/debugStore'
  import {
    brushWorldPos,
    brushSize,
    brushMode,
    editorTool,
    regionMetaVersion,
    currentEditorRegion,
  } from '../../stores/editorStore'
  import type { BrushMode, EditorTool } from '../../stores/editorStore'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    terrainGroup?: THREE.Group | undefined
    heightManager?: TerrainHeightManager | null
    splatManager?: TerrainSplatManager | null
    metaManager?: TerrainMetaManager | null
    syncTileMeshes?: () => void
    renderer?: WebGPURenderer | null
    camera?: THREE.Camera | null
    refractionManager?: RefractionRenderManager | null
    reflectionManager?: ReflectionRenderManager | null
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    terrainGroup = $bindable<THREE.Group | undefined>(undefined),
    heightManager = null,
    splatManager = null,
    metaManager = null,
    syncTileMeshes = $bindable<() => void>(() => {}),
    renderer = null,
    camera = null,
    refractionManager = null,
    reflectionManager = null,
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
    THREE.UnsignedByteType,
  )
  defaultSplat.wrapS = defaultSplat.wrapT = THREE.ClampToEdgeWrapping
  defaultSplat.minFilter = THREE.LinearFilter
  defaultSplat.magFilter = THREE.LinearFilter
  defaultSplat.needsUpdate = true

  // Shared brush/grid uniforms
  const brushUniforms: SplatBrushUniforms = createSplatBrushUniforms()

  // Track whether editor overlay is compiled into materials.
  // Starts false for faster initial pipeline compilation; upgraded on first editor use.
  let editorOverlayCompiled = false

  // ── Material + Geometry pools (created on demand, reused across tile lifecycles) ──
  const materialPool: MeshStandardNodeMaterial[] = []
  const geometryPool: THREE.BufferGeometry[] = []
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
  // hitch. We preseed the pool with enough materials+geometries to cover tile
  // cycling, then precompile them for every render target (main, refraction,
  // reflection) before they're ever needed.
  const PRECOMPILE_POOL_SIZE = 8
  const precompileScene = new THREE.Scene()
  let poolPrecompiled = false

  async function preseedAndPrecompilePool() {
    if (poolPrecompiled) return
    if (!renderer || !camera || !terrainGeometry || !defaultAtlas || !_defaultLayers) return
    poolPrecompiled = true

    // Preseed pool with N ready-to-use material+geometry pairs.
    const twins: THREE.Mesh[] = []
    for (let i = 0; i < PRECOMPILE_POOL_SIZE; i++) {
      const mat = createDefaultMaterial()
      const geo = terrainGeometry.clone()
      materialPool.push(mat)
      geometryPool.push(geo)
      const twin = new THREE.Mesh(geo, mat)
      twin.frustumCulled = false
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
      if (refractionManager) await compileForTarget(refractionManager.target)
      if (reflectionManager) await compileForTarget(reflectionManager.target)
    } catch (e) {
      console.warn('[TerrainLayer] pipeline precompile failed', e)
    }

    // Remove twins but keep the materials/geometries in their pools for reuse.
    for (const twin of twins) precompileScene.remove(twin)
  }

  $effect(() => {
    if (materialsReady && renderer && camera && terrainGeometry && !poolPrecompiled) {
      preseedAndPrecompilePool()
    }
  })

  /** Create a new terrain material using the default atlas. */
  function createDefaultMaterial(): MeshStandardNodeMaterial {
    const mat = makeSplatStandardMaterial({
      atlas: defaultAtlas!,
      tileScales: [_defaultLayers![0].tile, _defaultLayers![1].tile, _defaultLayers![2].tile, _defaultLayers![3].tile],
      splatMap: defaultSplat,
      splatScale: 1.0,
      sharedBrushUniforms: brushUniforms,
      includeEditorOverlay: editorOverlayCompiled,
    })
    return mat
  }

  /** Upgrade all existing materials to include editor overlay.
   *  Called once when the map editor is first activated. */
  function upgradeToEditorMaterials() {
    if (editorOverlayCompiled) return
    editorOverlayCompiled = true
    // Dispose and flush pool — pooled materials lack editor overlay
    for (const m of materialPool) m.dispose()
    materialPool.length = 0
    // Recreate materials for all active tiles
    for (const [key, oldMat] of materialMap) {
      const newMat = createDefaultMaterial()
      // Transfer all per-tile uniform values from old material
      const oldU = oldMat.userData.uniforms
      const newU = newMat.userData.uniforms
      if (oldU && newU) {
        for (const k of Object.keys(oldU)) {
          if (k in newU && oldU[k]?.value !== undefined) {
            newU[k].value = oldU[k].value
          }
        }
      }
      materialMap.set(key, newMat)
      // Update the mesh reference
      const geo = geoMap.get(key)
      if (geo && terrainGroup) {
        terrainGroup.children.forEach((child) => {
          if (child instanceof THREE.Mesh && child.geometry === geo) {
            child.material = newMat
          }
        })
      }
      oldMat.dispose()
    }
  }

  /** Take a material from the pool, or create one on demand. */
  function acquireMaterial(): MeshStandardNodeMaterial | null {
    const mat = materialPool.pop()
    if (mat) {
      resetMaterialToDefaults(mat)
      return mat
    }
    // Create on demand — spreads TSL construction across frames
    if (!defaultAtlas || !_defaultLayers) return null
    return createDefaultMaterial()
  }

  /** Return a material to the pool for reuse. */
  function releaseMaterial(mat: MeshStandardNodeMaterial) {
    materialPool.push(mat)
  }

  /** Take a geometry from the pool (reset to flat), or clone if pool empty. */
  function acquireGeometry(): THREE.BufferGeometry {
    const geo = geometryPool.pop()
    if (geo && templatePositions && templateNormals) {
      // Fast memcpy reset to flat plane — avoids full clone cost
      ;(geo.getAttribute('position').array as Float32Array).set(templatePositions)
      geo.getAttribute('position').needsUpdate = true
      ;(geo.getAttribute('normal').array as Float32Array).set(templateNormals)
      geo.getAttribute('normal').needsUpdate = true
      return geo
    }
    return terrainGeometry!.clone()
  }

  /** Return a geometry to the pool for reuse. */
  function releaseGeometry(geo: THREE.BufferGeometry) {
    geometryPool.push(geo)
  }

  /** Reset a pooled material's uniforms back to defaults. */
  function resetMaterialToDefaults(mat: MeshStandardNodeMaterial) {
    const u = mat.userData?.uniforms
    if (!u || !defaultAtlas || !_defaultLayers) return
    u.splatMap.value = defaultSplat
    u.diffuseAtlas.value = defaultAtlas.diffuseAtlas
    if (u.normalAtlas && defaultAtlas.normalAtlas) {
      u.normalAtlas.value = defaultAtlas.normalAtlas
    }
    if (u.ormAtlas && defaultAtlas.ormAtlas) {
      u.ormAtlas.value = defaultAtlas.ormAtlas
    }
    u.uTile0.value = _defaultLayers[0].tile
    u.uTile1.value = _defaultLayers[1].tile
    u.uTile2.value = _defaultLayers[2].tile
    u.uTile3.value = _defaultLayers[3].tile
  }

  /** Update a per-tile material's atlas/tileScales from resolved region layers. */
  function applyLayersToMaterial(
    mat: MeshStandardNodeMaterial,
    resolved: { layers: SplatLayer[] },
  ) {
    const atlas = buildSplatAtlas(resolved.layers)
    const u = mat.userData?.uniforms
    if (!u) return
    u.diffuseAtlas.value = atlas.diffuseAtlas
    if (u.normalAtlas && atlas.normalAtlas) {
      u.normalAtlas.value = atlas.normalAtlas
    }
    if (u.ormAtlas && atlas.ormAtlas) {
      u.ormAtlas.value = atlas.ormAtlas
    }
    u.uTile0.value = resolved.layers[0].tile
    u.uTile1.value = resolved.layers[1].tile
    u.uTile2.value = resolved.layers[2].tile
    u.uTile3.value = resolved.layers[3].tile
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
      brushUniforms.gridVisible.value =
        editorActive || gridOn ? 1.0 : 0.0
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
      }),
    )
  }

  onDestroy(() => {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []
  })

  // ── Geometry management (SvelteMap, needed for template) ──────
  const geoMap = new SvelteMap<string, THREE.BufferGeometry>()

  // ── Per-tile materials (SvelteMap for template reactivity) ──
  const materialMap = new SvelteMap<string, MeshStandardNodeMaterial>()

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
        const geo = geoMap.get(`${nx}_${nz}`)
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
    for (const [id, geo] of geoMap) {
      if (!currentTileIds.has(id)) {
        releaseGeometry(geo)
        geoMap.delete(id)
        const mat = materialMap.get(id)
        if (mat) releaseMaterial(mat)
        materialMap.delete(id)
      }
    }

    // Create geometries + assign pooled material + kick off async loads for new tiles
    const mgr = heightManager
    const sMgr = splatManager
    const mMgr = metaManager
    for (const tile of terrainTiles) {
      if (geoMap.has(tile.id)) continue

      const tileMat = acquireMaterial()
      if (!tileMat) continue // pool exhausted (shouldn't happen)

      const geo = acquireGeometry()
      geoMap.set(tile.id, geo)
      materialMap.set(tile.id, tileMat)

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
        sMgr.loadSplatmap(tileX, tileZ).then((tex) => {
          const mat = materialMap.get(tileId)
          if (mat) mat.userData.uniforms.splatMap.value = tex
        })
      }

      if (mMgr) {
        mMgr
          .getLayersForTile(tileX, tileZ)
          .then((resolved) => {
            const mat = materialMap.get(tileId)
            if (mat) applyLayersToMaterial(mat, resolved)
          })
          .catch(() => {})
      }
    }
  })

  // Re-resolve region layers when meta changes (texture swap in SplatBrushPanel)
  regionMetaVersion.subscribe((ver) => {
    if (ver === 0 || !metaManager) return
    const region = get(currentEditorRegion)
    if (!region) return
    const { rx, rz } = region
    const mMgr = metaManager

    for (const tile of terrainTiles) {
      const { tileX, tileZ } = getTileCoords(tile)
      if (tileToRegion(tileX) === rx && tileToRegion(tileZ) === rz) {
        mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
          const mat = materialMap.get(tile.id)
          if (mat) applyLayersToMaterial(mat, resolved)
        })
      }
    }
  })
</script>

{#if terrainGeometry && materialsReady}
  <T.Group bind:ref={terrainGroup}>
    {#each terrainTiles as tile, index (tile.id)}
      {@const geo = geoMap.get(tile.id) ?? null}
      {@const tileMat = materialMap.get(tile.id) ?? null}
      {#if geo && tileMat}
        <SplatTerrain
          geometry={geo}
          material={tileMat}
          tileId={tile.id}
          position={tile.position}
          bind:mesh={terrainMeshes[index]}
        />
      {/if}
    {/each}
  </T.Group>
{/if}
